//! Boucle de simulation d'une sortie, indépendante du rendu et du matériel.

use crate::config::Profile;
use crate::gearing::{Gear, VirtualGearbox};
use crate::physics::{self, Params};
use crate::protocol::ControlCommand;
use crate::route::Route;

/// Mesures reçues des capteurs à chaque pas.
#[derive(Debug, Clone, Copy, Default)]
pub struct Inputs {
    pub power_w: f64,
    pub cadence_rpm: f64,
}

/// Ce que l'overlay et la scène 3D affichent.
#[derive(Debug, Clone, Copy)]
pub struct Telemetry {
    pub elapsed_s: f64,
    pub distance_m: f64,
    pub speed_ms: f64,
    pub power_w: f64,
    pub cadence_rpm: f64,
    pub grade: f64,
    pub elevation_m: f64,
    pub gear: Gear,
    pub gear_index: usize,
    pub finished: bool,
}

pub struct Ride {
    pub profile: Profile,
    pub route: Route,
    pub gearbox: VirtualGearbox,
    params: Params,
    elapsed_s: f64,
    distance_m: f64,
    speed_ms: f64,
    last: Inputs,
}

impl Ride {
    pub fn new(profile: Profile, route: Route) -> Self {
        let altitude = route.points.first().map_or(0.0, |p| p.elevation_m);
        let params = Params::from_profile(&profile).with_altitude(altitude);
        let gearbox = VirtualGearbox::synchro(&profile.bike);
        Self {
            profile,
            route,
            gearbox,
            params,
            elapsed_s: 0.0,
            distance_m: 0.0,
            speed_ms: 0.0,
            last: Inputs::default(),
        }
    }

    pub fn finished(&self) -> bool {
        self.distance_m >= self.route.length_m()
    }

    /// Avance la simulation de `dt` secondes.
    pub fn tick(&mut self, inputs: Inputs, dt: f64) -> Telemetry {
        self.last = inputs;
        if !self.finished() {
            let grade = self.route.grade_at(self.distance_m);
            self.speed_ms =
                physics::step_speed(&self.params, self.speed_ms, inputs.power_w, grade, dt);
            self.distance_m = (self.distance_m + self.speed_ms * dt).min(self.route.length_m());
            self.elapsed_s += dt;
        }
        self.telemetry()
    }

    pub fn telemetry(&self) -> Telemetry {
        let point = self.route.point_at(self.distance_m);
        Telemetry {
            elapsed_s: self.elapsed_s,
            distance_m: self.distance_m,
            speed_ms: self.speed_ms,
            power_w: self.last.power_w,
            cadence_rpm: self.last.cadence_rpm,
            grade: self.route.grade_at(self.distance_m),
            elevation_m: point.elevation_m,
            gear: self.gearbox.current(),
            gear_index: self.gearbox.index(),
            finished: self.finished(),
        }
    }

    /// Commande à envoyer au home trainer : pente effective tenant compte du
    /// rapport virtuel, de la difficulté choisie et des limites du trainer.
    pub fn trainer_command(&self) -> ControlCommand {
        let t = &self.profile.trainer;
        let road = self.route.grade_at(self.distance_m) * t.grade_difficulty;
        let physical = Gear {
            chainring: t.physical_chainring,
            cog: t.physical_cog,
        }
        .ratio();
        // Sans cadence, on ne peut pas estimer la vitesse du volant : pente de la route.
        let grade = if self.last.cadence_rpm > 20.0 {
            physics::effective_grade(
                &self.params,
                self.last.cadence_rpm,
                self.profile.bike.wheel_circumference_m,
                physical,
                self.gearbox.current().ratio(),
                road,
            )
        } else {
            road
        };
        ControlCommand::Simulation {
            wind_ms: 0.0,
            grade: grade.clamp(t.min_grade, t.max_grade),
            crr: self.params.crr,
            cw_kg_m: 0.5 * self.params.air_density * self.params.cda_m2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_route(len_m: f64) -> Route {
        let deg = (len_m / 6_371_008.8f64).to_degrees();
        Route::from_lat_lon_ele(
            None,
            &[(45.0, 6.0, 100.0), (45.0 + deg, 6.0, 100.0)],
            5.0,
            40.0,
        )
        .unwrap()
    }

    #[test]
    fn rides_to_the_finish() {
        let mut ride = Ride::new(Profile::martin(), flat_route(2000.0));
        let inputs = Inputs {
            power_w: 200.0,
            cadence_rpm: 90.0,
        };
        let mut t = ride.telemetry();
        while !t.finished && t.elapsed_s < 3600.0 {
            t = ride.tick(inputs, 0.05);
        }
        assert!(t.finished);
        // 2 km à ~33 km/h départ arrêté : un peu plus de 3 min 30.
        assert!((200.0..260.0).contains(&t.elapsed_s), "{}", t.elapsed_s);
    }

    #[test]
    fn clicks_change_trainer_grade() {
        let mut ride = Ride::new(Profile::martin(), flat_route(1000.0));
        ride.tick(
            Inputs {
                power_w: 200.0,
                cadence_rpm: 90.0,
            },
            0.05,
        );
        let grade = |r: &Ride| match r.trainer_command() {
            ControlCommand::Simulation { grade, .. } => grade,
            _ => unreachable!(),
        };
        let before = grade(&ride);
        ride.gearbox.shift_up();
        ride.gearbox.shift_up();
        assert!(grade(&ride) > before);
    }
}
