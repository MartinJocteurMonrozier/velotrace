//! Relief provisoire autour du parcours, en attendant le vrai MNT (IGN ou
//! Copernicus) : le sol épouse l'altitude de la route à proximité, puis des
//! collines procédurales apparaissent en s'éloignant.

use crate::route::Route;
use std::collections::HashMap;

/// Taille des cases de l'index spatial (m).
const BUCKET_M: f64 = 50.0;

pub struct Terrain {
    /// (x, y_nord, altitude) des points du parcours.
    points: Vec<(f64, f64, f64)>,
    buckets: HashMap<(i64, i64), Vec<usize>>,
    /// Altitude moyenne du parcours, pour le relief lointain.
    mean_elevation: f64,
}

impl Terrain {
    pub fn new(route: &Route) -> Self {
        let points: Vec<_> = route
            .points
            .iter()
            .map(|p| (p.x, p.y, p.elevation_m))
            .collect();
        let mut buckets: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, &(x, y, _)) in points.iter().enumerate() {
            buckets.entry(key(x, y)).or_default().push(i);
        }
        let mean_elevation = points.iter().map(|p| p.2).sum::<f64>() / points.len().max(1) as f64;
        Self {
            points,
            buckets,
            mean_elevation,
        }
    }

    /// Point du parcours le plus proche : (distance horizontale, altitude).
    ///
    /// On parcourt les cases de l'index en anneaux croissants et on s'arrête
    /// dès qu'aucune case plus lointaine ne peut contenir un point plus proche.
    pub fn nearest(&self, x: f64, y: f64, max_m: f64) -> Option<(f64, f64)> {
        let (cx, cy) = key(x, y);
        let reach = (max_m / BUCKET_M).ceil() as i64 + 1;
        let mut best: Option<(f64, f64)> = None;
        for ring in 0..=reach {
            if let Some((bd, _)) = best {
                if bd <= (ring - 1) as f64 * BUCKET_M {
                    break;
                }
            }
            for dx in -ring..=ring {
                for dy in -ring..=ring {
                    if dx.abs() != ring && dy.abs() != ring {
                        continue;
                    }
                    let Some(ids) = self.buckets.get(&(cx + dx, cy + dy)) else {
                        continue;
                    };
                    for &i in ids {
                        let (px, py, pe) = self.points[i];
                        let d = (px - x).hypot(py - y);
                        if d <= max_m && !matches!(best, Some((bd, _)) if bd <= d) {
                            best = Some((d, pe));
                        }
                    }
                }
            }
        }
        best
    }

    /// Altitude du sol en (x vers l'est, y vers le nord).
    pub fn height(&self, x: f64, y: f64) -> f64 {
        const FAR_M: f64 = 400.0;
        let (dist, near_elev) = self
            .nearest(x, y, FAR_M)
            .unwrap_or((FAR_M, self.mean_elevation));
        // Plateforme de la route : légèrement sous la chaussée.
        let base = near_elev - 0.25;
        // Collines : nulles au bord de la route, pleines à 150 m.
        let t = smoothstep(12.0, 150.0, dist);
        let hills = fbm(x * 0.004, y * 0.004) * 60.0 + fbm(x * 0.02, y * 0.02) * 6.0;
        // Au-delà du couloir, on rejoint doucement l'altitude moyenne.
        let far = smoothstep(250.0, FAR_M, dist);
        let ground = base * (1.0 - far) + self.mean_elevation * far;
        ground + hills.max(-2.0) * t
    }
}

fn key(x: f64, y: f64) -> (i64, i64) {
    ((x / BUCKET_M).floor() as i64, (y / BUCKET_M).floor() as i64)
}

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash(ix: i64, iy: i64) -> f64 {
    let mut h = (ix as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (iy as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h as f64 / u64::MAX as f64) * 2.0 - 1.0
}

/// Bruit de valeur lissé, dans [-1, 1].
fn value_noise(x: f64, y: f64) -> f64 {
    let (ix, iy) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let s = |t: f64| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(fx), s(fy));
    let a = hash(ix, iy) + (hash(ix + 1, iy) - hash(ix, iy)) * sx;
    let b = hash(ix, iy + 1) + (hash(ix + 1, iy + 1) - hash(ix, iy + 1)) * sx;
    a + (b - a) * sy
}

fn fbm(x: f64, y: f64) -> f64 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut f = 1.0;
    for _ in 0..4 {
        sum += value_noise(x * f, y * f) * amp;
        f *= 2.0;
        amp *= 0.5;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_follows_the_road_and_rises_away() {
        let deg = (2000.0 / 6_371_008.8f64).to_degrees();
        let route = Route::from_lat_lon_ele(
            None,
            &[(45.0, 6.0, 300.0), (45.0 + deg, 6.0, 400.0)],
            5.0,
            40.0,
        )
        .unwrap();
        let t = Terrain::new(&route);
        // Sur la route, à mi-parcours : 350 m moins la plateforme.
        assert!(
            (t.height(0.0, 1000.0) - 349.75).abs() < 0.5,
            "{}",
            t.height(0.0, 1000.0)
        );
        let (d, _) = t.nearest(30.0, 1000.0, 100.0).unwrap();
        assert!((d - 30.0).abs() < 0.1);
        assert!(t.nearest(1000.0, 1000.0, 100.0).is_none());
        // Loin de la route, le sol reste fini et proche de l'altitude moyenne.
        let far = t.height(5000.0, 1000.0);
        assert!((250.0..450.0).contains(&far), "{far}");
    }
}
