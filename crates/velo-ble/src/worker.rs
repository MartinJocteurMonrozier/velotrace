//! Fil Bluetooth : runtime tokio, adaptateur, balayage et liaisons.
//!
//! Le [`Worker`] reçoit les demandes de l'application, tient la liste des
//! appareils voulus et lance une tâche par liaison ([`Link`]). Chaque
//! liaison se connecte, s'abonne aux mesures de son rôle, publie les valeurs
//! et signale sa fin ; le worker décide alors de la reconnexion.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::mpsc::SyncSender;
use std::time::{Duration, Instant};

use btleplug::api::{
    Central, CentralEvent, CentralState, CharPropFlags, Characteristic, Manager as _,
    Peripheral as _, PeripheralProperties, ScanFilter, ValueNotification, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral, PeripheralId};
use futures::stream::Fuse;
use futures::{Stream, StreamExt};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use tokio::time::{interval, timeout, MissedTickBehavior};
use uuid::Uuid;
use velo_core::devices::{DeviceEvent, DeviceKind, RememberedDevice};
use velo_core::protocol::{self, uuid as ids, zwift, ControlCommand};

use crate::cadence::CrankCadence;
use crate::click::{ClickDecoder, ClickMessage};
use crate::detect::{detect_kinds, kind_rank, uuid16, Advertisement};
use crate::ftms::{
    is_setpoint, parse_control_response, CommandThrottle, ControlResponse, ControlResult,
    OP_REQUEST_CONTROL, OP_START_OR_RESUME,
};
use crate::{lock, AdapterState, FoundDevice, LinkState, SharedState};

/// Demandes de l'application au fil Bluetooth.
pub(crate) enum Request {
    Scan(Duration),
    Retry,
    Connect { id: String, kind: DeviceKind },
    Disconnect(DeviceKind),
    ConnectRemembered(Vec<RememberedDevice>),
    Control(ControlCommand),
    Shutdown,
}

/// Niveau de batterie (service 0x180F).
const BATTERY_LEVEL: u16 = 0x2A19;

/// Ouverture de l'adaptateur.
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
/// Connexion, découverte des services et abonnements.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// Attente de la réponse du trainer à une commande de gestion.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(1);
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// Délai total accordé aux liaisons pour se fermer à l'arrêt.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
/// Balayage lancé automatiquement pour retrouver un appareil mémorisé.
const AUTO_SCAN: Duration = Duration::from_secs(6);
/// Sans « RideOn » régulier, le Click droit se met en veille.
const CLICK_KEEPALIVE: Duration = Duration::from_secs(3);
/// Vérification périodique que l'appareil est toujours connecté.
const HEALTH_CHECK: Duration = Duration::from_secs(5);
/// Délai avant de redemander le contrôle à un trainer qui l'a refusé.
const CONTROL_RETRY: Duration = Duration::from_secs(5);
const WORKER_TICK: Duration = Duration::from_millis(250);
const LINK_TICK: Duration = Duration::from_millis(100);

/// Nombre d'appareils connectés en même temps pour un rôle : les Zwift
/// Click vont par paire (gauche et droit), le reste est unique.
pub(crate) fn max_per_kind(kind: DeviceKind) -> usize {
    match kind {
        DeviceKind::Click => 2,
        _ => 1,
    }
}

/// Attente avant la tentative suivante, après `failures` échecs d'affilée.
pub(crate) fn retry_delay(failures: u32) -> Duration {
    Duration::from_secs(match failures {
        0 => 1,
        1 => 3,
        2 => 5,
        3 => 10,
        4 => 20,
        _ => 30,
    })
}

/// Identifiant stable d'un appareil, conservé dans les réglages : l'adresse
/// Bluetooth, sauf sur les systèmes Apple qui la masquent.
fn device_id(p: &Peripheral) -> String {
    #[cfg(target_vendor = "apple")]
    {
        p.id().to_string()
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        p.address().to_string()
    }
}

fn log(msg: &str) {
    eprintln!("velo-ble : {msg}");
}

/// Point d'entrée du fil Bluetooth.
pub(crate) fn run(
    requests: UnboundedReceiver<Request>,
    shared: SharedState,
    events: SyncSender<DeviceEvent>,
) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("velo-ble-rt")
        .enable_all()
        .build();
    let rt = match rt {
        Ok(rt) => rt,
        Err(e) => {
            lock(&shared).adapter =
                AdapterState::Unavailable(format!("impossible de démarrer le Bluetooth : {e}"));
            return;
        }
    };
    rt.block_on(async move {
        let (internal_tx, internal_rx) = unbounded_channel();
        let mut worker = Worker::new(shared, events, internal_tx);
        worker.run(requests, internal_rx).await;
    });
    rt.shutdown_timeout(Duration::from_secs(1));
}

type CentralEvents = Pin<Box<dyn Stream<Item = CentralEvent> + Send>>;
type Notifications = Fuse<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>>;

/// Prochain événement de l'adaptateur ; en attente indéfinie s'il n'y en a pas.
async fn next_central(events: &mut Option<CentralEvents>) -> CentralEvent {
    loop {
        match events {
            Some(s) => match s.next().await {
                Some(e) => return e,
                None => *events = None,
            },
            None => futures::future::pending::<()>().await,
        }
    }
}

