//! Sortie complète en mode test sur un GPX : home trainer, cadence, FC et
//! Zwift Click simulés, sans aucun matériel.
//!
//! cargo run -p velo-core --example simulate -- data/exemple-col.gpx              # cadence 88 rpm, Click automatiques
//! cargo run -p velo-core --example simulate -- data/exemple-col.gpx --power 200

use std::fs::File;
use velo_core::{
    config::Profile,
    devices::DeviceHub,
    protocol::ControlCommand,
    ride::Ride,
    route::Route,
    sim::{RiderModel, SimSettings, SimulatedDevices},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args
        .first()
        .ok_or("usage : simulate <fichier.gpx> [--power W]")?;
    let mut settings = SimSettings::default();
    if let Some(i) = args.iter().position(|a| a == "--power") {
        let watts: f64 = args
            .get(i + 1)
            .ok_or("--power attend une valeur")?
            .parse()?;
        settings.rider = RiderModel::Power {
            watts,
            cadence_rpm: 88.0,
        };
        settings.auto_shift = None;
    }

    let file = File::open(path).map_err(|e| format!("impossible d'ouvrir « {path} » : {e}"))?;
    let route = Route::from_gpx(file, 5.0, 40.0)?;
    println!(
        "{} : {:.1} km, D+ {:.0} m",
        route.name.as_deref().unwrap_or("parcours"),
        route.length_m() / 1000.0,
        route.elevation_gain_m()
    );

    let mut profile = Profile::martin();
    profile.app.test_mode = true;
    let mut devices = SimulatedDevices::new(&profile, settings);
    devices.send(ControlCommand::RequestControl);
    devices.send(ControlCommand::StartOrResume);
    let mut ride = Ride::new(profile, route);

    let dt = 0.05;
    let mut next_km = 1000.0;
    loop {
        for event in devices.poll(dt) {
            ride.handle(&event);
        }
        let t = ride.step(dt);
        devices.send(ride.trainer_command());
        if t.distance_m >= next_km || t.finished {
            println!(
                "{:5.1} km  {:3.0} min  {:4.1} km/h  {:3.0} W  {:2.0} rpm  {:3} bpm  {}×{:<2}  pente {:+5.1} %  trainer {:+5.1} %",
                t.distance_m / 1000.0,
                t.elapsed_s / 60.0,
                t.speed_ms * 3.6,
                t.power_w,
                t.cadence_rpm,
                t.heart_rate_bpm.unwrap_or(0),
                t.gear.chainring,
                t.gear.cog,
                t.grade * 100.0,
                devices.current_grade() * 100.0,
            );
            next_km += 1000.0;
        }
        if t.finished {
            break;
        }
    }
    Ok(())
}
