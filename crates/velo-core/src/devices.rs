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
