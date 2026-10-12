//! Coureurs virtuels : ils roulent sur le même parcours que le cycliste, à
//! une puissance fixée en W/kg, avec un peu de variabilité, plus d'effort
//! dans les montées, roue libre dans les descentes et un effet d'aspiration
//! simple.

use crate::config::Profile;
use crate::physics::{self, Params};
use crate::route::Route;

/// Réglage d'un coureur virtuel.
#[derive(Debug, Clone, PartialEq)]
pub struct OpponentSpec {
    pub name: String,
    pub w_per_kg: f64,
    pub rider_kg: f64,
}

/// État d'un coureur virtuel.
#[derive(Debug, Clone, PartialEq)]
pub struct Opponent {
    pub name: String,
    pub w_per_kg: f64,
    pub rider_kg: f64,
    pub distance_m: f64,
    pub speed_ms: f64,
    pub power_w: f64,
    pub cadence_rpm: f64,
    pub finished: bool,
    /// Temps de sortie à l'arrivée (s).
    pub finish_time_s: Option<f64>,
}

/// Variabilité propre à chaque coureur : deux sinusoïdes lentes.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rhythm {
    periods_s: [f64; 3],
    phases: [f64; 3],
}

impl Rhythm {
    fn from_name(name: &str) -> Self {
        // FNV-1a puis xorshift : graine reproductible tirée du prénom.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in name.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        let mut next = || {
            h ^= h << 13;
            h ^= h >> 7;
            h ^= h << 17;
            (h >> 11) as f64 / (1u64 << 53) as f64
        };
        Self {
            periods_s: [
                40.0 + 40.0 * next(),
                150.0 + 120.0 * next(),
                20.0 + 15.0 * next(),
            ],
            phases: [
                std::f64::consts::TAU * next(),
                std::f64::consts::TAU * next(),
                std::f64::consts::TAU * next(),
            ],
        }
    }

    fn wave(&self, i: usize, t: f64) -> f64 {
        (std::f64::consts::TAU * t / self.periods_s[i] + self.phases[i]).sin()
    }

    /// Facteur de puissance dans [0,96 ; 1,04].
    fn power_factor(&self, t: f64) -> f64 {
        1.0 + 0.04 * (0.6 * self.wave(0, t) + 0.4 * self.wave(1, t))
    }

    /// Cadence entre 85 et 95 rpm.
    fn cadence(&self, t: f64) -> f64 {
        90.0 + 5.0 * self.wave(2, t)
    }
}

/// Effort selon la pente : jusqu'à +8 % au-delà de 4 % de montée, de moins
/// en moins en descente jusqu'à la roue libre sous −4 %.
fn grade_factor(grade: f64) -> f64 {
    if grade >= 0.0 {
        1.0 + 0.08 * (grade / 0.04).min(1.0)
    } else {
        (1.0 + grade / 0.04).max(0.0)
    }
}

/// Réduction de CdA derrière un coureur situé `gap_m` mètres devant :
/// 30 % dans la roue (jusqu'à 2 m), 10 % à 6 m, rien au-delà.
fn draft_reduction(gap_m: f64) -> f64 {
    if !(0.5..=6.0).contains(&gap_m) {
        return 0.0;
    }
    0.30 - 0.20 * ((gap_m - 2.0).max(0.0) / 4.0)
}

/// Groupe de coureurs virtuels.
#[derive(Debug, Clone)]
pub struct Pack {
    pub opponents: Vec<Opponent>,
    /// Paramètres physiques communs (vélo, air) ; la masse varie par coureur.
    params: Params,
    bike_kg: f64,
    rhythms: Vec<Rhythm>,
}

impl Pack {
    /// Les coureurs partent arrêtés sur la ligne, avec le vélo, le CdA et le
    /// Crr du profil.
    pub fn new(specs: Vec<OpponentSpec>, profile: &Profile, start_altitude: f64) -> Self {
        let rhythms = specs.iter().map(|s| Rhythm::from_name(&s.name)).collect();
        let opponents = specs
            .into_iter()
            .map(|s| Opponent {
                name: s.name,
                w_per_kg: s.w_per_kg,
                rider_kg: s.rider_kg,
                distance_m: 0.0,
                speed_ms: 0.0,
                power_w: 0.0,
                cadence_rpm: 0.0,
                finished: false,
                finish_time_s: None,
            })
            .collect();
        Self {
            opponents,
            params: Params::from_profile(profile).with_altitude(start_altitude),
            bike_kg: profile.bike.weight_kg,
            rhythms,
        }
    }

