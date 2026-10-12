//! La route vue en 3D : tracé lissé échantillonné tous les mètres, chaussée
//! avec accotements ou trottoirs, et trajectoire du cycliste (à droite, à la
//! corde dans les virages sur le plat et en descente).

use crate::mesh::{Instances, MeshData};
use velo_core::route::Route;

/// Demi-largeur de la chaussée par défaut (m).
pub const HALF_WIDTH: f64 = 3.0;
/// Largeur de chaque accotement par défaut (m).
pub const VERGE: f64 = 1.3;
/// Le bord extérieur de l'accotement descend un peu pour rejoindre le sol.
const VERGE_DROP: f64 = 0.25;
/// Hauteur d'une bordure de trottoir (m).
pub const KERB: f64 = 0.14;
/// Pas d'échantillonnage du tracé lissé (m).
pub const STEP: f64 = 1.0;
/// Le cycliste roule à ~1,1 m du bord droit de la chaussée.
const FROM_EDGE: f64 = 1.1;
/// Déport vers l'intérieur par unité de courbure (m · m) : un virage de
/// 30 m de rayon fait gagner ~1,6 m vers la corde.
const APEX_GAIN: f64 = 50.0;

/// Marquages et particularités d'une section de chaussée (bits).
pub mod marks {
    /// Axe discontinu (3 m peints, 10 m de vide).
    pub const CENTRE_DASH: u8 = 1;
    /// Axe continu (virages sans visibilité, routes à 4 voies).
    pub const CENTRE_SOLID: u8 = 2;
    pub const EDGE_LEFT: u8 = 4;
    pub const EDGE_RIGHT: u8 = 8;
    /// Ligne de rive discontinue : débouché d'une route à gauche ou à droite.
    pub const GAP_LEFT: u8 = 16;
    pub const GAP_RIGHT: u8 = 32;
    /// Ligne « cédez le passage » en travers, au début d'une amorce.
    pub const GIVE_WAY: u8 = 64;
    /// Pelouse (îlot central de rond-point).
    pub const GRASS: u8 = 128;
}

/// Coupe en travers de la chaussée en un point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    pub half_width: f64,
    /// Largeur des trottoirs à gauche et à droite (0 : pas de trottoir).
    pub sidewalk: [f64; 2],
    /// Largeur des accotements au-delà (herbe, gravier).
    pub verge: [f64; 2],
    /// Nombre de voies ; 0 pour une piste non revêtue.
    pub lanes: u8,
    pub oneway: bool,
    pub marks: u8,
    /// Section recouverte par autre chose (anneau d'un rond-point) : pas dessinée.
    pub hidden: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            half_width: HALF_WIDTH,
            sidewalk: [0.0; 2],
            verge: [VERGE; 2],
            lanes: 2,
            oneway: false,
            marks: marks::CENTRE_DASH | marks::EDGE_LEFT | marks::EDGE_RIGHT,
            hidden: false,
        }
    }
}

impl Profile {
    /// Distance de l'axe au bout de l'accotement, côté 0 (gauche) ou 1 (droite).
    pub fn reach(&self, side: usize) -> f64 {
        self.half_width + self.sidewalk[side] + self.verge[side]
    }

    /// Écart habituel du cycliste à droite de l'axe, et bornes de sa trajectoire.
    fn rider_band(&self) -> (f64, f64, f64) {
        let hw = self.half_width;
        let keep = (hw - FROM_EDGE).max(0.3);
        let (lo, hi) = if self.oneway || hw < 2.3 {
            (-hw + 0.9, hw - 0.4)
        } else {
            (0.6, hw - 0.3)
        };
        (keep.clamp(lo, hi.max(lo)), lo, hi.max(lo))
    }
}

/// Une section du ruban de chaussée à dessiner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Section {
    pub x: f64,
    pub n: f64,
    /// Altitude absolue de la chaussée.
    pub elevation: f64,
    /// Vecteur unitaire vers la droite.
    pub right: (f64, f64),
    /// Abscisse le long du ruban (m), pour les motifs.
    pub s: f64,
    pub profile: Profile,
}

