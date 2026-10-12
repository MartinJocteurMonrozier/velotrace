//! Interface commune aux appareils, réels (Bluetooth) ou simulés (mode test).
//!
//! L'application ne voit que des [`DeviceEvent`] et envoie des
//! [`ControlCommand`] : elle ne sait pas si un vrai D500 est branché.

use crate::protocol::{ControlCommand, IndoorBikeData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shift {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceEvent {
    /// Données du home trainer (FTMS).
    Trainer(IndoorBikeData),
    /// Cadence du capteur dédié (CSC), prioritaire sur celle du trainer.
    Cadence(f64),
    HeartRate(u16),
    /// Appui sur un bouton des Zwift Click.
    Shift(Shift),
}

pub trait DeviceHub {
    /// Envoie une commande au home trainer.
    fn send(&mut self, command: ControlCommand);
    /// Avance de `dt` secondes et renvoie les événements reçus entre-temps.
    fn poll(&mut self, dt: f64) -> Vec<DeviceEvent>;
}

/// Rôle d'un appareil Bluetooth pour velotrace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    /// Home trainer FTMS (puissance, cadence, pente ou ERG).
    Trainer,
    /// Capteur de cadence CSC.
    Cadence,
    /// Ceinture ou montre en diffusion de FC (HRS).
    HeartRate,
    /// Zwift Click (changement de vitesse virtuel).
    Click,
}

impl DeviceKind {
    pub const ALL: [DeviceKind; 4] = [
        DeviceKind::Trainer,
        DeviceKind::Cadence,
        DeviceKind::HeartRate,
        DeviceKind::Click,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DeviceKind::Trainer => "Home trainer",
            DeviceKind::Cadence => "Capteur de cadence",
            DeviceKind::HeartRate => "Fréquence cardiaque",
            DeviceKind::Click => "Zwift Click",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            DeviceKind::Trainer => "trainer",
            DeviceKind::Cadence => "cadence",
            DeviceKind::HeartRate => "heart_rate",
            DeviceKind::Click => "click",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }
}

/// Appareil appairé, reconnecté automatiquement au lancement.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RememberedDevice {
    /// Identifiant Bluetooth (adresse sous Linux, identifiant WinRT sous Windows).
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
}
