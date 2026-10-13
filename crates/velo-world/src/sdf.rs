//! Modelage par fonctions de distance signée (SDF) pour les objets organiques
//! du décor, d'abord le vélo : tubes à section profilée qui se fondent les uns
//! dans les autres par des congés arrondis (union lisse), boîtes et cylindres
//! arrondis, disques dentés, perçages répétés, polygones extrudés.
//!
//! Les primitives s'ajoutent dans l'ordre, chacune combinée à ce qui précède
//! par union, soustraction ou intersection, lisse sur un rayon `k`.
//! [`Sdf::build`] échantillonne le champ sur une grille fine, seulement près de
//! la surface (bande étroite par blocs), extrait la surface par « surface nets »,
//! recale les sommets sur l'isosurface, puis simplifie le maillage à une erreur
//! donnée (meshoptimizer). Les normales viennent du gradient du champ : elles
//! restent lisses même après simplification.
//!
//! Chaque primitive porte un matériau (le maillage est découpé en surfaces par
//! matériau) et une couleur, mélangée dans les congés : on peut ainsi dégrader
//! une peinture d'un tube à l'autre.

use crate::mesh::MeshData;
use std::collections::HashMap;

type V3 = [f32; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}
fn normalize(a: V3) -> V3 {
    let l = len(a).max(1e-12);
    mul(a, 1.0 / l)
}
fn len2(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

/// Repère local : `p_local = rot · (p - origin)`, `rot` orthonormée (lignes).
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub origin: V3,
    /// Axes locaux x, y, z exprimés dans le repère global.
    pub axes: [V3; 3],
}

impl Frame {
    fn local(&self, p: V3) -> V3 {
        let d = sub(p, self.origin);
        [
            dot(d, self.axes[0]),
            dot(d, self.axes[1]),
            dot(d, self.axes[2]),
        ]
    }
    fn to_global(&self, l: V3) -> V3 {
        add(
            self.origin,
            add(
                add(mul(self.axes[0], l[0]), mul(self.axes[1], l[1])),
                mul(self.axes[2], l[2]),
            ),
        )
    }
}

/// Section d'un tube : enveloppe convexe de deux cercles (centres en
/// coordonnées (u latéral, v le long de la corde), rayons), éventuellement
/// tronquée à l'arrière (`cut` > 0 : arête vive à v = -cut, profil Kamm).
#[derive(Debug, Clone, Copy, Default)]
pub struct Section {
    pub c1: [f32; 2],
    pub r1: f32,
    pub c2: [f32; 2],
    pub r2: f32,
    pub cut: f32,
}

impl Section {
    fn lerp(a: &Section, b: &Section, t: f32) -> Section {
        let l = |x: f32, y: f32| x + (y - x) * t;
        Section {
            c1: [l(a.c1[0], b.c1[0]), l(a.c1[1], b.c1[1])],
            r1: l(a.r1, b.r1),
            c2: [l(a.c2[0], b.c2[0]), l(a.c2[1], b.c2[1])],
            r2: l(a.r2, b.r2),
            cut: l(a.cut, b.cut),
        }
    }

    /// Distance signée 2D au profil.
    fn dist(&self, u: f32, v: f32) -> f32 {
        let (ax, ay) = (self.c1[0], self.c1[1]);
        let (bx, by) = (self.c2[0], self.c2[1]);
        let (dx, dy) = (ax - bx, ay - by);
        let h = len2(dx, dy);
        let d = if h <= (self.r1 - self.r2).abs() + 1e-7 {
            (len2(u - ax, v - ay) - self.r1).min(len2(u - bx, v - by) - self.r2)
        } else {
            // Capsule inégale (cercle r2 en B, r1 en A), dans le repère de l'axe BA.
            let (ex, ey) = (dx / h, dy / h);
            let (px, py) = (u - bx, v - by);
            let y = px * ex + py * ey;
            let x = (px * ey - py * ex).abs();
            let b = (self.r2 - self.r1) / h;
            let a = (1.0 - b * b).max(0.0).sqrt();
            let k = -b * x + a * y;
            if k < 0.0 {
                len2(x, y) - self.r2
            } else if k > a * h {
                len2(x, y - h) - self.r1
            } else {
                x * a + y * b - self.r2
            }
        };
        if self.cut > 0.0 {
            d.max(-v - self.cut)
        } else {
            d
        }
    }

    fn extent(&self) -> f32 {
        (len2(self.c1[0], self.c1[1]) + self.r1).max(len2(self.c2[0], self.c2[1]) + self.r2)
    }
}

#[derive(Debug, Clone)]
pub struct Seg {
    a: V3,
    t: V3,
    len: f32,
    /// Plans bissecteurs aux deux bouts (normales orientées vers l'avant du tube).
    n0: V3,
    n1: V3,
    first: bool,
    last: bool,
    min: V3,
    max: V3,
}

