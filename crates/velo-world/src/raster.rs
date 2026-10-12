//! Petites images couvrant un carré du sol : carte d'occupation (OSM)
//! rastérisée, couleurs de repli quand il n'y a pas d'image aérienne.

use crate::osm::{Cover, LineKind, OsmData};

/// Grille carrée alignée sur le repère local : la ligne 0 est au nord.
#[derive(Debug, Clone)]
pub struct Grid<T> {
    pub size: usize,
    /// Coin ouest et bord nord.
    pub x0: f64,
    pub n1: f64,
    pub px_m: f64,
    pub data: Vec<T>,
}

impl<T: Copy> Grid<T> {
    pub fn new(x0: f64, n1: f64, extent_m: f64, size: usize, fill: T) -> Self {
        Self {
            size,
            x0,
            n1,
            px_m: extent_m / size as f64,
            data: vec![fill; size * size],
        }
    }

    /// Centre du pixel (i, j) dans le repère local.
    pub fn center(&self, i: usize, j: usize) -> (f64, f64) {
        (
            self.x0 + (i as f64 + 0.5) * self.px_m,
            self.n1 - (j as f64 + 0.5) * self.px_m,
        )
    }

    pub fn index_of(&self, x: f64, n: f64) -> Option<usize> {
        let i = ((x - self.x0) / self.px_m).floor();
        let j = ((self.n1 - n) / self.px_m).floor();
        let s = self.size as f64;
        (i >= 0.0 && j >= 0.0 && i < s && j < s).then(|| j as usize * self.size + i as usize)
    }

    pub fn at(&self, x: f64, n: f64) -> Option<T> {
        self.index_of(x, n).map(|k| self.data[k])
    }

    /// Remplit des anneaux (règle pair-impair) avec `value`.
    pub fn fill_rings(&mut self, rings: &[Vec<(f64, f64)>], value: T) {
        let mut xs: Vec<f64> = Vec::new();
        for j in 0..self.size {
            let n = self.n1 - (j as f64 + 0.5) * self.px_m;
            xs.clear();
            for ring in rings {
                for w in ring.windows(2) {
                    let ((ax, an), (bx, bn)) = (w[0], w[1]);
                    if (an <= n) != (bn <= n) {
                        xs.push(ax + (n - an) / (bn - an) * (bx - ax));
                    }
                }
            }
            if xs.len() < 2 {
                continue;
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.chunks_exact(2) {
                let a = ((pair[0] - self.x0) / self.px_m - 0.5).ceil().max(0.0) as usize;
                let b = ((pair[1] - self.x0) / self.px_m - 0.5).floor();
                if b < 0.0 {
                    continue;
                }
                let b = (b as usize).min(self.size - 1);
                if a <= b {
                    self.data[j * self.size + a..=j * self.size + b].fill(value);
                }
            }
        }
    }

    /// Trace une polyligne d'épaisseur `width_m`.
    pub fn draw_line(&mut self, points: &[(f64, f64)], width_m: f64, value: T) {
        let half = (width_m / 2.0).max(self.px_m * 0.5);
        for w in points.windows(2) {
            let ((ax, an), (bx, bn)) = (w[0], w[1]);
            let i0 = ((ax.min(bx) - half - self.x0) / self.px_m).floor().max(0.0) as usize;
            let i1 = ((ax.max(bx) + half - self.x0) / self.px_m).ceil();
            let j0 = ((self.n1 - an.max(bn) - half) / self.px_m).floor().max(0.0) as usize;
            let j1 = ((self.n1 - an.min(bn) + half) / self.px_m).ceil();
            if i1 < 0.0 || j1 < 0.0 {
                continue;
            }
            let i1 = (i1 as usize).min(self.size);
            let j1 = (j1 as usize).min(self.size);
            let (dx, dn) = (bx - ax, bn - an);
            let len2 = (dx * dx + dn * dn).max(1e-9);
            for j in j0..j1 {
                for i in i0..i1 {
                    let (x, n) = self.center(i, j);
                    let t = (((x - ax) * dx + (n - an) * dn) / len2).clamp(0.0, 1.0);
                    let (px, pn) = (ax + dx * t - x, an + dn * t - n);
                    if px * px + pn * pn <= half * half {
                        self.data[j * self.size + i] = value;
                    }
                }
            }
        }
    }
}

/// Carte d'occupation d'un carré du sol à partir d'OSM.
pub fn cover_grid(osm: &OsmData, x0: f64, n1: f64, extent_m: f64, size: usize) -> Grid<u8> {
    let mut g = Grid::new(x0, n1, extent_m, size, Cover::None as u8);
    let (min, max) = ((x0, n1 - extent_m), (x0 + extent_m, n1));
    for a in osm.areas.iter().filter(|a| a.bbox.intersects(min, max)) {
        g.fill_rings(&a.rings, a.cover as u8);
    }
    for l in osm.lines.iter().filter(|l| l.bbox.intersects(min, max)) {
        let c = match l.kind {
            LineKind::Road { paved: true, .. } => Cover::RoadPaved,
            LineKind::Road { paved: false, .. } | LineKind::Path => Cover::RoadUnpaved,
            LineKind::Waterway => Cover::Water,
        };
        g.draw_line(&l.points, l.width_m, c as u8);
    }
    for b in osm.buildings.iter().filter(|b| b.bbox.intersects(min, max)) {
        g.fill_rings(std::slice::from_ref(&b.ring), Cover::Building as u8);
    }
    g
}

fn hash2(a: i64, b: i64) -> u64 {
    let mut h = (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (b as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    h
}

/// Nombre pseudo-aléatoire stable dans [0, 1) pour une case et un canal.
pub fn rand01(ix: i64, iy: i64, channel: i64) -> f64 {
    (hash2(ix.wrapping_mul(7919) ^ channel, iy) >> 11) as f64 / (1u64 << 53) as f64
}

/// Bruit de valeur lissé dans [0, 1].
pub fn noise(x: f64, y: f64) -> f64 {
    let (ix, iy) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let s = |t: f64| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(fx), s(fy));
    let r = |a, b| rand01(a, b, 3);
    let a = r(ix, iy) + (r(ix + 1, iy) - r(ix, iy)) * sx;
    let b = r(ix, iy + 1) + (r(ix + 1, iy + 1) - r(ix, iy + 1)) * sx;
    a + (b - a) * sy
}

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f64) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0) as f32;
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Couleur du sol sans image aérienne : altitude, pente et occupation OSM.
pub fn ground_color(cover: Cover, altitude: f64, slope: f64, x: f64, n: f64) -> [u8; 3] {
    let v = noise(x * 0.02, n * 0.02) * 0.6 + noise(x * 0.11, n * 0.11) * 0.4;
    let grass = mix([0.30, 0.42, 0.17], [0.42, 0.50, 0.22], v);
    let alpine = mix([0.42, 0.46, 0.27], [0.50, 0.50, 0.33], v);
    let rock = mix([0.46, 0.44, 0.41], [0.58, 0.56, 0.52], v);
    let forest = mix([0.10, 0.20, 0.09], [0.16, 0.27, 0.12], v);
    let natural = {
        // Sans données, les versants raides sous la limite des arbres sont
        // le plus souvent boisés dans les Alpes.
        let wooded = smoothstep(
            0.30,
            0.55,
            slope + (noise(x * 0.006, n * 0.006) - 0.5) * 0.3,
        ) * (1.0 - smoothstep(1700.0, 2000.0, altitude));
        let g = mix(grass, alpine, (altitude - 1700.0) / 600.0);
        let g = mix(g, forest, wooded);
        let g = mix(g, rock, (slope - 1.0) / 0.5);
        mix(g, [0.93, 0.94, 0.97], (altitude - 2900.0) / 200.0)
    };
    let c = match cover {
        Cover::None | Cover::Meadow => natural,
        Cover::Farmland => mix(
            [0.55, 0.55, 0.30],
            [0.45, 0.52, 0.24],
            noise(x * 0.01, n * 0.01),
        ),
        Cover::Orchard => mix(grass, [0.25, 0.35, 0.14], 0.5),
        Cover::Urban => mix([0.50, 0.50, 0.46], grass, 0.35 + v * 0.3),
        Cover::Rock => rock,
        Cover::Glacier => mix([0.85, 0.89, 0.95], [0.95, 0.96, 0.98], v),
        Cover::Scrub => mix([0.28, 0.35, 0.16], [0.36, 0.40, 0.20], v),
        Cover::ForestBroad => mix([0.13, 0.25, 0.09], [0.19, 0.31, 0.12], v),
        Cover::ForestMixed => forest,
        Cover::ForestNeedle => mix([0.08, 0.17, 0.09], [0.13, 0.23, 0.12], v),
        Cover::Water => mix([0.16, 0.30, 0.38], [0.20, 0.36, 0.44], v),
        Cover::RoadPaved => [0.30, 0.30, 0.31],
        Cover::RoadUnpaved => [0.55, 0.49, 0.40],
        Cover::Building => [0.62, 0.58, 0.55],
    };
    [
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
    ]
}

