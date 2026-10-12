//! Sortie en cours : simulation, appareils (simulés ou Bluetooth), séance,
//! coureurs virtuels et enregistrement à 1 Hz.

use godot::prelude::*;
use velo_core::config::Profile;
use velo_core::devices::{DeviceEvent, DeviceHub, Shift};
use velo_core::opponents::{default_pack, OpponentSpec, Pack};
use velo_core::protocol::ControlCommand;
use velo_core::record::Recorder;
use velo_core::ride::{Ride, Telemetry};
use velo_core::route::Route;
use velo_core::sim::{AutoShift, RiderModel, SimSettings, SimulatedDevices};
use velo_core::workout::{Step, Target, Workout, WorkoutPlayer};

use crate::devices::VeloDevices;

/// Plage de puissance que le cycliste virtuel garde avec ses Click automatiques.
pub const AUTO_SHIFT: AutoShift = AutoShift {
    min_w: 150.0,
    max_w: 260.0,
    interval_s: 2.0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Libre,
    Seance,
    Adversaires,
    Test,
}

impl Mode {
    pub fn parse(s: &str) -> Self {
        match s {
            "libre" => Mode::Libre,
            "seance" => Mode::Seance,
            "adversaires" => Mode::Adversaires,
            _ => Mode::Test,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Mode::Libre => "libre",
            Mode::Seance => "seance",
            Mode::Adversaires => "adversaires",
            Mode::Test => "test",
        }
    }
}

/// D'où viennent les mesures : cycliste virtuel ou vrai matériel.
pub enum Source {
    Sim(Box<SimulatedDevices>),
    Ble(Gd<VeloDevices>),
}

impl Source {
    fn poll(&mut self, dt: f64) -> Vec<DeviceEvent> {
        match self {
            Source::Sim(s) => s.poll(dt),
            Source::Ble(d) => d.bind_mut().poll(dt),
        }
    }

    fn send(&mut self, c: ControlCommand) {
        match self {
            Source::Sim(s) => s.send(c),
            Source::Ble(d) => d.bind_mut().send(c),
        }
    }
}

