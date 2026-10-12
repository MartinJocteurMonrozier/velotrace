//! Pont GDExtension : expose une sortie velotrace à Godot.
//!
//! Godot ne voit qu'un nœud `VeloRide` : on lui donne un GPX, on démarre en
//! mode test (ou plus tard avec le vrai matériel), et à chaque image il
//! fournit la télémétrie et la position sur le parcours.

use godot::classes::ProjectSettings;
use godot::prelude::*;
use std::collections::BTreeSet;
use velo_core::config::Profile;
use velo_core::devices::{DeviceHub, Shift};
use velo_core::protocol::ControlCommand;
use velo_core::ride::Ride;
use velo_core::route::Route;
use velo_core::sim::{AutoShift, RiderModel, SimSettings, SimulatedDevices};
use velo_core::terrain::Terrain;

struct VeloExtension;

#[gdextension]
unsafe impl ExtensionLibrary for VeloExtension {}

/// Pas de simulation maximal, pour rester stable si une image prend du retard.
const MAX_STEP_S: f64 = 0.05;

#[derive(GodotClass)]
#[class(base = Node)]
pub struct VeloRide {
    base: Base<Node>,
    profile: Profile,
    route: Option<Route>,
    terrain: Option<Terrain>,
    ride: Option<Ride>,
    devices: Option<SimulatedDevices>,
    running: bool,
    last_error: String,
}

#[godot_api]
impl INode for VeloRide {
    fn init(base: Base<Node>) -> Self {
        let mut profile = Profile::martin();
        profile.app.test_mode = true;
        Self {
            base,
            profile,
            route: None,
            terrain: None,
            ride: None,
            devices: None,
            running: false,
            last_error: String::new(),
        }
    }

    fn process(&mut self, delta: f64) {
        if !self.running {
            return;
        }
        let (Some(ride), Some(devices)) = (self.ride.as_mut(), self.devices.as_mut()) else {
            return;
        };
        let mut remaining = delta;
        while remaining > 1e-6 {
            let dt = remaining.min(MAX_STEP_S);
            remaining -= dt;
            for event in devices.poll(dt) {
                ride.handle(&event);
            }
            let t = ride.step(dt);
            devices.send(ride.trainer_command());
            if t.finished {
                self.running = false;
                break;
            }
        }
    }
}

fn v3(x: f64, y: f64, z: f64) -> Vector3 {
    Vector3::new(x as f32, y as f32, z as f32)
}

#[godot_api]
impl VeloRide {
    /// Charge un GPX (chemin système ou `res://`). Renvoie `false` en cas d'erreur,
    /// le message est alors donné par `error_message()`.
    #[func]
    fn load_gpx(&mut self, path: GString) -> bool {
        let real = ProjectSettings::singleton()
            .globalize_path(&path)
            .to_string();
        let result = std::fs::File::open(&real)
            .map_err(|e| format!("impossible d'ouvrir « {real} » : {e}"))
            .and_then(|f| Route::from_gpx(f, 5.0, 40.0).map_err(|e| e.to_string()));
        match result {
            Ok(route) => {
                self.terrain = Some(Terrain::new(&route));
                self.route = Some(route);
                self.ride = None;
                self.running = false;
                true
            }
            Err(e) => {
                self.last_error = e;
                false
            }
        }
    }

    #[func]
    fn error_message(&self) -> GString {
        GString::from(self.last_error.as_str())
    }

    #[func]
    fn route_name(&self) -> GString {
        let name = self
            .route
            .as_ref()
            .and_then(|r| r.name.clone())
            .unwrap_or_else(|| "Parcours".into());
        GString::from(name.as_str())
    }

    #[func]
    fn route_length_m(&self) -> f64 {
        self.route.as_ref().map_or(0.0, |r| r.length_m())
    }

    #[func]
    fn route_elevation_gain_m(&self) -> f64 {
        self.route.as_ref().map_or(0.0, |r| r.elevation_gain_m())
    }

    /// Points du parcours en coordonnées Godot : x vers l'est, y altitude, z vers le sud.
    #[func]
    fn route_points(&self) -> PackedVector3Array {
        let mut out = PackedVector3Array::new();
        if let Some(r) = &self.route {
            for p in &r.points {
                out.push(v3(p.x, p.elevation_m, -p.y));
            }
        }
        out
    }

    /// Altitude du sol en coordonnées Godot (x est, z sud).
    #[func]
    fn ground_height(&self, x: f64, z: f64) -> f64 {
        self.terrain.as_ref().map_or(0.0, |t| t.height(x, -z))
    }

    /// Distance horizontale au parcours (m), plafonnée à `max_m`.
    #[func]
    fn distance_to_route(&self, x: f64, z: f64, max_m: f64) -> f64 {
        self.terrain
            .as_ref()
            .and_then(|t| t.nearest(x, -z, max_m))
            .map_or(max_m, |(d, _)| d)
    }

    /// Origines (x, z) des tuiles de relief de `size` mètres situées à moins
    /// de `margin` mètres du parcours.
    #[func]
    fn terrain_tiles(&self, size: f64, margin: f64) -> PackedVector2Array {
        let mut keys = BTreeSet::new();
        if let Some(r) = &self.route {
            let reach = (margin / size).ceil() as i64;
            for p in r.points.iter().step_by(4) {
                let (cx, cz) = ((p.x / size).floor() as i64, (-p.y / size).floor() as i64);
                for dx in -reach..=reach {
                    for dz in -reach..=reach {
                        keys.insert((cx + dx, cz + dz));
                    }
                }
            }
        }
        let mut out = PackedVector2Array::new();
        for (kx, kz) in keys {
            out.push(Vector2::new(
                (kx as f64 * size) as f32,
                (kz as f64 * size) as f32,
            ));
        }
        out
    }

