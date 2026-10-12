//! Trames Bluetooth des services standard, sans dépendre d'une pile BLE.
//!
//! - FTMS (Fitness Machine Service, 0x1826) : home trainer Van Rysel D500.
//! - CSC (Cycling Speed and Cadence, 0x1816) : capteur de cadence.
//! - HRS (Heart Rate Service, 0x180D) : FC diffusée par la montre Garmin.

pub mod uuid {
    pub const FTMS_SERVICE: u16 = 0x1826;
    pub const FTMS_INDOOR_BIKE_DATA: u16 = 0x2AD2;
    pub const FTMS_CONTROL_POINT: u16 = 0x2AD9;
    pub const CSC_SERVICE: u16 = 0x1816;
    pub const CSC_MEASUREMENT: u16 = 0x2A5B;
    pub const HRS_SERVICE: u16 = 0x180D;
    pub const HR_MEASUREMENT: u16 = 0x2A37;
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ParseError {
    #[error("trame tronquée")]
    Truncated,
}

struct Cursor<'a>(&'a [u8]);

impl Cursor<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], ParseError> {
        let (head, rest) = self.0.split_at_checked(N).ok_or(ParseError::Truncated)?;
        self.0 = rest;
        Ok(head.try_into().unwrap())
    }
    fn u8(&mut self) -> Result<u8, ParseError> {
        Ok(self.take::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16, ParseError> {
        Ok(u16::from_le_bytes(self.take()?))
    }
    fn i16(&mut self) -> Result<i16, ParseError> {
        Ok(i16::from_le_bytes(self.take()?))
    }
    fn u24(&mut self) -> Result<u32, ParseError> {
        let [a, b, c] = self.take()?;
        Ok(u32::from_le_bytes([a, b, c, 0]))
    }
}

/// Données « Indoor Bike Data » (0x2AD2) ; seuls les champs présents sont remplis.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IndoorBikeData {
    pub speed_kmh: Option<f64>,
    pub cadence_rpm: Option<f64>,
    pub distance_m: Option<u32>,
    pub resistance_level: Option<i16>,
    pub power_w: Option<i16>,
    pub heart_rate_bpm: Option<u8>,
}

pub fn parse_indoor_bike_data(data: &[u8]) -> Result<IndoorBikeData, ParseError> {
    let mut c = Cursor(data);
    let flags = c.u16()?;
    let has = |bit: u16| flags & (1 << bit) != 0;
    let mut out = IndoorBikeData::default();
    // Bit 0 « More Data » inversé : à 0, la vitesse instantanée est présente.
    if !has(0) {
        out.speed_kmh = Some(c.u16()? as f64 * 0.01);
    }
    if has(1) {
        c.u16()?; // vitesse moyenne
    }
    if has(2) {
        out.cadence_rpm = Some(c.u16()? as f64 * 0.5);
    }
    if has(3) {
        c.u16()?; // cadence moyenne
    }
    if has(4) {
        out.distance_m = Some(c.u24()?);
    }
    if has(5) {
        out.resistance_level = Some(c.i16()?);
    }
    if has(6) {
        out.power_w = Some(c.i16()?);
    }
    if has(7) {
        c.i16()?; // puissance moyenne
    }
    if has(8) {
        c.take::<5>()?; // énergie totale, par heure, par minute
    }
    if has(9) {
        out.heart_rate_bpm = Some(c.u8()?);
    }
    Ok(out)
}

/// Commandes du point de contrôle FTMS (0x2AD9).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControlCommand {
    RequestControl,
    Reset,
    StartOrResume,
    /// Mode ERG : puissance cible en watts.
    TargetPower(i16),
    /// Mode simulation : le trainer calcule la résistance à partir de la pente.
    Simulation {
        wind_ms: f64,
        grade: f64,
        crr: f64,
        cw_kg_m: f64,
    },
}

impl ControlCommand {
    pub fn encode(&self) -> Vec<u8> {
        match *self {
            Self::RequestControl => vec![0x00],
            Self::Reset => vec![0x01],
            Self::StartOrResume => vec![0x07],
            Self::TargetPower(w) => {
                let mut v = vec![0x05];
                v.extend_from_slice(&w.to_le_bytes());
                v
            }
            Self::Simulation {
                wind_ms,
                grade,
                crr,
                cw_kg_m,
            } => {
                let wind = (wind_ms * 1000.0)
                    .round()
                    .clamp(i16::MIN as f64, i16::MAX as f64) as i16;
                // Pente en centièmes de pourcent : 0,05 → 5 % → 500.
                let grade = (grade * 10_000.0)
                    .round()
                    .clamp(i16::MIN as f64, i16::MAX as f64) as i16;
                let crr = (crr * 10_000.0).round().clamp(0.0, 255.0) as u8;
                let cw = (cw_kg_m * 100.0).round().clamp(0.0, 255.0) as u8;
                let mut v = vec![0x11];
                v.extend_from_slice(&wind.to_le_bytes());
                v.extend_from_slice(&grade.to_le_bytes());
                v.push(crr);
                v.push(cw);
                v
            }
        }
    }
}