pub struct Session {
    pub ride: Ride,
    pub source: Source,
    pub recorder: Recorder,
    pub workout: Option<WorkoutPlayer>,
    pub pack: Option<Pack>,
    pub mode: Mode,
    /// Consigne du cycliste virtuel hors séance (pour y revenir après l'ERG).
    pub sim_rider: RiderModel,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn get_f(d: &VarDictionary, key: &str, default: f64) -> f64 {
    d.get(key)
        .and_then(|v| {
            v.try_to::<f64>()
                .ok()
                .or_else(|| v.try_to::<i64>().ok().map(|i| i as f64))
        })
        .unwrap_or(default)
}

fn get_s(d: &VarDictionary, key: &str, default: &str) -> String {
    d.get(key)
        .and_then(|v| v.try_to::<GString>().ok())
        .map_or(default.to_string(), |s| s.to_string())
}

fn get_b(d: &VarDictionary, key: &str, default: bool) -> bool {
    d.get(key)
        .and_then(|v| v.try_to::<bool>().ok())
        .unwrap_or(default)
}

fn get_d(d: &VarDictionary, key: &str) -> VarDictionary {
    d.get(key)
        .and_then(|v| v.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// Séance décrite par un dictionnaire Godot (voir `workout_to_dict`).
pub fn workout_from_dict(d: &VarDictionary) -> Option<Workout> {
    let steps = d.get("steps")?.try_to::<VarArray>().ok()?;
    let steps: Vec<Step> = steps
        .iter_shared()
        .filter_map(|v| v.try_to::<VarDictionary>().ok())
        .map(|s| {
            let from = get_f(&s, "from", 0.5);
            let to = get_f(&s, "to", from);
            let cadence = get_f(&s, "cadence_rpm", 0.0);
            Step {
                duration_s: get_f(&s, "duration_s", 60.0).max(1.0),
                target: if get_b(&s, "free", false) {
                    Target::Free
                } else if (from - to).abs() < 1e-6 {
                    Target::Power { ftp: from }
                } else {
                    Target::Ramp { from, to }
                },
                cadence_rpm: (cadence > 0.0).then_some(cadence),
                label: get_s(&s, "label", ""),
            }
        })
        .collect();
    (!steps.is_empty()).then(|| Workout {
        name: get_s(d, "name", "Séance"),
        description: get_s(d, "description", ""),
        steps,
    })
}

pub fn workout_to_dict(w: &Workout) -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set("name", w.name.as_str());
    d.set("description", w.description.as_str());
    d.set("duration_s", w.duration_s());
    let mut steps = VarArray::new();
    for s in &w.steps {
        let mut x = VarDictionary::new();
        let (from, to, free) = match s.target {
            Target::Free => (0.0, 0.0, true),
            Target::Power { ftp } => (ftp, ftp, false),
            Target::Ramp { from, to } => (from, to, false),
        };
        x.set("duration_s", s.duration_s);
        x.set("from", from);
        x.set("to", to);
        x.set("free", free);
        x.set("cadence_rpm", s.cadence_rpm.unwrap_or(0.0));
        x.set("label", s.label.as_str());
        steps.push(&x.to_variant());
    }
    d.set("steps", &steps);
    d
}

impl Session {
    /// Prépare une sortie d'après la configuration choisie au menu.
    pub fn start(
        profile: &Profile,
        route: Route,
        cfg: &VarDictionary,
        devices: Option<Gd<VeloDevices>>,
    ) -> Result<Self, String> {
        let mode = Mode::parse(&get_s(cfg, "mode", "test"));
        let simulated = get_b(cfg, "simulated", true) || mode == Mode::Test || devices.is_none();
        let sim_rider = if get_s(cfg, "test_rider", &profile.app.test_rider) == "puissance" {
            RiderModel::Power {
                watts: get_f(cfg, "test_value", 200.0).clamp(0.0, 1500.0),
                cadence_rpm: 88.0,
            }
        } else {
            RiderModel::Cadence {
                rpm: get_f(cfg, "test_value", 88.0).clamp(40.0, 130.0),
            }
        };
        let mut source = match (simulated, devices) {
            (false, Some(d)) => Source::Ble(d),
            _ => {
                let settings = SimSettings {
                    rider: sim_rider,
                    auto_shift: matches!(sim_rider, RiderModel::Cadence { .. })
                        .then_some(AUTO_SHIFT),
                    ..SimSettings::default()
                };
                Source::Sim(Box::new(SimulatedDevices::new(profile, settings)))
            }
        };
        source.send(ControlCommand::RequestControl);
        source.send(ControlCommand::StartOrResume);

        let workout = if mode == Mode::Seance {
            let w = workout_from_dict(&get_d(cfg, "workout"))
                .ok_or_else(|| "séance vide ou invalide".to_string())?;
            let mut p = WorkoutPlayer::new(w, profile.ftp());
            p.erg = get_b(cfg, "erg", true);
            p.intensity = get_f(cfg, "intensity", 1.0).clamp(0.3, 2.0);
            Some(p)
        } else {
            None
        };
        let altitude = route.points.first().map_or(0.0, |p| p.elevation_m);
        let pack = if mode == Mode::Adversaires {
            let o = get_d(cfg, "opponents");
            let count = get_f(&o, "count", 5.0).round().clamp(1.0, 12.0) as usize;
            let wkg = get_f(&o, "w_per_kg", 3.0).clamp(0.5, 8.0);
            let spread = get_f(&o, "spread", 0.3).clamp(0.0, 2.0);
            let mut specs: Vec<OpponentSpec> = default_pack(wkg, count);
            // Écart demandé : de wkg − spread à wkg + spread, du plus faible au plus fort.
            for (i, s) in specs.iter_mut().enumerate() {
                let f = if count > 1 {
                    i as f64 / (count - 1) as f64 * 2.0 - 1.0
                } else {
                    0.0
                };
                s.w_per_kg = (wkg + f * spread).max(0.5);
            }
            let mut pack = Pack::new(specs, profile, altitude);
            // Grille de départ : deux par rang, juste devant le joueur.
            for (i, o) in pack.opponents.iter_mut().enumerate() {
                o.distance_m = 3.0 + (i / 2) as f64 * 2.2;
            }
            Some(pack)
        } else {
            None
        };
        Ok(Self {
            ride: Ride::new(profile.clone(), route),
            source,
            recorder: Recorder::new(now_unix()),
            workout,
            pack,
            mode,
            sim_rider,
        })
    }

    pub fn simulated(&self) -> bool {
        matches!(self.source, Source::Sim(_))
    }

    pub fn sim_mut(&mut self) -> Option<&mut SimulatedDevices> {
        match &mut self.source {
            Source::Sim(s) => Some(s),
            Source::Ble(_) => None,
        }
    }

    /// Avance d'un pas : appareils, séance, physique, adversaires, enregistrement.
    pub fn step(&mut self, dt: f64) -> Telemetry {
        for event in self.source.poll(dt) {
            self.ride.handle(&event);
        }
        if let Some(w) = self.workout.as_mut() {
            w.advance(dt);
            let target = w.target_w();
            let erg = w.erg;
            self.ride.set_erg_target(if erg { target } else { None });
            // Le cycliste virtuel suit la consigne même sans ERG.
            let base = self.sim_rider;
            if let Source::Sim(s) = &mut self.source {
                s.settings.rider = match target {
                    Some(watts) => RiderModel::Power {
                        watts,
                        cadence_rpm: w.state().cadence_rpm.unwrap_or(90.0),
                    },
                    None => base,
                };
            }
        }
        let t = self.ride.step(dt);
        if let Some(p) = self.pack.as_mut() {
            p.step(dt, &self.ride.route, t.elapsed_s, t.distance_m);
        }
        self.source.send(self.ride.trainer_command());
        let point = self.ride.route.point_at(t.distance_m);
        self.recorder.push(&t, &point);
        t
    }

    /// Fin de sortie : plus d'ERG, le home trainer repasse à plat.
    pub fn finish(&mut self) {
        let p = &self.ride.profile;
        let cw_kg_m = 0.5 * 1.2 * p.bike.cda_m2;
        let crr = p.bike.crr;
        self.source.send(ControlCommand::Simulation {
            wind_ms: 0.0,
            grade: 0.0,
            crr,
            cw_kg_m,
        });
    }

    pub fn shift(&mut self, shift: Shift) {
        match &mut self.source {
            Source::Sim(s) => s.press(shift),
            // Clavier avec le vrai matériel : directement sur la boîte virtuelle.
            Source::Ble(_) => self.ride.handle(&DeviceEvent::Shift(shift)),
        }
    }
}