#[derive(Debug, Clone)]
pub enum Shape {
    /// Tube le long d'une polyligne, section et direction de corde par point.
    Tube {
        segs: Vec<Seg>,
        sections: Vec<Section>,
        chords: Vec<V3>,
    },
    /// Boîte arrondie (demi-côtés, rayon d'arrondi).
    RBox { f: Frame, half: V3, round: f32 },
    /// Ellipsoïde (demi-axes).
    Ellipsoid { f: Frame, radii: V3 },
    /// Cylindre d'axe y local, arêtes arrondies.
    Cyl {
        f: Frame,
        r: f32,
        half_h: f32,
        round: f32,
    },
    /// Disque denté d'axe y local (n = 0 : simple couronne), arêtes chanfreinées.
    Gear {
        f: Frame,
        n: f32,
        r_in: f32,
        r_tip: f32,
        r_pitch: f32,
        r_notch: f32,
        half_t: f32,
        chamfer: f32,
    },
    /// Cylindres d'axe y local répétés autour de l'axe (perçages, fenêtres),
    /// étirés en arc si `arc` > 0 (demi-angle).
    Holes {
        f: Frame,
        n: f32,
        r_center: f32,
        r_hole: f32,
        half_t: f32,
        phase: f32,
        arc: f32,
    },
    /// Polygone du plan local xz extrudé selon y, arêtes arrondies.
    Polygon {
        f: Frame,
        pts: Vec<[f32; 2]>,
        half_t: f32,
        round: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Union,
    Subtract,
    Intersect,
}

#[derive(Debug, Clone)]
pub struct Prim {
    pub shape: Shape,
    pub op: Op,
    pub k: f32,
    pub mat: u32,
    pub color: [f32; 4],
    min: V3,
    max: V3,
}

/// Une surface du maillage final : matériau, maillage et niveaux de détail
/// (erreur absolue en m, indices).
pub struct Surface {
    pub mat: u32,
    pub mesh: MeshData,
    pub lods: Vec<(f32, Vec<i32>)>,
}

#[derive(Default)]
pub struct Sdf {
    prims: Vec<Prim>,
}

const FAR: f32 = 1e6;

/// Union lisse : distance et poids de `b` (0 à 1).
fn smin(a: f32, b: f32, k: f32) -> (f32, f32) {
    // Rien avant : la distance de `b` telle quelle (1e6 + (b - 1e6) perdrait
    // toute la précision des f32).
    if a >= FAR * 0.5 {
        return (b, 1.0);
    }
    if k <= 0.0 {
        return if b < a { (b, 1.0) } else { (a, 0.0) };
    }
    let h = (0.5 + 0.5 * (a - b) / k).clamp(0.0, 1.0);
    (a + (b - a) * h - k * h * (1.0 - h), h)
}

fn smax(a: f32, b: f32, k: f32) -> (f32, f32) {
    let (d, w) = smin(-a, -b, k);
    (-d, w)
}

fn aabb_of(points: &[V3], pad: f32) -> (V3, V3) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in points {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i] - pad);
            hi[i] = hi[i].max(p[i] + pad);
        }
    }
    (lo, hi)
}

fn frame_aabb(f: &Frame, half: V3) -> (V3, V3) {
    let mut pts = Vec::with_capacity(8);
    for i in 0..8 {
        let l = [
            if i & 1 == 0 { -half[0] } else { half[0] },
            if i & 2 == 0 { -half[1] } else { half[1] },
            if i & 4 == 0 { -half[2] } else { half[2] },
        ];
        pts.push(f.to_global(l));
    }
    aabb_of(&pts, 0.0)
}

fn boxes_overlap(a0: V3, a1: V3, b0: V3, b1: V3) -> bool {
    (0..3).all(|i| a0[i] <= b1[i] && b0[i] <= a1[i])
}

/// Distance 2D signée à un polygone (iq).
fn polygon_dist(pts: &[[f32; 2]], x: f32, y: f32) -> f32 {
    let n = pts.len();
    let mut d = {
        let dx = x - pts[0][0];
        let dy = y - pts[0][1];
        dx * dx + dy * dy
    };
    let mut s = 1.0f32;
    let mut j = n - 1;
    for i in 0..n {
        let (vi, vj) = (pts[i], pts[j]);
        let (ex, ey) = (vj[0] - vi[0], vj[1] - vi[1]);
        let (wx, wy) = (x - vi[0], y - vi[1]);
        let t = ((wx * ex + wy * ey) / (ex * ex + ey * ey).max(1e-12)).clamp(0.0, 1.0);
        let (bx, by) = (wx - ex * t, wy - ey * t);
        d = d.min(bx * bx + by * by);
        let c = [y >= vi[1], y < vj[1], ex * wy > ey * wx];
        if (c[0] && c[1] && c[2]) || (!c[0] && !c[1] && !c[2]) {
            s = -s;
        }
        j = i;
    }
    s * d.sqrt()
}

/// Extrusion d'une distance 2D sur une épaisseur (arêtes arrondies de `round`).
fn extrude(d2: f32, y: f32, half_t: f32, round: f32) -> f32 {
    let wx = d2 + round;
    let wy = y.abs() - half_t + round;
    wx.max(wy).min(0.0) + len2(wx.max(0.0), wy.max(0.0)) - round
}