/// Ajoute au maillage un ruban de chaussée : accotement, trottoir et bordure
/// de chaque côté (8 sommets par section, trottoir de largeur nulle s'il n'y
/// en a pas). Coordonnées Godot, altitude relative à `base`.
/// UV.x : écart à l'axe (m, + à droite) ; UV.y : abscisse (m).
/// UV2 : demi-largeur (m), nombre de voies (négatif en sens unique).
/// Couleur : trottoirs gauche et droit (m / 4), marquages (bits / 255).
pub fn push_strip(mesh: &mut MeshData, secs: &[Section], base: f64) {
    if secs.len() < 2 {
        return;
    }
    let first = mesh.positions.len() as i32;
    for p in secs {
        let pr = &p.profile;
        let hw = pr.half_width;
        let y = p.elevation - base + 0.05;
        // 0 voie : piste non revêtue.
        let lanes = f32::from(pr.lanes) * if pr.oneway { -1.0 } else { 1.0 };
        let color = [
            (pr.sidewalk[0] / 4.0) as f32,
            (pr.sidewalk[1] / 4.0) as f32,
            0.0,
            f32::from(pr.marks) / 255.0,
        ];
        let mut verts = Vec::with_capacity(8);
        for side in [0usize, 1] {
            let sg = if side == 0 { -1.0 } else { 1.0 };
            let w = pr.sidewalk[side];
            let kerb = KERB * smoothstep(0.0, 0.4, w);
            let mut one = [
                // (écart réel, hauteur, écart pour le motif)
                (
                    hw + w + pr.verge[side],
                    -VERGE_DROP,
                    hw + w + pr.verge[side],
                ),
                (hw + w, kerb, hw + w),
                (hw, kerb, hw + w.min(0.15)),
                (hw, 0.0, hw),
            ];
            if side == 1 {
                one.reverse();
            }
            for (off, dy, u) in one {
                verts.push((sg * off, dy, sg * u));
            }
        }
        let (rx, rn) = p.right;
        for (off, dy, u) in verts {
            let (x, n) = (p.x + rx * off, p.n + rn * off);
            mesh.positions.push([x as f32, (y + dy) as f32, -n as f32]);
            mesh.normals.push([0.0, 1.0, 0.0]);
            mesh.uvs.push([u as f32, p.s as f32]);
            mesh.uv2s.push([hw as f32, lanes]);
            mesh.colors.push(color);
        }
    }
    for k in 0..secs.len() - 1 {
        if secs[k].profile.hidden || secs[k + 1].profile.hidden {
            continue;
        }
        let (rx, rn) = secs[k].right;
        let a = first + k as i32 * 8;
        for c in 0..7 {
            // Bordures verticales : face tournée vers la chaussée.
            let normal = match c {
                2 => [rx, 0.0, -rn],
                4 => [-rx, 0.0, rn],
                _ => [0.0, 1.0, 0.0],
            };
            let (l0, r0, l1, r1) = (a + c, a + c + 1, a + c + 8, a + c + 9);
            mesh.push_tri(l0, l1, r0, normal);
            mesh.push_tri(r0, l1, r1, normal);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathSample {
    pub x: f64,
    pub n: f64,
    pub elevation: f64,
    /// Distance correspondante sur le parcours (celle de la simulation).
    pub d: f64,
    /// Abscisse curviligne sur le tracé lissé.
    pub s: f64,
    /// Tangente unitaire (est, nord).
    pub tangent: (f64, f64),
    /// Courbure signée (1/m), positive dans un virage à gauche.
    pub curvature: f64,
    /// Écart du cycliste à droite de l'axe (m).
    pub offset: f64,
    pub profile: Profile,
}

impl PathSample {
    /// Vecteur unitaire vers la droite de la route.
    pub fn right(&self) -> (f64, f64) {
        (self.tangent.1, -self.tangent.0)
    }
}

/// Position du cycliste sur sa trajectoire.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiderPose {
    pub x: f64,
    pub n: f64,
    pub elevation: f64,
    pub heading: (f64, f64),
    /// Courbure de la trajectoire (1/m), positive à gauche.
    pub curvature: f64,
}

#[derive(Debug, Clone)]
pub struct RoadPath {
    pub samples: Vec<PathSample>,
}

impl RoadPath {
    pub fn new(route: &Route) -> Self {
        Self::with_axis(route, None)
    }

    /// Comme `new`, mais l'axe passe par `axis` (un point par point du
    /// parcours, par exemple recalé sur la route OSM) au lieu du GPX.
    pub fn with_axis(route: &Route, axis: Option<&[(f64, f64)]>) -> Self {
        let pts = &route.points;
        let xy = |i: usize| match axis {
            Some(a) => a[i],
            None => (pts[i].x, pts[i].y),
        };
        // 1. Lissage gaussien du tracé (σ ≈ 5 m) : les angles des nœuds OSM
        //    ou GPS disparaissent, les extrémités restent en place.
        let step = if pts.len() > 1 {
            (pts[1].distance_m - pts[0].distance_m).max(0.1)
        } else {
            5.0
        };
        let sigma = 5.0;
        let reach = ((3.0 * sigma) / step).ceil() as usize;
        let smooth: Vec<(f64, f64)> = (0..pts.len())
            .map(|i| {
                let r = reach.min(i).min(pts.len() - 1 - i);
                let (mut sx, mut sn, mut sw) = (0.0, 0.0, 0.0);
                for j in i - r..=i + r {
                    let k = (j as f64 - i as f64) * step;
                    let w = (-(k * k) / (2.0 * sigma * sigma)).exp();
                    let (px, pn) = xy(j);
                    sx += px * w;
                    sn += pn * w;
                    sw += w;
                }
                (sx / sw, sn / sw)
            })
            .collect();

        // 2. Catmull-Rom tous les mètres.
        let mut samples: Vec<PathSample> = Vec::new();
        let n = smooth.len();
        let ctrl = |i: isize| smooth[i.clamp(0, n as isize - 1) as usize];
        for i in 0..n.saturating_sub(1) {
            let (p0, p1, p2, p3) = (
                ctrl(i as isize - 1),
                ctrl(i as isize),
                ctrl(i as isize + 1),
                ctrl(i as isize + 2),
            );
            let len = (p2.0 - p1.0).hypot(p2.1 - p1.1);
            let k = (len / STEP).ceil().max(1.0) as usize;
            for j in 0..k {
                let t = j as f64 / k as f64;
                let cr = |a: f64, b: f64, c: f64, d: f64| {
                    0.5 * (2.0 * b
                        + (c - a) * t
                        + (2.0 * a - 5.0 * b + 4.0 * c - d) * t * t
                        + (3.0 * b - a - 3.0 * c + d) * t * t * t)
                };
                let (x, nn) = (cr(p0.0, p1.0, p2.0, p3.0), cr(p0.1, p1.1, p2.1, p3.1));
                let lerp = |a: f64, b: f64| a + (b - a) * t;
                samples.push(PathSample {
                    x,
                    n: nn,
                    elevation: lerp(pts[i].elevation_m, pts[i + 1].elevation_m),
                    d: lerp(pts[i].distance_m, pts[i + 1].distance_m),
                    s: 0.0,
                    tangent: (1.0, 0.0),
                    curvature: 0.0,
                    offset: 0.0,
                    profile: Profile::default(),
                });
            }
        }
        if let (Some(last), Some(&(x, nn))) = (pts.last(), smooth.last()) {
            samples.push(PathSample {
                x,
                n: nn,
                elevation: last.elevation_m,
                d: last.distance_m,
                s: 0.0,
                tangent: (1.0, 0.0),
                curvature: 0.0,
                offset: 0.0,
                profile: Profile::default(),
            });
        }
        let mut path = RoadPath { samples };
        path.compute_geometry();
        path.compute_racing_line(route);
        path
    }

    fn compute_geometry(&mut self) {
        let m = self.samples.len();
        let mut s = 0.0;
        for i in 0..m {
            if i > 0 {
                let (a, b) = (&self.samples[i - 1], &self.samples[i]);
                s += (b.x - a.x).hypot(b.n - a.n);
            }
            self.samples[i].s = s;
        }
        for i in 0..m {
            let a = &self.samples[i.saturating_sub(2)];
            let b = &self.samples[(i + 2).min(m - 1)];
            let (dx, dn) = (b.x - a.x, b.n - a.n);
            let l = dx.hypot(dn);
            if l > 1e-6 {
                self.samples[i].tangent = (dx / l, dn / l);
            } else if i > 0 {
                self.samples[i].tangent = self.samples[i - 1].tangent;
            }
        }
        let heading: Vec<f64> = self
            .samples
            .iter()
            .map(|p| p.tangent.1.atan2(p.tangent.0))
            .collect();
        let raw: Vec<f64> = (0..m)
            .map(|i| {
                let (a, b) = (i.saturating_sub(3), (i + 3).min(m - 1));
                let ds = self.samples[b].s - self.samples[a].s;
                if ds < 1e-6 {
                    return 0.0;
                }
                let mut dh = heading[b] - heading[a];
                while dh > std::f64::consts::PI {
                    dh -= std::f64::consts::TAU;
                }
                while dh < -std::f64::consts::PI {
                    dh += std::f64::consts::TAU;
                }
                dh / ds
            })
            .collect();
        let smooth = gaussian(&raw, 4.0 / STEP);
        for (p, k) in self.samples.iter_mut().zip(smooth) {
            p.curvature = k;
        }
    }

    /// Trajectoire : à droite en montée, à la corde sur le plat et en descente,
    /// dans les limites de la chaussée décrite par les profils.
    pub fn compute_racing_line(&mut self, route: &Route) {
        let wide = gaussian(
            &self.samples.iter().map(|p| p.curvature).collect::<Vec<_>>(),
            8.0 / STEP,
        );
        let target: Vec<f64> = self
            .samples
            .iter()
            .zip(&wide)
            .map(|(p, &k)| {
                let grade = route.grade_at(p.d);
                let w = 1.0 - smoothstep(0.0, 0.03, grade);
                let (keep, lo, hi) = p.profile.rider_band();
                (keep - w * k * APEX_GAIN).clamp(lo, hi)
            })
            .collect();
        for (p, o) in self.samples.iter_mut().zip(gaussian(&target, 10.0 / STEP)) {
            p.offset = o;
        }
    }

    pub fn length(&self) -> f64 {
        self.samples.last().map_or(0.0, |p| p.s)
    }

    /// Indice et fraction de l'échantillon correspondant à la distance `d` du parcours.
    fn locate(&self, d: f64) -> (usize, f64) {
        let m = self.samples.len();
        if m < 2 {
            return (0, 0.0);
        }
        let i = self
            .samples
            .partition_point(|p| p.d <= d)
            .saturating_sub(1)
            .min(m - 2);
        let (a, b) = (&self.samples[i], &self.samples[i + 1]);
        let span = b.d - a.d;
        let t = if span > 1e-9 {
            ((d - a.d) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (i, t)
    }

    fn rider_point(&self, i: usize, t: f64) -> (f64, f64, f64) {
        let a = &self.samples[i];
        let b = &self.samples[(i + 1).min(self.samples.len() - 1)];
        let lerp = |u: f64, v: f64| u + (v - u) * t;
        let (ra, rb) = (a.right(), b.right());
        let (x, n) = (lerp(a.x, b.x), lerp(a.n, b.n));
        let o = lerp(a.offset, b.offset);
        let (rx, rn) = (lerp(ra.0, rb.0), lerp(ra.1, rb.1));
        (x + rx * o, n + rn * o, lerp(a.elevation, b.elevation))
    }

    /// Point de la trajectoire du cycliste, `ahead_m` mètres plus loin sur le tracé.
    fn rider_at_s(&self, s: f64) -> (f64, f64, f64) {
        let s = s.clamp(0.0, self.length());
        let m = self.samples.len();
        let i = self
            .samples
            .partition_point(|p| p.s <= s)
            .saturating_sub(1)
            .min(m.saturating_sub(2));
        let (a, b) = (&self.samples[i], &self.samples[(i + 1).min(m - 1)]);
        let span = b.s - a.s;
        let t = if span > 1e-9 { (s - a.s) / span } else { 0.0 };
        self.rider_point(i, t)
    }

    /// Position, cap et courbure du cycliste à la distance `d` du parcours.
    pub fn rider_at(&self, d: f64) -> RiderPose {
        if self.samples.len() < 2 {
            return RiderPose {
                x: 0.0,
                n: 0.0,
                elevation: 0.0,
                heading: (0.0, 1.0),
                curvature: 0.0,
            };
        }
        let (i, t) = self.locate(d);
        let (x, n, elevation) = self.rider_point(i, t);
        let s = self.samples[i].s
            + (self.samples[(i + 1).min(self.samples.len() - 1)].s - self.samples[i].s) * t;
        let back = self.rider_at_s(s - 2.0);
        let ahead = self.rider_at_s(s + 2.0);
        let (hx, hn) = (ahead.0 - back.0, ahead.1 - back.1);
        let l = hx.hypot(hn).max(1e-9);
        let back2 = self.rider_at_s(s - 4.0);
        let ahead2 = self.rider_at_s(s + 4.0);
        let h1 = (back.1 - back2.1).atan2(back.0 - back2.0);
        let h2 = (ahead2.1 - ahead.1).atan2(ahead2.0 - ahead.0);
        let mut dh = h2 - h1;
        while dh > std::f64::consts::PI {
            dh -= std::f64::consts::TAU;
        }
        while dh < -std::f64::consts::PI {
            dh += std::f64::consts::TAU;
        }
        RiderPose {
            x,
            n,
            elevation,
            heading: (hx / l, hn / l),
            curvature: dh / 6.0,
        }
    }

    /// Points (x, n, altitude) de l'axe tous les `every` échantillons, pour
    /// l'index spatial qui aplanit le sol sous la chaussée.
    pub fn axis_points(&self, every: usize) -> Vec<(f64, f64, f64)> {
        let mut v: Vec<(f64, f64, f64)> = self
            .samples
            .iter()
            .step_by(every.max(1))
            .map(|p| (p.x, p.n, p.elevation))
            .collect();
        if let Some(p) = self.samples.last() {
            v.push((p.x, p.n, p.elevation));
        }
        v
    }

    /// Chaussée, trottoirs et accotements, découpés en tronçons d'environ
    /// `segment_m` mètres pour que Godot n'affiche que ceux qui sont dans le
    /// champ. Coordonnées Godot absolues (x est, y altitude − `base`, z sud).
    pub fn meshes(&self, base: f64, segment_m: f64) -> Vec<MeshData> {
        let mut out = Vec::new();
        let m = self.samples.len();
        if m < 2 {
            return out;
        }
        let per = (segment_m / STEP).round().max(2.0) as usize;
        let mut start = 0;
        while start < m - 1 {
            let end = (start + per).min(m - 1);
            let secs: Vec<Section> = self.samples[start..=end]
                .iter()
                .map(|p| Section {
                    x: p.x,
                    n: p.n,
                    elevation: p.elevation,
                    right: p.right(),
                    s: p.s,
                    profile: p.profile,
                })
                .collect();
            if secs.iter().any(|s| !s.profile.hidden) {
                let mut mesh = MeshData::default();
                push_strip(&mut mesh, &secs, base);
                out.push(mesh);
            }
            start = end;
        }
        out
    }

    /// Balises de bord de route (type J6), tous les `every_m` mètres des deux côtés.
    pub fn posts(&self, base: f64, every_m: f64) -> Instances {
        let mut inst = Instances::default();
        let mut next = 0.0;
        for p in &self.samples {
            if p.s < next {
                continue;
            }
            next = p.s + every_m;
            let (rx, rn) = p.right();
            let pr = &p.profile;
            for (k, side) in [(0, -1.0), (1, 1.0)] {
                // Pas de balise en ville (trottoir), au débouché d'une route
                // ni sur l'anneau d'un rond-point.
                let gap = [marks::GAP_LEFT, marks::GAP_RIGHT][k];
                if pr.hidden || pr.sidewalk[k] > 0.1 || pr.marks & gap != 0 {
                    continue;
                }
                let off = side * (pr.half_width + pr.verge[k] * 0.7);
                let (x, n) = (p.x + rx * off, p.n + rn * off);
                // Le -Z local de la balise suit le sens de la route (côté
                // droit) ou le sens inverse (côté gauche).
                let yaw = (-p.tangent.0).atan2(p.tangent.1)
                    + if side > 0.0 {
                        0.0
                    } else {
                        std::f64::consts::PI
                    };
                inst.push([x, p.elevation - base - 0.1, -n], yaw, 1.0, 1.0, [1.0; 4]);
            }
        }
        inst
    }
}

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Lissage gaussien d'une suite régulière, `sigma` en nombre d'échantillons.
fn gaussian(v: &[f64], sigma: f64) -> Vec<f64> {
    let r = (3.0 * sigma).ceil() as isize;
    let w: Vec<f64> = (-r..=r)
        .map(|k| (-(k * k) as f64 / (2.0 * sigma * sigma)).exp())
        .collect();
    let n = v.len() as isize;
    (0..n)
        .map(|i| {
            let (mut s, mut sw) = (0.0, 0.0);
            for k in -r..=r {
                let j = i + k;
                if (0..n).contains(&j) {
                    let wk = w[(k + r) as usize];
                    s += v[j as usize] * wk;
                    sw += wk;
                }
            }
            s / sw
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: f64 = 6_371_008.8;

    /// Ligne droite vers le nord puis virage à gauche de 40 m de rayon.
    fn route(grade: f64) -> Route {
        let mut raw = Vec::new();
        let mut push = |x: f64, n: f64, d: f64| {
            raw.push((
                45.0 + (n / R).to_degrees(),
                6.0 + (x / (R * 45f64.to_radians().cos())).to_degrees(),
                500.0 + d * grade,
            ));
        };
        for i in 0..=40 {
            push(0.0, i as f64 * 5.0, i as f64 * 5.0);
        }
        for i in 1..=30 {
            let a = i as f64 / 30.0 * std::f64::consts::FRAC_PI_2;
            push(
                -40.0 + 40.0 * a.cos(),
                200.0 + 40.0 * a.sin(),
                200.0 + i as f64 * 2.1,
            );
        }
        for i in 1..=40 {
            push(-40.0 - i as f64 * 5.0, 240.0, 263.0 + i as f64 * 5.0);
        }
        Route::from_lat_lon_ele(None, &raw, 5.0, 40.0).unwrap()
    }

    #[test]
    fn samples_every_meter_and_curvature_sign() {
        let r = route(0.0);
        let p = RoadPath::new(&r);
        assert!(
            (p.length() - r.length_m()).abs() < 3.0,
            "{} {}",
            p.length(),
            r.length_m()
        );
        let mid_turn = p.samples.iter().find(|s| s.d > 230.0).unwrap();
        assert!(
            (mid_turn.curvature - 1.0 / 40.0).abs() < 0.008,
            "{}",
            mid_turn.curvature
        );
        let straight = p.samples.iter().find(|s| s.d > 100.0).unwrap();
        assert!(straight.curvature.abs() < 1e-3);
    }

    #[test]
    fn keeps_right_climbing_and_cuts_the_corner_on_the_flat() {
        let climb = RoadPath::new(&route(0.07));
        let flat = RoadPath::new(&route(0.0));
        let keep = HALF_WIDTH - FROM_EDGE;
        let apex = |p: &RoadPath| p.samples.iter().find(|s| s.d > 232.0).unwrap().offset;
        assert!((apex(&climb) - keep).abs() < 0.05, "{}", apex(&climb));
        // Virage à gauche : la corde est vers l'axe, l'écart diminue.
        assert!(apex(&flat) < keep - 0.8, "{}", apex(&flat));
        let pose = flat.rider_at(100.0);
        assert!(
            (pose.x - keep).abs() < 0.1,
            "à droite de l'axe nord : {}",
            pose.x
        );
        assert!(pose.heading.1 > 0.99);
    }

    #[test]
    fn mesh_faces_up() {
        let p = RoadPath::new(&route(0.0));
        let meshes = p.meshes(500.0, 200.0);
        assert!(meshes.len() >= 2);
        let m = &meshes[0];
        assert_eq!(m.positions.len() % 8, 0);
        assert_eq!(m.indices.len(), (m.positions.len() / 8 - 1) * 42);
        assert_eq!(m.uv2s.len(), m.positions.len());
        // La chaussée (sommets 3 et 4) est plus haute que le bout des accotements.
        assert!(m.positions[3][1] > m.positions[0][1]);
    }

    #[test]
    fn narrow_oneway_lets_the_rider_use_the_whole_road() {
        let mut r = RoadPath::new(&route(0.0));
        for p in &mut r.samples {
            p.profile.half_width = 2.0;
            p.profile.oneway = true;
        }
        r.compute_racing_line(&route(0.0));
        let apex = r.samples.iter().find(|s| s.d > 232.0).unwrap().offset;
        assert!(apex < 0.0, "{apex}");
        assert!(apex >= -2.0 + 0.9 - 1e-9);
    }
}