async fn open_adapter() -> Result<(Manager, Adapter, CentralEvents, bool), String> {
    let unavailable =
        |e: btleplug::Error| format!("Bluetooth indisponible (service bluetooth lancé ?) : {e}");
    let open = async {
        let manager = Manager::new().await.map_err(unavailable)?;
        let adapter = manager
            .adapters()
            .await
            .map_err(unavailable)?
            .into_iter()
            .next()
            .ok_or_else(|| {
                "aucun adaptateur Bluetooth trouvé (Bluetooth activé ? bluetoothd lancé ?)"
                    .to_string()
            })?;
        let events = adapter.events().await.map_err(unavailable)?;
        let powered = !matches!(adapter.adapter_state().await, Ok(CentralState::PoweredOff));
        Ok((manager, adapter, events, powered))
    };
    timeout(OPEN_TIMEOUT, open)
        .await
        .map_err(|_| "Bluetooth indisponible : l'adaptateur ne répond pas".to_string())?
}

/// Appareil que l'application veut voir connecté.
struct Wanted {
    kind: DeviceKind,
    id: String,
    name: String,
    /// Réessayer tant que ce n'est pas connecté. Faux pour une première
    /// connexion manuelle (l'échec s'affiche), vrai pour un appareil
    /// mémorisé ou une fois la connexion réussie (reconnexion automatique).
    auto_retry: bool,
    failures: u32,
    next_attempt: Instant,
}

/// Messages du worker à une liaison.
enum LinkMsg {
    Control(ControlCommand),
    /// Le système signale la déconnexion de l'appareil.
    Lost,
    Stop {
        disconnect: bool,
    },
}

/// Messages d'une liaison au worker.
enum Internal {
    Up { generation: u64 },
    Ended { generation: u64, outcome: Outcome },
}

enum Outcome {
    Stopped,
    /// La connexion n'a pas abouti.
    Failed(String),
    /// La liaison établie a été perdue.
    Dropped(String),
}

struct LinkHandle {
    kind: DeviceKind,
    id: String,
    generation: u64,
    tx: UnboundedSender<LinkMsg>,
    task: JoinHandle<()>,
}

#[derive(Default)]
struct ScanState {
    active: bool,
    /// Fin du balayage demandé par l'utilisateur.
    user_until: Option<Instant>,
    /// Fin du balayage lancé pour retrouver un appareil mémorisé.
    auto_until: Option<Instant>,
}

struct Worker {
    shared: SharedState,
    events: SyncSender<DeviceEvent>,
    internal: UnboundedSender<Internal>,
    /// Gardé en vie avec l'adaptateur.
    _manager: Option<Manager>,
    adapter: Option<Adapter>,
    powered: bool,
    /// Dernière erreur de l'adaptateur (balayage refusé…), effacée par `retry`.
    failed: Option<String>,
    peripherals: HashMap<String, Peripheral>,
    ids: HashMap<PeripheralId, String>,
    wanted: Vec<Wanted>,
    links: Vec<LinkHandle>,
    /// Liaisons arrêtées qui finissent de se fermer.
    stopping: Vec<JoinHandle<()>>,
    scan: ScanState,
    /// Dernière consigne de l'application, transmise au prochain trainer.
    setpoint: Option<ControlCommand>,
    next_generation: u64,
}

impl Worker {
    fn new(
        shared: SharedState,
        events: SyncSender<DeviceEvent>,
        internal: UnboundedSender<Internal>,
    ) -> Self {
        Self {
            shared,
            events,
            internal,
            _manager: None,
            adapter: None,
            powered: false,
            failed: None,
            peripherals: HashMap::new(),
            ids: HashMap::new(),
            wanted: Vec::new(),
            links: Vec::new(),
            stopping: Vec::new(),
            scan: ScanState::default(),
            setpoint: None,
            next_generation: 0,
        }
    }