/// Mesure brute CSC : compteurs cumulés de tours de manivelle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrankSample {
    pub revolutions: u16,
    /// Horodatage du dernier tour, en 1/1024 s.
    pub event_time: u16,
}

pub fn parse_csc_crank(data: &[u8]) -> Result<Option<CrankSample>, ParseError> {
    let mut c = Cursor(data);
    let flags = c.u8()?;
    if flags & 0x01 != 0 {
        c.take::<6>()?; // données de roue
    }
    if flags & 0x02 == 0 {
        return Ok(None);
    }
    Ok(Some(CrankSample {
        revolutions: c.u16()?,
        event_time: c.u16()?,
    }))
}

/// Cadence entre deux mesures CSC, en gérant le retour à zéro des compteurs.
/// `None` si aucun nouveau tour n'a été enregistré.
pub fn csc_cadence(prev: CrankSample, cur: CrankSample) -> Option<f64> {
    let revs = cur.revolutions.wrapping_sub(prev.revolutions);
    let ticks = cur.event_time.wrapping_sub(prev.event_time);
    (revs > 0 && ticks > 0).then(|| revs as f64 * 60.0 * 1024.0 / ticks as f64)
}

pub fn parse_heart_rate(data: &[u8]) -> Result<u16, ParseError> {
    let mut c = Cursor(data);
    let flags = c.u8()?;
    if flags & 0x01 != 0 {
        c.u16()
    } else {
        Ok(c.u8()? as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indoor_bike_data_speed_cadence_power() {
        // Flags : cadence (bit 2) + puissance (bit 6), vitesse présente.
        let flags: u16 = (1 << 2) | (1 << 6);
        let mut d = flags.to_le_bytes().to_vec();
        d.extend_from_slice(&3250u16.to_le_bytes()); // 32,50 km/h
        d.extend_from_slice(&180u16.to_le_bytes()); // 90 rpm
        d.extend_from_slice(&215i16.to_le_bytes()); // 215 W
        let b = parse_indoor_bike_data(&d).unwrap();
        assert_eq!(b.speed_kmh, Some(32.5));
        assert_eq!(b.cadence_rpm, Some(90.0));
        assert_eq!(b.power_w, Some(215));
        assert_eq!(b.heart_rate_bpm, None);
    }

    #[test]
    fn indoor_bike_data_truncated() {
        assert_eq!(
            parse_indoor_bike_data(&[0x40, 0x00, 0x10]),
            Err(ParseError::Truncated)
        );
    }

    #[test]
    fn simulation_command_bytes() {
        let cmd = ControlCommand::Simulation {
            wind_ms: 0.0,
            grade: 0.05,
            crr: 0.004,
            cw_kg_m: 0.51,
        };
        assert_eq!(cmd.encode(), vec![0x11, 0, 0, 0xF4, 0x01, 40, 51]);
        let down = ControlCommand::Simulation {
            wind_ms: 0.0,
            grade: -0.035,
            crr: 0.004,
            cw_kg_m: 0.51,
        };
        assert_eq!(&down.encode()[3..5], &(-350i16).to_le_bytes());
    }

    #[test]
    fn csc_cadence_with_wraparound() {
        let a = CrankSample {
            revolutions: 65535,
            event_time: 65000,
        };
        // 1 tour en 0,666 s ≈ 90 rpm, en traversant le retour à zéro.
        let b = CrankSample {
            revolutions: 0,
            event_time: 65000u16.wrapping_add(683),
        };
        let rpm = csc_cadence(a, b).unwrap();
        assert!((rpm - 90.0).abs() < 0.2, "{rpm}");
        assert_eq!(csc_cadence(b, b), None);
        let frame = [0x02, 0x34, 0x12, 0x00, 0x04];
        assert_eq!(
            parse_csc_crank(&frame).unwrap(),
            Some(CrankSample {
                revolutions: 0x1234,
                event_time: 0x0400
            })
        );
    }

    #[test]
    fn heart_rate_8_and_16_bits() {
        assert_eq!(parse_heart_rate(&[0x00, 142]).unwrap(), 142);
        assert_eq!(parse_heart_rate(&[0x01, 0x2C, 0x01]).unwrap(), 300);
    }
}
