//! Construction du décor : relief, textures, arbres, bâtiments, eau, routes.

use crate::elevation::{smooth_profile, Dem};
use crate::geo::{mercator_px_x, mercator_px_y, meters_per_px, zoom_for, LocalProjection};
use crate::mesh::{building, flat_polygon, ribbon, terrain_grid, Instances, MeshData};
use crate::osm::{Cover, LineKind, OsmData};
use crate::raster::{cover_grid, ground_color, looks_like_trees, noise, rand01};
use crate::road::{RoadPath, HALF_WIDTH, VERGE};
use crate::tiles::{TileCache, TileSource};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use velo_core::config::Scenery;
use velo_core::route::Route;
use velo_core::terrain::Terrain;

/// Côté d'un carré proche (détaillé).
pub const NEAR_SIZE: f64 = 512.0;
/// Côté d'un grand carré (lointain).
pub const FAR_SIZE: f64 = 2048.0;
/// Zoom des tuiles d'altitude (≈ 13 m par pixel dans les Alpes).
const DEM_ZOOM: u8 = 13;
/// Distance à l'axe en deçà de laquelle le sol rejoint la chaussée.
const ROAD_BLEND_M: f64 = 28.0;
/// Plateforme plane sous la chaussée et les accotements (demi-largeur, m).
const ROAD_FLAT_M: f64 = HALF_WIDTH + VERGE + 0.7;
/// Pas du parcours (m), identique à celui de velo-godot.
const ROUTE_STEP_M: f64 = 5.0;

/// Réglages qui dépendent du niveau de qualité choisi.
#[derive(Debug, Clone, Copy)]
pub struct Quality {
    /// Décor lointain jusqu'à cette distance du parcours.
    pub far_margin_m: f64,
    /// Carrés détaillés jusqu'à cette distance du parcours.
    pub near_margin_m: f64,
    pub far_px: usize,
    pub near_px: usize,
    pub far_cells: usize,
    pub near_cells: usize,
    /// Arbres jusqu'à cette distance du parcours, un par case de `tree_cell_m`.
    pub tree_radius_m: f64,
    pub tree_cell_m: f64,
}