    async fn run(
        &mut self,
        mut requests: UnboundedReceiver<Request>,
        mut internal: UnboundedReceiver<Internal>,
    ) {
        let mut central = self.open().await;
        let mut tick = interval(WORKER_TICK);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                r = requests.recv() => match r {
                    None | Some(Request::Shutdown) => break,
                    Some(Request::Retry) => {
                        if self.adapter.is_none() {
                            central = self.open().await;
                        } else {
                            self.retry_adapter().await;
                        }
                    }
                    Some(r) => self.on_request(r).await,
                },
                ev = next_central(&mut central) => self.on_central(ev).await,
                Some(msg) = internal.recv() => self.on_internal(msg),
                _ = tick.tick() => self.maintain().await,
            }
        }
        self.shutdown().await;
    }

    // --- Adaptateur ---------------------------------------------------------

    async fn open(&mut self) -> Option<CentralEvents> {
        lock(&self.shared).adapter = AdapterState::Starting;
        match open_adapter().await {
            Ok((manager, adapter, events, powered)) => {
                self._manager = Some(manager);
                self.adapter = Some(adapter);
                self.powered = powered;
                self.failed = None;
                self.scan = ScanState::default();
                self.refresh_adapter_state();
                Some(events)
            }
            Err(msg) => {
                self._manager = None;
                self.adapter = None;
                lock(&self.shared).adapter = AdapterState::Unavailable(msg);
                None
            }
        }
    }

    async fn retry_adapter(&mut self) {
        self.failed = None;
        if let Some(adapter) = &self.adapter {
            if let Ok(state) = adapter.adapter_state().await {
                self.powered = state != CentralState::PoweredOff;
            }
        }
        self.refresh_adapter_state();
    }

    fn refresh_adapter_state(&self) {
        if self.adapter.is_none() {
            return;
        }
        let state = if !self.powered {
            AdapterState::Unavailable(
                "Bluetooth désactivé : active-le dans les réglages du système".into(),
            )
        } else if let Some(msg) = &self.failed {
            AdapterState::Unavailable(msg.clone())
        } else if self.scan.active {
            AdapterState::Scanning
        } else {
            AdapterState::Ready
        };
        lock(&self.shared).adapter = state;
    }

    // --- Balayage -----------------------------------------------------------

    async fn start_scan(&mut self, user: Option<Duration>, auto: Option<Duration>) {
        let Some(adapter) = self.adapter.clone() else {
            return;
        };
        if !self.powered {
            return;
        }
        let now = Instant::now();
        if let Some(d) = user {
            if self.scan.user_until.is_none() {
                self.forget_stale_devices();
            }
            self.scan.user_until = Some(self.scan.user_until.map_or(now + d, |u| u.max(now + d)));
        }
        if let Some(d) = auto {
            self.scan.auto_until = Some(now + d);
        }
        if !self.scan.active {
            match adapter.start_scan(ScanFilter::default()).await {
                Ok(()) => {
                    self.scan.active = true;
                    self.failed = None;
                }
                Err(e) => {
                    self.scan = ScanState::default();
                    self.failed = Some(format!("balayage Bluetooth impossible : {e}"));
                }
            }
        }
        self.refresh_adapter_state();
    }

    async fn stop_scan(&mut self) {
        if let Some(adapter) = self.adapter.clone() {
            adapter.stop_scan().await.ok();
            // Comme `velo-probe scan` : on relit la liste complète à la fin.
            self.sweep(&adapter).await;
        }
        self.scan = ScanState::default();
        self.refresh_adapter_state();
    }

    /// Retire de la liste les appareils ni connectés ni voulus, avant un
    /// nouveau balayage : ne restent que ceux à portée.
    fn forget_stale_devices(&self) {
        let keep: Vec<&str> = self
            .links
            .iter()
            .map(|l| l.id.as_str())
            .chain(self.wanted.iter().map(|w| w.id.as_str()))
            .collect();
        lock(&self.shared)
            .devices
            .retain(|id, _| keep.contains(&id.as_str()));
    }

    /// Relit les appareils connus de l'adaptateur.
    async fn sweep(&mut self, adapter: &Adapter) {
        let Ok(list) = adapter.peripherals().await else {
            return;
        };
        for p in list {
            let props = p.properties().await.ok().flatten();
            self.learn(p, props);
        }
    }

    /// Enregistre un appareil et met à jour la liste affichée.
    fn learn(&mut self, p: Peripheral, props: Option<PeripheralProperties>) {
        let id = device_id(&p);
        self.ids.insert(p.id(), id.clone());
        self.peripherals.insert(id.clone(), p);
        let Some(props) = props else {
            return;
        };
        let makers: Vec<u16> = props.manufacturer_data.keys().copied().collect();
        let kinds = detect_kinds(&Advertisement {
            name: props.local_name.as_deref(),
            services: &props.services,
            manufacturer_ids: &makers,
        });
        let wanted = self.wanted.iter().any(|w| w.id == id);
        let mut s = lock(&self.shared);
        match s.devices.get_mut(&id) {
            Some(d) => {
                if let Some(name) = props.local_name {
                    d.name = name;
                }
                let merged: Vec<DeviceKind> = DeviceKind::ALL
                    .into_iter()
                    .filter(|k| d.kinds.contains(k) || kinds.contains(k))
                    .collect();
                d.kinds = merged;
                if props.rssi.is_some() {
                    d.rssi = props.rssi;
                }
            }
            // Les appareils mis en cache par le système mais hors de portée
            // (sans RSSI) ne sont pas listés.
            None if !kinds.is_empty() && (props.rssi.is_some() || wanted) => {
                s.devices.insert(
                    id.clone(),
                    FoundDevice {
                        name: props.local_name.unwrap_or_else(|| id.clone()),
                        id,
                        kinds,
                        rssi: props.rssi,
                        state: LinkState::Disconnected,
                        role: None,
                        battery: None,
                    },
                );
            }
            None => {}
        }
    }

    // --- Demandes de l'application -----------------------------------------

    async fn on_request(&mut self, r: Request) {
        match r {
            Request::Scan(d) => self.start_scan(Some(d), None).await,
            Request::Connect { id, kind } => {
                let name = lock(&self.shared)
                    .devices
                    .get(&id)
                    .map_or_else(|| id.clone(), |d| d.name.clone());
                self.want(kind, id, name, false);
                self.maintain().await;
            }
            Request::Disconnect(kind) => {
                self.wanted.retain(|w| w.kind != kind);
                self.stop_links(|l| l.kind == kind);
            }
            Request::ConnectRemembered(list) => {
                for d in list {
                    self.want(d.kind, d.id, d.name, true);
                }
                self.maintain().await;
            }
            Request::Control(cmd) => {
                if is_setpoint(&cmd) {
                    self.setpoint = Some(cmd);
                } else if cmd == ControlCommand::Reset {
                    self.setpoint = None;
                }
                for l in self.links.iter().filter(|l| l.kind == DeviceKind::Trainer) {
                    l.tx.send(LinkMsg::Control(cmd)).ok();
                }
            }
            // Traités par la boucle principale.
            Request::Retry | Request::Shutdown => {}
        }
    }

    /// Ajoute un appareil voulu, en libérant la place s'il y en a déjà un
    /// (ou deux pour les Click) pour ce rôle.
    fn want(&mut self, kind: DeviceKind, id: String, name: String, auto_retry: bool) {
        let now = Instant::now();
        if let Some(w) = self
            .wanted
            .iter_mut()
            .find(|w| w.kind == kind && w.id == id)
        {
            w.auto_retry |= auto_retry;
            w.failures = 0;
            w.next_attempt = now;
            return;
        }
        while self.wanted.iter().filter(|w| w.kind == kind).count() >= max_per_kind(kind) {
            let Some(i) = self.wanted.iter().position(|w| w.kind == kind) else {
                break;
            };
            let old = self.wanted.remove(i);
            self.stop_links(|l| l.kind == kind && l.id == old.id);
            self.set_link_state(&old.id, LinkState::Disconnected, None);
        }
        {
            let mut s = lock(&self.shared);
            let entry = s.devices.entry(id.clone()).or_insert_with(|| FoundDevice {
                id: id.clone(),
                name: name.clone(),
                kinds: vec![kind],
                rssi: None,
                state: LinkState::Disconnected,
                role: None,
                battery: None,
            });
            if !entry.kinds.contains(&kind) {
                entry.kinds.push(kind);
                entry.kinds.sort_by_key(|&k| kind_rank(k));
            }
        }
        self.wanted.push(Wanted {
            kind,
            id,
            name,
            auto_retry,
            failures: 0,
            next_attempt: now,
        });
    }

    fn set_link_state(&self, id: &str, state: LinkState, role: Option<DeviceKind>) {
        if let Some(d) = lock(&self.shared).devices.get_mut(id) {
            d.state = state;
            d.role = role;
        }
    }

    /// Arrête les liaisons choisies, sans attendre qu'elles se ferment.
    fn stop_links(&mut self, pred: impl Fn(&LinkHandle) -> bool) {
        let (stop, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.links)
            .into_iter()
            .partition(|l| pred(l));
        self.links = keep;
        for l in stop {
            // Le même appareil peut servir à un autre rôle : on le garde alors.
            let other = self.links.iter().find(|k| k.id == l.id).map(|k| k.kind);
            l.tx.send(LinkMsg::Stop {
                disconnect: other.is_none(),
            })
            .ok();
            match other {
                Some(kind) => {
                    if let Some(d) = lock(&self.shared).devices.get_mut(&l.id) {
                        d.role = Some(kind);
                    }
                }
                None => self.set_link_state(&l.id, LinkState::Disconnected, None),
            }
            self.clear_live(l.kind);
            self.stopping.push(l.task);
        }
    }

    /// Oublie les valeurs d'un rôle qui n'a plus de liaison.
    fn clear_live(&self, kind: DeviceKind) {
        if !self.links.iter().any(|l| l.kind == kind) {
            lock(&self.shared).live.clear(kind);
        }
    }

    // --- Événements -------------------------------------------------------

    async fn on_central(&mut self, ev: CentralEvent) {
        match ev {
            CentralEvent::DeviceDiscovered(pid)
            | CentralEvent::DeviceUpdated(pid)
            | CentralEvent::ServicesAdvertisement { id: pid, .. }
            | CentralEvent::ManufacturerDataAdvertisement { id: pid, .. }
            | CentralEvent::ServiceDataAdvertisement { id: pid, .. } => {
                let Some(adapter) = self.adapter.clone() else {
                    return;
                };
                if let Ok(p) = adapter.peripheral(&pid).await {
                    let props = p.properties().await.ok().flatten();
                    self.learn(p, props);
                }
            }
            CentralEvent::DeviceDisconnected(pid) => {
                let Some(id) = self.ids.get(&pid) else {
                    return;
                };
                for l in self.links.iter().filter(|l| &l.id == id) {
                    l.tx.send(LinkMsg::Lost).ok();
                }
            }
            CentralEvent::DeviceConnected(_) => {}
            CentralEvent::StateUpdate(state) => {
                self.powered = state != CentralState::PoweredOff;
                if !self.powered {
                    self.scan = ScanState::default();
                }
                self.refresh_adapter_state();
            }
        }
    }

    fn on_internal(&mut self, msg: Internal) {
        match msg {
            Internal::Up { generation } => {
                let Some(l) = self.links.iter().find(|l| l.generation == generation) else {
                    return;
                };
                let (kind, id) = (l.kind, l.id.clone());
                if let Some(w) = self
                    .wanted
                    .iter_mut()
                    .find(|w| w.kind == kind && w.id == id)
                {
                    w.auto_retry = true;
                    w.failures = 0;
                }
                self.set_link_state(&id, LinkState::Connected, Some(kind));
            }
            Internal::Ended {
                generation,
                outcome,
            } => {
                // Liaison déjà retirée (arrêtée ou remplacée) : rien à faire.
                let Some(i) = self.links.iter().position(|l| l.generation == generation) else {
                    return;
                };
                let l = self.links.remove(i);
                self.clear_live(l.kind);
                let now = Instant::now();
                let wanted = self
                    .wanted
                    .iter()
                    .position(|w| w.kind == l.kind && w.id == l.id);
                match (outcome, wanted) {
                    (Outcome::Failed(msg), Some(w)) => {
                        log(&format!("{} : {msg}", self.wanted[w].name));
                        let w_ref = &mut self.wanted[w];
                        w_ref.failures += 1;
                        w_ref.next_attempt = now + retry_delay(w_ref.failures);
                        let role = w_ref.auto_retry.then_some(l.kind);
                        if !w_ref.auto_retry {
                            self.wanted.remove(w);
                        }
                        self.set_link_state(&l.id, LinkState::Failed(msg), role);
                    }
                    (outcome, Some(w)) => {
                        // Liaison perdue : reconnexion rapide, puis espacée.
                        let reason = match outcome {
                            Outcome::Dropped(msg) => msg,
                            _ => "liaison fermée".into(),
                        };
                        log(&format!("{} : {reason}, reconnexion", self.wanted[w].name));
                        let w_ref = &mut self.wanted[w];
                        w_ref.failures = 0;
                        w_ref.next_attempt = now + retry_delay(0);
                        self.set_link_state(&l.id, LinkState::Connecting, Some(l.kind));
                    }
                    (_, None) => self.set_link_state(&l.id, LinkState::Disconnected, None),
                }
            }
        }
    }

    /// Fin des balayages, tentatives de connexion et reconnexions.
    async fn maintain(&mut self) {
        let now = Instant::now();
        self.stopping.retain(|t| !t.is_finished());

        if self.scan.active {
            let until = self.scan.user_until.max(self.scan.auto_until);
            let auto_only = self.scan.user_until.is_none();
            let all_found = self
                .wanted
                .iter()
                .all(|w| self.peripherals.contains_key(&w.id));
            if until.map_or(true, |u| now >= u) || (auto_only && all_found) {
                self.stop_scan().await;
            }
        }

        let Some(adapter) = self.adapter.clone() else {
            return;
        };
        if !self.powered {
            return;
        }
        let due: Vec<usize> = (0..self.wanted.len())
            .filter(|&i| {
                let w = &self.wanted[i];
                now >= w.next_attempt
                    && !self.links.iter().any(|l| l.kind == w.kind && l.id == w.id)
            })
            .collect();
        if due.is_empty() {
            return;
        }
        if due
            .iter()
            .any(|&i| !self.peripherals.contains_key(&self.wanted[i].id))
        {
            // Appareil déjà connu du système (appairé, en cache) ?
            self.sweep(&adapter).await;
        }

        let mut need_scan = false;
        let mut give_up = Vec::new();
        for i in due {
            let (kind, id) = (self.wanted[i].kind, self.wanted[i].id.clone());
            if let Some(p) = self.peripherals.get(&id).cloned() {
                self.spawn_link(kind, id, p);
                continue;
            }
            let w = &mut self.wanted[i];
            if w.auto_retry {
                w.failures += 1;
                w.next_attempt = now + retry_delay(w.failures);
                need_scan = true;
                self.set_link_state(&id, LinkState::Connecting, Some(kind));
            } else {
                give_up.push(i);
            }
        }
        for i in give_up.into_iter().rev() {
            let w = self.wanted.remove(i);
            self.set_link_state(
                &w.id,
                LinkState::Failed(
                    "appareil introuvable : réveille-le et relance la recherche".into(),
                ),
                None,
            );
        }
        if need_scan && !self.scan.active {
            self.start_scan(None, Some(AUTO_SCAN)).await;
        }
    }

    fn spawn_link(&mut self, kind: DeviceKind, id: String, peripheral: Peripheral) {
        self.next_generation += 1;
        let generation = self.next_generation;
        let (tx, msgs) = unbounded_channel();
        let link = Link {
            ctx: LinkCtx {
                kind,
                id: id.clone(),
                peripheral,
                shared: self.shared.clone(),
                events: self.events.clone(),
            },
            generation,
            internal: self.internal.clone(),
            msgs,
            setpoint: if kind == DeviceKind::Trainer {
                self.setpoint
            } else {
                None
            },
        };
        let task = tokio::spawn(link.run());
        self.links.push(LinkHandle {
            kind,
            id: id.clone(),
            generation,
            tx,
            task,
        });
        self.set_link_state(&id, LinkState::Connecting, Some(kind));
    }

    /// Rend le contrôle du trainer et ferme toutes les liaisons.
    async fn shutdown(&mut self) {
        if self.scan.active {
            if let Some(adapter) = &self.adapter {
                adapter.stop_scan().await.ok();
            }
        }
        self.wanted.clear();
        let mut tasks = std::mem::take(&mut self.stopping);
        for l in std::mem::take(&mut self.links) {
            l.tx.send(LinkMsg::Stop { disconnect: true }).ok();
            tasks.push(l.task);
        }
        timeout(SHUTDOWN_TIMEOUT, futures::future::join_all(tasks))
            .await
            .ok();
    }
}

