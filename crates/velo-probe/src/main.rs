//! velo-probe : vérifie le vrai matériel avant de l'utiliser dans velotrace.
//!
//!   velo-probe scan [secondes]     liste les appareils Bluetooth à portée
//!   velo-probe trainer [nom]       lit le home trainer FTMS et lui envoie 0 %, 2 %, 8 %
//!   velo-probe capteurs            affiche la cadence (CSC) et la FC (HRS)
//!   velo-probe click               affiche les appuis sur les Zwift Click

use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

use btleplug::api::bleuuid::uuid_from_u16;
use btleplug::api::{
    Central, CharPropFlags, Characteristic, Manager as _, Peripheral as _, PeripheralProperties,
    ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;
use tokio::time::{interval, sleep, timeout, Instant};
use uuid::Uuid;
use velo_core::protocol::{self, uuid as ids, zwift, ControlCommand, CrankSample};

type Res<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Trainer,
    Cadence,
    HeartRate,
    ZwiftClick,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Trainer => "home trainer (FTMS)",
            Kind::Cadence => "capteur de cadence (CSC)",
            Kind::HeartRate => "fréquence cardiaque (HRS)",
            Kind::ZwiftClick => "Zwift Click / Ride",
        }
    }
}

fn kinds(props: &PeripheralProperties) -> Vec<Kind> {
    let has = |id: u16| props.services.contains(&uuid_from_u16(id));
    let zwift_service = Uuid::parse_str(zwift::SERVICE).unwrap();
    let name = props.local_name.clone().unwrap_or_default().to_lowercase();
    let mut out = Vec::new();
    if has(ids::FTMS_SERVICE) {
        out.push(Kind::Trainer);
    }
    if has(ids::CSC_SERVICE) {
        out.push(Kind::Cadence);
    }
    if has(ids::HRS_SERVICE) {
        out.push(Kind::HeartRate);
    }
    if props.services.contains(&zwift_service)
        || has(zwift::SERVICE_16)
        || props
            .manufacturer_data
            .contains_key(&zwift::MANUFACTURER_ID)
        || name.contains("zwift")
    {
        out.push(Kind::ZwiftClick);
    }
    out
}

async fn adapter() -> Res<Adapter> {
    let unavailable =
        |e: btleplug::Error| format!("Bluetooth indisponible (service bluetooth lancé ?) : {e}");
    let manager = Manager::new().await.map_err(unavailable)?;
    let adapter = manager
        .adapters()
        .await
        .map_err(unavailable)?
        .into_iter()
        .next()
        .ok_or("aucun adaptateur Bluetooth trouvé (Bluetooth activé ? bluetoothd lancé ?)")?;
    Ok(adapter)
}

/// Balaye pendant `secs` secondes et renvoie les appareils avec leurs propriétés.
async fn scan(adapter: &Adapter, secs: u64) -> Res<Vec<(Peripheral, PeripheralProperties)>> {
    adapter.start_scan(ScanFilter::default()).await?;
    sleep(Duration::from_secs(secs)).await;
    adapter.stop_scan().await.ok();
    let mut out = Vec::new();
    for p in adapter.peripherals().await? {
        if let Some(props) = p.properties().await? {
            out.push((p, props));
        }
    }
    out.sort_by_key(|(_, pr)| std::cmp::Reverse(pr.rssi.unwrap_or(i16::MIN)));
    Ok(out)
}

/// Premier appareil du type voulu (et dont le nom contient `name`, si donné).
async fn find(adapter: &Adapter, kind: Kind, name: Option<&str>) -> Res<Vec<Peripheral>> {
    println!("Recherche : {}…", kind.label());
    let found: Vec<Peripheral> = scan(adapter, 8)
        .await?
        .into_iter()
        .filter(|(_, pr)| kinds(pr).contains(&kind))
        .filter(|(_, pr)| match name {
            Some(n) => pr
                .local_name
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(&n.to_lowercase()),
            None => true,
        })
        .map(|(p, _)| p)
        .collect();
    if found.is_empty() {
        return Err(format!(
            "aucun {} trouvé ; réveille-le (pédale ou appuie) et réessaie",
            kind.label()
        )
        .into());
    }
    Ok(found)
}

async fn connect(p: &Peripheral) -> Res<String> {
    let name = p
        .properties()
        .await?
        .and_then(|pr| pr.local_name)
        .unwrap_or_else(|| p.address().to_string());
    println!("Connexion à {name}…");
    timeout(Duration::from_secs(15), p.connect())
        .await
        .map_err(|_| "délai de connexion dépassé")??;
    p.discover_services().await?;
    Ok(name)
}

fn characteristic(p: &Peripheral, id: Uuid) -> Option<Characteristic> {
    p.characteristics().into_iter().find(|c| c.uuid == id)
}

fn require(p: &Peripheral, id: u16, what: &str) -> Res<Characteristic> {
    characteristic(p, uuid_from_u16(id))
        .ok_or_else(|| format!("caractéristique {what} absente").into())
}