    /// Altitudes d'une tuile : (cells + 1)² valeurs, ligne par ligne selon z.
    #[func]
    fn terrain_tile_heights(&self, origin: Vector2, size: f64, cells: i64) -> PackedFloat32Array {
        let mut out = PackedFloat32Array::new();
        let Some(t) = &self.terrain else { return out };
        let step = size / cells as f64;
        for j in 0..=cells {
            for i in 0..=cells {
                let x = origin.x as f64 + i as f64 * step;
                let z = origin.y as f64 + j as f64 * step;
                out.push(t.height(x, -z) as f32);
            }
        }
        out
    }

    /// Distance cumulée (m) de chaque point de `route_points`.
    #[func]
    fn route_distances(&self) -> PackedFloat32Array {
        let mut out = PackedFloat32Array::new();
        if let Some(r) = &self.route {
            for p in &r.points {
                out.push(p.distance_m as f32);
            }
        }
        out
    }

    /// Démarre une sortie en mode test. `mode` vaut « cadence » (Click
    /// automatiques, la puissance dépend du rapport) ou « puissance ».
    #[func]
    fn start_test(&mut self, mode: GString, value: f64) -> bool {
        let Some(route) = self.route.clone() else {
            self.last_error = "aucun parcours chargé".into();
            return false;
        };
        let mut settings = SimSettings::default();
        if mode == "puissance" {
            settings.rider = RiderModel::Power {
                watts: value,
                cadence_rpm: 88.0,
            };
            settings.auto_shift = None;
        } else {
            settings.rider = RiderModel::Cadence { rpm: value };
            settings.auto_shift = Some(AutoShift {
                min_w: 150.0,
                max_w: 260.0,
                interval_s: 2.0,
            });
        }
        let mut devices = SimulatedDevices::new(&self.profile, settings);
        devices.send(ControlCommand::RequestControl);
        devices.send(ControlCommand::StartOrResume);
        self.ride = Some(Ride::new(self.profile.clone(), route));
        self.devices = Some(devices);
        self.running = true;
        true
    }

    #[func]
    fn set_paused(&mut self, paused: bool) {
        if self.ride.as_ref().is_some_and(|r| !r.finished()) {
            self.running = !paused;
        }
    }

    #[func]
    fn is_running(&self) -> bool {
        self.running
    }

    /// Appui sur un Click (clavier en mode test).
    #[func]
    fn shift_up(&mut self) {
        if let Some(d) = self.devices.as_mut() {
            d.press(Shift::Up);
        }
    }

    #[func]
    fn shift_down(&mut self) {
        if let Some(d) = self.devices.as_mut() {
            d.press(Shift::Down);
        }
    }

    /// Active ou coupe les Click automatiques du cycliste virtuel.
    #[func]
    fn set_auto_shift(&mut self, enabled: bool) {
        if let Some(d) = self.devices.as_mut() {
            d.settings.auto_shift = enabled.then_some(AutoShift {
                min_w: 150.0,
                max_w: 260.0,
                interval_s: 2.0,
            });
        }
    }

    /// Ajuste la consigne du cycliste virtuel (watts ou rpm selon le mode).
    #[func]
    fn adjust_rider(&mut self, delta: f64) {
        if let Some(d) = self.devices.as_mut() {
            d.settings.rider = match d.settings.rider {
                RiderModel::Power { watts, cadence_rpm } => RiderModel::Power {
                    watts: (watts + delta).clamp(0.0, 1500.0),
                    cadence_rpm,
                },
                RiderModel::Cadence { rpm } => RiderModel::Cadence {
                    rpm: (rpm + delta / 5.0).clamp(40.0, 130.0),
                },
            };
        }
    }

    /// Télémétrie courante, position et cap sur le parcours.
    #[func]
    fn telemetry(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(ride) = &self.ride else {
            return d;
        };
        let t = ride.telemetry();
        let here = ride.route.point_at(t.distance_m);
        let ahead = ride.route.point_at(t.distance_m + 4.0);
        let behind = ride.route.point_at(t.distance_m - 4.0);
        let gears = ride.gearbox.gears().len() as i64;
        d.set("elapsed_s", t.elapsed_s);
        d.set("distance_m", t.distance_m);
        d.set("remaining_m", ride.route.length_m() - t.distance_m);
        d.set("speed_kmh", t.speed_ms * 3.6);
        d.set("power_w", t.power_w);
        d.set("cadence_rpm", t.cadence_rpm);
        d.set("heart_rate", t.heart_rate_bpm.map_or(0, |h| h as i64));
        d.set("grade_pct", t.grade * 100.0);
        d.set(
            "grade_ahead_pct",
            ride.route.grade_at(t.distance_m + 200.0) * 100.0,
        );
        d.set("elevation_m", t.elevation_m);
        d.set(
            "gear",
            &GString::from(format!("{}×{}", t.gear.chainring, t.gear.cog).as_str()).to_variant(),
        );
        d.set("gear_index", t.gear_index as i64);
        d.set("gear_count", gears);
        d.set("rider_kg", self.profile.rider.weight_kg);
        d.set("position", v3(here.x, here.elevation_m, -here.y));
        d.set(
            "direction",
            v3(
                ahead.x - behind.x,
                ahead.elevation_m - behind.elevation_m,
                -(ahead.y - behind.y),
            ),
        );
        d.set("finished", t.finished);
        d
    }
}