// --- Liaison avec un appareil ----------------------------------------------

/// Ce qu'une liaison partage avec ses fonctions.
struct LinkCtx {
    kind: DeviceKind,
    id: String,
    peripheral: Peripheral,
    shared: SharedState,
    events: SyncSender<DeviceEvent>,
}

impl LinkCtx {
    fn emit(&self, ev: DeviceEvent) {
        // File pleine : l'application ne lit plus, l'événement est perdu.
        self.events.try_send(ev).ok();
    }

    fn name(&self) -> String {
        lock(&self.shared)
            .devices
            .get(&self.id)
            .map_or_else(|| self.id.clone(), |d| d.name.clone())
    }
}

struct Link {
    ctx: LinkCtx,
    generation: u64,
    internal: UnboundedSender<Internal>,
    msgs: UnboundedReceiver<LinkMsg>,
    setpoint: Option<ControlCommand>,
}

/// Commande du trainer.
struct TrainerCtl {
    data: Uuid,
    cp: Option<Characteristic>,
    throttle: CommandThrottle,
    control_requested: bool,
    control_retry_at: Option<Instant>,
}

enum Role {
    Trainer(TrainerCtl),
    Cadence {
        measurement: Uuid,
        cadence: CrankCadence,
    },
    HeartRate {
        measurement: Uuid,
    },
    Click {
        rx: Characteristic,
        decoder: ClickDecoder,
        last_keepalive: Instant,
        warned_locked: bool,
    },
}

