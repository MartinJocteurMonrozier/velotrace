//! Concentrateur Bluetooth de velotrace : home trainer FTMS, capteur de
//! cadence CSC, fréquence cardiaque HRS et Zwift Click.
//!
//! [`BleHub`] s'utilise depuis le fil principal de Godot, à chaque image :
//! toutes ses méthodes rendent la main en quelques microsecondes. Le travail
//! Bluetooth tourne sur un fil dédié (avec son propre runtime tokio) ; on lui
//! parle par des canaux et on lit son état dans un instantané partagé.
//!
//! - [`detect`] : rôle d'un appareil d'après son annonce, ordre de la liste.
//! - [`cadence`] : cadence à partir des tours de manivelle CSC.
//! - [`ftms`] : limitation du débit des consignes, réponses du trainer.
//! - [`click`] : boutons des Zwift Click → changements de vitesse.
//! - [`live`] : dernières valeurs reçues, pour l'écran des connexions.

pub mod cadence;
pub mod click;
pub mod detect;
pub mod ftms;
pub mod live;
mod worker;

use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use velo_core::devices::{DeviceEvent, DeviceHub, DeviceKind, RememberedDevice, Shift};
use velo_core::protocol::ControlCommand;

use crate::live::LiveState;
use crate::worker::Request;

/// État de l'adaptateur Bluetooth de l'ordinateur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterState {
    /// Ouverture en cours.
    Starting,
    Ready,
    Scanning,
    /// Pas de Bluetooth utilisable ; le message explique pourquoi.
    Unavailable(String),
}

/// État de la liaison avec un appareil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkState {
    Disconnected,
    Connecting,
    Connected,
    /// Dernière tentative échouée ; le message explique pourquoi.
    Failed(String),
}

/// Appareil vu lors d'un balayage, ou demandé par l'application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundDevice {
    /// Identifiant Bluetooth, à conserver dans [`RememberedDevice::id`].
    pub id: String,
    pub name: String,
    /// Rôles possibles, dans l'ordre de [`DeviceKind::ALL`].
    pub kinds: Vec<DeviceKind>,
    /// Puissance du signal en dBm.
    pub rssi: Option<i16>,
    pub state: LinkState,
    /// Rôle pour lequel l'appareil est connecté (ou en cours de connexion).
    pub role: Option<DeviceKind>,
    /// Niveau de batterie en %, si l'appareil le publie.
    pub battery: Option<u8>,
}

/// Dernières valeurs reçues, pour l'écran des connexions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LiveValues {
    pub power_w: Option<f64>,
    /// Cadence du capteur CSC, sinon celle du trainer.
    pub cadence_rpm: Option<f64>,
    pub speed_kmh: Option<f64>,
    /// FC de la ceinture ou de la montre, sinon celle relayée par le trainer.
    pub heart_rate: Option<u16>,
    pub last_click: Option<(Shift, Instant)>,
    /// Le trainer a accepté la prise de contrôle.
    pub trainer_controlled: bool,
    /// Moment de la dernière donnée reçue, par rôle connecté.
    pub last_update: HashMap<DeviceKind, Instant>,
}

impl LiveValues {
    /// Temps écoulé depuis la dernière donnée reçue pour ce rôle.
    pub fn age(&self, kind: DeviceKind, now: Instant) -> Option<Duration> {
        self.last_update
            .get(&kind)
            .map(|&t| now.saturating_duration_since(t))
    }
}

/// État partagé entre le fil Bluetooth et l'application.
#[derive(Debug)]
pub(crate) struct Shared {
    pub adapter: AdapterState,
    pub devices: HashMap<String, FoundDevice>,
    pub live: LiveState,
}

pub(crate) type SharedState = Arc<Mutex<Shared>>;

/// Verrouille l'état partagé, même si un fil a paniqué en le tenant.
pub(crate) fn lock(shared: &SharedState) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

/// Événements en attente au plus : au-delà (application qui ne lit plus),
/// les nouveaux sont perdus plutôt que d'occuper la mémoire.
const EVENT_BACKLOG: usize = 1024;

/// Délai maximal d'arrêt : libération du trainer et déconnexions.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(5);

/// Concentrateur Bluetooth, piloté depuis le fil principal.
pub struct BleHub {
    requests: UnboundedSender<Request>,
    shared: SharedState,
    events: mpsc::Receiver<DeviceEvent>,
    thread: Option<JoinHandle<()>>,
}

impl BleHub {
    /// Lance le fil Bluetooth et ouvre l'adaptateur en arrière-plan.
    pub fn start() -> BleHub {
        let shared = Arc::new(Mutex::new(Shared {
            adapter: AdapterState::Starting,
            devices: HashMap::new(),
            live: LiveState::default(),
        }));
        let (requests, requests_rx) = unbounded_channel();
        let (events_tx, events) = mpsc::sync_channel(EVENT_BACKLOG);
        let worker_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name("velo-ble".into())
            .spawn(move || worker::run(requests_rx, worker_shared, events_tx));
        let thread = match thread {
            Ok(t) => Some(t),
            Err(e) => {
                lock(&shared).adapter = AdapterState::Unavailable(format!(
                    "impossible de lancer le fil Bluetooth : {e}"
                ));
                None
            }
        };
        BleHub {
            requests,
            shared,
            events,
            thread,
        }
    }

