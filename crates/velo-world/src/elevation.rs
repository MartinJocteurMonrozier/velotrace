//! Relief réel : tuiles d'altitude « terrarium » (PNG, altitude = R·256 + G + B/256 − 32768).

use crate::geo::{mercator_px_x, mercator_px_y};
use std::collections::HashMap;

/// Mosaïque creuse de tuiles d'altitude décodées, à un seul niveau de zoom.
pub struct Dem {
    pub zoom: u8,
    tiles: HashMap<(u32, u32), Vec<f32>>,
}

impl Dem {
    pub fn new(zoom: u8) -> Self {
        Self {
            zoom,
            tiles: HashMap::new(),
        }
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// Ajoute une tuile PNG terrarium ; renvoie `false` si elle est illisible.
    pub fn insert_png(&mut self, x: u32, y: u32, bytes: &[u8]) -> bool {
        let Ok(img) = image::load_from_memory_with_format(bytes, image::ImageFormat::Png) else {
            return false;
        };
        let img = img.to_rgb8();
        if img.width() != 256 || img.height() != 256 {
            return false;
        }
        let heights = img
            .pixels()
            .map(|p| f32::from(p[0]) * 256.0 + f32::from(p[1]) + f32::from(p[2]) / 256.0 - 32768.0)
            .collect();
        self.tiles.insert((x, y), heights);
        true
    }

    /// Ajoute une tuile déjà décodée (256 × 256 altitudes, ligne par ligne).
    pub fn insert_heights(&mut self, x: u32, y: u32, heights: Vec<f32>) {
        assert_eq!(heights.len(), 256 * 256);
        self.tiles.insert((x, y), heights);
    }

    fn pixel(&self, px: i64, py: i64) -> Option<f32> {
        if px < 0 || py < 0 {
            return None;
        }
        let tile = self.tiles.get(&((px >> 8) as u32, (py >> 8) as u32))?;
        Some(tile[((py & 255) * 256 + (px & 255)) as usize])
    }

    /// Altitude interpolée (bilinéaire) en pixels Mercator globaux.
    pub fn sample_px(&self, gx: f64, gy: f64) -> Option<f64> {
        // Chaque valeur décrit le centre de son pixel.
        let (fx, fy) = (gx - 0.5, gy - 0.5);
        let (ix, iy) = (fx.floor() as i64, fy.floor() as i64);
        let (tx, ty) = (fx - ix as f64, fy - iy as f64);
        let h00 = self.pixel(ix, iy)?;
        let h10 = self.pixel(ix + 1, iy).unwrap_or(h00);
        let h01 = self.pixel(ix, iy + 1).unwrap_or(h00);
        let h11 = self.pixel(ix + 1, iy + 1).unwrap_or(h10);
        let a = f64::from(h00) + (f64::from(h10) - f64::from(h00)) * tx;
        let b = f64::from(h01) + (f64::from(h11) - f64::from(h01)) * tx;
        Some(a + (b - a) * ty)
    }

    pub fn sample(&self, lat: f64, lon: f64) -> Option<f64> {
        self.sample_px(mercator_px_x(lon, self.zoom), mercator_px_y(lat, self.zoom))
    }
}

/// Altitudes du parcours tirées du relief : médiane glissante sur ~100 m
/// (retire les pixels de falaise ou de ravin traversés en lacet), puis
/// moyenne sur ~200 m. Réglages vérifiés sur l'Alpe d'Huez : D+ ≈ 1 070 m
/// pour 1 100 m officiels, pentes de 7,5 à 11 % comme sur le terrain.
pub fn smooth_profile(raw: &[f64], step_m: f64) -> Vec<f64> {
    let n = raw.len();
    let half_med = ((50.0 / step_m).round() as usize).max(1);
    let median: Vec<f64> = (0..n)
        .map(|i| {
            let a = i.saturating_sub(half_med);
            let b = (i + half_med).min(n - 1);
            let mut w: Vec<f64> = raw[a..=b].to_vec();
            w.sort_by(f64::total_cmp);
            w[w.len() / 2]
        })
        .collect();
    let half_mean = ((100.0 / step_m).round() as usize).max(1);
    let mut prefix = vec![0.0; n + 1];
    for i in 0..n {
        prefix[i + 1] = prefix[i] + median[i];
    }
    (0..n)
        .map(|i| {
            let a = i.saturating_sub(half_mean);
            let b = (i + half_mean).min(n - 1);
            (prefix[b + 1] - prefix[a]) / (b - a + 1) as f64
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terrarium_png(f: impl Fn(u32, u32) -> f32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(256, 256, |x, y| {
            let v = f(x, y) + 32768.0;
            let r = (v / 256.0).floor();
            let g = (v - r * 256.0).floor();
            let b = ((v - r * 256.0 - g) * 256.0).round();
            image::Rgb([r as u8, g as u8, b as u8])
        });
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn decodes_and_interpolates() {
        let mut dem = Dem::new(13);
        assert!(dem.insert_png(10, 20, &terrarium_png(|x, _| 1000.0 + x as f32)));
        // Centre du pixel 4 : 1004 m ; à mi-chemin des pixels 4 et 5 : 1004,5 m.
        let h = dem
            .sample_px(10.0 * 256.0 + 4.5, 20.0 * 256.0 + 7.5)
            .unwrap();
        assert!((h - 1004.0).abs() < 0.01, "{h}");
        let h = dem
            .sample_px(10.0 * 256.0 + 5.0, 20.0 * 256.0 + 7.5)
            .unwrap();
        assert!((h - 1004.5).abs() < 0.01, "{h}");
        assert!(dem.sample_px(5.0, 5.0).is_none());
    }

    #[test]
    fn smoothing_removes_spikes_but_keeps_the_climb() {
        let mut raw: Vec<f64> = (0..400).map(|i| i as f64 * 5.0 * 0.08).collect();
        raw[200] += 40.0;
        let s = smooth_profile(&raw, 5.0);
        assert!((s[200] - raw[199]).abs() < 1.0, "{} {}", s[200], raw[199]);
        let grade = (s[300] - s[100]) / 1000.0;
        assert!((grade - 0.08).abs() < 1e-6, "{grade}");
    }
}