struct Session {
    notes: Notifications,
    role: Role,
    battery: Option<Uuid>,
}

impl Session {
    fn trainer(&mut self) -> Option<&mut TrainerCtl> {
        match &mut self.role {
            Role::Trainer(t) => Some(t),
            _ => None,
        }
    }
}

fn characteristic(p: &Peripheral, id: Uuid) -> Option<Characteristic> {
    p.characteristics().into_iter().find(|c| c.uuid == id)
}

fn require(p: &Peripheral, id: u16, what: &str) -> Result<Characteristic, String> {
    characteristic(p, uuid16(id)).ok_or_else(|| format!("caractéristique {what} absente"))
}

fn zwift_char(p: &Peripheral, prefix: &str) -> Option<Characteristic> {
    p.characteristics().into_iter().find(|c| {
        let s = c.uuid.to_string();
        s.starts_with(prefix) && s.ends_with(zwift::UUID_SUFFIX)
    })
}

async fn subscribe(p: &Peripheral, c: &Characteristic) -> Result<(), String> {
    p.subscribe(c)
        .await
        .map_err(|e| format!("abonnement impossible : {e}"))
}

async fn write(p: &Peripheral, c: &Characteristic, data: &[u8]) -> Result<(), String> {
    match timeout(WRITE_TIMEOUT, p.write(c, data, WriteType::WithResponse)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(format!("écriture impossible : {e}")),
        Err(_) => Err("délai d'écriture dépassé".into()),
    }
}

