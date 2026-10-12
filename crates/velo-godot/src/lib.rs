//! Pont GDExtension : expose une sortie velotrace à Godot.
//!
//! Godot ne voit qu'un nœud `VeloRide` : on lui donne un GPX, on démarre en
//! mode test (ou plus tard avec le vrai matériel), et à chaque image il
//! fournit la télémétrie et la position sur le parcours.

use godot::classes::image::Format;
use godot::classes::mesh::PrimitiveType;
use godot::classes::{ArrayMesh, Image, ImageTexture, ProjectSettings};
use godot::prelude::*;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use velo_core::config::Profile;
use velo_core::devices::{DeviceHub, Shift};
use velo_core::protocol::ControlCommand;
use velo_core::ride::Ride;
use velo_core::route::Route;
use velo_core::sim::{AutoShift, RiderModel, SimSettings, SimulatedDevices};
use velo_core::terrain::Terrain;
use velo_world::mesh::MeshData;
use velo_world::world::Texture;
use velo_world::{Streamer, World, WorldOptions};

struct VeloExtension;

#[gdextension]
unsafe impl ExtensionLibrary for VeloExtension {}

/// Construction du décor réel en cours dans un fil à part.
struct WorldBuild {
    handle: JoinHandle<Result<World, String>>,
    progress: Arc<Mutex<(f32, String)>>,
    cancel: Arc<AtomicBool>,
}

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
    build: Option<WorldBuild>,
    streamer: Option<Streamer>,
    world_error: String,
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
            build: None,
            streamer: None,
            world_error: String::new(),
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

/// Convertit un maillage velo-world en `ArrayMesh` Godot (`None` s'il est vide).
fn array_mesh(m: &MeshData) -> Option<Gd<ArrayMesh>> {
    if m.is_empty() {
        return None;
    }
    let to_v3 = |v: &[[f32; 3]]| -> PackedVector3Array {
        v.iter().map(|p| Vector3::new(p[0], p[1], p[2])).collect()
    };
    let mut arrays = VarArray::new();
    // Mesh.ARRAY_MAX = 13 ; 0 sommets, 1 normales, 3 couleurs, 4 UV, 12 indices.
    arrays.resize(13, &Variant::nil());
    arrays.set(0, &to_v3(&m.positions).to_variant());
    arrays.set(1, &to_v3(&m.normals).to_variant());
    if !m.colors.is_empty() {
        let c: PackedColorArray = m
            .colors
            .iter()
            .map(|c| Color::from_rgba(c[0], c[1], c[2], c[3]))
            .collect();
        arrays.set(3, &c.to_variant());
    }
    if !m.uvs.is_empty() {
        let uv: PackedVector2Array = m.uvs.iter().map(|u| Vector2::new(u[0], u[1])).collect();
        arrays.set(4, &uv.to_variant());
    }
    arrays.set(
        12,
        &PackedInt32Array::from(m.indices.as_slice()).to_variant(),
    );
    let mut mesh = ArrayMesh::new_gd();
    mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
    Some(mesh)
}

/// Texture GPU avec mipmaps déjà calculés par velo-world.
fn texture(t: &Texture) -> Option<Gd<ImageTexture>> {
    if t.size == 0 {
        return None;
    }
    let data = PackedByteArray::from(t.data.as_slice());
    let image: Gd<Image> =
        Image::create_from_data(t.size as i32, t.size as i32, true, Format::RGBA8, &data)?;
    ImageTexture::create_from_image(&image)
}

