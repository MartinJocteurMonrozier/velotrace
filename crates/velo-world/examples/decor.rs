//! Construit le décor d'un GPX sans Godot et affiche ce qui a été trouvé.
//!
//! cargo run --release -p velo-world --example decor -- data/alpe-d-huez.gpx \
//!     [--qualite 0|1|2] [--osm data/osm-alpe-d-huez.json] [--hors-ligne] [--png dossier]

use std::sync::atomic::AtomicBool;
use std::time::Instant;
use velo_core::config::Scenery;
use velo_core::route::Route;
use velo_world::{World, WorldOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let gpx = args
        .first()
        .ok_or("usage : decor <fichier.gpx> [options]")?;
    let opt = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
    };
    let mut opts = WorldOptions::from_scenery(&Scenery::default());
    if let Some(q) = opt("--qualite") {
        opts.quality = q.parse()?;
    }
    opts.osm_file = opt("--osm").map(Into::into);
    opts.online = !args.iter().any(|a| a == "--hors-ligne");
    if let Some(url) = opt("--images") {
        opts.imagery_url = url.clone();
    }

    let route = Route::from_gpx(
        std::fs::File::open(gpx).map_err(|e| format!("impossible d'ouvrir « {gpx} » : {e}"))?,
        5.0,
        40.0,
    )?;
    let gpx_gain = route.elevation_gain_m();
    let t0 = Instant::now();
    let world = World::build(route, &opts, &|_, _| {}, &AtomicBool::new(false))?;
    println!(
        "Construit en {:.1} s : {}",
        t0.elapsed().as_secs_f64(),
        world.summary
    );
    println!(
        "D+ : GPX {:.0} m, relief {:.0} m ; {}",
        gpx_gain,
        world.route.elevation_gain_m(),
        world.credits
    );

    let t1 = Instant::now();
    let (mut trees, mut buildings, mut verts) = (0, 0, 0);
    for i in 0..world.near.len() {
        let d = world.near_detail(i);
        trees += d.conifers.count() + d.broadleaves.count();
        buildings += d.buildings.vertex_count();
        verts += d.terrain.vertex_count();
        if let Some(dir) = opt("--png") {
            if i == 0 || d.conifers.count() + d.broadleaves.count() > 3000 && i % 7 == 0 {
                save(&format!("{dir}/proche-{i}.png"), &d.texture)?;
            }
        }
    }
    println!(
        "{} carrés détaillés en {:.1} s ({:.0} ms chacun) : {trees} arbres, {buildings} sommets de bâtiments, {verts} sommets de relief",
        world.near.len(),
        t1.elapsed().as_secs_f64(),
        t1.elapsed().as_secs_f64() * 1000.0 / world.near.len().max(1) as f64
    );
    if let Some(dir) = opt("--png") {
        for (i, c) in world.far.iter().enumerate().take(4) {
            save(&format!("{dir}/lointain-{i}.png"), &c.texture)?;
        }
    }
    Ok(())
}

fn save(path: &str, t: &velo_world::world::Texture) -> Result<(), Box<dyn std::error::Error>> {
    let img = image::RgbaImage::from_raw(
        t.size as u32,
        t.size as u32,
        t.data[..t.size * t.size * 4].to_vec(),
    )
    .ok_or("texture invalide")?;
    img.save(path)?;
    Ok(())
}