/// Un pixel d'image aérienne ressemble-t-il à des arbres (vert sombre) ?
///
/// Les forêts sont nettement plus sombres que les prés, avec plus de vert
/// que de rouge ; les ombres très noires et les toits ou l'eau sont écartés.
pub fn looks_like_trees(rgb: [u8; 3]) -> bool {
    let [r, g, b] = rgb.map(f32::from);
    let lum = (r + g + b) / 3.0;
    let excess_green = 2.0 * g - r - b;
    (20.0..105.0).contains(&lum) && g >= r * 0.98 && g >= b * 0.95 && excess_green > 4.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_a_square_with_a_hole() {
        let mut g: Grid<u8> = Grid::new(0.0, 100.0, 100.0, 10, 0);
        let outer = vec![
            (10.0, 90.0),
            (90.0, 90.0),
            (90.0, 10.0),
            (10.0, 10.0),
            (10.0, 90.0),
        ];
        let hole = vec![
            (40.0, 60.0),
            (60.0, 60.0),
            (60.0, 40.0),
            (40.0, 40.0),
            (40.0, 60.0),
        ];
        g.fill_rings(&[outer, hole], 1);
        assert_eq!(g.at(15.0, 85.0), Some(1));
        assert_eq!(g.at(50.0, 50.0), Some(0));
        assert_eq!(g.at(5.0, 50.0), Some(0));
        assert_eq!(g.data.iter().filter(|&&v| v == 1).count(), 64 - 4);
    }

    #[test]
    fn draws_thick_lines() {
        let mut g: Grid<u8> = Grid::new(0.0, 100.0, 100.0, 100, 0);
        g.draw_line(&[(0.0, 50.0), (100.0, 50.0)], 6.0, 2);
        assert_eq!(g.at(50.0, 52.5), Some(2));
        assert_eq!(g.at(50.0, 54.0), Some(0));
    }

    #[test]
    fn tree_classifier() {
        assert!(looks_like_trees([38, 58, 36]));
        assert!(!looks_like_trees([120, 140, 80]), "pré clair");
        assert!(!looks_like_trees([90, 90, 95]), "toit gris");
        assert!(!looks_like_trees([5, 6, 5]), "ombre noire");
    }
}
