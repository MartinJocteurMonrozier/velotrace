//! Décodage des messages des Zwift Click (protocole décrit dans
//! [`velo_core::protocol::zwift`]).

use velo_core::devices::Shift;
use velo_core::protocol::zwift;

/// Ce que signifie une notification reçue d'un Click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickMessage {
    /// Le Click a accepté la poignée de main « RideOn ».
    RideOnAck,
    /// Le Click demande un déverrouillage chiffré : il n'enverra rien.
    Locked,
    /// État des boutons : changements de vitesse déclenchés par cet appui
    /// (vide si rien de nouveau ou autre bouton).
    Buttons(Vec<Shift>),
    /// Autre message (batterie, version…), ignoré.
    Other,
}

/// Décodeur d'un Click : se souvient de l'état précédent des boutons pour
/// ne compter que les nouveaux appuis.
#[derive(Debug, Clone)]
pub struct ClickDecoder {
    last: u32,
}

impl Default for ClickDecoder {
    fn default() -> Self {
        // Tous les bits à 1 : aucun bouton appuyé.
        Self { last: u32::MAX }
    }
}

impl ClickDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, data: &[u8]) -> ClickMessage {
        if data.starts_with(zwift::RIDE_ON) {
            return ClickMessage::RideOnAck;
        }
        if data.first() == Some(&0xFF) {
            return ClickMessage::Locked;
        }
        let Some(map) = zwift::keypad_bitmap(data) else {
            return ClickMessage::Other;
        };
        let (up, down) = zwift::shifts(self.last, map);
        self.last = map;
        let mut shifts = Vec::new();
        if up {
            shifts.push(Shift::Up);
        }
        if down {
            shifts.push(Shift::Down);
        }
        ClickMessage::Buttons(shifts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Message 0x23 avec la table `map` en varint protobuf.
    fn keypad(map: u32) -> Vec<u8> {
        let mut out = vec![zwift::KEYPAD_STATUS, 0x08];
        let mut v = map as u64;
        loop {
            let b = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                break;
            }
            out.push(b | 0x80);
        }
        out
    }

    #[test]
    fn handshake_and_lock() {
        let mut d = ClickDecoder::new();
        assert_eq!(d.feed(b"RideOn\x01\x03"), ClickMessage::RideOnAck);
        assert_eq!(d.feed(&[0xFF, 0x04, 0x00]), ClickMessage::Locked);
        assert_eq!(d.feed(&[0x19, 0x10, 0x55]), ClickMessage::Other);
    }

    #[test]
    fn press_hold_release() {
        let idle = u32::MAX;
        let mut d = ClickDecoder::new();
        assert_eq!(d.feed(&keypad(idle)), ClickMessage::Buttons(vec![]));
        let up = idle & !zwift::SHIFT_UP_RIGHT;
        assert_eq!(d.feed(&keypad(up)), ClickMessage::Buttons(vec![Shift::Up]));
        // Bouton maintenu : un seul changement de vitesse.
        assert_eq!(d.feed(&keypad(up)), ClickMessage::Buttons(vec![]));
        assert_eq!(d.feed(&keypad(idle)), ClickMessage::Buttons(vec![]));
        let down = idle & !zwift::SHIFT_DOWN_LEFT;
        assert_eq!(
            d.feed(&keypad(down)),
            ClickMessage::Buttons(vec![Shift::Down])
        );
        // Les deux à la fois.
        let both = idle & !zwift::SHIFT_UP_LEFT & !zwift::SHIFT_DOWN_RIGHT;
        assert_eq!(d.feed(&keypad(idle)), ClickMessage::Buttons(vec![]));
        assert_eq!(
            d.feed(&keypad(both)),
            ClickMessage::Buttons(vec![Shift::Up, Shift::Down])
        );
    }

    #[test]
    fn other_buttons_do_not_shift() {
        let mut d = ClickDecoder::new();
        assert_eq!(d.feed(&keypad(!0x0001)), ClickMessage::Buttons(vec![]));
    }

    #[test]
    fn published_example_decodes() {
        // Exemple de makinolo.com : flèche gauche appuyée.
        let msg = [0x23, 0x08, 0xfe, 0xff, 0xff, 0xff, 0x0f, 0x12, 0x18];
        assert_eq!(keypad(0xFFFF_FFFE)[..7], msg[..7]);
        let mut d = ClickDecoder::new();
        assert_eq!(d.feed(&msg), ClickMessage::Buttons(vec![]));
    }
}
