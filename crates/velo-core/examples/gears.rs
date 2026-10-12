//! Affiche la suite de rapports virtuels parcourue aux Zwift Click.

use velo_core::{config::Profile, gearing::VirtualGearbox};

fn main() {
    let bike = Profile::martin().bike;
    for (i, g) in VirtualGearbox::synchro(&bike).gears().iter().enumerate() {
        println!(
            "{:2}  {}×{:<2}  {:.2} m/tour  {:4.1} km/h à 90 rpm",
            i + 1,
            g.chainring,
            g.cog,
            g.development_m(bike.wheel_circumference_m),
            g.development_m(bike.wheel_circumference_m) * 90.0 * 60.0 / 1000.0
        );
    }
}