    /// Avance de `dt` secondes. `elapsed_s` est le temps de sortie à la fin
    /// du pas ; le cycliste, à `player_distance_m`, abrite aussi les coureurs.
    pub fn step(&mut self, dt: f64, route: &Route, elapsed_s: f64, player_distance_m: f64) {
        let length = route.length_m();
        // Positions au début du pas, pour l'aspiration.
        let positions: Vec<f64> = self.opponents.iter().map(|o| o.distance_m).collect();
        for (i, o) in self.opponents.iter_mut().enumerate() {
            if o.finished {
                continue;
            }
            let rhythm = &self.rhythms[i];
            let grade = route.grade_at(o.distance_m);
            o.power_w =
                (o.w_per_kg * o.rider_kg * grade_factor(grade) * rhythm.power_factor(elapsed_s))
                    .max(0.0);
            o.cadence_rpm = if o.power_w > 5.0 {
                rhythm.cadence(elapsed_s)
            } else {
                0.0
            };

            let draft = positions
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, d)| *d)
                .chain(std::iter::once(player_distance_m))
                .map(|d| draft_reduction(d - o.distance_m))
                .fold(0.0, f64::max);
            let params = Params {
                mass_kg: o.rider_kg + self.bike_kg,
                cda_m2: self.params.cda_m2 * (1.0 - draft),
                ..self.params
            };
            o.speed_ms = physics::step_speed(&params, o.speed_ms, o.power_w, grade, dt);
            o.distance_m += o.speed_ms * dt;
            if o.distance_m >= length {
                // Heure d'arrivée interpolée dans le pas.
                let over = if o.speed_ms > 0.0 {
                    (o.distance_m - length) / o.speed_ms
                } else {
                    0.0
                };
                o.finish_time_s = Some(elapsed_s - over.min(dt));
                o.distance_m = length;
                o.finished = true;
                o.speed_ms = 0.0;
                o.power_w = 0.0;
                o.cadence_rpm = 0.0;
            }
        }
    }

    /// Règle le niveau d'un coureur (menu de pause).
    pub fn set_w_per_kg(&mut self, index: usize, w_per_kg: f64) {
        if let Some(o) = self.opponents.get_mut(index) {
            o.w_per_kg = w_per_kg.max(0.0);
        }
    }

    /// Donne le même niveau à tous les coureurs.
    pub fn set_all_w_per_kg(&mut self, w_per_kg: f64) {
        for o in &mut self.opponents {
            o.w_per_kg = w_per_kg.max(0.0);
        }
    }

    /// Classement des coureurs, du premier au dernier, avec leur écart au
    /// cycliste (m, positif devant lui).
    pub fn standings(&self, player_distance_m: f64) -> Vec<(String, f64)> {
        let mut out: Vec<(String, f64)> = self
            .opponents
            .iter()
            .map(|o| (o.name.clone(), o.distance_m - player_distance_m))
            .collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1));
        out
    }
}

const NAMES: &[&str] = &[
    "Julien", "Camille", "Thomas", "Léa", "Hugo", "Manon", "Nicolas", "Chloé", "Antoine", "Inès",
    "Maxime", "Sarah", "Romain", "Clara", "Bastien", "Élise",
];

const WEIGHTS_KG: &[f64] = &[68.0, 58.0, 74.0, 55.0, 79.0, 61.0, 71.0, 57.0, 65.0, 63.0];