async fn disconnect(p: &Peripheral) {
    timeout(DISCONNECT_TIMEOUT, p.disconnect()).await.ok();
}

/// Connexion, découverte des services et abonnements propres au rôle.
async fn setup(p: &Peripheral, kind: DeviceKind) -> Result<Session, String> {
    if !p.is_connected().await.unwrap_or(false) {
        p.connect()
            .await
            .map_err(|e| format!("connexion impossible : {e}"))?;
    }
    p.discover_services()
        .await
        .map_err(|e| format!("découverte des services impossible : {e}"))?;

    let role = match kind {
        DeviceKind::Trainer => {
            let data = require(p, ids::FTMS_INDOOR_BIKE_DATA, "Indoor Bike Data (FTMS)")?;
            let cp = characteristic(p, uuid16(ids::FTMS_CONTROL_POINT));
            subscribe(p, &data).await?;
            if let Some(cp) = &cp {
                subscribe(p, cp).await?;
            }
            Role::Trainer(TrainerCtl {
                data: data.uuid,
                cp,
                throttle: CommandThrottle::default(),
                control_requested: false,
                control_retry_at: None,
            })
        }
        DeviceKind::Cadence => {
            let c = require(p, ids::CSC_MEASUREMENT, "CSC Measurement")?;
            subscribe(p, &c).await?;
            Role::Cadence {
                measurement: c.uuid,
                cadence: CrankCadence::new(),
            }
        }
        DeviceKind::HeartRate => {
            let c = require(p, ids::HR_MEASUREMENT, "Heart Rate Measurement")?;
            subscribe(p, &c).await?;
            Role::HeartRate {
                measurement: c.uuid,
            }
        }
        DeviceKind::Click => {
            let (Some(rx), Some(tx)) = (
                zwift_char(p, zwift::SYNC_RX_PREFIX),
                zwift_char(p, zwift::SYNC_TX_PREFIX),
            ) else {
                return Err("caractéristiques Zwift introuvables".into());
            };
            for c in [Some(tx), zwift_char(p, zwift::ASYNC_PREFIX)]
                .into_iter()
                .flatten()
            {
                if c.properties
                    .intersects(CharPropFlags::NOTIFY | CharPropFlags::INDICATE)
                {
                    subscribe(p, &c).await?;
                }
            }
            Role::Click {
                rx,
                decoder: ClickDecoder::new(),
                last_keepalive: Instant::now(),
                warned_locked: false,
            }
        }
    };

    // La batterie est facultative : on ignore ses erreurs.
    let battery = characteristic(p, uuid16(BATTERY_LEVEL));
    if let Some(b) = &battery {
        if b.properties.contains(CharPropFlags::NOTIFY) {
            p.subscribe(b).await.ok();
        }
    }

    let notes = p
        .notifications()
        .await
        .map_err(|e| format!("notifications indisponibles : {e}"))?
        .fuse();

    if let Role::Click { rx, .. } = &role {
        // Poignée de main : sans elle, le Click n'envoie rien.
        write(p, rx, zwift::RIDE_ON).await?;
    }

    Ok(Session {
        notes,
        role,
        battery: battery.map(|b| b.uuid),
    })
}

impl Link {
    async fn run(mut self) {
        let outcome = self.run_inner().await;
        self.internal
            .send(Internal::Ended {
                generation: self.generation,
                outcome,
            })
            .ok();
    }

