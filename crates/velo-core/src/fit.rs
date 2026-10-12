//! Écriture d'une sortie au format FIT de Garmin, accepté par Strava,
//! Garmin Connect, Coros, Intervals.icu…
//!
//! Seul le strict nécessaire du protocole est écrit : en-tête de 14 octets,
//! messages de définition et de données en petit-boutiste, CRC final.
//! Messages : `file_id`, `sport`, `event` (départ), un `record` par
//! échantillon, `event` (arrêt), `lap`, `session` et `activity`.

use crate::record::{Sample, Summary};

/// Écart entre l'époque Unix et celle de FIT (31 décembre 1989, 0 h UTC).
pub const FIT_EPOCH_OFFSET_S: i64 = 631_065_600;

const PROTOCOL_VERSION: u8 = 0x20;
const PROFILE_VERSION: u16 = 2132;

mod global {
    pub const FILE_ID: u16 = 0;
    pub const SPORT: u16 = 12;
    pub const SESSION: u16 = 18;
    pub const LAP: u16 = 19;
    pub const RECORD: u16 = 20;
    pub const EVENT: u16 = 21;
    pub const ACTIVITY: u16 = 34;
}

const SPORT_CYCLING: u8 = 2;
const SUB_SPORT_VIRTUAL_ACTIVITY: u8 = 58;
const MANUFACTURER_DEVELOPMENT: u16 = 255;
const EVENT_TIMER: u8 = 0;
const EVENT_LAP: u8 = 9;
const EVENT_SESSION: u8 = 8;
const EVENT_ACTIVITY: u8 = 26;
const EVENT_TYPE_START: u8 = 0;
const EVENT_TYPE_STOP: u8 = 1;
const EVENT_TYPE_STOP_ALL: u8 = 4;

/// Valeur d'un champ, avec son type de base FIT.
#[derive(Debug, Clone)]
enum V {
    Enum(u8),
    U8(u8),
    U16(u16),
    U32(u32),
    U32z(u32),
    I32(i32),
    /// Chaîne UTF-8 terminée par un zéro, taille fixe.
    Str(Vec<u8>),
}

impl V {
    fn base_type(&self) -> u8 {
        match self {
            V::Enum(_) => 0x00,
            V::U8(_) => 0x02,
            V::U16(_) => 0x84,
            V::U32(_) => 0x86,
            V::U32z(_) => 0x8C,
            V::I32(_) => 0x85,
            V::Str(_) => 0x07,
        }
    }

    fn size(&self) -> u8 {
        match self {
            V::Enum(_) | V::U8(_) => 1,
            V::U16(_) => 2,
            V::U32(_) | V::U32z(_) | V::I32(_) => 4,
            V::Str(s) => s.len() as u8,
        }
    }

    fn write(&self, out: &mut Vec<u8>) {
        match self {
            V::Enum(v) | V::U8(v) => out.push(*v),
            V::U16(v) => out.extend_from_slice(&v.to_le_bytes()),
            V::U32(v) | V::U32z(v) => out.extend_from_slice(&v.to_le_bytes()),
            V::I32(v) => out.extend_from_slice(&v.to_le_bytes()),
            V::Str(s) => out.extend_from_slice(s),
        }
    }
}

/// Champs (numéro, taille, type de base) d'une définition.
type Layout = Vec<(u8, u8, u8)>;

struct Writer {
    body: Vec<u8>,
    /// Définition active pour chaque type local (0 à 15).
    defs: [Option<(u16, Layout)>; 16],
}

impl Writer {
    fn new() -> Self {
        Self {
            body: Vec::new(),
            defs: Default::default(),
        }
    }

    /// Écrit un message de données, précédé de sa définition si elle a changé.
    fn message(&mut self, local: u8, global: u16, fields: &[(u8, V)]) {
        let layout: Layout = fields
            .iter()
            .map(|(n, v)| (*n, v.size(), v.base_type()))
            .collect();
        let slot = &mut self.defs[local as usize];
        if slot.as_ref() != Some(&(global, layout.clone())) {
            self.body.push(0x40 | local);
            self.body.push(0); // réservé
            self.body.push(0); // petit-boutiste
            self.body.extend_from_slice(&global.to_le_bytes());
            self.body.push(layout.len() as u8);
            for (num, size, base) in &layout {
                self.body.extend_from_slice(&[*num, *size, *base]);
            }
            *slot = Some((global, layout));
        }
        self.body.push(local);
        for (_, v) in fields {
            v.write(&mut self.body);
        }
    }

