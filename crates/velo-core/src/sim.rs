//! Mode test : home trainer, capteur de cadence, FC et Zwift Click simulés.
//!
//! Le simulateur fabrique les mêmes trames Bluetooth que le vrai matériel
//! (FTMS, CSC, HRS) puis les décode avec [`crate::protocol`] : tout le chemin
//! de l'application est exercé, sauf la radio.

use crate::config::Profile;
use crate::devices::{DeviceEvent, DeviceHub, Shift};
use crate::gearing::Gear;
use crate::physics::G;
use crate::protocol::{self, ControlCommand, CrankSample};

/// Comportement du cycliste virtuel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RiderModel {
    /// Puissance imposée, comme en mode ERG ou au clavier.
    Power { watts: f64, cadence_rpm: f64 },
    /// Cadence imposée : la puissance découle de la résistance du trainer,
    /// donc du rapport choisi aux Click et de la pente.
    Cadence { rpm: f64 },
}

/// Changement de vitesse automatique : le cycliste virtuel « clique » pour
/// garder sa puissance dans une plage, ce qui fait vivre les Click en test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoShift {
    pub min_w: f64,
    pub max_w: f64,
    /// Délai minimal entre deux clics.
    pub interval_s: f64,
}

#[derive(Debug, Clone)]
pub struct SimSettings {
    pub rider: RiderModel,
    pub auto_shift: Option<AutoShift>,
    /// Bruit relatif sur la puissance (0,03 = ±3 %).
    pub power_noise: f64,
    pub resting_hr: f64,
    pub max_hr: f64,
    pub seed: u64,
}

impl Default for SimSettings {
    fn default() -> Self {
        Self {
            rider: RiderModel::Cadence { rpm: 88.0 },
            auto_shift: Some(AutoShift {
                min_w: 150.0,
                max_w: 260.0,
                interval_s: 2.0,
            }),
            power_noise: 0.03,
            resting_hr: 55.0,
            max_hr: 190.0,
            seed: 0x5EED,
        }
    }
}

pub struct SimulatedDevices {
    pub settings: SimSettings,
    mass_kg: f64,
    physical_ratio: f64,
    wheel_circumference_m: f64,
    ftp_w: f64,
    /// Dernière consigne reçue (pente, Crr, coefficient de traînée).
    grade: f64,
    crr: f64,
    cw_kg_m: f64,
    controlled: bool,
    /// Consigne ERG reçue (W) ; la pente est alors ignorée.
    erg_target_w: Option<f64>,
    pending_clicks: Vec<Shift>,
    power_w: f64,
    /// Puissance sans bruit, point de départ de la réponse ERG.
    smooth_power_w: f64,
    heart_rate: f64,
    crank_revs: f64,
    crank_time_s: f64,
    since_frame_s: f64,
    since_shift_s: f64,
    rng: u64,
}

impl SimulatedDevices {
    pub fn new(profile: &Profile, settings: SimSettings) -> Self {
        let t = &profile.trainer;
        Self {
            mass_kg: profile.total_mass_kg(),
            physical_ratio: Gear {
                chainring: t.physical_chainring,
                cog: t.physical_cog,
            }
            .ratio(),
            wheel_circumference_m: profile.bike.wheel_circumference_m,
            ftp_w: profile.rider.ftp_w.unwrap_or(200) as f64,
            grade: 0.0,
            crr: profile.bike.crr,
            cw_kg_m: 0.5 * 1.2 * profile.bike.cda_m2,
            controlled: false,
            erg_target_w: None,
            pending_clicks: Vec::new(),
            power_w: 0.0,
            smooth_power_w: 0.0,
            heart_rate: settings.resting_hr,
            crank_revs: 0.0,
            crank_time_s: 0.0,
            since_frame_s: 0.0,
            since_shift_s: 0.0,
            rng: settings.seed.max(1),
            settings,
        }
    }

    /// Appui sur un Click virtuel (clavier ou bouton de l'interface en mode test).
    pub fn press(&mut self, shift: Shift) {
        self.pending_clicks.push(shift);
    }

    /// Le trainer a-t-il accepté une prise de contrôle FTMS ?
    pub fn is_controlled(&self) -> bool {
        self.controlled
    }

    pub fn current_grade(&self) -> f64 {
        self.grade
    }

