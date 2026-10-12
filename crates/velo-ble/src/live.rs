//! Dernières valeurs reçues de chaque appareil, pour l'écran des connexions.

use std::collections::HashMap;
use std::time::Instant;

use velo_core::devices::{DeviceKind, Shift};
use velo_core::protocol::IndoorBikeData;

use crate::LiveValues;

/// Valeurs brutes par source ; [`LiveState::values`] choisit la meilleure.
#[derive(Debug, Clone, Default)]
pub struct LiveState {
    trainer: Option<IndoorBikeData>,
    csc_rpm: Option<f64>,
    hrs_bpm: Option<u16>,
    last_click: Option<(Shift, Instant)>,
    trainer_controlled: bool,
    last_update: HashMap<DeviceKind, Instant>,
}

impl LiveState {
    pub fn trainer(&mut self, data: IndoorBikeData, now: Instant) {
        self.trainer = Some(data);
        self.last_update.insert(DeviceKind::Trainer, now);
    }

    pub fn cadence(&mut self, rpm: f64, now: Instant) {
        self.csc_rpm = Some(rpm);
        self.last_update.insert(DeviceKind::Cadence, now);
    }

    pub fn heart_rate(&mut self, bpm: u16, now: Instant) {
        self.hrs_bpm = Some(bpm);
        self.last_update.insert(DeviceKind::HeartRate, now);
    }

    pub fn click(&mut self, shift: Shift, now: Instant) {
        self.last_click = Some((shift, now));
        self.last_update.insert(DeviceKind::Click, now);
    }

    /// Le Click a parlé sans changer de vitesse (poignée de main, bouton).
    pub fn click_alive(&mut self, now: Instant) {
        self.last_update.insert(DeviceKind::Click, now);
    }

    pub fn set_trainer_controlled(&mut self, controlled: bool) {
        self.trainer_controlled = controlled;
    }

    /// Oublie les valeurs d'un appareil déconnecté.
    pub fn clear(&mut self, kind: DeviceKind) {
        match kind {
            DeviceKind::Trainer => {
                self.trainer = None;
                self.trainer_controlled = false;
            }
            DeviceKind::Cadence => self.csc_rpm = None,
            DeviceKind::HeartRate => self.hrs_bpm = None,
            DeviceKind::Click => {}
        }
        self.last_update.remove(&kind);
    }

    /// Valeurs affichées : la cadence du capteur CSC et la FC de la ceinture
    /// priment sur celles que le trainer relaie.
    pub fn values(&self) -> LiveValues {
        let t = self.trainer.as_ref();
        LiveValues {
            power_w: t.and_then(|t| t.power_w).map(f64::from),
            cadence_rpm: self.csc_rpm.or(t.and_then(|t| t.cadence_rpm)),
            speed_kmh: t.and_then(|t| t.speed_kmh),
            heart_rate: self
                .hrs_bpm
                .or(t.and_then(|t| t.heart_rate_bpm).map(u16::from)),
            last_click: self.last_click,
            trainer_controlled: self.trainer_controlled,
            last_update: self.last_update.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedicated_sensors_take_precedence() {
        let now = Instant::now();
        let mut s = LiveState::default();
        assert_eq!(s.values(), LiveValues::default());
        s.trainer(
            IndoorBikeData {
                speed_kmh: Some(31.0),
                cadence_rpm: Some(84.0),
                power_w: Some(210),
                heart_rate_bpm: Some(140),
                ..Default::default()
            },
            now,
        );
        let v = s.values();
        assert_eq!(v.power_w, Some(210.0));
        assert_eq!(v.cadence_rpm, Some(84.0));
        assert_eq!(v.heart_rate, Some(140));
        assert_eq!(v.last_update.get(&DeviceKind::Trainer), Some(&now));

        s.cadence(88.5, now);
        s.heart_rate(152, now);
        let v = s.values();
        assert_eq!(v.cadence_rpm, Some(88.5));
        assert_eq!(v.heart_rate, Some(152));

        // Capteur de cadence perdu : retour à la cadence du trainer.
        s.clear(DeviceKind::Cadence);
        assert_eq!(s.values().cadence_rpm, Some(84.0));
        assert!(!s.values().last_update.contains_key(&DeviceKind::Cadence));
    }

    #[test]
    fn trainer_loss_resets_control() {
        let now = Instant::now();
        let mut s = LiveState::default();
        s.trainer(IndoorBikeData::default(), now);
        s.set_trainer_controlled(true);
        s.click(Shift::Up, now);
        assert!(s.values().trainer_controlled);
        s.clear(DeviceKind::Trainer);
        let v = s.values();
        assert!(!v.trainer_controlled);
        assert_eq!(v.power_w, None);
        assert_eq!(v.last_click, Some((Shift::Up, now)));
    }
}