impl VeloRide {
    /// Abandonne le décor réel en cours de construction ou chargé.
    fn drop_world(&mut self) {
        if let Some(b) = self.build.take() {
            // Le fil s'arrête de lui-même ; inutile de bloquer l'interface.
            b.cancel.store(true, Ordering::Relaxed);
        }
        self.streamer = None;
        self.world_error.clear();
    }
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
                self.drop_world();
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
        if let Some(s) = &self.streamer {
            return s.world().ground(x, -z);
        }
        self.terrain.as_ref().map_or(0.0, |t| t.height(x, -z))
    }

    /// Source des images aériennes ({z}, {x}, {y}) ; vide pour s'en passer.
    #[func]
    fn set_imagery_url(&mut self, url: GString) {
        self.profile.scenery.imagery_url = url.to_string();
    }

    /// Lance la construction du décor réel (relief, images, OSM) en tâche de
    /// fond. `quality` : 0 basse, 1 moyenne, 2 haute. `osm_file` : JSON
    /// Overpass à utiliser à la place du téléchargement (vide sinon).
    #[func]
    fn start_world(&mut self, quality: i64, online: bool, osm_file: GString) -> bool {
        let Some(route) = self.route.clone() else {
            self.last_error = "aucun parcours chargé".into();
            return false;
        };
        self.drop_world();
        let mut opts = WorldOptions::from_scenery(&self.profile.scenery);
        opts.quality = quality.clamp(0, 2) as u8;
        opts.online = online;
        if !osm_file.is_empty() {
            let real = ProjectSettings::singleton()
                .globalize_path(&osm_file)
                .to_string();
            opts.osm_file = Some(PathBuf::from(real));
        }
        let progress = Arc::new(Mutex::new((0.0, "Préparation du décor…".to_string())));
        let cancel = Arc::new(AtomicBool::new(false));
        let (p, c) = (progress.clone(), cancel.clone());
        let handle = std::thread::spawn(move || {
            World::build(
                route,
                &opts,
                &|f, msg| *p.lock().unwrap() = (f, msg.to_string()),
                &c,
            )
        });
        self.build = Some(WorldBuild {
            handle,
            progress,
            cancel,
        });
        true
    }

    /// État de la construction : `state` vaut « building », « ready », « error » ou « none ».
    #[func]
    fn world_status(&mut self) -> VarDictionary {
        if self.build.as_ref().is_some_and(|b| b.handle.is_finished()) {
            let b = self.build.take().unwrap();
            match b.handle.join() {
                Ok(Ok(world)) => {
                    // Le parcours suit désormais l'altitude du relief.
                    self.terrain = Some(Terrain::new(&world.route));
                    self.route = Some(world.route.clone());
                    let threads = std::thread::available_parallelism()
                        .map_or(2, |n| n.get().saturating_sub(2))
                        .clamp(1, 4);
                    self.streamer = Some(Streamer::new(Arc::new(world), threads));
                }
                Ok(Err(e)) => self.world_error = e,
                Err(_) => {
                    self.world_error = "erreur interne pendant la construction du décor".into()
                }
            }
        }
        let mut d = VarDictionary::new();
        let (state, progress, message) = if let Some(b) = &self.build {
            let (f, m) = b.progress.lock().unwrap().clone();
            ("building", f, m)
        } else if let Some(s) = &self.streamer {
            ("ready", 1.0, s.world().summary.clone())
        } else if !self.world_error.is_empty() {
            ("error", 0.0, self.world_error.clone())
        } else {
            ("none", 0.0, String::new())
        };
        d.set("state", state);
        d.set("progress", progress);
        d.set("message", &GString::from(message.as_str()).to_variant());
        d
    }

    #[func]
    fn world_credits(&self) -> GString {
        GString::from(
            self.streamer
                .as_ref()
                .map_or("", |s| s.world().credits.as_str()),
        )
    }

    /// Réglages dépendant de la qualité : distances du décor et des arbres.
    #[func]
    fn world_ranges(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        if let Some(s) = &self.streamer {
            let q = s.world().quality;
            d.set("far_margin", q.far_margin_m);
            d.set("near_margin", q.near_margin_m);
            d.set("tree_radius", q.tree_radius_m);
            d.set("near_size", velo_world::NEAR_SIZE);
            d.set("far_size", velo_world::FAR_SIZE);
        }
        d
    }

    #[func]
    fn world_far_count(&self) -> i64 {
        self.streamer
            .as_ref()
            .map_or(0, |s| s.world().far.len() as i64)
    }

    /// Grand carré lointain : `center` (Godot), `mesh` et `texture`.
    #[func]
    fn world_far(&self, index: i64) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(c) = self
            .streamer
            .as_ref()
            .and_then(|s| s.world().far.get(index as usize))
        else {
            return d;
        };
        d.set("center", v3(c.center.0, 0.0, -c.center.1));
        if let Some(m) = array_mesh(&c.mesh) {
            d.set("mesh", &m.to_variant());
        }
        if let Some(t) = texture(&c.texture) {
            d.set("texture", &t.to_variant());
        }
        d
    }

    /// Centres (Godot) des carrés détaillés, dans l'ordre de leurs indices.
    #[func]
    fn world_near_centers(&self) -> PackedVector3Array {
        let mut out = PackedVector3Array::new();
        if let Some(s) = &self.streamer {
            for c in &s.world().near {
                out.push(v3(c.center.0, 0.0, -c.center.1));
            }
        }
        out
    }

    /// Carrés détaillés à générer, du plus urgent au moins urgent.
    #[func]
    fn world_want(&self, indices: PackedInt32Array) {
        if let Some(s) = &self.streamer {
            let list: Vec<usize> = indices.as_slice().iter().map(|&i| i as usize).collect();
            s.want(&list);
        }
    }

    #[func]
    fn world_pending(&self) -> i64 {
        self.streamer.as_ref().map_or(0, |s| s.pending() as i64)
    }

    /// Carrés détaillés prêts (au plus `max`) : index, relief, texture, arbres
    /// (tampons MultiMesh), bâtiments, routes et eau.
    #[func]
    fn world_take(&self, max: i64) -> VarArray {
        let mut out = VarArray::new();
        let Some(s) = &self.streamer else { return out };
        for detail in s.take(max.max(0) as usize) {
            let mut d = VarDictionary::new();
            let c = s.world().near[detail.index].center;
            d.set("index", detail.index as i64);
            d.set("center", v3(c.0, 0.0, -c.1));
            for (key, mesh) in [
                ("terrain", &detail.terrain),
                ("buildings", &detail.buildings),
                ("roads", &detail.roads),
                ("water", &detail.water),
            ] {
                if let Some(m) = array_mesh(mesh) {
                    d.set(key, &m.to_variant());
                }
            }
            if let Some(t) = texture(&detail.texture) {
                d.set("texture", &t.to_variant());
            }
            d.set(
                "conifers",
                &PackedFloat32Array::from(detail.conifers.buffer.as_slice()).to_variant(),
            );
            d.set(
                "broadleaves",
                &PackedFloat32Array::from(detail.broadleaves.buffer.as_slice()).to_variant(),
            );
            out.push(&d.to_variant());
        }
        out
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

    /// Avance directement à une distance du parcours (essais).
    #[func]
    fn skip_to(&mut self, distance_m: f64) {
        if let Some(r) = self.ride.as_mut() {
            r.skip_to(distance_m);
        }
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
