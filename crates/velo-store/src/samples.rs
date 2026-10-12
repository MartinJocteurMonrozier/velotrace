//! Échantillons d'une sortie rangés en un seul BLOB compact, par colonnes.
//!
//! Format (tous les nombres en petit-boutiste) :
//!
//! | octets      | contenu                                              |
//! |-------------|------------------------------------------------------|
//! | 4           | signature `VTS1`                                     |
//! | 4           | `n`, nombre d'échantillons (u32)                     |
//! | 4·n         | `t_s` (f32)                                          |
//! | 4·n         | `distance_m` (f32)                                   |
//! | 8·n         | `lat` (f64)                                          |
//! | 8·n         | `lon` (f64)                                          |
//! | 4·n         | `elevation_m` (f32)                                  |
//! | 4·n         | `speed_ms` (f32)                                     |
//! | 4·n         | `power_w` (f32)                                      |
//! | 4·n         | `cadence_rpm` (f32)                                  |
//! | 2·n         | `heart_rate` (u16, `0xFFFF` = pas de cardio)         |
//! | 4·n         | `grade` (f32)                                        |
//!
//! Soit 46 octets par seconde : environ 660 Ko pour quatre heures. Les
//! coordonnées gardent la précision f64 ; le reste, mesuré à quelques
//! centimètres ou dixièmes de watt près, tient sans perte utile en f32.

use velo_core::record::Sample;

use crate::{Result, StoreError};

const MAGIC: &[u8; 4] = b"VTS1";
const HEADER: usize = 8;
const BYTES_PER_SAMPLE: usize = 7 * 4 + 2 * 8 + 2;
const NO_HEART_RATE: u16 = u16::MAX;

/// Code les échantillons au format décrit plus haut.
pub fn encode_samples(samples: &[Sample]) -> Vec<u8> {
    let n = samples.len();
    let mut out = Vec::with_capacity(HEADER + n * BYTES_PER_SAMPLE);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(n as u32).to_le_bytes());
    let f32_col = |out: &mut Vec<u8>, f: fn(&Sample) -> f64| {
        for s in samples {
            out.extend_from_slice(&(f(s) as f32).to_le_bytes());
        }
    };
    let f64_col = |out: &mut Vec<u8>, f: fn(&Sample) -> f64| {
        for s in samples {
            out.extend_from_slice(&f(s).to_le_bytes());
        }
    };
    f32_col(&mut out, |s| s.t_s);
    f32_col(&mut out, |s| s.distance_m);
    f64_col(&mut out, |s| s.lat);
    f64_col(&mut out, |s| s.lon);
    f32_col(&mut out, |s| s.elevation_m);
    f32_col(&mut out, |s| s.speed_ms);
    f32_col(&mut out, |s| s.power_w);
    f32_col(&mut out, |s| s.cadence_rpm);
    for s in samples {
        // 0xFFFF est réservé à l'absence de mesure.
        let hr = s
            .heart_rate
            .map_or(NO_HEART_RATE, |h| h.min(NO_HEART_RATE - 1));
        out.extend_from_slice(&hr.to_le_bytes());
    }
    f32_col(&mut out, |s| s.grade);
    out
}

/// Décode un BLOB produit par [`encode_samples`].
pub fn decode_samples(data: &[u8]) -> Result<Vec<Sample>> {
    if data.len() < HEADER || &data[..4] != MAGIC {
        return Err(StoreError::CorruptSamples("signature inconnue".into()));
    }
    let n = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
    if data.len() != HEADER + n * BYTES_PER_SAMPLE {
        return Err(StoreError::CorruptSamples(format!(
            "{} octets pour {n} échantillons",
            data.len()
        )));
    }
    let mut out = vec![Sample::default(); n];
    let mut pos = HEADER;
    let mut f32_col = |pos: &mut usize, set: fn(&mut Sample, f64)| {
        for (i, chunk) in data[*pos..*pos + 4 * n].chunks_exact(4).enumerate() {
            set(
                &mut out[i],
                f32::from_le_bytes(chunk.try_into().unwrap()) as f64,
            );
        }
        *pos += 4 * n;
    };
    f32_col(&mut pos, |s, v| s.t_s = v);
    f32_col(&mut pos, |s, v| s.distance_m = v);
    let lat_pos = pos;
    pos += 16 * n;
    f32_col(&mut pos, |s, v| s.elevation_m = v);
    f32_col(&mut pos, |s, v| s.speed_ms = v);
    f32_col(&mut pos, |s, v| s.power_w = v);
    f32_col(&mut pos, |s, v| s.cadence_rpm = v);
    let hr_pos = pos;
    pos += 2 * n;
    f32_col(&mut pos, |s, v| s.grade = v);
    let f64_at = |p: usize| f64::from_le_bytes(data[p..p + 8].try_into().unwrap());
    for (i, s) in out.iter_mut().enumerate() {
        s.lat = f64_at(lat_pos + 8 * i);
        s.lon = f64_at(lat_pos + 8 * (n + i));
        let hr = u16::from_le_bytes(data[hr_pos + 2 * i..hr_pos + 2 * i + 2].try_into().unwrap());
        s.heart_rate = (hr != NO_HEART_RATE).then_some(hr);
    }
    Ok(out)
}