impl Shape {
    /// Distance signée ; `segs` limite les segments de tube examinés.
    fn dist(&self, p: V3, segs: (u32, u32)) -> f32 {
        match self {
            Shape::Tube {
                segs: list,
                sections,
                chords,
            } => {
                let mut best = FAR;
                let mut fallback = FAR;
                for i in segs.0 as usize..segs.1 as usize {
                    let s = &list[i];
                    let rel = sub(p, s.a);
                    let along = dot(rel, s.t);
                    let in0 = dot(rel, s.n0) >= 0.0;
                    let in1 = dot(sub(p, add(s.a, mul(s.t, s.len))), s.n1) <= 0.0;
                    let t = (along / s.len).clamp(0.0, 1.0);
                    let sec = Section::lerp(&sections[i], &sections[i + 1], t);
                    let chord = add(mul(chords[i], 1.0 - t), mul(chords[i + 1], t));
                    let v = normalize(sub(chord, mul(s.t, dot(chord, s.t))));
                    let u = cross(s.t, v);
                    let q = sub(rel, mul(s.t, along));
                    let d2 = sec.dist(dot(q, u), dot(q, v));
                    // Bouts du tube : coupes planes.
                    let mut cap = f32::MIN;
                    if s.first {
                        cap = cap.max(-along);
                    }
                    if s.last {
                        cap = cap.max(along - s.len);
                    }
                    let d = if cap > f32::MIN {
                        d2.max(cap).min(0.0) + len2(d2.max(0.0), cap.max(0.0))
                    } else {
                        d2
                    };
                    if (in0 || s.first) && (in1 || s.last) {
                        best = best.min(d);
                    } else {
                        // Hors des plans bissecteurs : distance au segment fermé.
                        let over = if along < 0.0 {
                            -along
                        } else if along > s.len {
                            along - s.len
                        } else {
                            0.0
                        };
                        fallback = fallback.min(if d2 > 0.0 {
                            len2(d2, over)
                        } else {
                            d2.max(over)
                        });
                    }
                }
                if best < FAR {
                    best
                } else {
                    fallback
                }
            }
            Shape::RBox { f, half, round } => {
                let l = f.local(p);
                let q = [
                    l[0].abs() - half[0] + round,
                    l[1].abs() - half[1] + round,
                    l[2].abs() - half[2] + round,
                ];
                len([q[0].max(0.0), q[1].max(0.0), q[2].max(0.0)])
                    + q[0].max(q[1]).max(q[2]).min(0.0)
                    - round
            }
            Shape::Ellipsoid { f, radii } => {
                let l = f.local(p);
                let k0 = len([l[0] / radii[0], l[1] / radii[1], l[2] / radii[2]]);
                let k1 = len([
                    l[0] / (radii[0] * radii[0]),
                    l[1] / (radii[1] * radii[1]),
                    l[2] / (radii[2] * radii[2]),
                ]);
                if k1 < 1e-9 {
                    -radii[0].min(radii[1]).min(radii[2])
                } else {
                    k0 * (k0 - 1.0) / k1
                }
            }
            Shape::Cyl {
                f,
                r,
                half_h,
                round,
            } => {
                let l = f.local(p);
                extrude(len2(l[0], l[2]) - r, l[1], *half_h, *round)
            }
            Shape::Gear {
                f,
                n,
                r_in,
                r_tip,
                r_pitch,
                r_notch,
                half_t,
                chamfer,
            } => {
                let l = f.local(p);
                let r = len2(l[0], l[2]);
                let mut d2 = (r - r_tip).max(r_in - r);
                if *n > 0.0 {
                    let cell = std::f32::consts::TAU / n;
                    let th = l[2].atan2(l[0]);
                    let a = th - cell * (th / cell).round();
                    let (qx, qy) = (r * a.cos() - r_pitch, r * a.sin());
                    // Creux : cercle du rouleau, ouvert vers l'extérieur par
                    // des flancs qui s'écartent jusqu'au sommet des dents.
                    let circle = len2(qx, qy) - r_notch;
                    let flank = (qy.abs() - r_notch - qx.max(0.0) * 0.55) * 0.87;
                    let slot = flank.max(-qx);
                    d2 = d2.max(-circle.min(slot));
                }
                let wy = l[1].abs() - half_t;
                let d = d2.max(wy);
                d.max((d2 + wy + chamfer) * std::f32::consts::FRAC_1_SQRT_2)
            }
            Shape::Holes {
                f,
                n,
                r_center,
                r_hole,
                half_t,
                phase,
                arc,
            } => {
                let l = f.local(p);
                let r = len2(l[0], l[2]);
                let cell = std::f32::consts::TAU / n;
                let th = l[2].atan2(l[0]) - phase;
                let mut a = th - cell * (th / cell).round();
                // Fenêtre en arc : on ramène l'angle dans [-arc, arc] à zéro.
                if *arc > 0.0 {
                    a = a.signum() * (a.abs() - arc).max(0.0);
                }
                let (qx, qy) = (r * a.cos() - r_center, r * a.sin());
                extrude(len2(qx, qy) - r_hole, l[1], *half_t, 0.0)
            }
            Shape::Polygon {
                f,
                pts,
                half_t,
                round,
            } => {
                let l = f.local(p);
                extrude(polygon_dist(pts, l[0], l[2]), l[1], *half_t, *round)
            }
        }
    }
}

/// Primitive retenue pour un bloc : indice et plage de segments de tube.
type Cand = (u32, u32, u32);

