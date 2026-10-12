//! Cadence d'un capteur CSC à partir des compteurs de tours de manivelle.
//!
//! Le capteur ne donne que des compteurs cumulés : la cadence se calcule
//! entre deux mesures successives avec [`protocol::csc_cadence`]. Quand le
//! cycliste s'arrête, le compteur ne bouge plus (ou le capteur se tait) :
//! au-delà de [`STOP_AFTER`] sans nouveau tour, la cadence retombe à 0.

use std::time::{Duration, Instant};

use velo_core::protocol::{self, CrankSample};

/// Délai sans nouveau tour de manivelle au-delà duquel la cadence vaut 0.
pub const STOP_AFTER: Duration = Duration::from_millis(2500);

/// Cadence au-delà de laquelle une mesure est jugée aberrante.
pub const MAX_RPM: f64 = 250.0;

/// Machine à états de la cadence d'un capteur CSC.
#[derive(Debug, Clone, Default)]
pub struct CrankCadence {
    prev: Option<CrankSample>,
    /// Moment du dernier nouveau tour (ou de la première mesure).
    since: Option<Instant>,
    rpm: Option<f64>,
}

impl CrankCadence {
    pub fn new() -> Self {
        Self::default()
    }

    /// Dernière cadence publiée.
    pub fn rpm(&self) -> Option<f64> {
        self.rpm
    }

    /// Nouvelle mesure reçue à `now` ; renvoie la cadence à publier, s'il y
    /// en a une nouvelle.
    pub fn on_sample(&mut self, sample: CrankSample, now: Instant) -> Option<f64> {
        let Some(prev) = self.prev.replace(sample) else {
            // Première mesure : simple point de départ.
            self.since = Some(now);
            return None;
        };
        if sample.revolutions == prev.revolutions {
            return self.tick(now);
        }
        let gap = self.since.map_or(Duration::MAX, |s| now.duration_since(s));
        self.since = Some(now);
        if gap > STOP_AFTER {
            // Premier tour après un arrêt : l'intervalle depuis le tour
            // précédent n'a pas de sens (et le compteur de temps a pu faire
            // plusieurs fois le tour). On attend le tour suivant.
            return None;
        }
        let rpm = protocol::csc_cadence(prev, sample)?;
        if rpm > MAX_RPM {
            return None;
        }
        self.rpm = Some(rpm);
        Some(rpm)
    }

    /// À appeler régulièrement : publie 0 une fois quand le pédalage s'arrête.
    pub fn tick(&mut self, now: Instant) -> Option<f64> {
        let since = self.since?;
        if now.duration_since(since) > STOP_AFTER && self.rpm != Some(0.0) {
            self.rpm = Some(0.0);
            return Some(0.0);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(revolutions: u16, event_time: u16) -> CrankSample {
        CrankSample {
            revolutions,
            event_time,
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn steady_pedaling() {
        let t0 = Instant::now();
        let mut c = CrankCadence::new();
        assert_eq!(c.on_sample(sample(10, 1000), t0), None);
        // Un tour en 683/1024 s ≈ 90 rpm.
        let rpm = c.on_sample(sample(11, 1683), t0 + ms(700)).unwrap();
        assert!((rpm - 90.0).abs() < 0.2, "{rpm}");
        // Mesure répétée sans nouveau tour : rien de neuf.
        assert_eq!(c.on_sample(sample(11, 1683), t0 + ms(1000)), None);
        let rpm = c.on_sample(sample(13, 1683 + 1024), t0 + ms(1700)).unwrap();
        assert!((rpm - 120.0).abs() < 0.1, "{rpm}");
        assert_eq!(c.rpm(), Some(rpm));
    }

    #[test]
    fn stops_after_silence_or_frozen_counter() {
        let t0 = Instant::now();
        let mut c = CrankCadence::new();
        c.on_sample(sample(10, 1000), t0);
        c.on_sample(sample(11, 1683), t0 + ms(700));
        // Compteur figé mais mesures toujours envoyées.
        assert_eq!(c.on_sample(sample(11, 1683), t0 + ms(2000)), None);
        assert_eq!(c.on_sample(sample(11, 1683), t0 + ms(3300)), Some(0.0));
        // Le zéro n'est publié qu'une fois.
        assert_eq!(c.tick(t0 + ms(4000)), None);

        // Capteur muet : c'est le tic régulier qui publie le zéro.
        let mut c = CrankCadence::new();
        c.on_sample(sample(10, 1000), t0);
        c.on_sample(sample(11, 1683), t0 + ms(700));
        assert_eq!(c.tick(t0 + ms(3000)), None);
        assert_eq!(c.tick(t0 + ms(3300)), Some(0.0));
        assert_eq!(c.rpm(), Some(0.0));
    }

    #[test]
    fn zero_at_rest_from_the_start() {
        let t0 = Instant::now();
        let mut c = CrankCadence::new();
        c.on_sample(sample(500, 9000), t0);
        assert_eq!(c.tick(t0 + ms(2600)), Some(0.0));
    }

    #[test]
    fn restart_ignores_the_long_interval() {
        let t0 = Instant::now();
        let mut c = CrankCadence::new();
        c.on_sample(sample(10, 1000), t0);
        assert_eq!(c.tick(t0 + ms(3000)), Some(0.0));
        // Reprise 40 s plus tard : le premier tour sert de référence…
        let t1 = t0 + Duration::from_secs(40);
        assert_eq!(
            c.on_sample(sample(11, 1000u16.wrapping_add(40 * 1024)), t1),
            None
        );
        // … et le suivant donne la vraie cadence.
        let rpm = c
            .on_sample(
                sample(12, 1000u16.wrapping_add(40 * 1024 + 683)),
                t1 + ms(700),
            )
            .unwrap();
        assert!((rpm - 90.0).abs() < 0.2, "{rpm}");
    }

    #[test]
    fn aberrant_value_is_ignored() {
        let t0 = Instant::now();
        let mut c = CrankCadence::new();
        c.on_sample(sample(10, 1000), t0);
        // 5 tours en 0,1 s : capteur qui s'emballe.
        assert_eq!(c.on_sample(sample(15, 1102), t0 + ms(500)), None);
        assert_eq!(c.rpm(), None);
    }
}