    fn finish(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.body.len() + 16);
        out.push(14);
        out.push(PROTOCOL_VERSION);
        out.extend_from_slice(&PROFILE_VERSION.to_le_bytes());
        out.extend_from_slice(&(self.body.len() as u32).to_le_bytes());
        out.extend_from_slice(b".FIT");
        let header_crc = crc16(&out);
        out.extend_from_slice(&header_crc.to_le_bytes());
        out.extend_from_slice(&self.body);
        let file_crc = crc16(&out);
        out.extend_from_slice(&file_crc.to_le_bytes());
        out
    }
}

/// CRC-16 du protocole FIT.
pub fn crc16(data: &[u8]) -> u16 {
    const TABLE: [u16; 16] = [
        0x0000, 0xCC01, 0xD801, 0x1400, 0xF001, 0x3C00, 0x2800, 0xE401, 0xA001, 0x6C00, 0x7800,
        0xB401, 0x5000, 0x9C01, 0x8801, 0x4400,
    ];
    let mut crc: u16 = 0;
    for &byte in data {
        let tmp = TABLE[(crc & 0xF) as usize];
        crc = (crc >> 4) & 0x0FFF;
        crc = crc ^ tmp ^ TABLE[(byte & 0xF) as usize];
        let tmp = TABLE[(crc & 0xF) as usize];
        crc = (crc >> 4) & 0x0FFF;
        crc = crc ^ tmp ^ TABLE[(byte >> 4) as usize];
    }
    crc
}

/// Horodatage FIT (secondes depuis 1989-12-31 00:00:00 UTC).
fn fit_time(unix: i64) -> u32 {
    (unix - FIT_EPOCH_OFFSET_S).clamp(0, u32::MAX as i64 - 1) as u32
}

/// Degrés en semi-cercles (2³¹ semi-cercles = 180°).
fn semicircles(deg: f64) -> i32 {
    (deg * (2f64.powi(31) / 180.0))
        .round()
        .clamp(-(i32::MAX as f64), i32::MAX as f64 - 1.0) as i32
}

fn u8_of(v: f64) -> u8 {
    v.round().clamp(0.0, 254.0) as u8
}

fn u16_of(v: f64) -> u16 {
    v.round().clamp(0.0, 65_534.0) as u16
}

fn u32_of(v: f64) -> u32 {
    v.round().clamp(0.0, u32::MAX as f64 - 1.0) as u32
}

/// Altitude : échelle 5, décalage 500 m.
fn altitude(m: f64) -> V {
    V::U16(u16_of((m + 500.0) * 5.0))
}

fn hr(v: Option<f64>) -> V {
    V::U8(v.map_or(0xFF, u8_of))
}

/// Nom en chaîne FIT : 63 octets au plus, coupé sur un caractère entier.
fn fit_string(s: &str) -> Vec<u8> {
    let mut end = s.len().min(63);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut bytes = s.as_bytes()[..end].to_vec();
    bytes.push(0);
    bytes
}