impl Sdf {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.prims.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.prims.is_empty()
    }

    pub fn push(&mut self, shape: Shape, op: Op, k: f32, mat: u32, color: [f32; 4]) {
        let (min, max) = match &shape {
            Shape::Tube { segs, .. } => {
                let mut lo = [f32::MAX; 3];
                let mut hi = [f32::MIN; 3];
                for s in segs {
                    for i in 0..3 {
                        lo[i] = lo[i].min(s.min[i]);
                        hi[i] = hi[i].max(s.max[i]);
                    }
                }
                (lo, hi)
            }
            Shape::RBox { f, half, .. } => frame_aabb(f, *half),
            Shape::Ellipsoid { f, radii } => frame_aabb(f, *radii),
            Shape::Cyl { f, r, half_h, .. } => frame_aabb(f, [*r, *half_h, *r]),
            Shape::Gear {
                f, r_tip, half_t, ..
            } => frame_aabb(f, [*r_tip, *half_t, *r_tip]),
            Shape::Holes {
                f,
                r_center,
                r_hole,
                half_t,
                ..
            } => {
                let r = r_center + r_hole;
                frame_aabb(f, [r, *half_t, r])
            }
            Shape::Polygon { f, pts, half_t, .. } => {
                let m = pts
                    .iter()
                    .fold(0.0f32, |m, q| m.max(q[0].abs()).max(q[1].abs()));
                frame_aabb(f, [m, *half_t, m])
            }
        };
        self.prims.push(Prim {
            shape,
            op,
            k,
            mat,
            color,
            min,
            max,
        });
    }

    /// Tube le long de `points` ; sections et cordes par point (même nombre).
    pub fn tube(&mut self, points: &[V3], sections: &[Section], chords: &[V3]) -> Option<Shape> {
        let n = points.len();
        if n < 2 || sections.len() != n || chords.len() != n {
            return None;
        }
        let mut tangents = Vec::with_capacity(n - 1);
        for i in 0..n - 1 {
            tangents.push(normalize(sub(points[i + 1], points[i])));
        }
        let mut segs = Vec::with_capacity(n - 1);
        for i in 0..n - 1 {
            let t = tangents[i];
            let n0 = if i == 0 {
                t
            } else {
                normalize(add(tangents[i - 1], t))
            };
            let n1 = if i == n - 2 {
                t
            } else {
                normalize(add(t, tangents[i + 1]))
            };
            let ext = sections[i].extent().max(sections[i + 1].extent());
            let (min, max) = aabb_of(&[points[i], points[i + 1]], ext * 1.05);
            segs.push(Seg {
                a: points[i],
                t,
                len: len(sub(points[i + 1], points[i])).max(1e-6),
                n0,
                n1,
                first: i == 0,
                last: i == n - 2,
                min,
                max,
            });
        }
        Some(Shape::Tube {
            segs,
            sections: sections.to_vec(),
            chords: chords.to_vec(),
        })
    }

    /// Primitives (et plages de segments) dont la boîte, élargie de `margin`
    /// et du rayon de fondu, touche la boîte [lo, hi].
    fn candidates(&self, lo: V3, hi: V3, margin: f32) -> Vec<Cand> {
        let all: Vec<Cand> = (0..self.prims.len() as u32)
            .map(|i| (i, 0, u32::MAX))
            .collect();
        self.candidates_in(&all, lo, hi, margin)
    }

    /// Comme `candidates`, parmi une liste déjà filtrée (bloc englobant).
    fn candidates_in(&self, parent: &[Cand], lo: V3, hi: V3, margin: f32) -> Vec<Cand> {
        let mut out = Vec::new();
        for &(pi, pa, pb) in parent {
            let i = pi as usize;
            let p = &self.prims[i];
            let m = margin + p.k;
            let lo2 = sub(lo, [m; 3]);
            let hi2 = add(hi, [m; 3]);
            if p.op == Op::Intersect {
                out.push((i as u32, 0, u32::MAX));
                continue;
            }
            if !boxes_overlap(p.min, p.max, lo2, hi2) {
                continue;
            }
            if let Shape::Tube { segs, .. } = &p.shape {
                let mut a = u32::MAX;
                let mut b = 0;
                let hi_seg = (pb as usize).min(segs.len());
                for (j, s) in segs.iter().enumerate().take(hi_seg).skip(pa as usize) {
                    if boxes_overlap(s.min, s.max, lo2, hi2) {
                        a = a.min(j as u32);
                        b = j as u32 + 1;
                    }
                }
                if a < b {
                    // Un segment de part et d'autre pour les plans bissecteurs.
                    out.push((
                        i as u32,
                        a.saturating_sub(1),
                        (b + 1).min(segs.len() as u32),
                    ));
                }
            } else {
                out.push((i as u32, 0, 0));
            }
        }
        out
    }

    fn prim_dist(&self, c: &Cand, p: V3) -> f32 {
        let prim = &self.prims[c.0 as usize];
        let segs = match &prim.shape {
            Shape::Tube { segs, .. } => (c.1, c.2.min(segs.len() as u32)),
            _ => (0, 0),
        };
        prim.shape.dist(p, segs)
    }

    /// Distance signée au solide, avec les primitives `cands`.
    fn eval(&self, cands: &[Cand], p: V3) -> f32 {
        let mut d = FAR;
        for c in cands {
            let prim = &self.prims[c.0 as usize];
            let di = self.prim_dist(c, p);
            d = match prim.op {
                Op::Union => smin(d, di, prim.k).0,
                Op::Subtract => smax(d, -di, prim.k).0,
                Op::Intersect => smax(d, di, prim.k).0,
            };
        }
        d
    }

    /// Distance, couleur mélangée et matériau dominant.
    fn eval_attr(&self, cands: &[Cand], p: V3) -> (f32, [f32; 4], u32) {
        let mut d = FAR;
        let mut col = [0.0; 4];
        let mut mat = 0;
        let mut mat_w = -1.0f32;
        for c in cands {
            let prim = &self.prims[c.0 as usize];
            let di = self.prim_dist(c, p);
            let (nd, w) = match prim.op {
                Op::Union => smin(d, di, prim.k),
                Op::Subtract => smax(d, -di, prim.k),
                Op::Intersect => smax(d, di, prim.k),
            };
            if d >= FAR * 0.5 && prim.op == Op::Union {
                col = prim.color;
                mat = prim.mat;
                mat_w = 1.0;
            } else if prim.op != Op::Intersect {
                for i in 0..4 {
                    col[i] += (prim.color[i] - col[i]) * w;
                }
                if w > 0.5 || (w > 0.0 && mat_w < 0.0) {
                    mat = prim.mat;
                    mat_w = w;
                }
            }
            d = nd;
        }
        (d, col, mat)
    }

    fn grad(&self, cands: &[Cand], p: V3, e: f32) -> V3 {
        let dx = self.eval(cands, add(p, [e, 0.0, 0.0])) - self.eval(cands, sub(p, [e, 0.0, 0.0]));
        let dy = self.eval(cands, add(p, [0.0, e, 0.0])) - self.eval(cands, sub(p, [0.0, e, 0.0]));
        let dz = self.eval(cands, add(p, [0.0, 0.0, e])) - self.eval(cands, sub(p, [0.0, 0.0, e]));
        normalize([dx, dy, dz])
    }

    /// Boîte englobante des unions.
    fn bounds(&self) -> Option<(V3, V3)> {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        let mut any = false;
        for p in &self.prims {
            if p.op != Op::Union {
                continue;
            }
            any = true;
            for i in 0..3 {
                lo[i] = lo[i].min(p.min[i] - p.k);
                hi[i] = hi[i].max(p.max[i] + p.k);
            }
        }
        any.then_some((lo, hi))
    }

    /// Ramène des points sur la surface (décalée de `offset`), avec leurs normales.
    pub fn project(&self, points: &[V3], offset: f32) -> (Vec<V3>, Vec<V3>) {
        let mut out = Vec::with_capacity(points.len());
        let mut nrm = Vec::with_capacity(points.len());
        for &p0 in points {
            let cands = self.candidates(p0, p0, 0.03);
            let mut p = p0;
            let mut g = [0.0, 1.0, 0.0];
            for _ in 0..6 {
                let d = self.eval(&cands, p) - offset;
                g = self.grad(&cands, p, 1e-4);
                p = sub(p, mul(g, d));
            }
            out.push(p);
            nrm.push(g);
        }
        (out, nrm)
    }

    /// Maillage du solide : grille de pas `cell` (m), simplifié à l'erreur
    /// absolue `error` (m), niveaux de détail aux erreurs `lod_errors`.
    pub fn build(&self, cell: f32, error: f32, lod_errors: &[f32]) -> Vec<Surface> {
        let Some((lo, hi)) = self.bounds() else {
            return Vec::new();
        };
        const B: i32 = 8;
        let h = cell;
        let origin = sub(lo, [2.0 * h; 3]);
        let dims: [i32; 3] = std::array::from_fn(|i| ((hi[i] - lo[i]) / h).ceil() as i32 + 4);
        let nb: [i32; 3] = std::array::from_fn(|i| (dims[i] + B - 1) / B);
        let corner = |i: i32, j: i32, k: i32| -> V3 {
            [
                origin[0] + i as f32 * h,
                origin[1] + j as f32 * h,
                origin[2] + k as f32 * h,
            ]
        };
        let block_half = B as f32 * h * 0.5;
        let block_rad = block_half * 3f32.sqrt();
        let timing = std::env::var_os("VELO_SDF_TIMING").is_some();
        let t0 = std::time::Instant::now();

        // 1. Blocs proches de la surface, en parallèle : on écarte d'abord
        // des groupes de 4 × 4 × 4 blocs, puis les blocs loin de la surface.
        struct Block {
            key: [i32; 3],
            cands: Vec<Cand>,
            vals: Vec<f32>,
        }
        const G: i32 = 4;
        let ng: [i32; 3] = std::array::from_fn(|i| (nb[i] + G - 1) / G);
        let groups: Vec<[i32; 3]> = (0..ng[2])
            .flat_map(|gk| (0..ng[1]).flat_map(move |gj| (0..ng[0]).map(move |gi| [gi, gj, gk])))
            .collect();
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, 16);
        let n1 = (B + 1) as usize;
        let sample = |key: [i32; 3], cands: Vec<Cand>| -> Option<Block> {
            let mut vals = vec![0.0f32; n1 * n1 * n1];
            let mut pos = false;
            let mut neg = false;
            for k in 0..=B {
                for j in 0..=B {
                    for i in 0..=B {
                        let v = self.eval(
                            &cands,
                            corner(key[0] * B + i, key[1] * B + j, key[2] * B + k),
                        );
                        if v < 0.0 {
                            neg = true;
                        } else {
                            pos = true;
                        }
                        vals[(k as usize * n1 + j as usize) * n1 + i as usize] = v;
                    }
                }
            }
            (pos && neg).then_some(Block { key, cands, vals })
        };
        let group_half = (G * B) as f32 * h * 0.5;
        let next = std::sync::atomic::AtomicUsize::new(0);
        let mut blocks: Vec<Block> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads)
                .map(|_| {
                    s.spawn(|| {
                        let mut out = Vec::new();
                        loop {
                            let gi = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if gi >= groups.len() {
                                break;
                            }
                            let g = groups[gi];
                            let g0 = corner(g[0] * G * B, g[1] * G * B, g[2] * G * B);
                            let g1 = add(g0, [(G * B) as f32 * h; 3]);
                            let gc = self.candidates(g0, g1, 3.0 * h);
                            if gc.is_empty() {
                                continue;
                            }
                            let dc = self.eval(&gc, add(g0, [group_half; 3]));
                            if dc.abs() > group_half * 3f32.sqrt() * 1.1 + 3.0 * h {
                                continue;
                            }
                            for bk in 0..G {
                                for bj in 0..G {
                                    for bi in 0..G {
                                        let key = [g[0] * G + bi, g[1] * G + bj, g[2] * G + bk];
                                        if key[0] >= nb[0] || key[1] >= nb[1] || key[2] >= nb[2] {
                                            continue;
                                        }
                                        let b0 = corner(key[0] * B, key[1] * B, key[2] * B);
                                        let b1 = add(b0, [B as f32 * h; 3]);
                                        let cands = self.candidates_in(&gc, b0, b1, 3.0 * h);
                                        if cands.is_empty() {
                                            continue;
                                        }
                                        let dc = self.eval(&cands, add(b0, [block_half; 3]));
                                        if dc.abs() > block_rad * 1.1 + 2.0 * h {
                                            continue;
                                        }
                                        if let Some(b) = sample(key, cands) {
                                            out.push(b);
                                        }
                                    }
                                }
                            }
                        }
                        out
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect()
        });
        // Voisins écartés à tort : une face traversée par la surface force
        // l'échantillonnage du bloc d'à côté (sinon le maillage aurait un trou).
        let mut known: std::collections::HashSet<[i32; 3]> = blocks.iter().map(|b| b.key).collect();
        let mut scan = 0;
        while scan < blocks.len() {
            let mut todo = Vec::new();
            for b in &blocks[scan..] {
                for axis in 0..3 {
                    for side in [0, B] {
                        let mut pos = false;
                        let mut neg = false;
                        for u in 0..=B {
                            for v in 0..=B {
                                let mut c = [0; 3];
                                c[axis] = side;
                                c[(axis + 1) % 3] = u;
                                c[(axis + 2) % 3] = v;
                                let val = b.vals
                                    [(c[2] as usize * n1 + c[1] as usize) * n1 + c[0] as usize];
                                if val < 0.0 {
                                    neg = true;
                                } else {
                                    pos = true;
                                }
                            }
                        }
                        if pos && neg {
                            let mut key = b.key;
                            key[axis] += if side == 0 { -1 } else { 1 };
                            if key.iter().zip(nb.iter()).all(|(k, n)| *k >= 0 && k < n)
                                && known.insert(key)
                            {
                                todo.push(key);
                            }
                        }
                    }
                }
            }
            scan = blocks.len();
            for key in todo {
                let b0 = corner(key[0] * B, key[1] * B, key[2] * B);
                let b1 = add(b0, [B as f32 * h; 3]);
                let cands = self.candidates(b0, b1, 3.0 * h);
                if let Some(b) = sample(key, cands) {
                    blocks.push(b);
                }
            }
        }
        let all_len = (nb[0] * nb[1] * nb[2]) as usize;
        if timing {
            eprintln!(
                "sdf: {} blocs actifs / {} en {:?}",
                blocks.len(),
                all_len,
                t0.elapsed()
            );
        }
        // 2. Un sommet par case traversée (point moyen des croisements d'arêtes).
        let mut index: HashMap<(i32, i32, i32), u32> = HashMap::new();
        let mut verts: Vec<V3> = Vec::new();
        let mut vert_block: Vec<u32> = Vec::new();
        const EDGES: [(usize, usize); 12] = [
            (0, 1),
            (2, 3),
            (4, 5),
            (6, 7),
            (0, 2),
            (1, 3),
            (4, 6),
            (5, 7),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];
        for (bi, b) in blocks.iter().enumerate() {
            let at =
                |i: i32, j: i32, k: i32| b.vals[(k as usize * n1 + j as usize) * n1 + i as usize];
            for k in 0..B {
                for j in 0..B {
                    for i in 0..B {
                        let mut c = [0.0f32; 8];
                        for (q, v) in c.iter_mut().enumerate() {
                            *v = at(
                                i + (q & 1) as i32,
                                j + ((q >> 1) & 1) as i32,
                                k + ((q >> 2) & 1) as i32,
                            );
                        }
                        let neg = c.iter().filter(|v| **v < 0.0).count();
                        if neg == 0 || neg == 8 {
                            continue;
                        }
                        let mut acc = [0.0f32; 3];
                        let mut cnt = 0.0;
                        for &(a, bq) in &EDGES {
                            if (c[a] < 0.0) != (c[bq] < 0.0) {
                                let t = c[a] / (c[a] - c[bq]);
                                let pa =
                                    [(a & 1) as f32, ((a >> 1) & 1) as f32, ((a >> 2) & 1) as f32];
                                let pb = [
                                    (bq & 1) as f32,
                                    ((bq >> 1) & 1) as f32,
                                    ((bq >> 2) & 1) as f32,
                                ];
                                acc = add(acc, add(pa, mul(sub(pb, pa), t)));
                                cnt += 1.0;
                            }
                        }
                        let gi = b.key[0] * B + i;
                        let gj = b.key[1] * B + j;
                        let gk = b.key[2] * B + k;
                        let local = mul(acc, 1.0 / cnt);
                        let p = add(corner(gi, gj, gk), mul(local, h));
                        index.insert((gi, gj, gk), verts.len() as u32);
                        verts.push(p);
                        vert_block.push(bi as u32);
                    }
                }
            }
        }

        // 3. Recalage des sommets sur l'isosurface (Newton, borné à la case).
        let chunk = verts.len().div_ceil(threads).max(1);
        std::thread::scope(|s| {
            for (ci, part) in verts.chunks_mut(chunk).enumerate() {
                let vb = &vert_block;
                let blocks = &blocks;
                s.spawn(move || {
                    for (vi, p) in part.iter_mut().enumerate() {
                        let cands = &blocks[vb[ci * chunk + vi] as usize].cands;
                        let start = *p;
                        for _ in 0..2 {
                            let d = self.eval(cands, *p);
                            let e = h * 0.2;
                            let g = normalize([
                                self.eval(cands, add(*p, [e, 0.0, 0.0])) - d,
                                self.eval(cands, add(*p, [0.0, e, 0.0])) - d,
                                self.eval(cands, add(*p, [0.0, 0.0, e])) - d,
                            ]);
                            let np = sub(*p, mul(g, d));
                            let off = sub(np, start);
                            let l = len(off);
                            *p = if l > h {
                                add(start, mul(off, h / l))
                            } else {
                                np
                            };
                        }
                    }
                });
            }
        });

        if timing {
            eprintln!("sdf: {} sommets recalés à {:?}", verts.len(), t0.elapsed());
        }
        // 4. Quadrilatères autour de chaque arête traversée, par matériau.
        let mats: Vec<u32> = verts
            .iter()
            .zip(&vert_block)
            .map(|(p, b)| self.eval_attr(&blocks[*b as usize].cands, *p).2)
            .collect();
        let mut tris: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut emit = |q: [u32; 4], outward_flip: bool, normal: V3| {
            let p = |i: u32| verts[i as usize];
            let mut tri = |a: u32, b: u32, c: u32| {
                let g = cross(sub(p(b), p(a)), sub(p(c), p(a)));
                let (b, c) = if (dot(g, normal) <= 0.0) != outward_flip {
                    (b, c)
                } else {
                    (c, b)
                };
                let m = if mats[b as usize] == mats[c as usize] {
                    mats[b as usize]
                } else {
                    mats[a as usize]
                };
                tris.entry(m).or_default().extend([a, b, c]);
            };
            tri(q[0], q[1], q[2]);
            tri(q[0], q[2], q[3]);
        };
        for b in &blocks {
            let at =
                |i: i32, j: i32, k: i32| b.vals[(k as usize * n1 + j as usize) * n1 + i as usize];
            for k in 0..B {
                for j in 0..B {
                    for i in 0..B {
                        let v0 = at(i, j, k);
                        let (gi, gj, gk) = (b.key[0] * B + i, b.key[1] * B + j, b.key[2] * B + k);
                        for axis in 0..3 {
                            let v1 = match axis {
                                0 => at(i + 1, j, k),
                                1 => at(i, j + 1, k),
                                _ => at(i, j, k + 1),
                            };
                            if (v0 < 0.0) == (v1 < 0.0) {
                                continue;
                            }
                            let cells: [(i32, i32, i32); 4] = match axis {
                                0 => [
                                    (gi, gj, gk),
                                    (gi, gj - 1, gk),
                                    (gi, gj - 1, gk - 1),
                                    (gi, gj, gk - 1),
                                ],
                                1 => [
                                    (gi, gj, gk),
                                    (gi, gj, gk - 1),
                                    (gi - 1, gj, gk - 1),
                                    (gi - 1, gj, gk),
                                ],
                                _ => [
                                    (gi, gj, gk),
                                    (gi - 1, gj, gk),
                                    (gi - 1, gj - 1, gk),
                                    (gi, gj - 1, gk),
                                ],
                            };
                            let mut q = [0u32; 4];
                            let mut ok = true;
                            for (n, c) in cells.iter().enumerate() {
                                match index.get(c) {
                                    Some(&v) => q[n] = v,
                                    None => ok = false,
                                }
                            }
                            if !ok {
                                continue;
                            }
                            let mut normal = [0.0; 3];
                            normal[axis] = if v0 < 0.0 { 1.0 } else { -1.0 };
                            emit(q, false, normal);
                        }
                    }
                }
            }
        }

        if timing {
            eprintln!("sdf: quadrilatères à {:?}", t0.elapsed());
        }
        // 5. Simplification par matériau (bords entre matériaux verrouillés),
        // normales et couleurs tirées du champ.
        let mut out = Vec::new();
        let mut keys: Vec<u32> = tris.keys().copied().collect();
        keys.sort_unstable();
        for m in keys {
            let idx = &tris[&m];
            let opts =
                meshopt::SimplifyOptions::LockBorder | meshopt::SimplifyOptions::ErrorAbsolute;
            let simple = meshopt::simplify_decoder(idx, &verts, 0, error, opts, None);
            let mut remap: HashMap<u32, i32> = HashMap::new();
            let mut mesh = MeshData::default();
            let mut take = |v: u32, mesh: &mut MeshData| -> i32 {
                *remap.entry(v).or_insert_with(|| {
                    let p = verts[v as usize];
                    let cands = &blocks[vert_block[v as usize] as usize].cands;
                    let n = self.grad(cands, p, h * 0.2);
                    let (_, col, _) = self.eval_attr(cands, p);
                    mesh.positions.push(p);
                    mesh.normals.push(n);
                    mesh.colors.push(col);
                    mesh.positions.len() as i32 - 1
                })
            };
            for &v in &simple {
                let i = take(v, &mut mesh);
                mesh.indices.push(i);
            }
            let mut lods = Vec::new();
            let mut prev = simple.clone();
            for &e in lod_errors {
                let l = meshopt::simplify_decoder(
                    &prev,
                    &verts,
                    0,
                    e,
                    opts | meshopt::SimplifyOptions::Sparse,
                    None,
                );
                prev = l.clone();
                if l.len() * 10 > simple.len() * 9 {
                    continue;
                }
                let li: Vec<i32> = l.iter().map(|&v| take(v, &mut mesh)).collect();
                lods.push((e, li));
            }
            fix_winding(&mesh.positions, &mesh.normals, &mut mesh.indices);
            for (_, li) in lods.iter_mut() {
                fix_winding(&mesh.positions, &mesh.normals, li);
            }
            out.push(Surface { mat: m, mesh, lods });
        }
        if timing {
            eprintln!("sdf: simplifié à {:?}", t0.elapsed());
        }
        out
    }
}