impl Quality {
    pub fn level(q: u8) -> Self {
        match q {
            0 => Self {
                far_margin_m: 2500.0,
                near_margin_m: 600.0,
                far_px: 256,
                near_px: 256,
                far_cells: 32,
                near_cells: 96,
                tree_radius_m: 220.0,
                tree_cell_m: 9.0,
            },
            1 => Self {
                far_margin_m: 4000.0,
                near_margin_m: 900.0,
                far_px: 512,
                near_px: 512,
                far_cells: 48,
                near_cells: 128,
                tree_radius_m: 350.0,
                tree_cell_m: 7.0,
            },
            _ => Self {
                far_margin_m: 6000.0,
                near_margin_m: 1300.0,
                far_px: 512,
                near_px: 1024,
                far_cells: 64,
                near_cells: 128,
                tree_radius_m: 500.0,
                tree_cell_m: 5.5,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorldOptions {
    pub quality: u8,
    pub online: bool,
    pub use_dem_elevation: bool,
    pub elevation_url: String,
    pub imagery_url: String,
    pub overpass_url: String,
    pub cache_dir: PathBuf,
    /// Fichier OSM (JSON Overpass) à utiliser au lieu d'interroger Overpass.
    pub osm_file: Option<PathBuf>,
}

impl WorldOptions {
    pub fn from_scenery(s: &Scenery) -> Self {
        Self {
            quality: s.quality.min(2),
            online: s.online,
            use_dem_elevation: s.use_dem_elevation,
            elevation_url: s.elevation_url.clone(),
            imagery_url: s.imagery_url.clone(),
            overpass_url: s.overpass_url.clone(),
            cache_dir: if s.cache_dir.is_empty() {
                TileCache::default_root()
            } else {
                PathBuf::from(&s.cache_dir)
            },
            osm_file: None,
        }
    }
}

/// Image RGBA8 avec toute sa chaîne de mipmaps (format attendu par Godot).
#[derive(Debug, Clone, Default)]
pub struct Texture {
    pub size: usize,
    pub data: Vec<u8>,
    /// Part des pixels venant de l'image aérienne (0 à 1).
    pub imagery_share: f32,
}

#[derive(Debug, Clone)]
pub struct FarChunk {
    /// Centre (x est, n nord) dans le repère local.
    pub center: (f64, f64),
    pub mesh: MeshData,
    pub texture: Texture,
    /// Bâtiments vus de loin (cachés là où un carré détaillé est chargé).
    pub buildings: MeshData,
}

#[derive(Debug, Clone, Copy)]
pub struct NearChunk {
    pub center: (f64, f64),
}

/// Contenu détaillé d'un carré proche, prêt à être donné à Godot.
#[derive(Debug, Clone, Default)]
pub struct ChunkDetail {
    pub index: usize,
    pub terrain: MeshData,
    pub texture: Texture,
    pub conifers: Instances,
    pub broadleaves: Instances,
    pub bushes: Instances,
    pub buildings: MeshData,
    pub roads: MeshData,
    pub water: MeshData,
}

pub struct World {
    pub proj: LocalProjection,
    /// Parcours avec l'altitude du relief si demandé.
    pub route: Route,
    /// Tracé lissé de la route, tous les mètres.
    pub path: RoadPath,
    /// OpenStreetMap a-t-il fourni des données ?
    pub osm_ok: bool,
    pub quality: Quality,
    pub base_elevation: f64,
    pub far: Vec<FarChunk>,
    pub near: Vec<NearChunk>,
    pub credits: String,
    /// Résumé lisible de ce qui a été trouvé (relief, images, OSM).
    pub summary: String,
    dem: Dem,
    index: Terrain,
    osm: OsmData,
    cache: TileCache,
    imagery: TileSource,
    near_zoom: u8,
    far_zoom: u8,
    fallback_elevation: f64,
}

type Progress<'a> = &'a (dyn Fn(f32, &str) + Sync);

impl World {
    /// Construit le décor (à lancer dans un fil à part) : télécharge le relief,
    /// les données OSM et les images lointaines, puis prépare les grands carrés.
    pub fn build(
        mut route: Route,
        opts: &WorldOptions,
        progress: Progress,
        cancel: &AtomicBool,
    ) -> Result<World, String> {
        let quality = Quality::level(opts.quality);
        let proj = LocalProjection::for_route(&route);
        let cache = TileCache::new(opts.cache_dir.clone(), opts.online);
        let lat = proj.latitude();

        // 1. Relief.
        progress(0.0, "Relief : recherche des tuiles d'altitude…");
        let elevation = TileSource::new(
            &source_name("relief", &opts.elevation_url),
            &opts.elevation_url,
        );
        let tiles = dem_tiles(&route, &proj, quality.far_margin_m + FAR_SIZE * 0.75);
        let bytes = cache.fetch_all(&elevation, &tiles, 8, &|done, total| {
            progress(
                0.25 * done as f32 / total as f32,
                &format!("Relief : {done}/{total} tuiles d'altitude"),
            )
        });
        if cancel.load(Ordering::Relaxed) {
            return Err("annulé".into());
        }
        let mut dem = Dem::new(DEM_ZOOM);
        for ((x, y), b) in &bytes {
            dem.insert_png(*x, *y, b);
        }
        if dem.tile_count() == 0 {
            return Err(if opts.online {
                "relief introuvable : vérifie la connexion internet".into()
            } else {
                "relief absent du cache : active le téléchargement une première fois".into()
            });
        }

        // 2. Altitude du parcours tirée du relief, lissée.
        let gpx_has_elevation = route.points.iter().any(|p| p.elevation_m != 0.0);
        if opts.use_dem_elevation || !gpx_has_elevation {
            let raw: Vec<f64> = route
                .points
                .iter()
                .map(|p| dem.sample(p.lat, p.lon).unwrap_or(p.elevation_m))
                .collect();
            for (p, e) in route
                .points
                .iter_mut()
                .zip(smooth_profile(&raw, ROUTE_STEP_M))
            {
                p.elevation_m = e;
            }
            route.recompute_grades(40.0);
        }
        let fallback_elevation =
            route.points.iter().map(|p| p.elevation_m).sum::<f64>() / route.points.len() as f64;
        let base_elevation = route.points[0].elevation_m;
        let path = RoadPath::new(&route);
        let index = Terrain::from_points(path.axis_points(2));

        // 3. OpenStreetMap.
        progress(0.27, "OpenStreetMap : forêts, eau, bâtiments, routes…");
        let (osm, osm_note) = load_osm(
            opts,
            &cache,
            &route,
            &proj,
            quality.near_margin_m,
            quality.far_margin_m.min(2500.0),
        );
        if cancel.load(Ordering::Relaxed) {
            return Err("annulé".into());
        }

        let imagery = TileSource::new(&source_name("images", &opts.imagery_url), &opts.imagery_url);
        let near_zoom = zoom_for(lat, NEAR_SIZE / quality.near_px as f64 * 1.15, 19);
        let far_zoom = zoom_for(lat, FAR_SIZE / quality.far_px as f64 * 1.15, 19);

        let mut world = World {
            proj,
            route,
            path,
            osm_ok: !osm.is_empty(),
            quality,
            base_elevation,
            far: Vec::new(),
            near: Vec::new(),
            credits: String::new(),
            summary: String::new(),
            dem,
            index,
            osm,
            cache,
            imagery,
            near_zoom,
            far_zoom,
            fallback_elevation,
        };
        world.near = chunk_centers(&world.route, NEAR_SIZE, quality.near_margin_m)
            .into_iter()
            .map(|center| NearChunk { center })
            .collect();

        // 4. Grands carrés lointains, en parallèle.
        let centers = chunk_centers(&world.route, FAR_SIZE, quality.far_margin_m);
        let next = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        let out = Mutex::new(Vec::with_capacity(centers.len()));
        let threads = std::thread::available_parallelism()
            .map_or(4, |n| n.get())
            .clamp(2, 8);
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&center) = centers.get(i) else { break };
                    let chunk = world.far_chunk(center);
                    out.lock().unwrap().push((i, chunk));
                    let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                    progress(
                        0.35 + 0.6 * d as f32 / centers.len() as f32,
                        &format!("Paysage lointain : {d}/{}", centers.len()),
                    );
                });
            }
        });
        if cancel.load(Ordering::Relaxed) {
            return Err("annulé".into());
        }
        let mut far = out.into_inner().unwrap();
        far.sort_by_key(|(i, _)| *i);
        world.far = far.into_iter().map(|(_, c)| c).collect();

        let share = world
            .far
            .iter()
            .map(|c| c.texture.imagery_share)
            .sum::<f32>()
            / world.far.len().max(1) as f32;
        world.credits = credits(opts, share > 0.0, !world.osm.is_empty());
        world.summary = format!(
            "relief {} tuiles · images aériennes {} % · {} · {} grands carrés, {} carrés détaillés",
            world.dem.tile_count(),
            (share * 100.0).round(),
            osm_note,
            world.far.len(),
            world.near.len()
        );
        progress(1.0, "Décor prêt");
        Ok(world)
    }

    /// Altitude brute du relief en (x est, n nord).
    pub fn dem_height(&self, x: f64, n: f64) -> f64 {
        let (lat, lon) = self.proj.to_lat_lon(x, n);
        self.dem.sample(lat, lon).unwrap_or(self.fallback_elevation)
    }

    /// Altitude du sol : le relief, aplani sous la chaussée près du parcours.
    pub fn ground(&self, x: f64, n: f64) -> f64 {
        let dem = self.dem_height(x, n);
        match self.index.nearest(x, n, ROAD_BLEND_M) {
            Some((d, road)) => {
                let w = 1.0 - smoothstep(ROAD_FLAT_M, ROAD_BLEND_M, d);
                dem + (road - 0.35 - dem) * w
            }
            None => dem,
        }
    }

    /// Distance horizontale au parcours, plafonnée à `max_m`.
    pub fn distance_to_route(&self, x: f64, n: f64, max_m: f64) -> f64 {
        self.index.nearest(x, n, max_m).map_or(max_m, |(d, _)| d)
    }

    fn far_chunk(&self, center: (f64, f64)) -> FarChunk {
        let q = &self.quality;
        let half = FAR_SIZE / 2.0;
        let step = FAR_SIZE / q.far_cells as f64;
        let (cx, cn) = center;
        let mesh = terrain_grid(
            FAR_SIZE,
            q.far_cells,
            &|i, j| {
                self.ground(cx - half + i as f64 * step, cn + half - j as f64 * step)
                    - self.base_elevation
            },
            0.0,
        );
        let (rgb, from_img) = self.compose(cx - half, cn + half, FAR_SIZE, q.far_px, self.far_zoom);
        FarChunk {
            center,
            mesh,
            texture: texture_from(&rgb, &from_img, q.far_px),
            buildings: self.buildings_in(center, half),
        }
    }

    /// Génère le contenu détaillé du carré proche `index`.
    pub fn near_detail(&self, index: usize) -> ChunkDetail {
        let q = self.quality;
        let (cx, cn) = self.near[index].center;
        let half = NEAR_SIZE / 2.0;
        let base = self.base_elevation;
        let step = NEAR_SIZE / q.near_cells as f64;
        let terrain = terrain_grid(
            NEAR_SIZE,
            q.near_cells,
            &|i, j| self.ground(cx - half + i as f64 * step, cn + half - j as f64 * step) - base,
            12.0,
        );
        let (mut rgb, from_img) =
            self.compose(cx - half, cn + half, NEAR_SIZE, q.near_px, self.near_zoom);
        self.paint_verges(&mut rgb, cx - half, cn + half, q.near_px);
        let image_px = |x: f64, n: f64| {
            let i = ((x - (cx - half)) / NEAR_SIZE * q.near_px as f64) as usize;
            let j = (((cn + half) - n) / NEAR_SIZE * q.near_px as f64) as usize;
            let k = j.min(q.near_px - 1) * q.near_px + i.min(q.near_px - 1);
            from_img[k].then_some(rgb[k])
        };
        let covers = cover_grid(&self.osm, cx - half, cn + half, NEAR_SIZE, 256);
        let local = |x: f64, y: f64, n: f64| [x - cx, y - base, -(n - cn)];
        // Sol en coordonnées relatives Godot (x, z) → hauteur relative.
        let rel_ground = |x: f64, z: f64| self.ground(cx + x, cn - z) - base;

        let mut detail = ChunkDetail {
            index,
            terrain,
            texture: texture_from(&rgb, &from_img, q.near_px),
            ..Default::default()
        };

        // Arbres : cases fixes du repère global, donc stables d'une sortie à l'autre.
        if self.distance_to_route(cx, cn, q.tree_radius_m + half * 1.5)
            < q.tree_radius_m + half * 1.5
        {
            let cell = q.tree_cell_m;
            let (x0, x1, n0, n1) = (cx - half, cx + half, cn - half, cn + half);
            for gy in (n0 / cell).floor() as i64..=(n1 / cell).ceil() as i64 {
                for gx in (x0 / cell).floor() as i64..=(x1 / cell).ceil() as i64 {
                    let x = (gx as f64 + rand01(gx, gy, 1)) * cell;
                    let n = (gy as f64 + rand01(gx, gy, 2)) * cell;
                    if x < x0 || x >= x1 || n < n0 || n >= n1 {
                        continue;
                    }
                    let Some((d, _)) = self.index.nearest(x, n, q.tree_radius_m) else {
                        continue;
                    };
                    if d < 8.0 {
                        continue;
                    }
                    let cover = Cover::from_u8(covers.at(x, n).unwrap_or(0));
                    if cover.blocks_trees() {
                        continue;
                    }
                    let photo = image_px(x, n);
                    let photo_trees = photo.map(looks_like_trees);
                    let p = if cover.is_forest() {
                        if photo_trees == Some(false) {
                            0.3
                        } else {
                            0.85
                        }
                    } else if photo_trees == Some(true) {
                        if cover == Cover::Urban {
                            0.25
                        } else {
                            0.7
                        }
                    } else if cover == Cover::Scrub {
                        0.12
                    } else {
                        0.004
                    };
                    let r = rand01(gx, gy, 4);
                    if r >= p {
                        continue;
                    }
                    let y = self.ground(x, n);
                    let conifer_p = match cover {
                        Cover::ForestNeedle => 0.97,
                        Cover::ForestBroad => 0.05,
                        Cover::ForestMixed => 0.5,
                        _ if y > 1400.0 => 0.85,
                        _ if y > 900.0 => 0.5,
                        _ => 0.2,
                    };
                    let size = 0.75 + 0.5 * rand01(gx, gy, 5);
                    let shade = 0.82 + 0.3 * rand01(gx, gy, 6) as f32;
                    let yaw = rand01(gx, gy, 7) * std::f64::consts::TAU;
                    let pos = local(x, y - 0.3, n);
                    if rand01(gx, gy, 8) < conifer_p {
                        let h = 14.0 * size;
                        detail.conifers.push(
                            pos,
                            yaw,
                            h * 0.38,
                            h,
                            [shade * 0.9, shade, shade * 0.9, 1.0],
                        );
                    } else {
                        let h = 10.0 * size;
                        detail.broadleaves.push(
                            pos,
                            yaw,
                            h * 0.75,
                            h,
                            [shade, shade, shade * 0.85, 1.0],
                        );
                    }
                }
            }
        }

        detail.buildings = self.buildings_in((cx, cn), half);
        detail.bushes = self.bushes_in((cx, cn), half, &covers, &image_px);
        let inside = |p: (f64, f64)| (p.0 - cx).abs() <= half && (p.1 - cn).abs() <= half;

        // Routes et rivières drapées sur le sol, plans d'eau.
        let (min, max) = ((cx - half, cn - half), (cx + half, cn + half));
        for l in self
            .osm
            .lines
            .iter()
            .filter(|l| l.bbox.intersects(min, max))
        {
            let (mesh, lift, color, keep_off_route) = match l.kind {
                LineKind::Road { paved, major } => (
                    &mut detail.roads,
                    if major { 0.14 } else { 0.11 },
                    if paved {
                        [0.27, 0.27, 0.28, 1.0]
                    } else {
                        [0.50, 0.44, 0.36, 1.0]
                    },
                    true,
                ),
                LineKind::Waterway => (&mut detail.water, 0.2, [0.15, 0.27, 0.33, 1.0], false),
                LineKind::Path => continue,
            };
            let clear = 3.5 + l.width_m / 2.0;
            for run in runs_inside(&l.points, 6.0, &inside, &|p: (f64, f64)| {
                !keep_off_route || self.distance_to_route(p.0, p.1, clear) >= clear
            }) {
                let rel: Vec<(f64, f64)> = run.iter().map(|&(x, n)| (x - cx, -(n - cn))).collect();
                ribbon(mesh, &rel, l.width_m, lift, &rel_ground, color);
            }
        }
        for a in &self.osm.areas {
            if a.cover != Cover::Water || !a.still_water || !inside(a.bbox.center()) {
                continue;
            }
            let level = a
                .rings
                .iter()
                .flatten()
                .map(|&(x, n)| self.ground(x, n))
                .fold(f64::MAX, f64::min);
            let rings: Vec<Vec<(f64, f64)>> = a
                .rings
                .iter()
                .map(|r| r.iter().map(|&(x, n)| (x - cx, -(n - cn))).collect())
                .collect();
            flat_polygon(
                &mut detail.water,
                &rings,
                level + 0.3 - base,
                [0.15, 0.27, 0.33, 1.0],
            );
        }
        detail
    }

    /// Bâtiments OSM dont le centre tombe dans le carré, relatifs à son centre.
    fn buildings_in(&self, center: (f64, f64), half: f64) -> MeshData {
        let (cx, cn) = center;
        let base = self.base_elevation;
        let mut mesh = MeshData::default();
        let walls = [
            [0.86, 0.83, 0.76, 1.0],
            [0.92, 0.90, 0.86, 1.0],
            [0.80, 0.70, 0.56, 1.0],
            [0.62, 0.48, 0.34, 1.0],
            [0.74, 0.72, 0.68, 1.0],
        ];
        let roofs = [
            [0.48, 0.24, 0.18, 1.0],
            [0.30, 0.30, 0.32, 1.0],
            [0.40, 0.30, 0.24, 1.0],
            [0.22, 0.22, 0.24, 1.0],
        ];
        for b in &self.osm.buildings {
            let c = b.bbox.center();
            if (c.0 - cx).abs() > half || (c.1 - cn).abs() > half {
                continue;
            }
            // Emprise mal placée sur la chaussée : on l'ignore.
            if self.distance_to_route(c.0, c.1, ROAD_FLAT_M) < HALF_WIDTH + 1.0 {
                continue;
            }
            let heights: Vec<f64> = b.ring.iter().map(|&(x, n)| self.ground(x, n)).collect();
            let min = heights.iter().copied().fold(f64::MAX, f64::min);
            let mean = heights.iter().sum::<f64>() / heights.len() as f64;
            let ring: Vec<(f64, f64)> = b.ring.iter().map(|&(x, n)| (x - cx, -(n - cn))).collect();
            let r = |k: i64| rand01(b.id as i64, 11, k) as f32;
            let wall = walls[(r(1) * walls.len() as f32) as usize % walls.len()];
            let roof = roofs[(r(2) * roofs.len() as f32) as usize % roofs.len()];
            building(
                &mut mesh,
                &ring,
                min - 0.6 - base,
                mean + b.height_m - base,
                wall,
                roof,
            );
        }
        mesh
    }

    /// Buissons et arbustes au bord de la route, hors bâtiments, eau et routes.
    fn bushes_in(
        &self,
        center: (f64, f64),
        half: f64,
        covers: &crate::raster::Grid<u8>,
        photo: &dyn Fn(f64, f64) -> Option<[u8; 3]>,
    ) -> Instances {
        let (cx, cn) = center;
        let base = self.base_elevation;
        let mut out = Instances::default();
        let edge = HALF_WIDTH + VERGE;
        for (k, p) in self.path.samples.iter().enumerate() {
            if (p.x - cx).abs() > half + 8.0 || (p.n - cn).abs() > half + 8.0 {
                continue;
            }
            let (rx, rn) = p.right();
            for (side, sign) in [(0i64, -1.0), (1, 1.0)] {
                let key = k as i64 * 2 + side;
                let r = |c: i64| rand01(key, 77, c);
                let off = sign * (edge + 0.5 + r(1) * 2.8);
                let along = (r(2) - 0.5) * 0.8;
                let x = p.x + rx * off + p.tangent.0 * along;
                let n = p.n + rn * off + p.tangent.1 * along;
                if (x - cx).abs() > half || (n - cn).abs() > half {
                    continue;
                }
                // Une autre partie du parcours (lacet) passe tout près : pas de buisson.
                if self.distance_to_route(x, n, edge + 0.4) < edge + 0.3 {
                    continue;
                }
                let cover = Cover::from_u8(covers.at(x, n).unwrap_or(0));
                if cover.blocks_trees() {
                    continue;
                }
                let mut density = match cover {
                    Cover::Urban => 0.06,
                    c if c.is_forest() => 0.45,
                    Cover::Scrub => 0.5,
                    Cover::Rock | Cover::Glacier => 0.05,
                    _ => 0.28,
                };
                // Haies et fourrés par endroits plutôt qu'au hasard régulier.
                density *= 0.4 + 1.2 * noise(p.s * 0.02, sign * 7.0);
                if let Some([pr, pg, pb]) = photo(x, n) {
                    let green = 2 * i32::from(pg) - i32::from(pr) - i32::from(pb);
                    if green < 0 {
                        density *= 0.3;
                    }
                }
                if r(3) >= density {
                    continue;
                }
                let h = 0.6 + r(4) * 1.1;
                let w = h * (1.2 + r(5) * 0.8);
                let shade = 0.8 + 0.35 * r(6) as f32;
                let y = self.ground(x, n);
                out.push(
                    [x - cx, y - base - 0.15, -(n - cn)],
                    r(7) * std::f64::consts::TAU,
                    w,
                    h,
                    [shade * 0.95, shade, shade * 0.8, 1.0],
                );
            }
        }
        out
    }

    /// Près de la route, l'image aérienne (floue, et dont la vraie route ne
    /// tombe jamais exactement sous la nôtre) cède la place à un accotement
    /// herbeux qui se fond dans l'image à une dizaine de mètres.
    fn paint_verges(&self, rgb: &mut [[u8; 3]], x0: f64, n1: f64, size: usize) {
        let px = NEAR_SIZE / size as f64;
        let reach = 11.0;
        let mut near_road: crate::raster::Grid<u8> =
            crate::raster::Grid::new(x0, n1, NEAR_SIZE, size, 0);
        for w in self.path.samples.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            if (a.x - x0 - NEAR_SIZE / 2.0).abs() > NEAR_SIZE / 2.0 + reach
                || (n1 - NEAR_SIZE / 2.0 - a.n).abs() > NEAR_SIZE / 2.0 + reach
            {
                continue;
            }
            near_road.draw_line(&[(a.x, a.n), (b.x, b.n)], reach * 2.0, 1);
        }
        for j in 0..size {
            for i in 0..size {
                let k = j * size + i;
                if near_road.data[k] == 0 {
                    continue;
                }
                let (x, n) = (x0 + (i as f64 + 0.5) * px, n1 - (j as f64 + 0.5) * px);
                let d = self.distance_to_route(x, n, reach);
                let w = 1.0 - smoothstep(ROAD_FLAT_M, reach, d);
                if w <= 0.0 {
                    continue;
                }
                let v = noise(x * 0.35, n * 0.35) * 0.6 + noise(x * 1.7, n * 1.7) * 0.4;
                let verge = [0.34 + 0.10 * v, 0.40 + 0.08 * v, 0.20 + 0.04 * v];
                let c = &mut rgb[k];
                for ch in 0..3 {
                    let img = f64::from(c[ch]) / 255.0;
                    c[ch] = ((img + (verge[ch] - img) * w) * 255.0).round() as u8;
                }
            }
        }
    }

    /// Couleurs d'un carré du sol vu du ciel : image aérienne quand elle
    /// existe, sinon couleur déduite de l'altitude, de la pente et d'OSM.
    fn compose(
        &self,
        x0: f64,
        n1: f64,
        extent: f64,
        size: usize,
        zoom: u8,
    ) -> (Vec<[u8; 3]>, Vec<bool>) {
        let px = extent / size as f64;
        let mut rgb = vec![[0u8; 3]; size * size];
        let mut from_img = vec![false; size * size];
        // Zoom demandé d'abord, puis jusqu'à trois niveaux moins fins pour
        // combler les trous (bord de couverture, tuiles pas encore publiées).
        for zoom in (zoom.saturating_sub(3)..=zoom).rev() {
            if !self.imagery.is_enabled() || from_img.iter().all(|&b| b) {
                break;
            }
            let gx: Vec<f64> = (0..size)
                .map(|i| mercator_px_x(self.proj.lon_at(x0 + (i as f64 + 0.5) * px), zoom))
                .collect();
            let gy: Vec<f64> = (0..size)
                .map(|j| mercator_px_y(self.proj.lat_at(n1 - (j as f64 + 0.5) * px), zoom))
                .collect();
            let (tx0, tx1) = (
                (gx[0] / 256.0).floor() as u32,
                (gx[size - 1] / 256.0 + 1.0) as u32,
            );
            let (ty0, ty1) = (
                (gy[0] / 256.0).floor() as u32,
                (gy[size - 1] / 256.0 + 1.0) as u32,
            );
            let mut tiles: HashMap<(u32, u32), image::RgbImage> = HashMap::new();
            for ty in ty0..=ty1 {
                for tx in tx0..=tx1 {
                    if let Some(img) = self
                        .cache
                        .get(&self.imagery, zoom, tx, ty)
                        .and_then(|b| decode_photo(&b))
                    {
                        tiles.insert((tx, ty), img);
                    }
                }
            }
            if !tiles.is_empty() {
                let sample = |x: f64, y: f64| -> Option<[u8; 3]> {
                    let (fx, fy) = (x - 0.5, y - 0.5);
                    let (ix, iy) = (fx.floor() as i64, fy.floor() as i64);
                    let (tx, ty) = ((fx - ix as f64) as f32, (fy - iy as f64) as f32);
                    let get = |px: i64, py: i64| {
                        let px = px.max(0);
                        let py = py.max(0);
                        tiles
                            .get(&((px >> 8) as u32, (py >> 8) as u32))
                            .map(|t| t.get_pixel((px & 255) as u32, (py & 255) as u32).0)
                    };
                    let c00 = get(ix, iy)?;
                    let c10 = get(ix + 1, iy).unwrap_or(c00);
                    let c01 = get(ix, iy + 1).unwrap_or(c00);
                    let c11 = get(ix + 1, iy + 1).unwrap_or(c10);
                    let mut out = [0u8; 3];
                    for k in 0..3 {
                        let a = f32::from(c00[k]) + (f32::from(c10[k]) - f32::from(c00[k])) * tx;
                        let b = f32::from(c01[k]) + (f32::from(c11[k]) - f32::from(c01[k])) * tx;
                        out[k] = (a + (b - a) * ty).round() as u8;
                    }
                    Some(out)
                };
                for j in 0..size {
                    for i in 0..size {
                        if from_img[j * size + i] {
                            continue;
                        }
                        if let Some(c) = sample(gx[i], gy[j]) {
                            rgb[j * size + i] = c;
                            from_img[j * size + i] = true;
                        }
                    }
                }
            }
        }
        if from_img.iter().all(|&b| b) {
            return (rgb, from_img);
        }
        // Repli : relief échantillonné tous les ~8 m (altitude et pente
        // interpolées), occupation OSM au pixel.
        let covers = cover_grid(&self.osm, x0, n1, extent, size);
        let coarse = (size / ((8.0 / px).max(1.0) as usize)).max(2);
        let cstep = extent / coarse as f64;
        let w = coarse + 3;
        let mut h = vec![0.0; w * w];
        for j in 0..w {
            for i in 0..w {
                h[j * w + i] =
                    self.dem_height(x0 + (i as f64 - 1.0) * cstep, n1 - (j as f64 - 1.0) * cstep);
            }
        }
        // Altitude et pente à chaque nœud (différences centrées).
        let nodes = coarse + 1;
        let mut alt = vec![0.0; nodes * nodes];
        let mut slope = vec![0.0; nodes * nodes];
        for j in 0..nodes {
            for i in 0..nodes {
                let at = |a: usize, b: usize| h[b * w + a];
                let dx = at(i + 2, j + 1) - at(i, j + 1);
                let dn = at(i + 1, j + 2) - at(i + 1, j);
                alt[j * nodes + i] = at(i + 1, j + 1);
                slope[j * nodes + i] = dx.hypot(dn) / (2.0 * cstep);
            }
        }
        let lerp2 = |v: &[f64], fx: f64, fy: f64| {
            let (i, j) = ((fx as usize).min(coarse - 1), (fy as usize).min(coarse - 1));
            let (tx, ty) = (fx - i as f64, fy - j as f64);
            let a = v[j * nodes + i] + (v[j * nodes + i + 1] - v[j * nodes + i]) * tx;
            let b =
                v[(j + 1) * nodes + i] + (v[(j + 1) * nodes + i + 1] - v[(j + 1) * nodes + i]) * tx;
            a + (b - a) * ty
        };
        for j in 0..size {
            for i in 0..size {
                let k = j * size + i;
                if from_img[k] {
                    continue;
                }
                let (x, n) = covers.center(i, j);
                let (fx, fy) = ((x - x0) / cstep, (n1 - n) / cstep);
                rgb[k] = ground_color(
                    Cover::from_u8(covers.data[k]),
                    lerp2(&alt, fx, fy),
                    lerp2(&slope, fx, fy),
                    x,
                    n,
                );
            }
        }
        (rgb, from_img)
    }
}

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Décode une tuile aérienne ; une tuile uniforme (hors couverture) est ignorée.
fn decode_photo(bytes: &[u8]) -> Option<image::RgbImage> {
    let img = image::load_from_memory(bytes).ok()?.to_rgb8();
    if img.width() != 256 || img.height() != 256 {
        return None;
    }
    let first = img.get_pixel(0, 0).0;
    let uniform = (0..64).all(|k| {
        let p = img.get_pixel((k * 37) % 256, (k * 91) % 256).0;
        (0..3).all(|c| p[c].abs_diff(first[c]) < 3)
    });
    (!uniform).then_some(img)
}

