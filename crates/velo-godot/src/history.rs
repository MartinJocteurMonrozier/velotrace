//! Historique des sorties pour Godot : enregistrement en fin de sortie,
//! liste, détail, courbes de puissance, volume, forme et export .FIT.

use godot::classes::ProjectSettings;
use godot::prelude::*;
use velo_core::config::Profile;
use velo_core::fit;
use velo_core::record::Sample;
use velo_store::{NewRide, RideMode, RideRow, Store};

use crate::session::{Mode, Session};

/// Une sortie de moins de 10 s n'est pas enregistrée.
const MIN_SAMPLES: usize = 10;

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Libre => "sortie libre",
        Mode::Seance => "séance",
        Mode::Adversaires => "course",
        Mode::Test => "mode test",
    }
}

/// Enregistre la sortie terminée ; renvoie son identifiant.
pub fn save_session(s: &Session, profile: &Profile, route_name: &str) -> Result<i64, String> {
    let samples = &s.recorder.samples;
    if samples.len() < MIN_SAMPLES {
        return Err("sortie trop courte pour être enregistrée".into());
    }
    let ftp = profile.ftp();
    let name = match &s.workout {
        Some(w) => format!("{route_name} · {}", w.workout.name),
        None => format!("{route_name} · {}", mode_label(s.mode)),
    };
    let ride = NewRide {
        started_unix: s.recorder.start_unix,
        name,
        route_name: route_name.to_string(),
        mode: RideMode::parse(s.mode.key()),
        ftp_w: ftp,
        rider_kg: s.ride.profile.rider.weight_kg,
        summary: s.recorder.summary(ftp),
        samples: samples.clone(),
    };
    let mut store = Store::open_default().map_err(|e| e.to_string())?;
    store.save_ride(&ride).map_err(|e| e.to_string())
}

/// Fichier .FIT d'une sortie enregistrée.
pub fn fit_bytes(store: &Store, id: i64) -> Result<(RideRow, Vec<u8>), String> {
    let row = store
        .ride(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "sortie introuvable".to_string())?;
    let samples = store.samples(id).map_err(|e| e.to_string())?;
    let bytes = fit::encode_activity(row.started_unix, &samples, &row.summary, &row.name);
    Ok((row, bytes))
}

fn row_dict(r: &RideRow) -> VarDictionary {
    let s = &r.summary;
    let mut d = VarDictionary::new();
    d.set("id", r.id);
    d.set("started_unix", r.started_unix);
    d.set("name", r.name.as_str());
    d.set("route_name", r.route_name.as_str());
    d.set("mode", r.mode.as_str());
    d.set("duration_s", s.duration_s);
    d.set("distance_m", s.distance_m);
    d.set("elevation_gain_m", s.elevation_gain_m);
    d.set("avg_speed_kmh", s.avg_speed_ms * 3.6);
    d.set("max_speed_kmh", s.max_speed_ms * 3.6);
    d.set("avg_power_w", s.avg_power_w);
    d.set("max_power_w", s.max_power_w);
    d.set("np_w", s.normalized_power_w);
    d.set("if", s.intensity_factor);
    d.set("tss", s.tss);
    d.set("work_kj", s.work_kj);
    d.set("avg_cadence", s.avg_cadence_rpm);
    d.set("avg_hr", s.avg_heart_rate.unwrap_or(0.0));
    d.set("max_hr", s.max_heart_rate.map_or(0, |h| h as i64));
    d.set("ftp_w", r.ftp_w);
    d.set("rider_kg", r.rider_kg);
    d.set(
        "strava_activity_id",
        r.strava_activity_id.map_or(0, |a| a as i64),
    );
    d.set("notes", r.notes.as_str());
    d
}

fn curve(points: impl Iterator<Item = (u32, f64)>) -> PackedVector2Array {
    points
        .map(|(d, w)| Vector2::new(d as f32, w as f32))
        .collect()
}

/// Moyenne par tranches de `step` échantillons.
fn series(samples: &[Sample], step: usize, f: impl Fn(&Sample) -> f64) -> PackedFloat32Array {
    samples
        .chunks(step.max(1))
        .map(|c| (c.iter().map(&f).sum::<f64>() / c.len() as f64) as f32)
        .collect()
}

#[derive(GodotClass)]
#[class(base = Node)]
pub struct VeloHistory {
    base: Base<Node>,
    store: Option<Store>,
    last_error: String,
}

#[godot_api]
impl INode for VeloHistory {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            store: None,
            last_error: String::new(),
        }
    }
}

impl VeloHistory {
    fn store(&mut self) -> Option<&mut Store> {
        if self.store.is_none() {
            match Store::open_default() {
                Ok(s) => self.store = Some(s),
                Err(e) => {
                    self.last_error = format!("historique indisponible : {e}");
                    godot_warn!("{}", self.last_error);
                }
            }
        }
        self.store.as_mut()
    }

    fn ok<T: Default, E: std::fmt::Display>(&mut self, r: Result<T, E>) -> T {
        r.unwrap_or_else(|e| {
            self.last_error = e.to_string();
            T::default()
        })
    }
}

#[godot_api]
impl VeloHistory {
    #[func]
    fn error_message(&self) -> GString {
        GString::from(self.last_error.as_str())
    }

    /// Chemin du fichier de la base.
    #[func]
    fn database_path(&self) -> GString {
        let p = Store::default_path().map_or(String::new(), |p| p.to_string_lossy().into_owned());
        GString::from(p.as_str())
    }