    async fn run_inner(&mut self) -> Outcome {
        let ctx = &self.ctx;
        let p = &ctx.peripheral;

        // Connexion, interruptible par une demande d'arrêt.
        let mut early = Vec::new();
        let connecting = timeout(CONNECT_TIMEOUT, setup(p, ctx.kind));
        tokio::pin!(connecting);
        let setup = loop {
            tokio::select! {
                r = &mut connecting => break r,
                m = self.msgs.recv() => match m {
                    None | Some(LinkMsg::Stop { disconnect: true }) => {
                        disconnect(p).await;
                        return Outcome::Stopped;
                    }
                    Some(LinkMsg::Stop { disconnect: false }) => return Outcome::Stopped,
                    Some(LinkMsg::Control(c)) => early.push(c),
                    Some(LinkMsg::Lost) => {}
                },
            }
        };
        let mut s = match setup {
            Ok(Ok(s)) => s,
            Ok(Err(msg)) => {
                disconnect(p).await;
                return Outcome::Failed(msg);
            }
            Err(_) => {
                disconnect(p).await;
                return Outcome::Failed("délai de connexion dépassé".into());
            }
        };
        self.internal
            .send(Internal::Up {
                generation: self.generation,
            })
            .ok();
        read_battery(ctx, s.battery).await;

        // Dernière consigne connue, puis commandes reçues pendant la connexion.
        if let Some(cmd) = self.setpoint {
            command(ctx, &mut s, cmd).await;
        }
        for cmd in early {
            command(ctx, &mut s, cmd).await;
        }

        let mut tick = interval(LINK_TICK);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut last_health = Instant::now();
        loop {
            tokio::select! {
                n = s.notes.next() => match n {
                    Some(n) => {
                        on_notification(ctx, &mut s, n);
                    }
                    None => {
                        disconnect(p).await;
                        return Outcome::Dropped("liaison interrompue".into());
                    }
                },
                m = self.msgs.recv() => match m {
                    None | Some(LinkMsg::Stop { disconnect: true }) => {
                        release(ctx, &mut s).await;
                        disconnect(p).await;
                        return Outcome::Stopped;
                    }
                    Some(LinkMsg::Stop { disconnect: false }) => {
                        release(ctx, &mut s).await;
                        return Outcome::Stopped;
                    }
                    Some(LinkMsg::Control(cmd)) => command(ctx, &mut s, cmd).await,
                    Some(LinkMsg::Lost) => {
                        return Outcome::Dropped("appareil déconnecté".into());
                    }
                },
                _ = tick.tick() => {
                    let now = Instant::now();
                    on_tick(ctx, &mut s, now).await;
                    if now.duration_since(last_health) >= HEALTH_CHECK {
                        last_health = now;
                        if !p.is_connected().await.unwrap_or(true) {
                            return Outcome::Dropped("appareil hors de portée".into());
                        }
                    }
                }
            }
        }
    }
}

async fn read_battery(ctx: &LinkCtx, battery: Option<Uuid>) {
    let Some(uuid) = battery else {
        return;
    };
    let Some(c) = characteristic(&ctx.peripheral, uuid) else {
        return;
    };
    if !c.properties.contains(CharPropFlags::READ) {
        return;
    }
    if let Ok(Ok(v)) = timeout(WRITE_TIMEOUT, ctx.peripheral.read(&c)).await {
        if let Some(&level) = v.first() {
            set_battery(ctx, level);
        }
    }
}

fn set_battery(ctx: &LinkCtx, level: u8) {
    if let Some(d) = lock(&ctx.shared).devices.get_mut(&ctx.id) {
        d.battery = Some(level.min(100));
    }
}

/// Traite une notification ; renvoie la réponse du point de contrôle FTMS
/// si c'en est une.
fn on_notification(
    ctx: &LinkCtx,
    s: &mut Session,
    n: ValueNotification,
) -> Option<ControlResponse> {
    let now = Instant::now();
    if Some(n.uuid) == s.battery {
        if let Some(&level) = n.value.first() {
            set_battery(ctx, level);
        }
        return None;
    }
    match &mut s.role {
        Role::Trainer(t) => {
            if n.uuid == t.data {
                if let Ok(d) = protocol::parse_indoor_bike_data(&n.value) {
                    lock(&ctx.shared).live.trainer(d.clone(), now);
                    ctx.emit(DeviceEvent::Trainer(d));
                }
            } else if t.cp.as_ref().is_some_and(|c| c.uuid == n.uuid) {
                let r = parse_control_response(&n.value)?;
                match r.result {
                    ControlResult::Success => {
                        if r.request == OP_REQUEST_CONTROL {
                            lock(&ctx.shared).live.set_trainer_controlled(true);
                        }
                    }
                    ControlResult::ControlNotPermitted => {
                        // Contrôle perdu (autre appli, redémarrage) : on le
                        // redemandera avant de renvoyer la consigne.
                        t.control_requested = false;
                        t.throttle.resend();
                        lock(&ctx.shared).live.set_trainer_controlled(false);
                    }
                    other => log(&format!(
                        "le trainer répond « {} » à l'opcode 0x{:02X}",
                        other.label(),
                        r.request
                    )),
                }
                return Some(r);
            }
        }
        Role::Cadence {
            measurement,
            cadence,
        } => {
            if n.uuid == *measurement {
                if let Ok(Some(sample)) = protocol::parse_csc_crank(&n.value) {
                    if let Some(rpm) = cadence.on_sample(sample, now) {
                        publish_cadence(ctx, rpm, now);
                    }
                }
            }
        }
        Role::HeartRate { measurement } => {
            if n.uuid == *measurement {
                if let Ok(bpm) = protocol::parse_heart_rate(&n.value) {
                    lock(&ctx.shared).live.heart_rate(bpm, now);
                    ctx.emit(DeviceEvent::HeartRate(bpm));
                }
            }
        }
        Role::Click {
            decoder,
            warned_locked,
            ..
        } => match decoder.feed(&n.value) {
            ClickMessage::Buttons(shifts) => {
                let mut shared = lock(&ctx.shared);
                shared.live.click_alive(now);
                for shift in shifts {
                    shared.live.click(shift, now);
                    ctx.emit(DeviceEvent::Shift(shift));
                }
            }
            ClickMessage::RideOnAck => lock(&ctx.shared).live.click_alive(now),
            ClickMessage::Locked => {
                if !*warned_locked {
                    *warned_locked = true;
                    log(&format!(
                        "{} demande un déverrouillage chiffré (Click verrouillé)",
                        ctx.name()
                    ));
                }
            }
            ClickMessage::Other => {}
        },
    }
    None
}

fn publish_cadence(ctx: &LinkCtx, rpm: f64, now: Instant) {
    lock(&ctx.shared).live.cadence(rpm, now);
    ctx.emit(DeviceEvent::Cadence(rpm));
}