/// Texture RGBA avec mipmaps (moyenne 2×2 à chaque niveau).
fn texture_from(rgb: &[[u8; 3]], from_img: &[bool], size: usize) -> Texture {
    let mut level: Vec<[u8; 4]> = rgb.iter().map(|c| [c[0], c[1], c[2], 255]).collect();
    let mut data = Vec::with_capacity(size * size * 4 * 4 / 3 + 16);
    let mut s = size;
    loop {
        data.extend(level.iter().flatten());
        if s == 1 {
            break;
        }
        let h = (s / 2).max(1);
        let mut next = vec![[0u8; 4]; h * h];
        for j in 0..h {
            for i in 0..h {
                let mut acc = [0u32; 4];
                for (di, dj) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = level[(j * 2 + dj).min(s - 1) * s + (i * 2 + di).min(s - 1)];
                    for c in 0..4 {
                        acc[c] += u32::from(p[c]);
                    }
                }
                next[j * h + i] = acc.map(|v| ((v + 2) / 4) as u8);
            }
        }
        level = next;
        s = h;
    }
    Texture {
        size,
        data,
        imagery_share: from_img.iter().filter(|&&b| b).count() as f32
            / from_img.len().max(1) as f32,
    }
}

/// Centres des carrés de côté `size` à moins de `margin` du parcours.
fn chunk_centers(route: &Route, size: f64, margin: f64) -> Vec<(f64, f64)> {
    let reach = (margin / size).ceil() as i64 + 1;
    let mut keys = BTreeSet::new();
    for p in route.points.iter().step_by(4) {
        let (kx, kn) = ((p.x / size).floor() as i64, (p.y / size).floor() as i64);
        for dx in -reach..=reach {
            for dn in -reach..=reach {
                let (ix, in_) = (kx + dx, kn + dn);
                // Distance du point au carré.
                let x0 = ix as f64 * size;
                let n0 = in_ as f64 * size;
                let ddx = (x0 - p.x).max(0.0).max(p.x - (x0 + size));
                let ddn = (n0 - p.y).max(0.0).max(p.y - (n0 + size));
                if ddx.hypot(ddn) <= margin {
                    keys.insert((ix, in_));
                }
            }
        }
    }
    keys.into_iter()
        .map(|(ix, in_)| ((ix as f64 + 0.5) * size, (in_ as f64 + 0.5) * size))
        .collect()
}