/// Groupe de `count` coureurs autour de `w_per_kg` (écart de ±0,3 W/kg),
/// avec des prénoms et des poids variés.
pub fn default_pack(w_per_kg: f64, count: usize) -> Vec<OpponentSpec> {
    // Niveaux régulièrement répartis, mélangés pour ne pas suivre les prénoms.
    let spread = |i: usize| {
        if count <= 1 {
            0.0
        } else {
            // Pas premier avec `count` : chaque niveau sert une fois.
            let p = [7, 5, 3].into_iter().find(|p| count % p != 0).unwrap_or(1);
            let k = (i * p + 3) % count;
            -0.3 + 0.6 * k as f64 / (count - 1) as f64
        }
    };
    (0..count)
        .map(|i| OpponentSpec {
            name: match NAMES.get(i) {
                Some(n) => n.to_string(),
                None => format!("Coureur {}", i + 1),
            },
            w_per_kg: ((w_per_kg + spread(i)) * 100.0).round() / 100.0,
            rider_kg: WEIGHTS_KG[i % WEIGHTS_KG.len()],
        })
        .map(|mut s| {
            s.w_per_kg = s.w_per_kg.max(0.5);
            s
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(len_m: f64, grade: f64) -> Route {
        let deg = (len_m / 6_371_008.8f64).to_degrees();
        let pts = [(45.0, 6.0, 100.0), (45.0 + deg, 6.0, 100.0 + len_m * grade)];
        Route::from_lat_lon_ele(None, &pts, 5.0, 40.0).unwrap()
    }

    fn spec(name: &str, w_per_kg: f64) -> OpponentSpec {
        OpponentSpec {
            name: name.into(),
            w_per_kg,
            rider_kg: 70.0,
        }
    }

    #[test]
    fn three_w_per_kg_climbs_seven_percent_plausibly() {
        let r = route(10_000.0, 0.07);
        let mut pack = Pack::new(vec![spec("Julien", 3.0)], &Profile::martin(), 100.0);
        let dt = 0.1;
        let mut t = 0.0;
        // Le joueur reste loin derrière : pas d'aspiration.
        for _ in 0..3000 {
            t += dt;
            pack.step(dt, &r, t, -1000.0);
        }
        let o = &pack.opponents[0];
        let kmh = o.speed_ms * 3.6;
        // 70 + 11 kg à ~225 W sur 7 % : 12 à 14 km/h.
        assert!((11.0..15.5).contains(&kmh), "{kmh} km/h");
        assert!((85.0..=95.0).contains(&o.cadence_rpm));
        assert!((210.0 * 0.95..=210.0 * 1.08 * 1.05).contains(&o.power_w));
    }

    #[test]
    fn stronger_rider_finishes_first() {
        let deg = (6000.0 / 6_371_008.8f64).to_degrees();
        let pts = [
            (45.0, 6.0, 100.0),
            (45.0 + deg / 3.0, 6.0, 250.0),
            (45.0 + deg * 2.0 / 3.0, 6.0, 150.0),
            (45.0 + deg, 6.0, 160.0),
        ];
        let r = Route::from_lat_lon_ele(None, &pts, 5.0, 40.0).unwrap();
        let mut pack = Pack::new(
            vec![spec("Camille", 2.5), spec("Thomas", 3.5)],
            &Profile::martin(),
            100.0,
        );
        let dt = 0.1;
        let mut t = 0.0;
        while pack.opponents.iter().any(|o| !o.finished) && t < 7200.0 {
            t += dt;
            pack.step(dt, &r, t, -1000.0);
        }
        let slow = pack.opponents[0].finish_time_s.unwrap();
        let fast = pack.opponents[1].finish_time_s.unwrap();
        assert!(fast < slow, "{fast} vs {slow}");
        let st = pack.standings(0.0);
        assert!(st.iter().all(|(_, gap)| (*gap - r.length_m()).abs() < 1e-6));
    }

    #[test]
    fn drafting_makes_the_follower_faster() {
        let r = route(5000.0, 0.0);
        let p = Profile::martin();
        let start = |pack: &mut Pack, d: f64, i: usize| {
            pack.opponents[i].distance_m = d;
            pack.opponents[i].speed_ms = 10.0;
        };
        // Même coureur seul, puis dans la roue d'un autre, puis dans celle du joueur.
        let mut alone = Pack::new(vec![spec("Hugo", 3.0)], &p, 100.0);
        start(&mut alone, 100.0, 0);
        let mut behind = Pack::new(vec![spec("Léa", 3.0), spec("Hugo", 3.0)], &p, 100.0);
        start(&mut behind, 102.0, 0);
        start(&mut behind, 100.0, 1);
        let mut behind_player = Pack::new(vec![spec("Hugo", 3.0)], &p, 100.0);
        start(&mut behind_player, 100.0, 0);
        let dt = 0.05;
        for k in 1..=40 {
            let t = k as f64 * dt;
            alone.step(dt, &r, t, -1000.0);
            behind.step(dt, &r, t, -1000.0);
            let leader = behind_player.opponents[0].distance_m + 2.0;
            behind_player.step(dt, &r, t, leader);
        }
        let v_alone = alone.opponents[0].speed_ms;
        let v_draft = behind.opponents[1].speed_ms;
        let v_player = behind_player.opponents[0].speed_ms;
        assert!(v_draft > v_alone + 0.05, "{v_draft} vs {v_alone}");
        assert!(v_player > v_alone + 0.05, "{v_player} vs {v_alone}");
        // À puissance égale.
        assert!((behind.opponents[1].power_w - alone.opponents[0].power_w).abs() < 1e-9);
    }

    #[test]
    fn default_pack_spreads_levels_and_settings_apply() {
        let specs = default_pack(3.0, 5);
        assert_eq!(specs.len(), 5);
        assert!(specs.iter().all(|s| (2.7..=3.3).contains(&s.w_per_kg)));
        let mut levels: Vec<f64> = specs.iter().map(|s| s.w_per_kg).collect();
        levels.sort_by(f64::total_cmp);
        assert_eq!(levels, [2.7, 2.85, 3.0, 3.15, 3.3]);
        let names: std::collections::HashSet<_> = specs.iter().map(|s| &s.name).collect();
        assert_eq!(names.len(), 5);
        assert_eq!(default_pack(2.0, 20)[19].name, "Coureur 20");
        for n in [2, 7, 10, 14] {
            let levels: std::collections::HashSet<i64> = default_pack(3.0, n)
                .iter()
                .map(|s| (s.w_per_kg * 100.0).round() as i64)
                .collect();
            assert_eq!(levels.len(), n, "{n} coureurs");
        }
        assert_eq!(default_pack(2.0, 1)[0].w_per_kg, 2.0);

        let mut pack = Pack::new(specs, &Profile::martin(), 0.0);
        pack.set_w_per_kg(1, 4.0);
        assert_eq!(pack.opponents[1].w_per_kg, 4.0);
        pack.set_all_w_per_kg(2.5);
        assert!(pack.opponents.iter().all(|o| o.w_per_kg == 2.5));
    }

    #[test]
    fn descent_means_freewheeling() {
        assert_eq!(grade_factor(-0.05), 0.0);
        assert!((grade_factor(0.10) - 1.08).abs() < 1e-12);
        assert!((grade_factor(0.02) - 1.04).abs() < 1e-12);
        assert_eq!(grade_factor(0.0), 1.0);
    }
}
