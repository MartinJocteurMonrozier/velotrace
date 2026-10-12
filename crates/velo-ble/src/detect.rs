//! Reconnaissance des appareils à partir de leurs annonces Bluetooth, et
//! ordre d'affichage de la liste.

use std::cmp::Reverse;

use uuid::Uuid;
use velo_core::devices::DeviceKind;
use velo_core::protocol::{uuid as ids, zwift};

use crate::FoundDevice;

/// Ce qu'une annonce Bluetooth révèle d'un appareil.
#[derive(Debug, Clone, Copy, Default)]
pub struct Advertisement<'a> {
    pub name: Option<&'a str>,
    pub services: &'a [Uuid],
    /// Identifiants des fabricants présents dans les données constructeur.
    pub manufacturer_ids: &'a [u16],
}

/// UUID complet d'un service ou d'une caractéristique standard 16 bits.
pub fn uuid16(id: u16) -> Uuid {
    // Base Bluetooth : 0000xxxx-0000-1000-8000-00805f9b34fb.
    Uuid::from_u128(0x0000_0000_0000_1000_8000_0080_5f9b_34fb | ((id as u128) << 96))
}

/// Rôles possibles d'un appareil, dans l'ordre de [`DeviceKind::ALL`].
/// Même logique que `velo-probe scan`, éprouvée sur le vrai matériel.
pub fn detect_kinds(ad: &Advertisement) -> Vec<DeviceKind> {
    let has = |id: u16| ad.services.contains(&uuid16(id));
    let name = ad.name.unwrap_or_default().to_lowercase();
    let zwift_service = Uuid::parse_str(zwift::SERVICE).ok();
    let mut out = Vec::new();
    if has(ids::FTMS_SERVICE) {
        out.push(DeviceKind::Trainer);
    }
    if has(ids::CSC_SERVICE) {
        out.push(DeviceKind::Cadence);
    }
    if has(ids::HRS_SERVICE) {
        out.push(DeviceKind::HeartRate);
    }
    if zwift_service.is_some_and(|s| ad.services.contains(&s))
        || has(zwift::SERVICE_16)
        || ad.manufacturer_ids.contains(&zwift::MANUFACTURER_ID)
        || name.contains("zwift")
    {
        out.push(DeviceKind::Click);
    }
    out
}

/// Rang d'un rôle dans [`DeviceKind::ALL`] : trainer, cadence, FC, Click.
pub fn kind_rank(kind: DeviceKind) -> usize {
    DeviceKind::ALL
        .iter()
        .position(|&k| k == kind)
        .unwrap_or(usize::MAX)
}

/// Rôle sous lequel ranger un appareil : celui de sa connexion, sinon le
/// premier qu'il annonce.
fn primary_kind(d: &FoundDevice) -> usize {
    d.role
        .or_else(|| d.kinds.first().copied())
        .map_or(usize::MAX, kind_rank)
}

/// Trie la liste affichée : par rôle, puis du signal le plus fort au plus
/// faible, puis par nom.
pub fn sort_devices(devices: &mut [FoundDevice]) {
    devices.sort_by(|a, b| {
        (
            primary_kind(a),
            Reverse(a.rssi.unwrap_or(i16::MIN)),
            &a.name,
            &a.id,
        )
            .cmp(&(
                primary_kind(b),
                Reverse(b.rssi.unwrap_or(i16::MIN)),
                &b.name,
                &b.id,
            ))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LinkState;

    fn device(id: &str, kinds: &[DeviceKind], rssi: Option<i16>) -> FoundDevice {
        FoundDevice {
            id: id.into(),
            name: id.into(),
            kinds: kinds.to_vec(),
            rssi,
            state: LinkState::Disconnected,
            role: None,
            battery: None,
        }
    }

    #[test]
    fn uuid16_matches_btleplug() {
        for id in [0x1826, 0x180D, 0x2A19, 0xFC82] {
            assert_eq!(uuid16(id), btleplug::api::bleuuid::uuid_from_u16(id));
        }
    }

    #[test]
    fn standard_services() {
        let services = [uuid16(ids::FTMS_SERVICE), uuid16(ids::CSC_SERVICE)];
        let ad = Advertisement {
            name: Some("D500 1234"),
            services: &services,
            manufacturer_ids: &[],
        };
        assert_eq!(
            detect_kinds(&ad),
            vec![DeviceKind::Trainer, DeviceKind::Cadence]
        );

        let services = [uuid16(ids::HRS_SERVICE)];
        let ad = Advertisement {
            name: Some("Forerunner 255"),
            services: &services,
            manufacturer_ids: &[0x0087],
        };
        assert_eq!(detect_kinds(&ad), vec![DeviceKind::HeartRate]);
    }

    #[test]
    fn zwift_click_by_service_manufacturer_or_name() {
        let long = [Uuid::parse_str(zwift::SERVICE).unwrap()];
        let short = [uuid16(zwift::SERVICE_16)];
        let by_long = Advertisement {
            services: &long,
            ..Default::default()
        };
        let by_short = Advertisement {
            services: &short,
            ..Default::default()
        };
        let by_maker = Advertisement {
            manufacturer_ids: &[zwift::MANUFACTURER_ID],
            ..Default::default()
        };
        let by_name = Advertisement {
            name: Some("Zwift Click"),
            ..Default::default()
        };
        for ad in [by_long, by_short, by_maker, by_name] {
            assert_eq!(detect_kinds(&ad), vec![DeviceKind::Click]);
        }
    }

    #[test]
    fn unknown_device_has_no_kind() {
        let services = [uuid16(0x180A)];
        let ad = Advertisement {
            name: Some("Casque audio"),
            services: &services,
            manufacturer_ids: &[0x004C],
        };
        assert!(detect_kinds(&ad).is_empty());
    }

    #[test]
    fn sorted_by_kind_then_signal() {
        let mut list = vec![
            device("click", &[DeviceKind::Click], Some(-40)),
            device("fc-loin", &[DeviceKind::HeartRate], Some(-90)),
            device("trainer", &[DeviceKind::Trainer], None),
            device("fc-pres", &[DeviceKind::HeartRate], Some(-50)),
            device("cadence", &[DeviceKind::Cadence], Some(-70)),
        ];
        // Un trainer connecté comme capteur de cadence se range en cadence.
        let mut combo = device(
            "combo",
            &[DeviceKind::Trainer, DeviceKind::Cadence],
            Some(-30),
        );
        combo.role = Some(DeviceKind::Cadence);
        list.push(combo);
        sort_devices(&mut list);
        let order: Vec<&str> = list.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(
            order,
            ["trainer", "combo", "cadence", "fc-pres", "fc-loin", "click"]
        );
    }
}