/// Travail périodique : consignes en attente, cadence nulle, Click éveillé.
async fn on_tick(ctx: &LinkCtx, s: &mut Session, now: Instant) {
    match &mut s.role {
        Role::Trainer(_) => flush(ctx, s).await,
        Role::Cadence { cadence, .. } => {
            if let Some(rpm) = cadence.tick(now) {
                publish_cadence(ctx, rpm, now);
            }
        }
        Role::HeartRate { .. } => {}
        Role::Click {
            rx, last_keepalive, ..
        } => {
            if now.duration_since(*last_keepalive) >= CLICK_KEEPALIVE {
                *last_keepalive = now;
                write(&ctx.peripheral, rx, zwift::RIDE_ON).await.ok();
            }
        }
    }
}

/// Commande de l'application pour le trainer.
async fn command(ctx: &LinkCtx, s: &mut Session, cmd: ControlCommand) {
    let Some(t) = s.trainer() else {
        return;
    };
    if is_setpoint(&cmd) {
        t.throttle.submit(cmd);
        flush(ctx, s).await;
        return;
    }
    let Some(cp) = t.cp.clone() else {
        return;
    };
    match cmd {
        ControlCommand::RequestControl => {
            t.control_retry_at = None;
            take_control(ctx, s).await;
        }
        ControlCommand::Reset => {
            t.throttle.clear();
            t.control_requested = false;
            lock(&ctx.shared).live.set_trainer_controlled(false);
            if let Err(e) = write(&ctx.peripheral, &cp, &cmd.encode()).await {
                log(&format!("remise à zéro du trainer : {e}"));
            }
        }
        _ => {
            if let Err(e) = write(&ctx.peripheral, &cp, &cmd.encode()).await {
                log(&format!("commande du trainer : {e}"));
            }
        }
    }
}

/// Attend la réponse du trainer à l'opcode `request`, en traitant les
/// autres notifications au passage. `None` si elle ne vient pas.
async fn await_response(ctx: &LinkCtx, s: &mut Session, request: u8) -> Option<ControlResult> {
    let deadline = tokio::time::Instant::now() + RESPONSE_TIMEOUT;
    loop {
        let n = tokio::time::timeout_at(deadline, s.notes.next())
            .await
            .ok()??;
        if let Some(r) = on_notification(ctx, s, n) {
            if r.request == request {
                return Some(r.result);
            }
        }
    }
}

/// Prise de contrôle puis démarrage, comme `velo-probe trainer`.
async fn take_control(ctx: &LinkCtx, s: &mut Session) -> bool {
    let Some(cp) = s.trainer().and_then(|t| t.cp.clone()) else {
        return false;
    };
    for (cmd, op) in [
        (ControlCommand::RequestControl, OP_REQUEST_CONTROL),
        (ControlCommand::StartOrResume, OP_START_OR_RESUME),
    ] {
        if let Err(e) = write(&ctx.peripheral, &cp, &cmd.encode()).await {
            log(&format!("prise de contrôle du trainer : {e}"));
            return false;
        }
        // Sans réponse, on considère la commande acceptée (certains
        // trainers n'indiquent rien) ; un refus explicite arrête tout.
        match await_response(ctx, s, op).await {
            None | Some(ControlResult::Success) => {}
            Some(r) if op == OP_REQUEST_CONTROL => {
                log(&format!("prise de contrôle refusée : {}", r.label()));
                return false;
            }
            Some(_) => {}
        }
    }
    if let Some(t) = s.trainer() {
        t.control_requested = true;
    }
    true
}

/// Envoie la consigne en attente si son tour est venu.
async fn flush(ctx: &LinkCtx, s: &mut Session) {
    let now = Instant::now();
    let Some(t) = s.trainer() else {
        return;
    };
    if t.throttle.latest().is_none() {
        return;
    }
    let Some(cp) = t.cp.clone() else {
        // Trainer non pilotable : les consignes sont ignorées.
        t.throttle.clear();
        return;
    };
    if !t.control_requested {
        if t.control_retry_at.is_some_and(|at| now < at) {
            return;
        }
        if !take_control(ctx, s).await {
            if let Some(t) = s.trainer() {
                t.control_retry_at = Some(Instant::now() + CONTROL_RETRY);
            }
            return;
        }
    }
    let Some(t) = s.trainer() else {
        return;
    };
    let Some(cmd) = t.throttle.due(Instant::now()) else {
        return;
    };
    if let Err(e) = write(&ctx.peripheral, &cp, &cmd.encode()).await {
        log(&format!("consigne non transmise : {e}"));
        if let Some(t) = s.trainer() {
            t.throttle.resend();
        }
    }
}

/// Rend la main au trainer avant de le quitter.
async fn release(ctx: &LinkCtx, s: &mut Session) {
    let Some(t) = s.trainer() else {
        return;
    };
    if let (true, Some(cp)) = (t.control_requested, t.cp.clone()) {
        t.control_requested = false;
        timeout(
            RESPONSE_TIMEOUT,
            ctx.peripheral.write(
                &cp,
                &ControlCommand::Reset.encode(),
                WriteType::WithResponse,
            ),
        )
        .await
        .ok();
    }
    lock(&ctx.shared).live.set_trainer_controlled(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_grows_then_caps() {
        assert_eq!(retry_delay(0), Duration::from_secs(1));
        let delays: Vec<u64> = (1..8).map(|n| retry_delay(n).as_secs()).collect();
        assert_eq!(delays, [3, 5, 10, 20, 30, 30, 30]);
    }

    #[test]
    fn clicks_go_in_pairs() {
        assert_eq!(max_per_kind(DeviceKind::Click), 2);
        for k in [
            DeviceKind::Trainer,
            DeviceKind::Cadence,
            DeviceKind::HeartRate,
        ] {
            assert_eq!(max_per_kind(k), 1);
        }
    }
}