/// Encode une sortie en fichier FIT d'activité (vélo, sortie virtuelle).
///
/// `start_unix` : début de la sortie (s depuis 1970, UTC) ; les échantillons
/// sont horodatés à `start_unix + t_s`. `name` est écrit dans le message
/// `sport` (les sites d'envoi prennent le nom de l'activité à part).
pub fn encode_activity(
    start_unix: i64,
    samples: &[Sample],
    summary: &Summary,
    name: &str,
) -> Vec<u8> {
    let start = fit_time(start_unix);
    let last_t = samples.last().map_or(0.0, |s| s.t_s);
    let end = start.saturating_add(u32_of(last_t));
    let elapsed_s = summary.duration_s.max(last_t);
    let timer = V::U32(u32_of(elapsed_s * 1000.0));
    let first = samples.first().copied().unwrap_or_default();
    let last = samples.last().copied().unwrap_or_default();
    let max_speed = samples.iter().map(|s| s.speed_ms).fold(0.0, f64::max);
    let max_cadence = samples.iter().map(|s| s.cadence_rpm).fold(0.0, f64::max);
    let cycles: f64 = samples.iter().map(|s| s.cadence_rpm / 60.0).sum();
    // Rendement musculaire ≈ 24 % : 1 kJ mécanique ≈ 1 kcal dépensée.
    let calories = u16_of(summary.work_kj);
    let avg_hr = summary.avg_heart_rate;
    let max_hr = summary.max_heart_rate.map(|h| h as f64);

    let mut w = Writer::new();
    w.message(
        0,
        global::FILE_ID,
        &[
            (0, V::Enum(4)), // activité
            (1, V::U16(MANUFACTURER_DEVELOPMENT)),
            (2, V::U16(1)),
            (3, V::U32z(start.max(1))),
            (4, V::U32(start)),
        ],
    );
    w.message(
        1,
        global::SPORT,
        &[
            (0, V::Enum(SPORT_CYCLING)),
            (1, V::Enum(SUB_SPORT_VIRTUAL_ACTIVITY)),
            (3, V::Str(fit_string(name))),
        ],
    );
    let event = |time: u32, event_type: u8| {
        [
            (253, V::U32(time)),
            (0, V::Enum(EVENT_TIMER)),
            (1, V::Enum(event_type)),
            (4, V::U8(0)),
        ]
    };
    w.message(2, global::EVENT, &event(start, EVENT_TYPE_START));
    for s in samples {
        w.message(
            3,
            global::RECORD,
            &[
                (253, V::U32(start.saturating_add(u32_of(s.t_s)))),
                (0, V::I32(semicircles(s.lat))),
                (1, V::I32(semicircles(s.lon))),
                (2, altitude(s.elevation_m)),
                (3, hr(s.heart_rate.map(|h| h as f64))),
                (4, V::U8(u8_of(s.cadence_rpm))),
                (5, V::U32(u32_of(s.distance_m * 100.0))),
                (6, V::U16(u16_of(s.speed_ms * 1000.0))),
                (7, V::U16(u16_of(s.power_w))),
            ],
        );
    }
    w.message(2, global::EVENT, &event(end, EVENT_TYPE_STOP_ALL));

    // Champs communs au tour unique et à la session.
    let distance = V::U32(u32_of(summary.distance_m * 100.0));
    let avg_speed = V::U16(u16_of(summary.avg_speed_ms * 1000.0));
    let max_speed = V::U16(u16_of(max_speed * 1000.0));
    let avg_cadence = V::U8(u8_of(summary.avg_cadence_rpm));
    let max_cadence = V::U8(u8_of(max_cadence));
    let avg_power = V::U16(u16_of(summary.avg_power_w));
    let max_power = V::U16(u16_of(summary.max_power_w));
    let ascent = V::U16(u16_of(summary.elevation_gain_m));
    let np = V::U16(u16_of(summary.normalized_power_w));
    let work = V::U32(u32_of(summary.work_kj * 1000.0));
    w.message(
        4,
        global::LAP,
        &[
            (254, V::U16(0)),
            (253, V::U32(end)),
            (0, V::Enum(EVENT_LAP)),
            (1, V::Enum(EVENT_TYPE_STOP)),
            (2, V::U32(start)),
            (3, V::I32(semicircles(first.lat))),
            (4, V::I32(semicircles(first.lon))),
            (5, V::I32(semicircles(last.lat))),
            (6, V::I32(semicircles(last.lon))),
            (7, timer.clone()),
            (8, timer.clone()),
            (9, distance.clone()),
            (10, V::U32(u32_of(cycles))),
            (11, V::U16(calories)),
            (13, avg_speed.clone()),
            (14, max_speed.clone()),
            (15, hr(avg_hr)),
            (16, hr(max_hr)),
            (17, avg_cadence.clone()),
            (18, max_cadence.clone()),
            (19, avg_power.clone()),
            (20, max_power.clone()),
            (21, ascent.clone()),
            (24, V::Enum(7)), // fin de session
            (25, V::Enum(SPORT_CYCLING)),
            (33, np.clone()),
            (39, V::Enum(SUB_SPORT_VIRTUAL_ACTIVITY)),
            (41, work.clone()),
        ],
    );
    w.message(
        5,
        global::SESSION,
        &[
            (254, V::U16(0)),
            (253, V::U32(end)),
            (0, V::Enum(EVENT_SESSION)),
            (1, V::Enum(EVENT_TYPE_STOP)),
            (2, V::U32(start)),
            (3, V::I32(semicircles(first.lat))),
            (4, V::I32(semicircles(first.lon))),
            (5, V::Enum(SPORT_CYCLING)),
            (6, V::Enum(SUB_SPORT_VIRTUAL_ACTIVITY)),
            (7, timer.clone()),
            (8, timer.clone()),
            (9, distance),
            (10, V::U32(u32_of(cycles))),
            (11, V::U16(calories)),
            (14, avg_speed),
            (15, max_speed),
            (16, hr(avg_hr)),
            (17, hr(max_hr)),
            (18, avg_cadence),
            (19, max_cadence),
            (20, avg_power),
            (21, max_power),
            (22, ascent),
            (25, V::U16(0)),  // premier tour
            (26, V::U16(1)),  // nombre de tours
            (28, V::Enum(0)), // fin d'activité
            (34, np),
            (35, V::U16(u16_of(summary.tss * 10.0))),
            (36, V::U16(u16_of(summary.intensity_factor * 1000.0))),
            (48, work),
        ],
    );
    w.message(
        6,
        global::ACTIVITY,
        &[
            (253, V::U32(end)),
            (0, timer),
            (1, V::U16(1)),
            (2, V::Enum(0)), // manuelle
            (3, V::Enum(EVENT_ACTIVITY)),
            (4, V::Enum(EVENT_TYPE_STOP)),
        ],
    );
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::summarize;
    use std::collections::HashMap;

    /// Message décodé : numéro global et octets bruts de chaque champ.
    struct Msg {
        global: u16,
        fields: HashMap<u8, Vec<u8>>,
    }

    impl Msg {
        fn u(&self, num: u8) -> u64 {
            let b = &self.fields[&num];
            let mut a = [0u8; 8];
            a[..b.len()].copy_from_slice(b);
            u64::from_le_bytes(a)
        }
        fn i32(&self, num: u8) -> i32 {
            i32::from_le_bytes(self.fields[&num][..4].try_into().unwrap())
        }
    }

    /// Petit décodeur : vérifie les CRC puis parcourt définitions et données.
    fn decode(file: &[u8]) -> Vec<Msg> {
        assert_eq!(file[0], 14);
        assert_eq!(file[1], 0x20);
        assert_eq!(&file[8..12], b".FIT");
        let header_crc = u16::from_le_bytes([file[12], file[13]]);
        assert_eq!(crc16(&file[..12]), header_crc);
        let size = u32::from_le_bytes(file[4..8].try_into().unwrap()) as usize;
        assert_eq!(file.len(), 14 + size + 2);
        let file_crc = u16::from_le_bytes([file[14 + size], file[15 + size]]);
        assert_eq!(crc16(&file[..14 + size]), file_crc);
        // Propriété du CRC : il s'annule sur les données suivies de lui-même.
        assert_eq!(crc16(file), 0);

        let data = &file[14..14 + size];
        let mut defs: HashMap<u8, (u16, Vec<(u8, usize)>)> = HashMap::new();
        let mut out = Vec::new();
        let mut i = 0;
        while i < data.len() {
            let h = data[i];
            i += 1;
            assert_eq!(h & 0x80, 0, "pas d'en-tête compressé");
            let local = h & 0x0F;
            if h & 0x40 != 0 {
                assert_eq!(h & 0x20, 0, "pas de données de développeur");
                assert_eq!(data[i + 1], 0, "petit-boutiste");
                let global = u16::from_le_bytes([data[i + 2], data[i + 3]]);
                let n = data[i + 4] as usize;
                i += 5;
                let fields = (0..n)
                    .map(|k| (data[i + 3 * k], data[i + 3 * k + 1] as usize))
                    .collect();
                i += 3 * n;
                defs.insert(local, (global, fields));
            } else {
                let (global, fields) = defs.get(&local).expect("définition manquante");
                let mut m = Msg {
                    global: *global,
                    fields: HashMap::new(),
                };
                for (num, size) in fields {
                    m.fields.insert(*num, data[i..i + size].to_vec());
                    i += size;
                }
                out.push(m);
            }
        }
        assert_eq!(i, data.len());
        out
    }

    fn samples(n: usize) -> Vec<Sample> {
        (0..n)
            .map(|i| Sample {
                t_s: i as f64,
                distance_m: i as f64 * 8.5,
                lat: 45.1885 + i as f64 * 1e-5,
                lon: 5.7245,
                elevation_m: 212.0 + i as f64 * 0.1,
                speed_ms: 8.5,
                power_w: 200.0 + (i % 7) as f64,
                cadence_rpm: 90.0,
                heart_rate: (i > 0).then_some(130),
                grade: 0.01,
            })
            .collect()
    }

    #[test]
    fn crc_of_known_vector() {
        // Valeur de référence du CRC-16/ARC, que FIT utilise.
        assert_eq!(crc16(b"123456789"), 0xBB3D);
    }

    #[test]
    fn activity_file_decodes() {
        let s = samples(120);
        let sum = summarize(&s, 250.0);
        let start = 1_760_000_000;
        let file = encode_activity(start, &s, &sum, "Col de Porte");
        let msgs = decode(&file);

        let kinds: Vec<u16> = msgs.iter().map(|m| m.global).collect();
        assert_eq!(kinds[0], global::FILE_ID);
        assert_eq!(*kinds.last().unwrap(), global::ACTIVITY);
        let records: Vec<&Msg> = msgs.iter().filter(|m| m.global == global::RECORD).collect();
        assert_eq!(records.len(), s.len());

        let file_id = &msgs[0];
        assert_eq!(file_id.u(0), 4);
        assert_eq!(file_id.u(1), 255);

        let r0 = records[0];
        assert_eq!(r0.u(253), (start - FIT_EPOCH_OFFSET_S) as u64);
        assert_eq!(r0.u(7), 200);
        assert_eq!(r0.u(4), 90);
        assert_eq!(r0.u(3), 0xFF, "FC absente");
        assert_eq!(r0.u(6), 8500);
        assert_eq!(r0.u(2), ((212.0 + 500.0) * 5.0) as u64);
        let lat = r0.i32(0) as f64 * 180.0 / 2f64.powi(31);
        assert!((lat - 45.1885).abs() < 1e-6, "{lat}");
        let r9 = records[9];
        assert_eq!(r9.u(253), r0.u(253) + 9);
        assert_eq!(r9.u(7), 202);
        assert_eq!(r9.u(3), 130);
        assert_eq!(r9.u(5), 9 * 850);

        let session = msgs.iter().find(|m| m.global == global::SESSION).unwrap();
        assert_eq!(session.u(5), 2);
        assert_eq!(session.u(6), 58);
        assert_eq!(session.u(7), 120_000);
        assert_eq!(session.u(9), (119.0 * 8.5 * 100.0) as u64);
        assert_eq!(session.u(20), sum.avg_power_w.round() as u64);
        assert_eq!(session.u(17), 130);
        assert_eq!(session.u(35), (sum.tss * 10.0).round() as u64);
        assert_eq!(
            session.u(36),
            (sum.intensity_factor * 1000.0).round() as u64
        );
        assert_eq!(session.u(48), (sum.work_kj * 1000.0).round() as u64);

        let sport = msgs.iter().find(|m| m.global == global::SPORT).unwrap();
        assert_eq!(sport.fields[&3], b"Col de Porte\0");
        let events: Vec<&Msg> = msgs.iter().filter(|m| m.global == global::EVENT).collect();
        assert_eq!(events.len(), 2);
        assert_eq!((events[0].u(1), events[1].u(1)), (0, 4));
        let activity = msgs.last().unwrap();
        assert_eq!(activity.u(1), 1);
        assert_eq!(activity.u(3), 26);
    }

    #[test]
    fn empty_ride_is_still_a_valid_file() {
        let file = encode_activity(1_760_000_000, &[], &Summary::default(), "");
        let msgs = decode(&file);
        assert_eq!(
            msgs.iter().filter(|m| m.global == global::RECORD).count(),
            0
        );
        assert!(msgs.iter().any(|m| m.global == global::SESSION));
    }

    #[test]
    fn long_names_are_cut_on_a_char_boundary() {
        let name = "é".repeat(40);
        let b = fit_string(&name);
        assert_eq!(b.len(), 63);
        assert!(std::str::from_utf8(&b[..62]).is_ok());
    }
}