    /// Consigne ERG en cours, si le trainer est en mode puissance imposée.
    pub fn erg_target(&self) -> Option<f64> {
        self.erg_target_w
    }

    /// Dernière puissance produite par le cycliste virtuel (W).
    pub fn current_power(&self) -> f64 {
        self.power_w
    }

    fn cadence(&self) -> f64 {
        match self.settings.rider {
            RiderModel::Power { cadence_rpm, .. } => cadence_rpm,
            RiderModel::Cadence { rpm } => rpm,
        }
    }

    /// Bruit uniforme dans [-1, 1] (xorshift, reproductible).
    fn noise(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng as f64 / u64::MAX as f64) * 2.0 - 1.0
    }

    /// Puissance que demande le volant à cette cadence avec la consigne actuelle.
    fn trainer_power(&self, cadence_rpm: f64) -> f64 {
        let v = cadence_rpm / 60.0 * self.physical_ratio * self.wheel_circumference_m;
        let force = self.mass_kg * G * (self.crr + self.grade) + self.cw_kg_m * v * v;
        (force * v).max(0.0)
    }

    fn indoor_bike_frame(&self, cadence_rpm: f64) -> Vec<u8> {
        // Vitesse (implicite), cadence (bit 2), puissance (bit 6).
        let flags: u16 = (1 << 2) | (1 << 6);
        let speed_kmh = cadence_rpm / 60.0 * self.physical_ratio * self.wheel_circumference_m * 3.6;
        let mut f = flags.to_le_bytes().to_vec();
        f.extend_from_slice(&((speed_kmh * 100.0).round() as u16).to_le_bytes());
        f.extend_from_slice(&((cadence_rpm * 2.0).round() as u16).to_le_bytes());
        f.extend_from_slice(&(self.power_w.round() as i16).to_le_bytes());
        f
    }

    fn csc_frame(&self) -> Vec<u8> {
        let revs = (self.crank_revs.floor() as u64 % 65_536) as u16;
        let ticks = ((self.crank_time_s * 1024.0).round() as u64 % 65_536) as u16;
        let mut f = vec![0x02];
        f.extend_from_slice(&revs.to_le_bytes());
        f.extend_from_slice(&ticks.to_le_bytes());
        f
    }
}

const FRAME_PERIOD_S: f64 = 0.25;
/// Constante de temps du trainer pour rejoindre la consigne ERG.
const ERG_RESPONSE_S: f64 = 1.5;

impl DeviceHub for SimulatedDevices {
    fn send(&mut self, command: ControlCommand) {
        // On repasse par l'encodage pour tester les trames envoyées.
        let bytes = command.encode();
        match bytes[0] {
            0x00 => self.controlled = true,
            0x01 => {
                self.controlled = false;
                self.grade = 0.0;
                self.erg_target_w = None;
            }
            0x05 if self.controlled => {
                let w = i16::from_le_bytes([bytes[1], bytes[2]]);
                self.erg_target_w = Some(w.max(0) as f64);
            }
            0x11 if self.controlled => {
                // Une consigne de pente quitte le mode ERG.
                self.erg_target_w = None;
                self.grade = i16::from_le_bytes([bytes[3], bytes[4]]) as f64 / 10_000.0;
                self.crr = bytes[5] as f64 / 10_000.0;
                self.cw_kg_m = bytes[6] as f64 / 100.0;
            }
            _ => {}
        }
    }

