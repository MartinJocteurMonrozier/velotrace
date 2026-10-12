//! Enregistrement d'une sortie : un échantillon par seconde, bilan et
//! courbe de puissance (meilleures puissances moyennes par durée).

use crate::ride::Telemetry;
use crate::route::RoutePoint;

/// Mesures d'une seconde de sortie.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Sample {
    /// Temps de sortie (s), pauses exclues.
    pub t_s: f64,
    pub distance_m: f64,
    pub lat: f64,
    pub lon: f64,
    pub elevation_m: f64,
    pub speed_ms: f64,
    pub power_w: f64,
    pub cadence_rpm: f64,
    pub heart_rate: Option<u16>,
    /// Pente (fraction).
    pub grade: f64,
}

/// Enregistre une sortie à 1 Hz à partir de la télémétrie.
#[derive(Debug, Clone)]
pub struct Recorder {
    /// Début de la sortie, en secondes depuis le 1er janvier 1970 (UTC).
    pub start_unix: i64,
    pub samples: Vec<Sample>,
    next_t: f64,
}

impl Recorder {
    pub fn new(start_unix: i64) -> Self {
        Self {
            start_unix,
            samples: Vec::new(),
            next_t: 0.0,
        }
    }

    /// Ajoute un échantillon quand une nouvelle seconde de sortie est écoulée.
    /// Renvoie vrai si un échantillon a été ajouté.
    pub fn push(&mut self, t: &Telemetry, point: &RoutePoint) -> bool {
        if t.elapsed_s + 1e-9 < self.next_t {
            return false;
        }
        self.next_t = t.elapsed_s.floor() + 1.0;
        self.samples.push(Sample {
            t_s: t.elapsed_s.floor(),
            distance_m: t.distance_m,
            lat: point.lat,
            lon: point.lon,
            elevation_m: t.elevation_m,
            speed_ms: t.speed_ms,
            power_w: t.power_w,
            cadence_rpm: t.cadence_rpm,
            heart_rate: t.heart_rate_bpm,
            grade: t.grade,
        });
        true
    }

    pub fn summary(&self, ftp_w: f64) -> Summary {
        summarize(&self.samples, ftp_w)
    }
}

/// Bilan d'une sortie.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Summary {
    pub duration_s: f64,
    pub distance_m: f64,
    pub elevation_gain_m: f64,
    pub avg_speed_ms: f64,
    pub max_speed_ms: f64,
    pub avg_power_w: f64,
    pub max_power_w: f64,
    /// Puissance normalisée (moyenne d'ordre 4 de la puissance lissée sur 30 s).
    pub normalized_power_w: f64,
    /// Facteur d'intensité : NP / FTP.
    pub intensity_factor: f64,
    /// Charge d'entraînement (TSS) : durée (h) × IF² × 100.
    pub tss: f64,
    pub work_kj: f64,
    pub avg_cadence_rpm: f64,
    pub avg_heart_rate: Option<f64>,
    pub max_heart_rate: Option<u16>,
}

pub fn summarize(samples: &[Sample], ftp_w: f64) -> Summary {
    let n = samples.len();
    if n == 0 {
        return Summary::default();
    }
    let duration_s = n as f64;
    let distance_m = samples[n - 1].distance_m - samples[0].distance_m;
    let mut gain = 0.0;
    for w in samples.windows(2) {
        gain += (w[1].elevation_m - w[0].elevation_m).max(0.0);
    }
    let power: Vec<f64> = samples.iter().map(|s| s.power_w.max(0.0)).collect();
    let work_j: f64 = power.iter().sum();
    let avg_power_w = work_j / duration_s;
    let max_power_w = power.iter().cloned().fold(0.0, f64::max);
    let normalized_power_w = normalized_power(&power);
    let intensity_factor = if ftp_w > 0.0 {
        normalized_power_w / ftp_w
    } else {
        0.0
    };
    let pedaling: Vec<f64> = samples
        .iter()
        .map(|s| s.cadence_rpm)
        .filter(|c| *c > 0.0)
        .collect();
    let hr: Vec<u16> = samples.iter().filter_map(|s| s.heart_rate).collect();
    Summary {
        duration_s,
        distance_m,
        elevation_gain_m: gain,
        avg_speed_ms: distance_m / duration_s,
        max_speed_ms: samples.iter().map(|s| s.speed_ms).fold(0.0, f64::max),
        avg_power_w,
        max_power_w,
        normalized_power_w,
        intensity_factor,
        tss: duration_s / 3600.0 * intensity_factor * intensity_factor * 100.0,
        work_kj: work_j / 1000.0,
        avg_cadence_rpm: if pedaling.is_empty() {
            0.0
        } else {
            pedaling.iter().sum::<f64>() / pedaling.len() as f64
        },
        avg_heart_rate: if hr.is_empty() {
            None
        } else {
            Some(hr.iter().map(|h| *h as f64).sum::<f64>() / hr.len() as f64)
        },
        max_heart_rate: hr.iter().max().copied(),
    }
}