    fn request(&self, r: Request) {
        // Fil arrêté : il n'y a plus personne à qui parler.
        self.requests.send(r).ok();
    }

    pub fn adapter_state(&self) -> AdapterState {
        lock(&self.shared).adapter.clone()
    }

    /// Réessaie d'ouvrir l'adaptateur (après « Bluetooth indisponible »).
    pub fn retry(&self) {
        self.request(Request::Retry);
    }

    /// Balaye pendant `seconds` secondes (entre 1 et 60).
    pub fn scan(&self, seconds: f64) {
        let s = if seconds.is_finite() {
            seconds.clamp(1.0, 60.0)
        } else {
            10.0
        };
        self.request(Request::Scan(Duration::from_secs_f64(s)));
    }

    /// Appareils au rôle connu, triés par rôle puis par puissance du signal.
    pub fn devices(&self) -> Vec<FoundDevice> {
        let mut list: Vec<FoundDevice> = lock(&self.shared)
            .devices
            .values()
            .filter(|d| !d.kinds.is_empty())
            .cloned()
            .collect();
        detect::sort_devices(&mut list);
        list
    }

    /// Connecte l'appareil `id` pour le rôle `kind`. Un seul appareil par
    /// rôle (deux pour les Zwift Click, qui vont par paire) : le précédent
    /// est déconnecté.
    pub fn connect(&self, id: &str, kind: DeviceKind) {
        self.request(Request::Connect {
            id: id.to_owned(),
            kind,
        });
    }

    /// Déconnecte l'appareil (ou les Click) du rôle `kind`.
    pub fn disconnect(&self, kind: DeviceKind) {
        self.request(Request::Disconnect(kind));
    }

    /// Cherche les appareils mémorisés et s'y connecte, en réessayant
    /// régulièrement tant qu'ils restent introuvables.
    pub fn connect_remembered(&self, devices: &[RememberedDevice]) {
        self.request(Request::ConnectRemembered(devices.to_vec()));
    }

    /// Dernières valeurs reçues.
    pub fn live(&self) -> LiveValues {
        lock(&self.shared).live.values()
    }

    /// Rend le contrôle du trainer, déconnecte tout et arrête le fil
    /// Bluetooth. Attend au plus quelques secondes ; sans effet la seconde fois.
    pub fn shutdown(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        self.request(Request::Shutdown);
        let deadline = Instant::now() + SHUTDOWN_WAIT;
        while !thread.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if thread.is_finished() {
            thread.join().ok();
        }
        // Sinon le fil est abandonné : le système fermera les liaisons.
    }
}

impl DeviceHub for BleHub {
    /// Met la commande en file pour le point de contrôle du trainer. Les
    /// consignes (pente, puissance) sont limitées à 4 par seconde, la plus
    /// récente l'emportant ; la prise de contrôle est automatique.
    fn send(&mut self, command: ControlCommand) {
        self.request(Request::Control(command));
    }

    /// Événements reçus depuis le dernier appel. `dt` est ignoré : le temps
    /// est celui des appareils réels.
    fn poll(&mut self, _dt: f64) -> Vec<DeviceEvent> {
        self.events.try_iter().collect()
    }
}

impl Drop for BleHub {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_age() {
        let t0 = Instant::now();
        let mut v = LiveValues::default();
        assert_eq!(v.age(DeviceKind::Trainer, t0), None);
        v.last_update.insert(DeviceKind::Trainer, t0);
        assert_eq!(
            v.age(DeviceKind::Trainer, t0 + Duration::from_millis(1500)),
            Some(Duration::from_millis(1500))
        );
    }

    /// Démarre le vrai concentrateur : sans Bluetooth (CI, conteneur), il doit
    /// signaler l'absence d'adaptateur sans bloquer ni paniquer. Ignoré par
    /// défaut car il touche à la pile Bluetooth de la machine.
    #[test]
    #[ignore]
    fn hub_starts_and_stops_without_blocking() {
        let mut hub = BleHub::start();
        let t = Instant::now();
        hub.scan(2.0);
        hub.connect("00:11:22:33:44:55", DeviceKind::Trainer);
        hub.send(ControlCommand::Simulation {
            wind_ms: 0.0,
            grade: 0.03,
            crr: 0.004,
            cw_kg_m: 0.51,
        });
        let _ = hub.devices();
        let _ = hub.live();
        let _ = hub.poll(0.016);
        assert!(t.elapsed() < Duration::from_millis(5), "{:?}", t.elapsed());

        let deadline = Instant::now() + Duration::from_secs(10);
        while hub.adapter_state() == AdapterState::Starting && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let state = hub.adapter_state();
        assert_ne!(state, AdapterState::Starting);
        eprintln!("état de l'adaptateur : {state:?}");
        if let AdapterState::Unavailable(_) = state {
            hub.retry();
            std::thread::sleep(Duration::from_millis(200));
        }
        let t = Instant::now();
        hub.shutdown();
        assert!(t.elapsed() < SHUTDOWN_WAIT + Duration::from_secs(1));
        hub.shutdown();
    }
}
