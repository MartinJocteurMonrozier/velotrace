//! Appareils Bluetooth pour Godot : recherche, connexion, valeurs en direct
//! (écran des connexions) et source de mesures d'une sortie réelle.

use godot::prelude::*;
use std::time::Instant;
use velo_ble::{AdapterState, BleHub, LinkState};
use velo_core::devices::{DeviceEvent, DeviceHub, DeviceKind, RememberedDevice, Shift};
use velo_core::protocol::ControlCommand;

#[derive(GodotClass)]
#[class(base = Node)]
pub struct VeloDevices {
    base: Base<Node>,
    /// Démarré au premier besoin : en mode test, le Bluetooth n'est jamais ouvert.
    hub: Option<BleHub>,
}

#[godot_api]
impl INode for VeloDevices {
    fn init(base: Base<Node>) -> Self {
        Self { base, hub: None }
    }

    fn exit_tree(&mut self) {
        if let Some(h) = self.hub.as_mut() {
            h.shutdown();
        }
    }
}

impl VeloDevices {
    fn hub(&mut self) -> &mut BleHub {
        self.hub.get_or_insert_with(BleHub::start)
    }
}

impl DeviceHub for VeloDevices {
    fn send(&mut self, command: ControlCommand) {
        if let Some(h) = self.hub.as_mut() {
            h.send(command);
        }
    }

    fn poll(&mut self, dt: f64) -> Vec<DeviceEvent> {
        self.hub.as_mut().map_or_else(Vec::new, |h| h.poll(dt))
    }
}

fn kind(key: &GString) -> Option<DeviceKind> {
    DeviceKind::from_key(&key.to_string())
}

#[godot_api]
impl VeloDevices {
    /// `state` : « starting », « ready », « scanning » ou « unavailable », et `message`.
    #[func]
    fn adapter_state(&mut self) -> VarDictionary {
        let (state, message) = match self.hub().adapter_state() {
            AdapterState::Starting => ("starting", String::new()),
            AdapterState::Ready => ("ready", String::new()),
            AdapterState::Scanning => ("scanning", String::new()),
            AdapterState::Unavailable(m) => ("unavailable", m),
        };
        let mut d = VarDictionary::new();
        d.set("state", state);
        d.set("message", message.as_str());
        d
    }

    #[func]
    fn retry(&mut self) {
        self.hub().retry();
    }

    #[func]
    fn scan(&mut self, seconds: f64) {
        self.hub().scan(seconds);
    }

    /// Appareils trouvés : `id`, `name`, `kinds`, `rssi` (−999 inconnu), `state`,
    /// `message`, `role` (rôle de la connexion, ou vide), `battery` (−1 inconnue).
    #[func]
    fn found(&mut self) -> VarArray {
        let mut out = VarArray::new();
        for f in self.hub().devices() {
            let mut d = VarDictionary::new();
            d.set("id", f.id.as_str());
            d.set("name", f.name.as_str());
            let kinds: PackedStringArray = f.kinds.iter().map(|k| GString::from(k.key())).collect();
            d.set("kinds", &kinds);
            d.set("rssi", f.rssi.map_or(-999, |r| r as i64));
            let (state, message) = match f.state {
                LinkState::Disconnected => ("disconnected", String::new()),
                LinkState::Connecting => ("connecting", String::new()),
                LinkState::Connected => ("connected", String::new()),
                LinkState::Failed(m) => ("failed", m),
            };
            d.set("state", state);
            d.set("message", message.as_str());
            d.set("role", f.role.map_or("", |k| k.key()));
            d.set("battery", f.battery.map_or(-1, |b| b as i64));
            out.push(&d.to_variant());
        }
        out
    }

    #[func]
    fn connect_device(&mut self, id: GString, kind_key: GString) {
        if let Some(k) = kind(&kind_key) {
            self.hub().connect(&id.to_string(), k);
        }
    }

    #[func]
    fn disconnect_kind(&mut self, kind_key: GString) {
        if let Some(k) = kind(&kind_key) {
            self.hub().disconnect(k);
        }
    }

    /// Reconnexion aux appareils appairés (`{id, name, kind}`), en tâche de fond.
    #[func]
    fn connect_remembered(&mut self, list: VarArray) {
        let devices: Vec<RememberedDevice> = list
            .iter_shared()
            .filter_map(|v| v.try_to::<VarDictionary>().ok())
            .filter_map(|d| {
                let get = |k: &str| d.get(k).map(|v| v.to_string()).unwrap_or_default();
                Some(RememberedDevice {
                    id: get("id"),
                    name: get("name"),
                    kind: DeviceKind::from_key(&get("kind"))?,
                })
            })
            .collect();
        if !devices.is_empty() {
            self.hub().connect_remembered(&devices);
        }
    }

    /// Dernières valeurs reçues (−1 si inconnues) et dernier Click.
    #[func]
    fn live(&mut self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let v = self.hub().live();
        d.set("power_w", v.power_w.unwrap_or(-1.0));
        d.set("cadence_rpm", v.cadence_rpm.unwrap_or(-1.0));
        d.set("speed_kmh", v.speed_kmh.unwrap_or(-1.0));
        d.set("heart_rate", v.heart_rate.map_or(-1.0, |h| h as f64));
        let (click, age) = match v.last_click {
            Some((s, at)) => (
                if s == Shift::Up { "up" } else { "down" },
                Instant::now().duration_since(at).as_secs_f64(),
            ),
            None => ("", -1.0),
        };
        d.set("click", click);
        d.set("click_age_s", age);
        d.set("trainer_controlled", v.trainer_controlled);
        d
    }

    /// Un home trainer est-il connecté ? (sans ouvrir le Bluetooth s'il ne l'est pas encore)
    #[func]
    fn has_trainer(&self) -> bool {
        self.hub.as_ref().is_some_and(|h| {
            h.devices().iter().any(|f| {
                f.role == Some(DeviceKind::Trainer) && matches!(f.state, LinkState::Connected)
            })
        })
    }
}
