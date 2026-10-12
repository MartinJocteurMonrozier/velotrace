//! Simule une sortie à puissance constante sur un GPX, sans matériel.
//!
//! cargo run -p velo-core --example simulate -- parcours.gpx 200

use std::fs::File;
use velo_core::{
    config::Profile,
    ride::{Inputs, Ride},
    route::Route,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage : simulate <fichier.gpx> [puissance_w]")?;
    let power: f64 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(200.0);

    let route = Route::from_gpx(File::open(&path)?, 5.0, 40.0)?;
    println!(
        "{} : {:.1} km, D+ {:.0} m",
        route.name.as_deref().unwrap_or("parcours"),
        route.length_m() / 1000.0,
        route.elevation_gain_m()
    );
    let mut ride = Ride::new(Profile::martin(), route);
    let inputs = Inputs {
        power_w: power,
        cadence_rpm: 90.0,
    };
    let mut next_km = 1000.0;
    loop {
        let t = ride.tick(inputs, 0.05);
        if t.distance_m >= next_km || t.finished {
            println!(
                "{:5.1} km  {:3.0} min  {:4.1} km/h  pente {:+5.1} %  alt {:4.0} m",
                t.distance_m / 1000.0,
                t.elapsed_s / 60.0,
                t.speed_ms * 3.6,
                t.grade * 100.0,
                t.elevation_m
            );
            next_km += 1000.0;
        }
        if t.finished {
            break;
        }
    }
    Ok(())
}
