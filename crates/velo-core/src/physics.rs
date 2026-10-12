//! Modèle de forces du cycliste et pilotage du home trainer.

pub const G: f64 = 9.80665;

/// Paramètres physiques d'une sortie, issus du profil.
#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub mass_kg: f64,
    /// Masse équivalente des parties tournantes (roues), ajoutée à l'inertie.
    pub rotating_mass_kg: f64,
    pub cda_m2: f64,
    pub crr: f64,
    pub drivetrain_efficiency: f64,
    pub air_density: f64,
}

impl Params {
    pub fn from_profile(p: &crate::config::Profile) -> Self {
        Self {
            mass_kg: p.total_mass_kg(),
            rotating_mass_kg: 1.0,
            cda_m2: p.bike.cda_m2,
            crr: p.bike.crr,
            drivetrain_efficiency: p.bike.drivetrain_efficiency,
            air_density: 1.2,
        }
    }

    /// Densité de l'air selon l'altitude (atmosphère standard simplifiée).
    pub fn with_altitude(mut self, altitude_m: f64) -> Self {
        self.air_density = 1.225 * (-altitude_m / 8500.0).exp();
        self
    }
}

/// Force qui s'oppose à l'avancement (N), pente exprimée en fraction (0,05 = 5 %).
pub fn resistive_force(p: &Params, speed_ms: f64, grade: f64, wind_ms: f64) -> f64 {
    let theta = grade.atan();
    let rolling = p.mass_kg * G * p.crr * theta.cos();
    let gravity = p.mass_kg * G * theta.sin();
    let rel = speed_ms + wind_ms;
    let aero = 0.5 * p.air_density * p.cda_m2 * rel * rel.abs();
    rolling + gravity + aero
}

/// Avance la vitesse d'un pas `dt` à partir de la puissance mesurée au pédalier.
pub fn step_speed(p: &Params, speed_ms: f64, power_w: f64, grade: f64, dt: f64) -> f64 {
    // En dessous de 0,5 m/s, P/v diverge : on borne la force de propulsion.
    let v = speed_ms.max(0.5);
    let propulsive = power_w.max(0.0) * p.drivetrain_efficiency / v;
    let accel =
        (propulsive - resistive_force(p, speed_ms, grade, 0.0)) / (p.mass_kg + p.rotating_mass_kg);
    (speed_ms + accel * dt).max(0.0)
}

/// Vitesse stabilisée pour une puissance et une pente données (pour les tests et l'affichage).
pub fn steady_speed(p: &Params, power_w: f64, grade: f64) -> f64 {
    let wheel = power_w * p.drivetrain_efficiency;
    let (mut lo, mut hi) = (0.0, 40.0);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if resistive_force(p, mid, grade, 0.0) * mid < wheel {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Pente à envoyer au home trainer pour que le couple ressenti au pédalier soit
/// celui du rapport virtuel `virtual_ratio` alors que la chaîne est sur
/// `physical_ratio`.
///
/// Le trainer en mode simulation applique `F(v_p, pente)` à son volant, où
/// `v_p = ω · r_p · circonférence`. Sur la route, le cycliste subirait
/// `F(v_v, θ)` avec `v_v = ω · r_v · circonférence`. Le couple au pédalier
/// vaut `F · R · r` (r = plateau / pignon), d'où `F_cible = F(v_v, θ) · r_v / r_p`.
pub fn effective_grade(
    p: &Params,
    cadence_rpm: f64,
    wheel_circumference_m: f64,
    physical_ratio: f64,
    virtual_ratio: f64,
    road_grade: f64,
) -> f64 {
    let rev_s = cadence_rpm / 60.0;
    let v_virtual = rev_s * virtual_ratio * wheel_circumference_m;
    let v_physical = rev_s * physical_ratio * wheel_circumference_m;
    let target = resistive_force(p, v_virtual, road_grade, 0.0) * virtual_ratio / physical_ratio;
    let aero_physical = 0.5 * p.air_density * p.cda_m2 * v_physical * v_physical;
    // Petites pentes : sin θ ≈ tan θ, et cos θ ≈ 1 pour le roulement.
    (target - aero_physical) / (p.mass_kg * G) - p.crr
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Profile;

    fn params() -> Params {
        Params::from_profile(&Profile::martin())
    }

    #[test]
    fn flat_200w_is_about_33_kmh() {
        let v = steady_speed(&params(), 200.0, 0.0) * 3.6;
        assert!((31.0..36.0).contains(&v), "{v} km/h");
    }

    #[test]
    fn climb_8pct_200w_is_about_12_kmh() {
        // 70 kg à 200 W sur 8 % : environ 12 km/h.
        let v = steady_speed(&params(), 200.0, 0.08) * 3.6;
        assert!((10.5..13.0).contains(&v), "{v} km/h");
    }

    #[test]
    fn integration_converges_to_steady_speed() {
        let p = params();
        let mut v = 0.0;
        for _ in 0..(120 * 20) {
            v = step_speed(&p, v, 250.0, 0.03, 0.05);
        }
        assert!((v - steady_speed(&p, 250.0, 0.03)).abs() < 0.05);
    }

    #[test]
    fn same_gear_means_road_grade() {
        let p = params();
        let r = 52.0 / 19.0;
        let g = effective_grade(&p, 90.0, 2.136, r, r, 0.06);
        assert!((g - 0.06).abs() < 1e-3, "{g}");
    }

    #[test]
    fn harder_virtual_gear_feels_harder() {
        let p = params();
        let phys = 52.0 / 19.0;
        let soft = effective_grade(&p, 85.0, 2.136, phys, 36.0 / 25.0, 0.05);
        let hard = effective_grade(&p, 85.0, 2.136, phys, 52.0 / 15.0, 0.05);
        assert!(hard > soft, "soft {soft} hard {hard}");
    }
}