async fn cmd_scan(secs: u64) -> Res<()> {
    let adapter = adapter().await?;
    println!("Balayage pendant {secs} s…\n");
    for (p, pr) in scan(&adapter, secs).await? {
        let k = kinds(&pr);
        let tags = if k.is_empty() {
            String::new()
        } else {
            k.iter().map(|k| k.label()).collect::<Vec<_>>().join(", ")
        };
        println!(
            "{:>4} dBm  {}  {:<24}  {}",
            pr.rssi.map_or("?".into(), |r| r.to_string()),
            p.address(),
            pr.local_name.unwrap_or_else(|| "(sans nom)".into()),
            tags
        );
    }
    Ok(())
}

/// Écrit une commande FTMS et attend sa réponse sur le point de contrôle.
async fn ftms_write(p: &Peripheral, cp: &Characteristic, cmd: ControlCommand) -> Res<()> {
    p.write(cp, &cmd.encode(), WriteType::WithResponse).await?;
    Ok(())
}

fn ftms_result(data: &[u8]) -> Option<String> {
    // Réponse : 0x80, opcode de la requête, code résultat.
    if data.len() < 3 || data[0] != 0x80 {
        return None;
    }
    let result = match data[2] {
        0x01 => "OK",
        0x02 => "non pris en charge",
        0x03 => "paramètre invalide",
        0x04 => "échec",
        0x05 => "contrôle non accordé",
        _ => "code inconnu",
    };
    Some(format!(
        "réponse du trainer à l'opcode 0x{:02X} : {result}",
        data[1]
    ))
}

async fn cmd_trainer(name: Option<&str>) -> Res<()> {
    let adapter = adapter().await?;
    let p = find(&adapter, Kind::Trainer, name).await?.remove(0);
    connect(&p).await?;
    let data = require(&p, ids::FTMS_INDOOR_BIKE_DATA, "Indoor Bike Data")?;
    let cp = require(&p, ids::FTMS_CONTROL_POINT, "point de contrôle FTMS")?;
    p.subscribe(&data).await?;
    p.subscribe(&cp).await?;
    let mut notes = p.notifications().await?;

    ftms_write(&p, &cp, ControlCommand::RequestControl).await?;
    ftms_write(&p, &cp, ControlCommand::StartOrResume).await?;

    // Séquence de pentes : chaque palier dure 30 s.
    let steps = [0.0, 0.02, 0.08, 0.0];
    let step_len = Duration::from_secs(30);
    println!(
        "\nPédale normalement. Paliers de 30 s : 0 %, 2 %, 8 %, puis 0 %. Ctrl+C pour arrêter.\n"
    );
    let start = Instant::now();
    let mut current = usize::MAX;
    let mut tick = interval(Duration::from_secs(1));
    let mut last = protocol::IndoorBikeData::default();
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            Some(n) = notes.next() => {
                if n.uuid == data.uuid {
                    if let Ok(d) = protocol::parse_indoor_bike_data(&n.value) { last = d; }
                } else if n.uuid == cp.uuid {
                    if let Some(msg) = ftms_result(&n.value) { println!("  {msg}"); }
                }
            }
            _ = tick.tick() => {
                let i = (start.elapsed().as_secs() / step_len.as_secs()) as usize;
                if i >= steps.len() { break; }
                if i != current {
                    current = i;
                    println!("→ pente envoyée : {:.0} %", steps[i] * 100.0);
                    let cmd = ControlCommand::Simulation { wind_ms: 0.0, grade: steps[i], crr: 0.004, cw_kg_m: 0.51 };
                    if let Err(e) = ftms_write(&p, &cp, cmd).await { println!("  échec de l'envoi : {e}"); }
                }
                println!(
                    "  {:>5.1} km/h  {:>5.1} rpm  {:>4} W{}",
                    last.speed_kmh.unwrap_or(0.0),
                    last.cadence_rpm.unwrap_or(0.0),
                    last.power_w.unwrap_or(0),
                    last.heart_rate_bpm.map_or(String::new(), |h| format!("  {h} bpm")),
                );
            }
        }
    }
    let flat = ControlCommand::Simulation {
        wind_ms: 0.0,
        grade: 0.0,
        crr: 0.004,
        cw_kg_m: 0.51,
    };
    ftms_write(&p, &cp, flat).await.ok();
    ftms_write(&p, &cp, ControlCommand::Reset).await.ok();
    p.disconnect().await.ok();
    println!("\nTerminé : as-tu senti la résistance augmenter à 2 % puis nettement à 8 % ?");
    Ok(())
}

