//! Appareils Bluetooth pour Godot (provisoire, en attente du hub).

use godot::prelude::*;
use velo_core::devices::{DeviceEvent, DeviceHub};
use velo_core::protocol::ControlCommand;

#[derive(GodotClass)]
#[class(base = Node, init)]
pub struct VeloDevices {
    base: Base<Node>,
}

impl DeviceHub for VeloDevices {
    fn send(&mut self, _command: ControlCommand) {}
    fn poll(&mut self, _dt: f64) -> Vec<DeviceEvent> {
        Vec::new()
    }
}

#[godot_api]
impl VeloDevices {
    #[func]
    fn has_trainer(&self) -> bool {
        false
    }
    #[func]
    fn connect_remembered(&mut self, _list: VarArray) {}
}