/// Puissance normalisée d'une suite de puissances à 1 Hz.
pub fn normalized_power(power: &[f64]) -> f64 {
    if power.len() < 30 {
        return power.iter().sum::<f64>() / power.len().max(1) as f64;
    }
    let mut sum: f64 = power[..30].iter().sum();
    let mut acc = (sum / 30.0).powi(4);
    for i in 30..power.len() {
        sum += power[i] - power[i - 30];
        acc += (sum / 30.0).powi(4);
    }
    (acc / (power.len() - 29) as f64).powf(0.25)
}

/// Durées (s) de la courbe de puissance.
pub const CURVE_DURATIONS: &[u32] = &[
    1, 2, 3, 5, 10, 15, 20, 30, 45, 60, 90, 120, 180, 240, 300, 420, 600, 900, 1200, 1800, 2700,
    3600, 5400, 7200, 10800, 14400,
];

/// Meilleure puissance moyenne pour chaque durée de [`CURVE_DURATIONS`] que
/// la sortie couvre : `(durée en s, watts)`.
pub fn power_curve(samples: &[Sample]) -> Vec<(u32, f64)> {
    let mut prefix = Vec::with_capacity(samples.len() + 1);
    prefix.push(0.0);
    for s in samples {
        prefix.push(prefix.last().unwrap() + s.power_w.max(0.0));
    }
    let n = samples.len();
    CURVE_DURATIONS
        .iter()
        .filter(|d| **d as usize <= n)
        .map(|&d| {
            let d_us = d as usize;
            let best = (d_us..=n)
                .map(|end| prefix[end] - prefix[end - d_us])
                .fold(0.0, f64::max);
            (d, best / d as f64)
        })
        .collect()
}

/// FTP estimée d'après une courbe de puissance : 95 % des 20 min, sinon rien.
pub fn estimated_ftp(curve: &[(u32, f64)]) -> Option<f64> {
    curve
        .iter()
        .find(|(d, _)| *d == 1200)
        .map(|(_, w)| w * 0.95)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steady(watts: &[f64]) -> Vec<Sample> {
        watts
            .iter()
            .enumerate()
            .map(|(i, w)| Sample {
                t_s: i as f64,
                distance_m: i as f64 * 8.0,
                power_w: *w,
                cadence_rpm: 90.0,
                speed_ms: 8.0,
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn steady_hour_at_ftp_is_100_tss() {
        let s = steady(&vec![250.0; 3600]);
        let sum = summarize(&s, 250.0);
        assert!((sum.normalized_power_w - 250.0).abs() < 1e-6);
        assert!((sum.tss - 100.0).abs() < 0.1, "{}", sum.tss);
        assert!((sum.work_kj - 900.0).abs() < 1e-6);
    }

    #[test]
    fn curve_finds_the_best_effort() {
        let mut w = vec![150.0; 600];
        for x in w.iter_mut().skip(200).take(60) {
            *x = 400.0;
        }
        w[300] = 900.0;
        let c = power_curve(&steady(&w));
        let at = |d: u32| c.iter().find(|(x, _)| *x == d).unwrap().1;
        assert_eq!(at(1), 900.0);
        assert!((at(60) - 400.0).abs() < 1e-6);
        assert!(at(300) < at(60));
        assert!(c.iter().all(|(d, _)| *d <= 600));
        // Une courbe de puissance ne remonte jamais.
        assert!(c.windows(2).all(|p| p[1].1 <= p[0].1 + 1e-9));
    }

    #[test]
    fn variable_effort_has_higher_np() {
        let w: Vec<f64> = (0..1200)
            .map(|i| if (i / 60) % 2 == 0 { 350.0 } else { 100.0 })
            .collect();
        let sum = summarize(&steady(&w), 250.0);
        assert!(sum.normalized_power_w > sum.avg_power_w + 20.0);
    }
}
