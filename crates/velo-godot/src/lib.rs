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
use velo_core::devices::{DeviceKind, RememberedDevice, Shift};
use velo_core::route::Route;
use velo_core::sim::RiderModel;
use velo_core::terrain::Terrain;
use velo_core::workout::Workout;
use velo_world::mesh::MeshData;
use velo_world::road::RoadPath;
use velo_world::world::Texture;
use velo_world::{Streamer, World, WorldOptions};

use devices::VeloDevices;
use session::Session;

mod devices;
mod history;
mod profile;
mod session;
mod strava;

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
    /// Tracé lissé de la route et trajectoire du cycliste.
    path: Option<RoadPath>,
    session: Option<Session>,
    /// Appareils Bluetooth (nœud partagé, voir `set_devices`).
    devices: Option<Gd<VeloDevices>>,
    running: bool,
    last_error: String,
    build: Option<WorldBuild>,
    streamer: Option<Streamer>,
    world_error: String,
}

#[godot_api]
impl INode for VeloRide {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            profile: profile::load(),
            route: None,
            terrain: None,
            path: None,
            session: None,
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
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let mut remaining = delta;
        while remaining > 1e-6 {
            let dt = remaining.min(MAX_STEP_S);
            remaining -= dt;
            let t = session.step(dt);
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
    // Mesh.ARRAY_MAX = 13 ; 0 sommets, 1 normales, 3 couleurs, 4 UV, 5 UV2,
    // 6 CUSTOM0 (octets RGBA, le format par défaut), 12 indices.
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
    if !m.uv2s.is_empty() {
        let uv: PackedVector2Array = m.uv2s.iter().map(|u| Vector2::new(u[0], u[1])).collect();
        arrays.set(5, &uv.to_variant());
    }
    if !m.customs.is_empty() {
        let bytes: Vec<u8> = m.customs.iter().flatten().copied().collect();
        arrays.set(6, &PackedByteArray::from(bytes.as_slice()).to_variant());
    }
    arrays.set(
        12,
        &PackedInt32Array::from(m.indices.as_slice()).to_variant(),
    );
    let mut mesh = ArrayMesh::new_gd();
    mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
    Some(mesh)
}

/// Côté des cellules d'arbres (m) : chaque cellule choisit son niveau de
/// détail selon sa distance à la caméra.
const TREE_CELL_M: f32 = 128.0;

/// Répartit les arbres d'un carré de 512 m en cellules de 128 m : `center`
/// (relatif au carré) et tampons MultiMesh recentrés sur la cellule.
fn tree_cells(
    conifers: &velo_world::mesh::Instances,
    broadleaves: &velo_world::mesh::Instances,
) -> VarArray {
    use std::collections::BTreeMap;
    const STRIDE: usize = velo_world::mesh::Instances::STRIDE;
    let mut cells: BTreeMap<(i32, i32), [Vec<f32>; 2]> = BTreeMap::new();
    for (kind, inst) in [conifers, broadleaves].into_iter().enumerate() {
        for one in inst.buffer.chunks_exact(STRIDE) {
            let key = (
                (one[3] / TREE_CELL_M).floor() as i32,
                (one[11] / TREE_CELL_M).floor() as i32,
            );
            cells.entry(key).or_default()[kind].extend_from_slice(one);
        }
    }
    let mut out = VarArray::new();
    for ((kx, kz), mut bufs) in cells {
        let (cx, cz) = (
            (kx as f32 + 0.5) * TREE_CELL_M,
            (kz as f32 + 0.5) * TREE_CELL_M,
        );
        let mut d = VarDictionary::new();
        d.set("center", Vector3::new(cx, 0.0, cz));
        for (key, buf) in ["conifers", "broadleaves"].into_iter().zip(bufs.iter_mut()) {
            for one in buf.chunks_exact_mut(STRIDE) {
                one[3] -= cx;
                one[11] -= cz;
            }
            d.set(key, &PackedFloat32Array::from(buf.as_slice()).to_variant());
        }
        out.push(&d.to_variant());
    }
    out
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
                self.path = Some(RoadPath::new(&route));
                self.route = Some(route);
                self.session = None;
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
    fn has_route(&self) -> bool {
        self.route.is_some()
    }

    /// Valeur d'un champ du profil (« rider.weight_kg », « bike.cassette »…).
    #[func]
    fn profile_get(&self, key: GString) -> Variant {
        profile::get(&self.profile, &key.to_string())
    }

    /// Modifie un champ du profil (en mémoire) ; `false` si la valeur est
    /// refusée, la raison est alors donnée par `error_message()`.
    #[func]
    fn profile_set(&mut self, key: GString, value: Variant) -> bool {
        match profile::set(&self.profile, &key.to_string(), &value) {
            Ok(p) => {
                self.profile = p;
                true
            }
            Err(e) => {
                self.last_error = e;
                false
            }
        }
    }

    /// Enregistre le profil sur disque.
    #[func]
    fn profile_save(&mut self) -> bool {
        match profile::save(&self.profile) {
            Ok(()) => true,
            Err(e) => {
                self.last_error = format!("profil non enregistré : {e}");
                false
            }
        }
    }

    #[func]
    fn profile_reload(&mut self) {
        self.profile = profile::load();
    }

    #[func]
    fn profile_path(&self) -> GString {
        GString::from(profile::path().to_string_lossy().as_ref())
    }

    /// Appareils appairés : `{id, name, kind}`.
    #[func]
    fn remembered_devices(&self) -> VarArray {
        let mut out = VarArray::new();
        for d in &self.profile.devices {
            let mut x = VarDictionary::new();
            x.set("id", d.id.as_str());
            x.set("name", d.name.as_str());
            x.set("kind", d.kind.key());
            out.push(&x.to_variant());
        }
        out
    }

    /// Mémorise l'appareil d'un rôle (il remplace le précédent).
    #[func]
    fn remember_device(&mut self, id: GString, name: GString, kind: GString) {
        let Some(kind) = DeviceKind::from_key(&kind.to_string()) else {
            return;
        };
        self.profile.devices.retain(|d| d.kind != kind);
        self.profile.devices.push(RememberedDevice {
            id: id.to_string(),
            name: name.to_string(),
            kind,
        });
    }

    #[func]
    fn forget_device(&mut self, kind: GString) {
        if let Some(kind) = DeviceKind::from_key(&kind.to_string()) {
            self.profile.devices.retain(|d| d.kind != kind);
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
                    self.path = Some(world.path.clone());
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
        if let Some(m) = array_mesh(&c.buildings) {
            d.set("buildings", &m.to_variant());
        }
        d
    }

    /// Chaussée et accotements en tronçons de ~200 m, puis amorces des routes
    /// adjacentes et ronds-points du décor réel (coordonnées Godot, altitude
    /// relative à `base`).
    #[func]
    fn road_meshes(&self, base: f64) -> VarArray {
        let mut out = VarArray::new();
        let mut meshes = self
            .path
            .as_ref()
            .map_or_else(Vec::new, |p| p.meshes(base, 200.0));
        if let Some(s) = &self.streamer {
            let world = s.world();
            meshes.extend(world.net.meshes(base));
            for m in &mut meshes {
                world.paint_ground(m);
            }
        }
        for m in meshes {
            if let Some(mesh) = array_mesh(&m) {
                out.push(&mesh.to_variant());
            }
        }
        out
    }

    /// Murs de soutènement et parapets le long des talus raides du décor réel.
    #[func]
    fn road_walls(&self, base: f64) -> VarArray {
        let mut out = VarArray::new();
        if let Some(s) = &self.streamer {
            for m in s.world().net.wall_meshes(base) {
                if let Some(mesh) = array_mesh(&m) {
                    out.push(&mesh.to_variant());
                }
            }
        }
        out
    }

    /// Falaises sculptées le long des talus rocheux du décor réel.
    #[func]
    fn road_cliffs(&self, base: f64) -> VarArray {
        let mut out = VarArray::new();
        if let Some(s) = &self.streamer {
            for m in s.world().net.cliff_meshes(base) {
                if let Some(mesh) = array_mesh(&m) {
                    out.push(&mesh.to_variant());
                }
            }
        }
        out
    }

    /// Arbustes des îlots de ronds-points (tampon MultiMesh).
    #[func]
    fn road_bushes(&self, base: f64) -> PackedFloat32Array {
        self.streamer
            .as_ref()
            .map_or_else(PackedFloat32Array::new, |s| {
                PackedFloat32Array::from(s.world().net.island_bushes(base).buffer.as_slice())
            })
    }

    /// Arches de course (départ, pied et sommet des ascensions, arrivée,
    /// flamme rouge, banderoles des derniers kilomètres) : position Godot
    /// (altitude absolue), cap (est, nord), distance de l'axe aux piles,
    /// distance sur le parcours, pente, type (0 départ, 1 pied, 2 sommet,
    /// 3 arrivée, 4 flamme rouge, 5 kilomètres) et textes.
    #[func]
    fn route_arches(&self) -> VarArray {
        let mut out = VarArray::new();
        let (Some(route), Some(path)) = (&self.route, &self.path) else {
            return out;
        };
        for a in velo_world::race::arches(route, path) {
            let mut d = VarDictionary::new();
            d.set("position", v3(a.x, a.elevation, -a.n));
            d.set(
                "heading",
                Vector2::new(a.heading.0 as f32, a.heading.1 as f32),
            );
            d.set("left", a.left);
            d.set("right", a.right);
            d.set("d", a.d);
            d.set("grade", a.grade);
            d.set("kind", a.kind as i64);
            d.set("title", &GString::from(a.title.as_str()));
            d.set("subtitle", &GString::from(a.subtitle.as_str()));
            out.push(&d.to_variant());
        }
        out
    }

    /// Barrières le long de la route aux abords des arches (tampon
    /// MultiMesh ; l'alpha de la couleur porte le motif de la banderole).
    #[func]
    fn route_barriers(&self, base: f64) -> PackedFloat32Array {
        let (Some(route), Some(path)) = (&self.route, &self.path) else {
            return PackedFloat32Array::new();
        };
        let arches = velo_world::race::arches(route, path);
        PackedFloat32Array::from(
            velo_world::race::barriers(path, &arches, base)
                .buffer
                .as_slice(),
        )
    }

    /// Spectateurs derrière les barrières (tampon MultiMesh, couleur du maillot).
    #[func]
    fn route_spectators(&self, base: f64) -> PackedFloat32Array {
        let (Some(route), Some(path)) = (&self.route, &self.path) else {
            return PackedFloat32Array::new();
        };
        let arches = velo_world::race::arches(route, path);
        PackedFloat32Array::from(
            velo_world::race::spectators(path, &arches, base)
                .buffer
                .as_slice(),
        )
    }

    /// Balises de bord de route (tampon MultiMesh), tous les 50 m.
    #[func]
    fn road_posts(&self, base: f64) -> PackedFloat32Array {
        self.path
            .as_ref()
            .map_or_else(PackedFloat32Array::new, |p| {
                PackedFloat32Array::from(p.posts(base, 50.0).buffer.as_slice())
            })
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
            if let Some(t) = texture(&detail.ground) {
                d.set("ground", &t.to_variant());
            }
            d.set(
                "tree_cells",
                &tree_cells(&detail.conifers, &detail.broadleaves).to_variant(),
            );
            d.set(
                "bushes",
                &PackedFloat32Array::from(detail.bushes.buffer.as_slice()).to_variant(),
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

    /// Nœud des appareils Bluetooth, utilisé quand la sortie n'est pas simulée.
    #[func]
    fn set_devices(&mut self, devices: Gd<VeloDevices>) {
        self.devices = Some(devices);
    }

    /// Démarre une sortie. `cfg` : `mode` (« libre », « seance »,
    /// « adversaires », « test »), `simulated` (cycliste virtuel au lieu du
    /// matériel), `test_rider` et `test_value`, `workout`, `erg`, `intensity`,
    /// `opponents` (`count`, `w_per_kg`, `spread`).
    #[func]
    fn start_ride(&mut self, cfg: VarDictionary) -> bool {
        let Some(route) = self.route.clone() else {
            self.last_error = "aucun parcours chargé".into();
            return false;
        };
        match Session::start(&self.profile, route, &cfg, self.devices.clone()) {
            Ok(s) => {
                self.session = Some(s);
                self.running = true;
                true
            }
            Err(e) => {
                self.last_error = e;
                false
            }
        }
    }

    /// Avance directement à une distance du parcours (essais).
    #[func]
    fn skip_to(&mut self, distance_m: f64) {
        if let Some(s) = self.session.as_mut() {
            s.ride.skip_to(distance_m);
            if let Some(p) = s.pack.as_mut() {
                p.skip_to(distance_m);
            }
        }
    }

    #[func]
    fn set_paused(&mut self, paused: bool) {
        if self.session.as_ref().is_some_and(|s| !s.ride.finished()) {
            self.running = !paused;
        }
    }

    #[func]
    fn is_running(&self) -> bool {
        self.running
    }

    /// Appui sur un Click (clavier).
    #[func]
    fn shift_up(&mut self) {
        if let Some(s) = self.session.as_mut() {
            s.shift(Shift::Up);
        }
    }

    #[func]
    fn shift_down(&mut self) {
        if let Some(s) = self.session.as_mut() {
            s.shift(Shift::Down);
        }
    }

    /// Active ou coupe les Click automatiques du cycliste virtuel.
    #[func]
    fn set_auto_shift(&mut self, enabled: bool) {
        if let Some(d) = self.session.as_mut().and_then(|s| s.sim_mut()) {
            d.settings.auto_shift = enabled.then_some(session::AUTO_SHIFT);
        }
    }

    /// Ajuste la consigne du cycliste virtuel (watts ou rpm selon le mode).
    #[func]
    fn adjust_rider(&mut self, delta: f64) {
        let Some(s) = self.session.as_mut() else {
            return;
        };
        s.sim_rider = match s.sim_rider {
            RiderModel::Power { watts, cadence_rpm } => RiderModel::Power {
                watts: (watts + delta).clamp(0.0, 1500.0),
                cadence_rpm,
            },
            RiderModel::Cadence { rpm } => RiderModel::Cadence {
                rpm: (rpm + delta / 5.0).clamp(40.0, 130.0),
            },
        };
        let rider = s.sim_rider;
        if s.workout.is_none() {
            if let Some(d) = s.sim_mut() {
                d.settings.rider = rider;
            }
        }
    }

    /// Réglage changé dans le menu pause : `grade_difficulty` (fraction),
    /// `rider_kg`, `erg`, `intensity` (fraction), `skip_step`, `w_per_kg`
    /// (coureurs virtuels), `test_value` (watts ou rpm du cycliste virtuel).
    #[func]
    fn adjust(&mut self, key: GString, value: Variant) {
        let Some(s) = self.session.as_mut() else {
            return;
        };
        let num = value
            .try_to::<f64>()
            .ok()
            .or_else(|| value.try_to::<i64>().ok().map(|i| i as f64));
        match (key.to_string().as_str(), num) {
            ("grade_difficulty", Some(v)) => {
                let v = v.clamp(0.0, 1.0);
                s.ride.set_grade_difficulty(v);
                self.profile.trainer.grade_difficulty = v;
            }
            ("rider_kg", Some(v)) => {
                let v = v.clamp(25.0, 200.0);
                s.ride.set_rider_weight(v);
                self.profile.rider.weight_kg = v;
            }
            ("intensity", Some(v)) => {
                if let Some(w) = s.workout.as_mut() {
                    w.intensity = v.clamp(0.3, 2.0);
                }
            }
            ("w_per_kg", Some(v)) => {
                if let Some(p) = s.pack.as_mut() {
                    p.set_all_w_per_kg(v.clamp(0.5, 8.0));
                }
            }
            ("test_value", Some(v)) => {
                s.sim_rider = match s.sim_rider {
                    RiderModel::Power { cadence_rpm, .. } => RiderModel::Power {
                        watts: v.clamp(0.0, 1500.0),
                        cadence_rpm,
                    },
                    RiderModel::Cadence { .. } => RiderModel::Cadence {
                        rpm: v.clamp(40.0, 130.0),
                    },
                };
                let rider = s.sim_rider;
                if s.workout.is_none() {
                    if let Some(d) = s.sim_mut() {
                        d.settings.rider = rider;
                    }
                }
            }
            ("erg", _) => {
                if let (Some(w), Ok(b)) = (s.workout.as_mut(), value.try_to::<bool>()) {
                    w.erg = b;
                }
            }
            ("skip_step", _) => {
                if let Some(w) = s.workout.as_mut() {
                    w.skip_step();
                }
            }
            _ => {}
        }
    }

    /// Réglages courants de la sortie, pour le menu pause.
    #[func]
    fn ride_state(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(s) = &self.session else {
            return d;
        };
        d.set("mode", s.mode.key());
        d.set("simulated", s.simulated());
        d.set("grade_difficulty", s.ride.profile.trainer.grade_difficulty);
        d.set("rider_kg", s.ride.profile.rider.weight_kg);
        if let Some(w) = &s.workout {
            let st = w.state();
            d.set("erg", w.erg);
            d.set("intensity", w.intensity);
            d.set("workout_name", w.workout.name.as_str());
            d.set("step_index", st.step_index as i64);
            d.set("step_count", st.step_count as i64);
        }
        if let Some(p) = &s.pack {
            let n = p.opponents.len().max(1) as f64;
            d.set(
                "w_per_kg",
                p.opponents.iter().map(|o| o.w_per_kg).sum::<f64>() / n,
            );
        }
        match s.sim_rider {
            RiderModel::Power { watts, .. } => {
                d.set("test_rider", "puissance");
                d.set("test_value", watts);
            }
            RiderModel::Cadence { rpm } => {
                d.set("test_rider", "cadence");
                d.set("test_value", rpm);
            }
        }
        d
    }

    /// Séances prédéfinies (voir `session::workout_to_dict` pour la forme).
    #[func]
    fn workout_presets(&self) -> VarArray {
        let mut out = VarArray::new();
        for w in velo_core::workout::presets() {
            out.push(&session::workout_to_dict(&w).to_variant());
        }
        out
    }

    /// Importe une séance Zwift (.zwo) ; dictionnaire vide en cas d'erreur.
    #[func]
    fn workout_from_zwo(&mut self, path: GString) -> VarDictionary {
        let real = ProjectSettings::singleton()
            .globalize_path(&path)
            .to_string();
        let result = std::fs::read_to_string(&real)
            .map_err(|e| format!("impossible d'ouvrir « {real} » : {e}"))
            .and_then(|t| Workout::from_zwo(&t).map_err(|e| e.to_string()));
        match result {
            Ok(w) => session::workout_to_dict(&w),
            Err(e) => {
                self.last_error = e;
                VarDictionary::new()
            }
        }
    }

    /// Coureurs virtuels : nom, écart au joueur (m, positif devant), W/kg,
    /// puissance, vitesse, cadence, position et cap en coordonnées Godot.
    #[func]
    fn opponents_state(&self) -> VarArray {
        let mut out = VarArray::new();
        let Some(s) = &self.session else {
            return out;
        };
        let Some(pack) = &s.pack else {
            return out;
        };
        let me = s.ride.telemetry().distance_m;
        for (i, o) in pack.opponents.iter().enumerate() {
            let mut d = VarDictionary::new();
            d.set("name", o.name.as_str());
            d.set("distance_m", o.distance_m);
            d.set("gap_m", o.distance_m - me);
            d.set("w_per_kg", o.w_per_kg);
            d.set("rider_kg", o.rider_kg);
            d.set("power_w", o.power_w);
            d.set("speed_kmh", o.speed_ms * 3.6);
            d.set("cadence_rpm", o.cadence_rpm);
            d.set("finished", o.finished);
            d.set("grade_pct", s.ride.route.grade_at(o.distance_m) * 100.0);
            // Chacun sa place dans la largeur de la route, à gauche du joueur
            // pour ne jamais le traverser quand il double ou se fait doubler.
            const LANES: [f64; 8] = [-1.0, -1.9, -1.45, -0.85, -2.2, -1.2, -1.7, -0.95];
            let lane = LANES[i % LANES.len()];
            match &self.path {
                Some(p) => {
                    let pose = p.rider_at(o.distance_m);
                    let (hx, hn) = pose.heading;
                    let l = hx.hypot(hn).max(1e-9);
                    let (rx, rn) = (hn / l, -hx / l);
                    let ahead = s.ride.route.point_at(o.distance_m + 4.0);
                    let behind = s.ride.route.point_at(o.distance_m - 4.0);
                    let slope = (ahead.elevation_m - behind.elevation_m) / 8.0;
                    d.set(
                        "position",
                        v3(pose.x + rx * lane, pose.elevation, -(pose.n + rn * lane)),
                    );
                    d.set("direction", v3(hx, slope, -hn));
                    let lean = (o.speed_ms * o.speed_ms * pose.curvature / 9.81).atan();
                    d.set("lean", lean.clamp(-0.7, 0.7));
                }
                None => {
                    let here = s.ride.route.point_at(o.distance_m);
                    let ahead = s.ride.route.point_at(o.distance_m + 4.0);
                    let behind = s.ride.route.point_at(o.distance_m - 4.0);
                    d.set("position", v3(here.x, here.elevation_m, -here.y));
                    d.set(
                        "direction",
                        v3(
                            ahead.x - behind.x,
                            ahead.elevation_m - behind.elevation_m,
                            -(ahead.y - behind.y),
                        ),
                    );
                    d.set("lean", 0.0);
                }
            }
            out.push(&d.to_variant());
        }
        out
    }

    /// Termine la sortie : le home trainer revient à plat, puis la sortie est
    /// enregistrée dans l'historique si `save`. Renvoie son identifiant (−1
    /// sans enregistrement ou en cas d'erreur, voir `error_message()`).
    #[func]
    fn finish_ride(&mut self, save: bool) -> i64 {
        self.running = false;
        let Some(mut s) = self.session.take() else {
            return -1;
        };
        s.finish();
        if !save {
            return -1;
        }
        let route_name = self
            .route
            .as_ref()
            .and_then(|r| r.name.clone())
            .unwrap_or_else(|| "Parcours".into());
        match history::save_session(&s, &self.profile, &route_name) {
            Ok(id) => id,
            Err(e) => {
                self.last_error = e;
                -1
            }
        }
    }

    /// Télémétrie courante, position et cap sur le parcours.
    #[func]
    fn telemetry(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(session) = &self.session else {
            return d;
        };
        let ride = &session.ride;
        let t = ride.telemetry();
        let here = ride.route.point_at(t.distance_m);
        let ahead = ride.route.point_at(t.distance_m + 4.0);
        let behind = ride.route.point_at(t.distance_m - 4.0);
        let slope = (ahead.elevation_m - behind.elevation_m) / 8.0;
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
        match &self.path {
            // Sur sa trajectoire : à droite, à la corde quand ça ne monte pas.
            Some(p) => {
                let pose = p.rider_at(t.distance_m);
                d.set("position", v3(pose.x, pose.elevation, -pose.n));
                d.set("direction", v3(pose.heading.0, slope, -pose.heading.1));
                // Inclinaison d'équilibre en virage : tan(φ) = v² κ / g.
                let lean = (t.speed_ms * t.speed_ms * pose.curvature / 9.81).atan();
                d.set("lean", lean.clamp(-0.7, 0.7));
            }
            None => {
                d.set("position", v3(here.x, here.elevation_m, -here.y));
                d.set(
                    "direction",
                    v3(
                        ahead.x - behind.x,
                        ahead.elevation_m - behind.elevation_m,
                        -(ahead.y - behind.y),
                    ),
                );
                d.set("lean", 0.0);
            }
        }
        d.set("finished", t.finished);
        d.set("mode", session.mode.key());
        if let Some(w) = &session.workout {
            let st = w.state();
            let mut x = VarDictionary::new();
            x.set("name", w.workout.name.as_str());
            x.set("step_index", st.step_index as i64);
            x.set("step_count", st.step_count as i64);
            x.set("step_elapsed_s", st.step_elapsed_s);
            x.set("step_remaining_s", st.step_remaining_s);
            x.set("target_w", st.target_w.unwrap_or(-1.0));
            x.set("next_target_w", st.next_target_w.unwrap_or(-1.0));
            x.set("cadence_rpm", st.cadence_rpm.unwrap_or(0.0));
            x.set("total_elapsed_s", st.total_elapsed_s);
            x.set("total_s", st.total_s);
            x.set("erg", w.erg);
            x.set("intensity", w.intensity);
            x.set("finished", w.finished());
            d.set("workout", &x);
        }
        d
    }
}