async fn cmd_capteurs() -> Res<()> {
    let adapter = adapter().await?;
    let mut peripherals = Vec::new();
    for kind in [Kind::Cadence, Kind::HeartRate] {
        match find(&adapter, kind, None).await {
            Ok(mut v) => peripherals.push((kind, v.remove(0))),
            Err(e) => println!("  {e}"),
        }
    }
    if peripherals.is_empty() {
        return Err("aucun capteur trouvé".into());
    }
    let mut streams = futures::stream::SelectAll::new();
    for (kind, p) in &peripherals {
        connect(p).await?;
        let c = match kind {
            Kind::Cadence => require(p, ids::CSC_MEASUREMENT, "CSC Measurement")?,
            _ => require(p, ids::HR_MEASUREMENT, "Heart Rate Measurement")?,
        };
        p.subscribe(&c).await?;
        streams.push(p.notifications().await?);
    }
    println!("\nPédale et garde la montre en diffusion de FC. Ctrl+C pour arrêter.\n");
    let mut prev: Option<CrankSample> = None;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            Some(n) = streams.next() => {
                if n.uuid == uuid_from_u16(ids::CSC_MEASUREMENT) {
                    if let Ok(Some(s)) = protocol::parse_csc_crank(&n.value) {
                        if let Some(rpm) = prev.and_then(|p| protocol::csc_cadence(p, s)) {
                            println!("  cadence {rpm:>5.1} rpm");
                        }
                        prev = Some(s);
                    }
                } else if let Ok(bpm) = protocol::parse_heart_rate(&n.value) {
                    println!("  FC {bpm} bpm");
                }
            }
        }
    }
    for (_, p) in &peripherals {
        p.disconnect().await.ok();
    }
    Ok(())
}

fn zwift_char(p: &Peripheral, prefix: &str) -> Option<Characteristic> {
    p.characteristics().into_iter().find(|c| {
        let s = c.uuid.to_string();
        s.starts_with(prefix) && s.ends_with(zwift::UUID_SUFFIX)
    })
}

async fn cmd_click() -> Res<()> {
    let adapter = adapter().await?;
    let found = find(&adapter, Kind::ZwiftClick, None).await?;
    let mut streams = futures::stream::SelectAll::new();
    let mut writers = Vec::new();
    let mut names = HashMap::new();
    for p in &found {
        let name = connect(p).await?;
        let (Some(rx), Some(tx)) = (
            zwift_char(p, zwift::SYNC_RX_PREFIX),
            zwift_char(p, zwift::SYNC_TX_PREFIX),
        ) else {
            println!("  {name} : caractéristiques Zwift introuvables, ignoré");
            continue;
        };
        for c in [Some(tx), zwift_char(p, zwift::ASYNC_PREFIX)]
            .into_iter()
            .flatten()
        {
            if c.properties
                .intersects(CharPropFlags::NOTIFY | CharPropFlags::INDICATE)
            {
                p.subscribe(&c).await?;
            }
        }
        p.write(&rx, zwift::RIDE_ON, WriteType::WithResponse)
            .await?;
        let addr = p.address().to_string();
        names.insert(addr.clone(), name);
        let addr_for_stream = addr.clone();
        streams.push(
            p.notifications()
                .await?
                .map(move |n| (addr_for_stream.clone(), n))
                .boxed(),
        );
        writers.push((p.clone(), rx));
    }
    if writers.is_empty() {
        return Err("aucun Click utilisable".into());
    }
    println!("\nAppuie sur les boutons + et − de chaque Click. Ctrl+C pour arrêter.");
    println!("Note : le Click gauche V2 peut exiger un « déverrouillage » quotidien par l'appli Zwift.\n");
    let mut last: HashMap<String, u32> = HashMap::new();
    let mut keepalive = interval(Duration::from_secs(3));
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = keepalive.tick() => {
                // Sans « RideOn » régulier, le Click droit se met en veille.
                for (p, rx) in &writers {
                    p.write(rx, zwift::RIDE_ON, WriteType::WithResponse).await.ok();
                }
            }
            Some((addr, n)) = streams.next() => {
                let who = names.get(&addr).cloned().unwrap_or(addr.clone());
                if n.value.starts_with(zwift::RIDE_ON) {
                    println!("  {who} : poignée de main RideOn acceptée");
                } else if n.value.first() == Some(&0xFF) {
                    println!("  {who} : demande de déverrouillage chiffré (Click verrouillé)");
                } else if let Some(map) = zwift::keypad_bitmap(&n.value) {
                    let prev = *last.get(&addr).unwrap_or(&u32::MAX);
                    let (up, down) = zwift::shifts(prev, map);
                    let pressed = zwift::newly_pressed(prev, map);
                    if up { println!("  {who} : + (vitesse plus dure)"); }
                    if down { println!("  {who} : − (vitesse plus souple)"); }
                    if pressed != 0 && !up && !down {
                        println!("  {who} : autre bouton, masque 0x{pressed:08X}");
                    }
                    last.insert(addr, map);
                }
            }
        }
    }
    for (p, _) in &writers {
        p.disconnect().await.ok();
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("scan") => cmd_scan(args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10)).await,
        Some("trainer") => cmd_trainer(args.get(1).map(String::as_str)).await,
        Some("capteurs") => cmd_capteurs().await,
        Some("click") => cmd_click().await,
        _ => {
            eprintln!("usage : velo-probe scan [secondes] | trainer [nom] | capteurs | click");
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("Erreur : {e}");
        std::process::exit(1);
    }
}