/// Retourne les triangles repliés : après recalage des sommets sur la surface
/// et simplification, quelques petits triangles finissent à l'envers et
/// seraient éliminés par le rendu (trous visibles sur les tubes). On les remet
/// face à la normale du champ (faces avant dans le sens horaire pour Godot).
fn fix_winding(pos: &[V3], nrm: &[V3], idx: &mut [i32]) {
    for t in idx.chunks_mut(3) {
        let p = |i: i32| pos[i as usize];
        let g = cross(sub(p(t[1]), p(t[0])), sub(p(t[2]), p(t[0])));
        let n = add(
            add(nrm[t[0] as usize], nrm[t[1] as usize]),
            nrm[t[2] as usize],
        );
        if dot(g, n) > 0.0 {
            t.swap(1, 2);
        }
    }
}

/// Niveaux de détail d'un maillage quelconque (pièces tournées ou balayées) :
/// simplifications enchaînées, une par erreur absolue (m) de `errors`. Les
/// niveaux qui ne retirent pas au moins 10 % des triangles sont omis.
pub fn lods(positions: &[V3], indices: &[u32], errors: &[f32]) -> Vec<(f32, Vec<u32>)> {
    let opts = meshopt::SimplifyOptions::ErrorAbsolute | meshopt::SimplifyOptions::Sparse;
    let mut out = Vec::new();
    let mut prev = indices.to_vec();
    for &e in errors {
        let l = meshopt::simplify_decoder(&prev, positions, 0, e, opts, None);
        prev = l.clone();
        if l.len() * 10 > indices.len() * 9 {
            continue;
        }
        out.push((e, l));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_mesh_is_on_surface() {
        let mut s = Sdf::new();
        let f = Frame {
            origin: [0.0; 3],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        };
        s.push(
            Shape::Ellipsoid {
                f,
                radii: [0.05; 3],
            },
            Op::Union,
            0.0,
            0,
            [1.0; 4],
        );
        let out = s.build(0.004, 0.0002, &[]);
        assert_eq!(out.len(), 1);
        let m = &out[0].mesh;
        assert!(m.indices.len() > 100);
        for (p, n) in m.positions.iter().zip(&m.normals) {
            assert!((len(*p) - 0.05).abs() < 0.0015, "{p:?}");
            assert!(dot(normalize(*p), *n) > 0.95);
        }
        // Godot : faces avant dans le sens horaire, produit vectoriel vers l'intérieur.
        for t in m.indices.chunks(3) {
            let p = |i: i32| m.positions[i as usize];
            let g = cross(sub(p(t[1]), p(t[0])), sub(p(t[2]), p(t[0])));
            let c = mul(add(add(p(t[0]), p(t[1])), p(t[2])), 1.0 / 3.0);
            assert!(dot(g, c) < 0.0);
        }
    }

    #[test]
    fn tube_section_teardrop() {
        let sec = Section {
            c1: [0.0, 0.01],
            r1: 0.02,
            c2: [0.0, -0.03],
            r2: 0.005,
            cut: 0.0,
        };
        assert!((sec.dist(0.0, 0.03) - 0.0).abs() < 1e-5);
        assert!((sec.dist(0.0, -0.035) - 0.0).abs() < 1e-5);
        assert!((sec.dist(0.02, 0.01)).abs() < 1e-5);
        assert!(sec.dist(0.0, 0.0) < 0.0);
    }

    #[test]
    fn tube_mesh_is_closed_and_simplified() {
        let mut s = Sdf::new();
        let pts: Vec<V3> = (0..10)
            .map(|i| [0.0, i as f32 * 0.05, (i as f32 * 0.3).sin() * 0.02])
            .collect();
        let secs: Vec<Section> = (0..10)
            .map(|_| Section {
                c1: [0.0, 0.01],
                r1: 0.02,
                c2: [0.0, -0.02],
                r2: 0.008,
                cut: 0.0,
            })
            .collect();
        let chords = vec![[0.0, 0.0, 1.0]; 10];
        let t = s.tube(&pts, &secs, &chords).unwrap();
        s.push(t, Op::Union, 0.0, 0, [1.0; 4]);
        let out = s.build(0.0016, 0.00015, &[0.001]);
        let m = &out[0].mesh;
        assert_eq!(out[0].lods.len(), 1);
        assert!(out[0].lods[0].1.len() < m.indices.len());
        let mut edges: HashMap<(i32, i32), i32> = HashMap::new();
        for t in m.indices.chunks(3) {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let border = edges.values().filter(|&&c| c == 1).count();
        let nonman = edges.values().filter(|&&c| c > 2).count();
        // Surface fermée, sans arête partagée par plus de deux triangles.
        assert_eq!(border, 0);
        assert_eq!(nonman, 0);
        assert!(m.indices.len() / 3 < 20_000, "{}", m.indices.len() / 3);
    }

    #[test]
    fn lods_reduce_a_dense_grid() {
        // Plan de 40 × 40 quadrilatères : tout niveau de détail le réduit fortement.
        let n = 41;
        let pos: Vec<V3> = (0..n * n)
            .map(|i| [(i % n) as f32 * 0.01, 0.0, (i / n) as f32 * 0.01])
            .collect();
        let mut idx = Vec::new();
        for y in 0..n - 1 {
            for x in 0..n - 1 {
                let a = (y * n + x) as u32;
                idx.extend_from_slice(&[
                    a,
                    a + 1,
                    a + n as u32,
                    a + 1,
                    a + n as u32 + 1,
                    a + n as u32,
                ]);
            }
        }
        let l = lods(&pos, &idx, &[0.001]);
        assert_eq!(l.len(), 1);
        assert!(l[0].1.len() * 4 < idx.len());
    }
}