/// Tuiles d'altitude couvrant le couloir autour du parcours.
fn dem_tiles(route: &Route, proj: &LocalProjection, reach_m: f64) -> Vec<(u8, u32, u32)> {
    let tile_m = meters_per_px(proj.latitude(), DEM_ZOOM) * 256.0;
    let r = (reach_m / tile_m).ceil() as i64;
    let mut set = BTreeSet::new();
    for p in route.points.iter().step_by(20) {
        let tx = (mercator_px_x(p.lon, DEM_ZOOM) / 256.0).floor() as i64;
        let ty = (mercator_px_y(p.lat, DEM_ZOOM) / 256.0).floor() as i64;
        for dy in -r..=r {
            for dx in -r..=r {
                let max = 1i64 << DEM_ZOOM;
                let (x, y) = (tx + dx, ty + dy);
                if (0..max).contains(&x) && (0..max).contains(&y) {
                    set.insert((DEM_ZOOM, x as u32, y as u32));
                }
            }
        }
    }
    set.into_iter().collect()
}

/// Morceaux d'une polyligne (rééchantillonnée tous les `step` m) dont les
/// segments ont leur milieu dans le carré et leurs extrémités acceptées.
fn runs_inside(
    points: &[(f64, f64)],
    step: f64,
    inside: &dyn Fn((f64, f64)) -> bool,
    ok: &dyn Fn((f64, f64)) -> bool,
) -> Vec<Vec<(f64, f64)>> {
    let mut dense = Vec::new();
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        let k = (len / step).ceil().max(1.0) as usize;
        for s in 0..k {
            let t = s as f64 / k as f64;
            dense.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    if let Some(&last) = points.last() {
        dense.push(last);
    }
    let mut runs = Vec::new();
    let mut cur: Vec<(f64, f64)> = Vec::new();
    for w in dense.windows(2) {
        let mid = ((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0);
        if inside(mid) && ok(w[0]) && ok(w[1]) {
            if cur.is_empty() {
                cur.push(w[0]);
            }
            cur.push(w[1]);
        } else if !cur.is_empty() {
            runs.push(std::mem::take(&mut cur));
        }
    }
    if cur.len() >= 2 {
        runs.push(cur);
    }
    runs
}

/// Charge OSM depuis un fichier, le cache ou Overpass. Ne fait jamais échouer
/// la construction : sans OSM, le décor reste fait de relief et d'images.
fn load_osm(
    opts: &WorldOptions,
    cache: &TileCache,
    route: &Route,
    proj: &LocalProjection,
    radius_m: f64,
    building_radius_m: f64,
) -> (OsmData, String) {
    let parse = |text: &str, source: &str| match OsmData::from_overpass_json(text, proj) {
        Ok(d) => {
            let note = format!(
                "OSM ({source}) : {} bâtiments, {} lignes, {} surfaces",
                d.buildings.len(),
                d.lines.len(),
                d.areas.len()
            );
            (d, note)
        }
        Err(e) => (
            OsmData::default(),
            format!("OSM illisible ({source}) : {e}"),
        ),
    };
    if let Some(file) = &opts.osm_file {
        return match std::fs::read_to_string(file) {
            Ok(text) => parse(&text, "fichier"),
            Err(e) => (OsmData::default(), format!("fichier OSM illisible : {e}")),
        };
    }
    if opts.overpass_url.trim().is_empty() {
        return (OsmData::default(), "OSM désactivé".into());
    }
    let path: Vec<(f64, f64)> = route
        .points
        .iter()
        .step_by(30)
        .chain(route.points.last())
        .map(|p| (p.lat, p.lon))
        .collect();
    let query = OsmData::overpass_query(&path, radius_m, building_radius_m);
    let file = cache
        .root()
        .join("osm")
        .join(format!("{:016x}.json", fnv1a(query.as_bytes())));
    if let Ok(text) = std::fs::read_to_string(&file) {
        return parse(&text, "cache");
    }
    // Le serveur principal est souvent saturé : on essaie aussi des miroirs.
    let mut servers = vec![opts.overpass_url.trim().to_string()];
    for mirror in OVERPASS_MIRRORS {
        if !servers.iter().any(|s| s == mirror) {
            servers.push(mirror.to_string());
        }
    }
    let mut errors = Vec::new();
    for server in &servers {
        match cache.post_form(server, &[("data", &query)]) {
            Ok(text) => {
                let result = parse(&text, "Overpass");
                if !result.0.is_empty() {
                    let _ = std::fs::create_dir_all(file.parent().unwrap());
                    let _ = std::fs::write(&file, &text);
                    return result;
                }
                errors.push(format!("{} : réponse vide", host_of(server)));
            }
            Err(e) => errors.push(format!("{} : {e}", host_of(server))),
        }
        if !cache.is_online() {
            break;
        }
    }
    (
        OsmData::default(),
        format!("OpenStreetMap indisponible ({})", errors.join(" ; ")),
    )
}

/// Serveurs Overpass essayés après celui du profil.
const OVERPASS_MIRRORS: [&str; 2] = [
    "https://overpass.kumi.systems/api/interpreter",
    "https://overpass.private.coffee/api/interpreter",
];

fn host_of(url: &str) -> &str {
    url.split("//")
        .nth(1)
        .and_then(|s| s.split('/').next())
        .unwrap_or(url)
}

/// Dossier de cache propre à chaque URL : changer de source ne mélange pas les tuiles.
fn source_name(kind: &str, url: &str) -> String {
    format!("{kind}-{:08x}", fnv1a(url.trim().as_bytes()) as u32)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}

fn credits(opts: &WorldOptions, imagery: bool, osm: bool) -> String {
    let mut parts =
        vec!["Relief : Terrain Tiles (Mapzen, AWS ; SRTM, Copernicus, EU-DEM)".to_string()];
    if imagery {
        if opts.imagery_url.contains("geopf.fr") || opts.imagery_url.contains("ign.fr") {
            parts.push("Images : © IGN, BD ORTHO®".into());
        } else if opts.imagery_url.starts_with("file://") {
            parts.push("Images : fichiers locaux".into());
        } else {
            let host = opts
                .imagery_url
                .split("//")
                .nth(1)
                .and_then(|s| s.split('/').next())
                .unwrap_or("source personnalisée");
            parts.push(format!("Images : {host}"));
        }
    }
    if osm {
        parts.push("© contributeurs OpenStreetMap (ODbL)".into());
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mipmaps_go_down_to_one_pixel() {
        let rgb = vec![[200u8, 100, 50]; 16];
        let t = texture_from(&rgb, &[true; 16], 4);
        assert_eq!(t.data.len(), (16 + 4 + 1) * 4);
        assert_eq!(&t.data[t.data.len() - 4..], &[200, 100, 50, 255]);
        assert_eq!(t.imagery_share, 1.0);
    }

    #[test]
    fn runs_skip_the_outside() {
        let pts = [(0.0, 0.0), (100.0, 0.0)];
        let runs = runs_inside(&pts, 10.0, &|p| p.0 < 50.0, &|p| p.0 > 15.0);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].first(), Some(&(20.0, 0.0)));
        assert_eq!(runs[0].last(), Some(&(50.0, 0.0)));
    }
}