    /// Sorties, de la plus récente à la plus ancienne.
    #[func]
    fn rides(&mut self, limit: i64, offset: i64) -> VarArray {
        let mut out = VarArray::new();
        let Some(store) = self.store() else {
            return out;
        };
        let rows = store.list_rides(limit.max(0) as usize, offset.max(0) as usize);
        for r in self.ok(rows) {
            out.push(&row_dict(&r).to_variant());
        }
        out
    }

    #[func]
    fn ride_count(&mut self) -> i64 {
        let Some(store) = self.store() else {
            return 0;
        };
        let n = store.ride_count();
        self.ok(n) as i64
    }

    /// Une sortie (dictionnaire vide si elle n'existe pas).
    #[func]
    fn ride(&mut self, id: i64) -> VarDictionary {
        let Some(store) = self.store() else {
            return VarDictionary::new();
        };
        let r = store.ride(id);
        self.ok(r).map_or_else(VarDictionary::new, |r| row_dict(&r))
    }

    /// Courbes d'une sortie, moyennées par tranches de `step_s` secondes :
    /// `t` (s), `distance_km`, `power`, `elevation`, `heart_rate`, `cadence`, `speed_kmh`.
    #[func]
    fn ride_series(&mut self, id: i64, step_s: i64) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(store) = self.store() else {
            return d;
        };
        let samples = store.samples(id);
        let samples = self.ok(samples);
        let step = step_s.max(1) as usize;
        d.set("t", &series(&samples, step, |s| s.t_s));
        d.set(
            "distance_km",
            &series(&samples, step, |s| s.distance_m / 1000.0),
        );
        d.set("power", &series(&samples, step, |s| s.power_w));
        d.set("elevation", &series(&samples, step, |s| s.elevation_m));
        d.set(
            "heart_rate",
            &series(&samples, step, |s| s.heart_rate.map_or(0.0, |h| h as f64)),
        );
        d.set("cadence", &series(&samples, step, |s| s.cadence_rpm));
        d.set("speed_kmh", &series(&samples, step, |s| s.speed_ms * 3.6));
        d
    }

    /// Courbe de puissance d'une sortie : x = durée (s), y = watts.
    #[func]
    fn ride_curve(&mut self, id: i64) -> PackedVector2Array {
        let Some(store) = self.store() else {
            return PackedVector2Array::new();
        };
        let c = store.ride_curve(id);
        curve(self.ok(c).into_iter())
    }

    /// Meilleure courbe de puissance des `days` derniers jours (0 = depuis toujours).
    #[func]
    fn best_curve(&mut self, days: i64) -> PackedVector2Array {
        let since = (days > 0).then(|| now_unix() - days * 86_400);
        let Some(store) = self.store() else {
            return PackedVector2Array::new();
        };
        let c = store.best_curve(since);
        curve(self.ok(c).into_iter().map(|p| (p.duration_s, p.watts)))
    }

    /// Totaux par semaine (du lundi), de la plus ancienne à la plus récente.
    #[func]
    fn weekly(&mut self, weeks: i64) -> VarArray {
        let mut out = VarArray::new();
        let Some(store) = self.store() else {
            return out;
        };
        let w = store.weekly_totals(weeks.max(1) as usize, now_unix());
        for w in self.ok(w) {
            let mut d = VarDictionary::new();
            d.set("week_start_unix", w.week_start_unix);
            d.set("rides", w.rides as i64);
            d.set("duration_s", w.duration_s);
            d.set("distance_m", w.distance_m);
            d.set("elevation_m", w.elevation_m);
            d.set("tss", w.tss);
            d.set("work_kj", w.work_kj);
            out.push(&d.to_variant());
        }
        out
    }

    /// Forme jour par jour : charge (TSS), CTL (forme de fond, 42 j),
    /// ATL (fatigue, 7 j) et TSB (fraîcheur).
    #[func]
    fn fitness(&mut self, days: i64) -> VarArray {
        let mut out = VarArray::new();
        let Some(store) = self.store() else {
            return out;
        };
        let f = store.fitness(days.max(1) as usize, now_unix());
        for f in self.ok(f) {
            let mut d = VarDictionary::new();
            d.set("day_unix", f.day_unix);
            d.set("tss", f.tss);
            d.set("ctl", f.ctl);
            d.set("atl", f.atl);
            d.set("tsb", f.tsb);
            out.push(&d.to_variant());
        }
        out
    }

    #[func]
    fn records(&mut self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(store) = self.store() else {
            return d;
        };
        let r = store.records();
        let r = self.ok(r);
        d.set("longest_s", r.longest_s);
        d.set("farthest_m", r.farthest_m);
        d.set("most_elevation_m", r.most_elevation_m);
        d.set("best_20min_w", r.best_20min_w.unwrap_or(0.0));
        d.set("estimated_ftp_w", r.estimated_ftp_w.unwrap_or(0.0));
        d
    }

    #[func]
    fn delete_ride(&mut self, id: i64) -> bool {
        let Some(store) = self.store() else {
            return false;
        };
        let r = store.delete_ride(id);
        self.ok(r)
    }

    /// Écrit le fichier .FIT de la sortie (Strava, Garmin Connect, Coros…).
    #[func]
    fn export_fit(&mut self, id: i64, path: GString) -> bool {
        let real = ProjectSettings::singleton()
            .globalize_path(&path)
            .to_string();
        let Some(store) = self.store() else {
            return false;
        };
        let result = fit_bytes(store, id)
            .and_then(|(_, b)| std::fs::write(&real, b).map_err(|e| format!("{real} : {e}")));
        match result {
            Ok(()) => true,
            Err(e) => {
                self.last_error = e;
                false
            }
        }
    }
}