    fn poll(&mut self, dt: f64) -> Vec<DeviceEvent> {
        let mut events: Vec<DeviceEvent> = self
            .pending_clicks
            .drain(..)
            .map(DeviceEvent::Shift)
            .collect();

        let cadence = self.cadence();
        let target = match (self.erg_target_w, self.settings.rider) {
            // ERG : le frein rejoint la consigne quel que soit le pédalage
            // (réponse du premier ordre), en partant de la puissance actuelle.
            (Some(erg), _) => {
                let current = self.smooth_power_w;
                current + (erg - current) * (dt / ERG_RESPONSE_S).min(1.0)
            }
            (None, RiderModel::Power { watts, .. }) => watts,
            (None, RiderModel::Cadence { rpm }) => self.trainer_power(rpm),
        };
        self.smooth_power_w = target;
        self.power_w = (target * (1.0 + self.settings.power_noise * self.noise())).max(0.0);

        // Le dernier tour de manivelle complet fixe l'horodatage CSC.
        let revs_before = self.crank_revs.floor();
        self.crank_revs += cadence / 60.0 * dt;
        if self.crank_revs.floor() > revs_before && cadence > 0.0 {
            self.crank_time_s = self.crank_revs.floor() / (cadence / 60.0);
        }

        // FC : réponse du premier ordre (≈ 30 s) vers une cible liée à la puissance.
        let effort = (self.power_w / (self.ftp_w * 1.1)).clamp(0.0, 1.0);
        let hr_target =
            self.settings.resting_hr + (self.settings.max_hr - self.settings.resting_hr) * effort;
        self.heart_rate += (hr_target - self.heart_rate) * (dt / 30.0).min(1.0);

        self.since_shift_s += dt;
        // En ERG, changer de rapport ne change pas la puissance : pas de clic.
        if let (Some(a), None) = (self.settings.auto_shift, self.erg_target_w) {
            if self.since_shift_s >= a.interval_s {
                let click = if self.power_w > a.max_w {
                    Some(Shift::Down)
                } else if self.power_w < a.min_w {
                    Some(Shift::Up)
                } else {
                    None
                };
                if let Some(c) = click {
                    events.push(DeviceEvent::Shift(c));
                    self.since_shift_s = 0.0;
                }
            }
        }

        self.since_frame_s += dt;
        if self.since_frame_s >= FRAME_PERIOD_S {
            self.since_frame_s = 0.0;
            let frame = self.indoor_bike_frame(cadence);
            if let Ok(data) = protocol::parse_indoor_bike_data(&frame) {
                events.push(DeviceEvent::Trainer(data));
            }
            if let Ok(Some(CrankSample { .. })) = protocol::parse_csc_crank(&self.csc_frame()) {
                events.push(DeviceEvent::Cadence(cadence));
            }
            let hr = [0x00, self.heart_rate.round().clamp(0.0, 255.0) as u8];
            if let Ok(bpm) = protocol::parse_heart_rate(&hr) {
                events.push(DeviceEvent::HeartRate(bpm));
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ride::Ride;
    use crate::route::Route;

    fn route(len_m: f64, grade: f64) -> Route {
        let deg = (len_m / 6_371_008.8f64).to_degrees();
        let pts = [(45.0, 6.0, 100.0), (45.0 + deg, 6.0, 100.0 + len_m * grade)];
        Route::from_lat_lon_ele(None, &pts, 5.0, 40.0).unwrap()
    }

    fn run(ride: &mut Ride, sim: &mut SimulatedDevices, seconds: f64) {
        let dt = 0.05;
        for _ in 0..(seconds / dt) as usize {
            for ev in sim.poll(dt) {
                ride.handle(&ev);
            }
            ride.step(dt);
            sim.send(ride.trainer_command());
        }
    }

    #[test]
    fn simulation_command_needs_control() {
        let p = Profile::martin();
        let mut sim = SimulatedDevices::new(&p, SimSettings::default());
        let cmd = ControlCommand::Simulation {
            wind_ms: 0.0,
            grade: 0.05,
            crr: 0.004,
            cw_kg_m: 0.19,
        };
        sim.send(cmd);
        assert_eq!(sim.current_grade(), 0.0);
        sim.send(ControlCommand::RequestControl);
        sim.send(cmd);
        assert!((sim.current_grade() - 0.05).abs() < 1e-9);
    }

    #[test]
    fn cadence_rider_reaches_the_virtual_gear_speed() {
        // À cadence fixe et sans changement, la vitesse dans le jeu doit
        // correspondre au développement du rapport virtuel.
        let p = Profile::martin();
        let settings = SimSettings {
            auto_shift: None,
            power_noise: 0.0,
            ..SimSettings::default()
        };
        let mut sim = SimulatedDevices::new(&p, settings);
        sim.send(ControlCommand::RequestControl);
        let mut ride = Ride::new(p.clone(), route(20_000.0, 0.0));
        let gear = ride.gearbox.current();
        run(&mut ride, &mut sim, 120.0);
        let expected = 88.0 / 60.0 * gear.ratio() * p.bike.wheel_circumference_m;
        let speed = ride.telemetry().speed_ms;
        assert!(
            (speed - expected).abs() / expected < 0.03,
            "{speed} vs {expected} ({gear:?})"
        );
    }

    #[test]
    fn auto_shift_keeps_power_in_band_on_a_climb() {
        let p = Profile::martin();
        let settings = SimSettings {
            power_noise: 0.0,
            ..SimSettings::default()
        };
        let mut sim = SimulatedDevices::new(&p, settings);
        sim.send(ControlCommand::RequestControl);
        let mut ride = Ride::new(p, route(20_000.0, 0.07));
        run(&mut ride, &mut sim, 180.0);
        let t = ride.telemetry();
        assert!(t.gear.chainring == 36, "{:?}", t.gear);
        assert!((100.0..300.0).contains(&t.power_w), "{} W", t.power_w);
        assert!(t.heart_rate_bpm.unwrap() > 100);
    }

    #[test]
    fn erg_target_needs_control_and_slope_cancels_it() {
        let p = Profile::martin();
        let mut sim = SimulatedDevices::new(&p, SimSettings::default());
        sim.send(ControlCommand::TargetPower(250));
        assert_eq!(sim.erg_target(), None);
        sim.send(ControlCommand::RequestControl);
        sim.send(ControlCommand::TargetPower(250));
        assert_eq!(sim.erg_target(), Some(250.0));
        sim.send(ControlCommand::Simulation {
            wind_ms: 0.0,
            grade: 0.02,
            crr: 0.004,
            cw_kg_m: 0.19,
        });
        assert_eq!(sim.erg_target(), None);
        assert!((sim.current_grade() - 0.02).abs() < 1e-9);
    }

    #[test]
    fn erg_power_follows_target_with_first_order_response() {
        let p = Profile::martin();
        let settings = SimSettings {
            rider: RiderModel::Power {
                watts: 150.0,
                cadence_rpm: 90.0,
            },
            power_noise: 0.0,
            ..SimSettings::default()
        };
        let mut sim = SimulatedDevices::new(&p, settings);
        sim.send(ControlCommand::RequestControl);
        let dt = 0.05;
        for _ in 0..40 {
            sim.poll(dt);
        }
        assert!((sim.current_power() - 150.0).abs() < 1e-9);
        sim.send(ControlCommand::TargetPower(300));
        for _ in 0..30 {
            sim.poll(dt);
        }
        // Après une constante de temps : environ 63 % du chemin.
        let after = sim.current_power();
        assert!((235.0..260.0).contains(&after), "{after} W");
        for _ in 0..200 {
            sim.poll(dt);
        }
        assert!((sim.current_power() - 300.0).abs() < 1.0);
    }

    #[test]
    fn erg_ride_holds_power_on_a_climb_without_shifting() {
        // Cycliste à cadence imposée et Click automatiques : en ERG, la
        // puissance suit la consigne et les Click restent muets.
        let p = Profile::martin();
        let mut sim = SimulatedDevices::new(&p, SimSettings::default());
        sim.send(ControlCommand::RequestControl);
        let mut ride = Ride::new(p, route(20_000.0, 0.07));
        ride.set_erg_target(Some(180.0));
        let gear = ride.gearbox.index();
        run(&mut ride, &mut sim, 60.0);
        let t = ride.telemetry();
        assert_eq!(ride.gearbox.index(), gear);
        assert!((170.0..190.0).contains(&t.power_w), "{} W", t.power_w);
        // Retour à la pente simulée : les Click reprennent la main.
        ride.set_erg_target(None);
        run(&mut ride, &mut sim, 60.0);
        assert_eq!(sim.erg_target(), None);
        assert_ne!(ride.gearbox.index(), gear);
    }

    #[test]
    fn manual_clicks_reach_the_gearbox() {
        let p = Profile::martin();
        let settings = SimSettings {
            auto_shift: None,
            ..SimSettings::default()
        };
        let mut sim = SimulatedDevices::new(&p, settings);
        let mut ride = Ride::new(p, route(1000.0, 0.0));
        let start = ride.gearbox.index();
        sim.press(Shift::Up);
        sim.press(Shift::Up);
        sim.press(Shift::Down);
        for ev in sim.poll(0.05) {
            ride.handle(&ev);
        }
        assert_eq!(ride.gearbox.index(), start + 1);
    }
}
