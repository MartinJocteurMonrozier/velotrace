//! La route d'après OpenStreetMap : tracé recalé sur la vraie route, largeur,
//! voies, marquages et trottoirs, amorces des routes qui débouchent sur le
//! parcours et ronds-points traversés.

use crate::mesh::{Instances, MeshData};
use crate::osm::{join_rings, Line, LineKind, OsmData};
use crate::road::{marks, push_strip, Profile, RoadPath, Section, KERB, VERGE};
use std::collections::{HashMap, HashSet};
use std::f64::consts::TAU;
use velo_core::route::Route;

/// Écart maximal entre le GPX et la route OSM sur laquelle on le recale (m).
const MATCH_RADIUS: f64 = 14.0;
/// Longueur des amorces des routes qui croisent le parcours (m).
pub const STUB_LEN: f64 = 30.0;
/// Évasement des amorces au débouché (m de plus de chaque côté).
const FLARE: f64 = 2.5;
/// Accotement réduit entre trottoir et façades.
const TOWN_VERGE: f64 = 0.6;

/// Index spatial simple : identifiants rangés par case carrée.
#[derive(Debug, Clone, Default)]
struct Grid {
    cell: f64,
    map: HashMap<(i64, i64), Vec<u32>>,
}

impl Grid {
    fn new(cell: f64) -> Self {
        Grid {
            cell,
            map: HashMap::new(),
        }
    }

    fn key(&self, x: f64, n: f64) -> (i64, i64) {
        (
            (x / self.cell).floor() as i64,
            (n / self.cell).floor() as i64,
        )
    }

    fn insert(&mut self, x: f64, n: f64, id: u32) {
        let k = self.key(x, n);
        self.map.entry(k).or_default().push(id);
    }

    /// Range `id` dans toutes les cases que touche le rectangle `a`–`b`.
    fn insert_box(&mut self, a: (f64, f64), b: (f64, f64), id: u32) {
        let (i0, j0) = self.key(a.0.min(b.0), a.1.min(b.1));
        let (i1, j1) = self.key(a.0.max(b.0), a.1.max(b.1));
        for i in i0..=i1 {
            for j in j0..=j1 {
                self.map.entry((i, j)).or_default().push(id);
            }
        }
    }

    /// Appelle `f` pour chaque identifiant des cases à moins de `r` (doublons possibles).
    fn near(&self, x: f64, n: f64, r: f64, mut f: impl FnMut(u32)) {
        let (i0, j0) = self.key(x - r, n - r);
        let (i1, j1) = self.key(x + r, n + r);
        for i in i0..=i1 {
            for j in j0..=j1 {
                if let Some(v) = self.map.get(&(i, j)) {
                    for &id in v {
                        f(id);
                    }
                }
            }
        }
    }
}

/// Échantillons du tracé, pour trouver le plus proche d'un point.
#[derive(Debug, Clone, Default)]
pub struct SampleIndex {
    grid: Grid,
    pts: Vec<(f64, f64, usize)>,
}

impl SampleIndex {
    fn new(path: &RoadPath) -> Self {
        let mut idx = SampleIndex {
            grid: Grid::new(16.0),
            pts: Vec::new(),
        };
        let m = path.samples.len();
        for (k, p) in path.samples.iter().enumerate() {
            if k % 2 == 0 || k + 1 == m {
                idx.grid.insert(p.x, p.n, idx.pts.len() as u32);
                idx.pts.push((p.x, p.n, k));
            }
        }
        idx
    }

    /// Distance et indice de l'échantillon le plus proche à moins de `max_m`.
    pub fn nearest(&self, x: f64, n: f64, max_m: f64) -> Option<(f64, usize)> {
        let mut best: Option<(f64, usize)> = None;
        self.grid.near(x, n, max_m, |i| {
            let (px, pn, k) = self.pts[i as usize];
            let d = (px - x).hypot(pn - n);
            if d <= max_m && best.map_or(true, |(bd, _)| d < bd) {
                best = Some((d, k));
            }
        });
        best
    }
}

#[derive(Debug, Clone, Copy)]
struct FlatPoint {
    x: f64,
    n: f64,
    /// Hauteur visée du sol.
    target: f64,
    /// Rayon du replat, puis largeur du fondu vers le relief.
    flat: f64,
    blend: f64,
    route: bool,
}

/// Où et à quelle hauteur aplanir le relief sous les chaussées.
#[derive(Debug, Clone, Default)]
pub struct Flatten {
    grid: Grid,
    pts: Vec<FlatPoint>,
    reach: f64,
}

impl Flatten {
    fn new() -> Self {
        Flatten {
            grid: Grid::new(16.0),
            pts: Vec::new(),
            reach: 0.0,
        }
    }

    fn add(&mut self, p: FlatPoint) {
        self.reach = self.reach.max(p.flat + p.blend);
        self.grid.insert(p.x, p.n, self.pts.len() as u32);
        self.pts.push(p);
    }

    /// Poids (1 : sous une chaussée) et hauteur visée du sol en (x, n).
    pub fn weight(&self, x: f64, n: f64) -> Option<(f64, f64)> {
        let mut best: Option<(f64, f64, f64)> = None;
        self.grid.near(x, n, self.reach, |i| {
            let p = &self.pts[i as usize];
            let d = (p.x - x).hypot(p.n - n);
            if d >= p.flat + p.blend {
                return;
            }
            let w = 1.0 - smoothstep(p.flat, p.flat + p.blend, d);
            let better = match best {
                None => true,
                Some((bw, bd, _)) => w > bw + 1e-9 || (w > bw - 1e-9 && d < bd),
            };
            if better {
                best = Some((w, d, p.target));
            }
        });
        best.map(|(w, _, t)| (w, t))
    }

    /// Altitude du relief `dem` aplanie sous les chaussées.
    pub fn apply(&self, dem: f64, x: f64, n: f64) -> f64 {
        match self.weight(x, n) {
            Some((w, t)) => dem + (t - dem) * w,
            None => dem,
        }
    }

    /// Le point est-il à moins de `margin` de la chaussée d'une amorce ou
    /// d'un rond-point (pas du parcours lui-même) ?
    pub fn near_other(&self, x: f64, n: f64, margin: f64) -> bool {
        let mut hit = false;
        self.grid.near(x, n, self.reach, |i| {
            let p = &self.pts[i as usize];
            if !p.route && (p.x - x).hypot(p.n - n) < p.flat + margin {
                hit = true;
            }
        });
        hit
    }
}

/// Rond-point traversé ou longé par le parcours (sens antihoraire).
#[derive(Debug, Clone, PartialEq)]
pub struct Roundabout {
    pub center: (f64, f64),
    /// Rayon de l'axe de l'anneau (m).
    pub radius: f64,
    pub half_width: f64,
    /// Bande pavée franchissable autour de l'îlot.
    pub apron: f64,
    pub elevation: f64,
    pub lanes: u8,
}

impl Roundabout {
    /// Rayon de l'îlot central.
    pub fn island(&self) -> f64 {
        (self.radius - self.half_width - self.apron).max(0.5)
    }
}

/// Tout ce qui entoure le parcours d'après OSM.
#[derive(Debug, Clone, Default)]
pub struct RoadNetwork {
    /// Amorces : rubans de chaussée, altitude absolue.
    pub stubs: Vec<Vec<Section>>,
    pub roundabouts: Vec<Roundabout>,
    pub flat: Flatten,
    pub samples: SampleIndex,
}

/// Point de raccord d'une route au parcours ou à un rond-point.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Hit {
    Route(usize),
    Ring(usize),
}

/// Point du GPX rattaché à une route OSM : indice de la ligne, point projeté
/// et direction du segment.
type Match = (usize, (f64, f64), (f64, f64));

/// Rattachement envisagé : coût, point projeté, direction du segment.
type Candidate = (f64, (f64, f64), (f64, f64));

/// Anneau possible : points, lignes OSM qui peuvent le former, étiqueté ou non.
type RingCandidate = (Vec<(f64, f64)>, Vec<usize>, bool);

/// Amorce avant le calcul de son altitude.
struct StubDraft {
    pts: Vec<(f64, f64)>,
    start: Hit,
    end: Option<Hit>,
    half_width: f64,
    lanes: u8,
    oneway: bool,
    give_way: bool,
}

/// Largeur de chaussée (m) et nombre de voies d'une route OSM.
pub fn carriageway(line: &Line) -> (f64, u8) {
    let i = line.info;
    let lanes = i.lanes.unwrap_or(if i.rank == 0 {
        1
    } else if i.oneway {
        if i.rank >= 5 {
            2
        } else {
            1
        }
    } else {
        2
    });
    let lane_w = match i.rank {
        5 | 6 => 3.5,
        4 => 3.25,
        3 => 3.0,
        2 => 2.75,
        1 => 2.6,
        _ => 2.0,
    };
    let w = if i.width_tagged {
        line.width_m
    } else if i.rank == 0 && i.lanes.is_none() {
        line.width_m.max(2.5)
    } else {
        (f64::from(lanes) * lane_w).max(if lanes == 1 { 3.5 } else { 0.0 })
    };
    (w.clamp(2.5, 28.0), lanes)
}

/// Profil de la chaussée d'une route OSM (sans les trottoirs).
fn profile_for(line: &Line) -> Profile {
    let (w, lanes) = carriageway(line);
    let i = line.info;
    let mut m = 0;
    if !i.oneway && lanes >= 2 && (i.rank >= 3 || i.lanes.is_some()) {
        m |= if lanes >= 4 {
            marks::CENTRE_SOLID
        } else {
            marks::CENTRE_DASH
        };
    }
    if i.rank >= 3 {
        m |= marks::EDGE_LEFT | marks::EDGE_RIGHT;
    }
    let paved = matches!(line.kind, LineKind::Road { paved: true, .. });
    if !paved {
        m = 0;
    }
    Profile {
        half_width: w / 2.0,
        sidewalk: [0.0; 2],
        verge: [VERGE; 2],
        lanes: if paved { lanes } else { 0 },
        oneway: i.oneway,
        marks: m,
        hidden: false,
    }
}

fn project(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> (f64, (f64, f64)) {
    let (dx, dn) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dn * dn;
    let t = if l2 > 1e-12 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dn) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = (a.0 + dx * t, a.1 + dn * t);
    ((p.0 - q.0).hypot(p.1 - q.1), q)
}

fn unit(v: (f64, f64)) -> (f64, f64) {
    let l = v.0.hypot(v.1);
    if l > 1e-9 {
        (v.0 / l, v.1 / l)
    } else {
        (0.0, 0.0)
    }
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Lissage gaussien d'une suite régulière, `sigma` en nombre d'éléments.
fn gaussian(v: &[f64], sigma: f64) -> Vec<f64> {
    let r = (3.0 * sigma).ceil() as isize;
    let n = v.len() as isize;
    (0..n)
        .map(|i| {
            let (mut s, mut sw) = (0.0, 0.0);
            for k in -r..=r {
                let j = i + k;
                if (0..n).contains(&j) {
                    let w = (-((k * k) as f64) / (2.0 * sigma * sigma)).exp();
                    s += v[j as usize] * w;
                    sw += w;
                }
            }
            s / sw
        })
        .collect()
}

/// Supprime les morceaux vrais de moins de `min_on` éléments et bouche les
/// trous de moins de `min_off` éléments.
fn clean_runs(v: &mut [bool], min_off: usize, min_on: usize) {
    let runs = |v: &[bool]| {
        let mut out = Vec::new();
        let mut a = 0;
        for i in 1..=v.len() {
            if i == v.len() || v[i] != v[a] {
                out.push((a, i, v[a]));
                a = i;
            }
        }
        out
    };
    for (a, b, on) in runs(v) {
        if !on && b - a < min_off && a > 0 && b < v.len() {
            v[a..b].fill(true);
        }
    }
    for (a, b, on) in runs(v) {
        if on && b - a < min_on {
            v[a..b].fill(false);
        }
    }
}

/// La polyligne est-elle un arc de cercle de 5 à 45 m de rayon (morceau
/// d'anneau de rond-point) ?
fn is_arc(pts: &[(f64, f64)]) -> bool {
    if pts.len() < 3 {
        return false;
    }
    let (a, b, c) = (pts[0], pts[pts.len() / 2], *pts.last().unwrap());
    let d = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
    if d.abs() < 1e-9 {
        return false;
    }
    let sq = |p: (f64, f64)| p.0 * p.0 + p.1 * p.1;
    let center = (
        (sq(a) * (b.1 - c.1) + sq(b) * (c.1 - a.1) + sq(c) * (a.1 - b.1)) / d,
        (sq(a) * (c.0 - b.0) + sq(b) * (a.0 - c.0) + sq(c) * (b.0 - a.0)) / d,
    );
    let r = dist(a, center);
    let chord = dist(a, c);
    (5.0..45.0).contains(&r)
        && chord > r * 0.35
        && pts
            .iter()
            .all(|&p| (dist(p, center) - r).abs() < r * 0.15 + 0.5)
}

/// Points tous les `step` mètres au plus le long d'une polyligne.
fn densify(pts: &[(f64, f64)], step: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::with_capacity(pts.len() * 2);
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let k = (dist(a, b) / step).ceil().max(1.0) as usize;
        for j in 0..k {
            let t = j as f64 / k as f64;
            out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    if let Some(&l) = pts.last() {
        out.push(l);
    }
    out
}

/// Polyligne rééchantillonnée tous les mètres, avec l'abscisse de chaque point.
fn resample(pts: &[(f64, f64)]) -> Vec<(f64, f64, f64)> {
    let mut out = vec![(pts[0].0, pts[0].1, 0.0)];
    let mut s0 = 0.0;
    let mut next = 1.0;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let l = dist(a, b);
        while next <= s0 + l {
            let t = (next - s0) / l.max(1e-9);
            out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, next));
            next += 1.0;
        }
        s0 += l;
    }
    let last = *pts.last().unwrap();
    if s0 - out.last().unwrap().2 > 0.3 {
        out.push((last.0, last.1, s0));
    }
    out
}

/// Recale le parcours sur OSM et décrit la route et ses abords.
/// `dem(x, n)` donne l'altitude brute du relief.
pub fn build(
    route: &Route,
    osm: &OsmData,
    dem: &dyn Fn(f64, f64) -> f64,
) -> (RoadPath, RoadNetwork) {
    let roads: Vec<usize> = (0..osm.lines.len())
        .filter(|&i| matches!(osm.lines[i].kind, LineKind::Road { .. }))
        .collect();
    let mut segs: Vec<(usize, usize)> = Vec::new();
    let mut seg_grid = Grid::new(32.0);
    for &li in &roads {
        let pts = &osm.lines[li].points;
        for si in 0..pts.len().saturating_sub(1) {
            seg_grid.insert_box(pts[si], pts[si + 1], segs.len() as u32);
            segs.push((li, si));
        }
    }

    // 1. Chaque point du GPX est rattaché à la route OSM la plus plausible :
    //    proche, dans le même sens, et de préférence la même qu'au point d'avant.
    let rp = &route.points;
    let m = rp.len();
    let mut matched: Vec<Option<Match>> = vec![None; m];
    let mut prev: Option<usize> = None;
    for i in 0..m {
        let p = (rp[i].x, rp[i].y);
        let (a, b) = (&rp[i.saturating_sub(1)], &rp[(i + 1).min(m - 1)]);
        let h = unit((b.x - a.x, b.y - a.y));
        // Par route : coût, point projeté, direction du segment.
        let mut by_line: HashMap<usize, Candidate> = HashMap::new();
        seg_grid.near(p.0, p.1, MATCH_RADIUS, |id| {
            let (li, si) = segs[id as usize];
            let line = &osm.lines[li];
            let (sa, sb) = (line.points[si], line.points[si + 1]);
            let (d, q) = project(p, sa, sb);
            if d > MATCH_RADIUS {
                return;
            }
            let dir = unit((sb.0 - sa.0, sb.1 - sa.1));
            let cos = dir.0 * h.0 + dir.1 * h.1;
            let mut cost = d + 8.0 * (1.0 - cos.abs());
            if line.info.oneway && cos < 0.0 {
                cost += 15.0;
            }
            let e = by_line.entry(li).or_insert((f64::MAX, q, dir));
            if cost < e.0 {
                *e = (cost, q, dir);
            }
        });
        let best = by_line
            .iter()
            .min_by(|a, b| a.1 .0.total_cmp(&b.1 .0).then(a.0.cmp(b.0)))
            .map(|(&li, &v)| (li, v));
        let chosen = match (best, prev.and_then(|pl| by_line.get(&pl).map(|v| (pl, *v)))) {
            (Some((_, bv)), Some((pl, pv))) if pv.0 <= bv.0 + 3.0 => Some((pl, pv)),
            (b, _) => b,
        };
        matched[i] = chosen.map(|(li, (_, q, dir))| (li, q, dir));
        prev = chosen.map(|(li, _)| li);
    }
    // Rattachements trop brefs : sans doute une autre route croisée.
    let mut i = 0;
    while i < m {
        if matched[i].is_none() {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < m && matched[j].is_some() {
            j += 1;
        }
        if j - i < 4 {
            matched[i..j].fill(None);
        }
        i = j;
    }
    let off_x: Vec<f64> = (0..m)
        .map(|i| matched[i].map_or(0.0, |(_, q, _)| q.0 - rp[i].x))
        .collect();
    let off_n: Vec<f64> = (0..m)
        .map(|i| matched[i].map_or(0.0, |(_, q, _)| q.1 - rp[i].y))
        .collect();
    let (off_x, off_n) = (gaussian(&off_x, 1.5), gaussian(&off_n, 1.5));
    let axis: Vec<(f64, f64)> = (0..m)
        .map(|i| (rp[i].x + off_x[i], rp[i].y + off_n[i]))
        .collect();
    let mut path = RoadPath::with_axis(route, Some(&axis));
    let samples = SampleIndex::new(&path);
    let route_index = |d: f64| {
        let i = rp.partition_point(|p| p.distance_m <= d).saturating_sub(1);
        if i + 1 < m && rp[i + 1].distance_m - d < d - rp[i].distance_m {
            i + 1
        } else {
            i
        }
    };
    let ms = path.samples.len();
    // Route OSM sous chaque échantillon, et parcourue à contresens ?
    let way: Vec<Option<(usize, bool)>> = path
        .samples
        .iter()
        .map(|p| {
            matched[route_index(p.d)]
                .map(|(li, _, dir)| (li, dir.0 * p.tangent.0 + dir.1 * p.tangent.1 < 0.0))
        })
        .collect();

    // 2. Ronds-points : anneaux étiquetés `junction=roundabout`, ou petites
    //    boucles fermées presque circulaires.
    let mut ring_lines: HashSet<usize> = HashSet::new();
    let mut candidates: Vec<RingCandidate> = Vec::new();
    let tagged: Vec<usize> = roads
        .iter()
        .copied()
        .filter(|&i| osm.lines[i].info.roundabout)
        .collect();
    for ring in join_rings(
        tagged
            .iter()
            .map(|&i| osm.lines[i].points.clone())
            .collect(),
    ) {
        candidates.push((ring, tagged.clone(), true));
    }
    // Sans étiquette : boucle fermée d'un seul tenant, ou arcs de cercle
    // bout à bout qui se referment.
    let mut arcs: Vec<usize> = Vec::new();
    for &li in &roads {
        let l = &osm.lines[li];
        if l.info.roundabout || l.info.rank == 0 {
            continue;
        }
        let closed = l.points.len() >= 7 && dist(l.points[0], *l.points.last().unwrap()) < 0.05;
        if closed {
            candidates.push((l.points.clone(), vec![li], false));
        } else if is_arc(&l.points) {
            arcs.push(li);
        }
    }
    for ring in join_rings(arcs.iter().map(|&i| osm.lines[i].points.clone()).collect()) {
        candidates.push((ring, arcs.clone(), false));
    }
    let mut rings: Vec<Roundabout> = Vec::new();
    for (pts, lines, tagged) in candidates {
        if dist(pts[0], *pts.last().unwrap()) > 0.05 {
            continue;
        }
        let ring = &pts[..pts.len() - 1];
        let k = ring.len() as f64;
        let c = (
            ring.iter().map(|p| p.0).sum::<f64>() / k,
            ring.iter().map(|p| p.1).sum::<f64>() / k,
        );
        let radii: Vec<f64> = ring.iter().map(|&p| dist(p, c)).collect();
        let r = radii.iter().sum::<f64>() / k;
        let sd = (radii.iter().map(|x| (x - r) * (x - r)).sum::<f64>() / k).sqrt();
        let round_enough = if tagged {
            sd / r < 0.35 && (4.0..80.0).contains(&r)
        } else {
            sd / r < 0.2 && (5.0..45.0).contains(&r)
        };
        if !round_enough {
            continue;
        }
        let Some((_, j)) = samples.nearest(c.0, c.1, r + 30.0) else {
            continue;
        };
        // Les lignes de l'anneau : celles dont un point est sur le cercle.
        let on_ring: Vec<usize> = lines
            .into_iter()
            .filter(|&li| {
                osm.lines[li]
                    .points
                    .iter()
                    .all(|&p| (dist(p, c) - r).abs() < r * 0.5 + 2.0)
            })
            .collect();
        let first = on_ring.first().map(|&li| &osm.lines[li]);
        let lanes = first.and_then(|l| l.info.lanes).unwrap_or(1);
        let mut hw = match first {
            Some(l) if l.info.width_tagged => l.width_m / 2.0,
            _ => (f64::from(lanes) * 3.5).max(7.0) / 2.0,
        };
        hw = hw.min(r * 0.55);
        ring_lines.extend(on_ring);
        rings.push(Roundabout {
            center: c,
            radius: r,
            half_width: hw,
            apron: (r - hw - 2.0).clamp(0.0, 1.5),
            elevation: path.samples[j].elevation,
            lanes,
        });
    }

    // 3. Profil de la chaussée sous chaque échantillon.
    for (p, w) in path.samples.iter_mut().zip(&way) {
        if let Some((li, _)) = *w {
            p.profile = profile_for(&osm.lines[li]);
        }
        p.profile.hidden = rings
            .iter()
            .any(|r| dist((p.x, p.n), r.center) < r.radius + r.half_width - 0.7);
    }

    // 4. Amorces des routes qui quittent le parcours ou un rond-point.
    let hit_at = |q: (f64, f64), path: &RoadPath| -> Option<Hit> {
        for (k, r) in rings.iter().enumerate() {
            if dist(q, r.center) < r.radius + r.half_width + 0.3 {
                return Some(Hit::Ring(k));
            }
        }
        let (d, j) = samples.nearest(q.0, q.1, 12.0)?;
        let pr = &path.samples[j].profile;
        (d < pr.half_width + 0.6 && !pr.hidden).then_some(Hit::Route(j))
    };
    // Écart (signé) au bord de la chaussée qu'on rejoint, un peu en dedans.
    let edge_gap = |hit: Hit, q: (f64, f64), path: &RoadPath| -> f64 {
        match hit {
            Hit::Ring(k) => {
                dist(q, rings[k].center) - (rings[k].radius + rings[k].half_width - 0.3)
            }
            Hit::Route(j) => {
                let p = &path.samples[j];
                let (rx, rn) = p.right();
                ((q.0 - p.x) * rx + (q.1 - p.n) * rn).abs() - (p.profile.half_width - 0.3)
            }
        }
    };
    let mut drafts: Vec<StubDraft> = Vec::new();
    for &li in &roads {
        if ring_lines.contains(&li) {
            continue;
        }
        let line = &osm.lines[li];
        let dens = densify(&line.points, 2.0);
        let hits: Vec<Option<Hit>> = dens.iter().map(|&q| hit_at(q, &path)).collect();
        if hits.iter().all(Option::is_none) || hits.iter().all(Option::is_some) {
            continue;
        }
        let (width, lanes) = carriageway(line);
        let main_rank = |h: Hit| match h {
            Hit::Route(j) => way[j].map_or(3, |(l, _)| osm.lines[l].info.rank),
            Hit::Ring(_) => 6,
        };
        let mut i = 0;
        while i < dens.len() {
            if hits[i].is_some() {
                i += 1;
                continue;
            }
            let a = i;
            while i < dens.len() && hits[i].is_none() {
                i += 1;
            }
            let b = i - 1;
            let start = if a > 0 { hits[a - 1] } else { None };
            let end = if i < dens.len() { hits[i] } else { None };
            // Point où la route franchit le bord de la chaussée.
            let crossing = |hit: Hit, inside: (f64, f64), outside: (f64, f64)| {
                let (mut lo, mut hi) = (0.0, 1.0);
                for _ in 0..14 {
                    let t: f64 = (lo + hi) / 2.0;
                    let q = (
                        inside.0 + (outside.0 - inside.0) * t,
                        inside.1 + (outside.1 - inside.1) * t,
                    );
                    if edge_gap(hit, q, &path) < 0.0 {
                        lo = t;
                    } else {
                        hi = t;
                    }
                }
                (
                    inside.0 + (outside.0 - inside.0) * lo,
                    inside.1 + (outside.1 - inside.1) * lo,
                )
            };
            let mut poly: Vec<(f64, f64)> = Vec::new();
            if let Some(h) = start {
                poly.push(crossing(h, dens[a - 1], dens[a]));
            }
            poly.extend_from_slice(&dens[a..=b]);
            if let Some(h) = end {
                poly.push(crossing(h, dens[b + 1], dens[b]));
            }
            if poly.len() < 2 {
                continue;
            }
            let len: f64 = poly.windows(2).map(|w| dist(w[0], w[1])).sum();
            let give_way = |h: Hit| line.info.rank >= 1 && line.info.rank <= main_rank(h);
            let mut push = |pts: Vec<(f64, f64)>, s: Hit, e: Option<Hit>| {
                let l: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
                if l < 3.0 {
                    return;
                }
                // Une route qui longe le parcours n'en débouche pas.
                if let Hit::Route(j) = s {
                    let p = &path.samples[j];
                    let k = pts
                        .iter()
                        .position(|&q| dist(q, pts[0]) > 6.0)
                        .unwrap_or(pts.len() - 1);
                    let d = unit((pts[k].0 - pts[0].0, pts[k].1 - pts[0].1));
                    if (d.0 * p.tangent.0 + d.1 * p.tangent.1).abs() > 0.9 {
                        return;
                    }
                }
                drafts.push(StubDraft {
                    pts,
                    start: s,
                    end: e,
                    half_width: (width / 2.0).clamp(1.4, 6.0),
                    lanes: if matches!(line.kind, LineKind::Road { paved: true, .. }) {
                        lanes
                    } else {
                        0
                    },
                    oneway: line.info.oneway,
                    give_way: give_way(s),
                });
            };
            let cut = |pts: &[(f64, f64)]| {
                let mut out = vec![pts[0]];
                let mut l = 0.0;
                for w in pts.windows(2) {
                    let d = dist(w[0], w[1]);
                    if l + d >= STUB_LEN {
                        let t = (STUB_LEN - l) / d;
                        out.push((
                            w[0].0 + (w[1].0 - w[0].0) * t,
                            w[0].1 + (w[1].1 - w[0].1) * t,
                        ));
                        break;
                    }
                    l += d;
                    out.push(w[1]);
                }
                out
            };
            match (start, end) {
                (Some(s), Some(e)) if len <= 2.0 * STUB_LEN + 10.0 => push(poly, s, Some(e)),
                _ => {
                    if let Some(s) = start {
                        push(cut(&poly), s, None);
                    }
                    if let Some(e) = end {
                        let rev: Vec<(f64, f64)> = poly.iter().rev().copied().collect();
                        push(cut(&rev), e, None);
                    }
                }
            }
        }
    }

    // Débouchés sur le parcours : rive discontinue, trottoir interrompu.
    let mut cut_sidewalk = [vec![false; ms], vec![false; ms]];
    for d in &drafts {
        for (hit, pts) in [
            (Some(d.start), d.pts.clone()),
            (d.end, d.pts.iter().rev().copied().collect()),
        ] {
            let Some(Hit::Route(j)) = hit else { continue };
            let p = path.samples[j];
            let k = pts
                .iter()
                .position(|&q| dist(q, pts[0]) > 6.0)
                .unwrap_or(pts.len() - 1);
            let dir = unit((pts[k].0 - pts[0].0, pts[k].1 - pts[0].1));
            let (rx, rn) = p.right();
            let side = usize::from(dir.0 * rx + dir.1 * rn > 0.0);
            let sin = (dir.0 * p.tangent.1 - dir.1 * p.tangent.0).abs().max(0.3);
            let reach = ((d.half_width + FLARE) / sin).min(18.0) + 1.0;
            let gap = [marks::GAP_LEFT, marks::GAP_RIGHT][side];
            let (lo, hi) = (
                j.saturating_sub(reach as usize + 1),
                (j + reach as usize + 2).min(ms),
            );
            for (q, cut) in path.samples[lo..hi]
                .iter_mut()
                .zip(&mut cut_sidewalk[side][lo..hi])
            {
                if (q.s - p.s).abs() < reach {
                    q.profile.marks |= gap;
                    *cut = true;
                }
            }
        }
    }

    // 5. Trottoirs : étiquettes OSM, sinon trottoir dessiné à part, sinon
    //    façades proches (rue de village).
    let mut walk_grid = Grid::new(32.0);
    let mut walk_segs: Vec<((f64, f64), (f64, f64))> = Vec::new();
    for l in osm.lines.iter().filter(|l| l.info.is_sidewalk) {
        for w in l.points.windows(2) {
            walk_grid.insert_box(w[0], w[1], walk_segs.len() as u32);
            walk_segs.push((w[0], w[1]));
        }
    }
    let mut house_grid = Grid::new(32.0);
    let houses: Vec<(f64, f64)> = osm.buildings.iter().map(|b| b.bbox.center()).collect();
    for (i, h) in houses.iter().enumerate() {
        house_grid.insert(h.0, h.1, i as u32);
    }
    let mut town = [vec![false; ms], vec![false; ms]];
    let mut last_guess = [false; 2];
    for k in 0..ms {
        let p = path.samples[k];
        let Some((li, rev)) = way[k] else { continue };
        let info = osm.lines[li].info;
        if p.profile.hidden || info.rank == 0 {
            continue;
        }
        let (rx, rn) = p.right();
        for side in 0..2 {
            let tag = info.sidewalk[if rev { 1 - side } else { side }];
            town[side][k] = match tag {
                Some(b) => b,
                None if k % 4 == 0 => {
                    let sg = if side == 0 { -1.0 } else { 1.0 };
                    let hw = p.profile.half_width;
                    let probe = (p.x + rx * sg * (hw + 2.5), p.n + rn * sg * (hw + 2.5));
                    let mut found = false;
                    walk_grid.near(probe.0, probe.1, 4.0, |id| {
                        let (a, b) = walk_segs[id as usize];
                        let dir = unit((b.0 - a.0, b.1 - a.1));
                        if project(probe, a, b).0 < 3.5
                            && (dir.0 * p.tangent.0 + dir.1 * p.tangent.1).abs() > 0.8
                        {
                            found = true;
                        }
                    });
                    if !found {
                        let mut count = 0;
                        let mut seen = HashSet::new();
                        house_grid.near(p.x, p.n, 40.0, |id| {
                            if !seen.insert(id) {
                                return;
                            }
                            let h = houses[id as usize];
                            let (vx, vn) = (h.0 - p.x, h.1 - p.n);
                            let lateral = (vx * rx + vn * rn) * sg;
                            let along = vx * p.tangent.0 + vn * p.tangent.1;
                            if (hw + 1.0..hw + 28.0).contains(&lateral) && along.abs() < 25.0 {
                                count += 1;
                            }
                        });
                        found = count >= 3;
                    }
                    last_guess[side] = found;
                    found
                }
                None => last_guess[side],
            };
        }
    }
    for side in 0..2 {
        clean_runs(&mut town[side], 15, 20);
        for (t, c) in town[side].iter_mut().zip(&cut_sidewalk[side]) {
            *t &= !c;
        }
        let presence: Vec<f64> = gaussian(
            &town[side]
                .iter()
                .map(|&b| f64::from(u8::from(b)))
                .collect::<Vec<_>>(),
            1.5,
        );
        for (k, pr) in presence.into_iter().enumerate() {
            let p = &mut path.samples[k];
            let rank = way[k].map_or(0, |(l, _)| osm.lines[l].info.rank);
            let full = if rank >= 3 { 2.0 } else { 1.6 };
            let w = if pr < 0.05 { 0.0 } else { full * pr };
            p.profile.sidewalk[side] = w;
            if w > 0.0 {
                p.profile.verge[side] = TOWN_VERGE + (VERGE - TOWN_VERGE) * (1.0 - pr);
                p.profile.marks &= ![marks::EDGE_LEFT, marks::EDGE_RIGHT][side];
            }
        }
    }

    // 6. Largeur lissée ; axe continu dans les virages serrés (lacets).
    let hw: Vec<f64> = path.samples.iter().map(|p| p.profile.half_width).collect();
    let wide_k: Vec<f64> = gaussian(
        &path.samples.iter().map(|p| p.curvature).collect::<Vec<_>>(),
        12.0,
    );
    for ((p, w), k) in path.samples.iter_mut().zip(gaussian(&hw, 5.0)).zip(wide_k) {
        p.profile.half_width = w;
        if p.profile.marks & marks::CENTRE_DASH != 0 && k.abs() > 1.0 / 70.0 {
            p.profile.marks = (p.profile.marks & !marks::CENTRE_DASH) | marks::CENTRE_SOLID;
        }
    }
    path.compute_racing_line(route);

    // 7. Relief aplani sous le parcours et les ronds-points, puis sous les amorces.
    let mut flat = Flatten::new();
    for (k, p) in path.samples.iter().enumerate() {
        if k % 2 != 0 || p.profile.hidden {
            continue;
        }
        let pr = &p.profile;
        flat.add(FlatPoint {
            x: p.x,
            n: p.n,
            target: p.elevation - 0.35,
            flat: pr.reach(0).max(pr.reach(1)) + 0.7,
            blend: 23.0,
            route: true,
        });
    }
    for r in &rings {
        let steps = (TAU * r.radius / 2.0).ceil() as usize;
        for i in 0..steps {
            let a = i as f64 / steps as f64 * TAU;
            flat.add(FlatPoint {
                x: r.center.0 + r.radius * a.cos(),
                n: r.center.1 + r.radius * a.sin(),
                target: r.elevation - 0.35,
                flat: r.half_width + 1.6,
                blend: 15.0,
                route: false,
            });
        }
        flat.add(FlatPoint {
            x: r.center.0,
            n: r.center.1,
            target: r.elevation - 0.35,
            flat: r.radius,
            blend: 15.0,
            route: false,
        });
    }
    let level = |h: Hit| match h {
        Hit::Route(j) => path.samples[j].elevation,
        Hit::Ring(k) => rings[k].elevation,
    };
    let mut stubs = Vec::with_capacity(drafts.len());
    let mut stub_flat = Vec::new();
    for d in &drafts {
        let mut pts = resample(&d.pts);
        // Léger lissage des angles des nœuds OSM, extrémités fixes.
        for _ in 0..2 {
            let prev = pts.clone();
            for i in 1..pts.len().saturating_sub(1) {
                pts[i].0 = (prev[i - 1].0 + 2.0 * prev[i].0 + prev[i + 1].0) / 4.0;
                pts[i].1 = (prev[i - 1].1 + 2.0 * prev[i].1 + prev[i + 1].1) / 4.0;
            }
        }
        let len = pts.last().map_or(0.0, |p| p.2);
        let y0 = level(d.start) - 0.03;
        let y1 = d.end.map(|h| level(h) - 0.03);
        let mut secs = Vec::with_capacity(pts.len());
        for (i, &(x, n, t)) in pts.iter().enumerate() {
            let a = pts[i.saturating_sub(1)];
            let b = pts[(i + 1).min(pts.len() - 1)];
            let tan = unit((b.0 - a.0, b.1 - a.1));
            let ground = flat.apply(dem(x, n), x, n) + 0.12;
            let mut y = y0 + (ground - y0) * smoothstep(2.0, 22.0, t);
            let mut flare = FLARE * (1.0 - smoothstep(0.0, 7.0, t)).powi(2);
            let mut s = t;
            if let Some(y1) = y1 {
                y += (y1 - y) * (1.0 - smoothstep(2.0, 22.0, len - t));
                flare += FLARE * (1.0 - smoothstep(0.0, 7.0, len - t)).powi(2);
                s = t.min(len - t);
            }
            secs.push(Section {
                x,
                n,
                elevation: y,
                right: (tan.1, -tan.0),
                s,
                profile: Profile {
                    half_width: d.half_width + flare,
                    sidewalk: [0.0; 2],
                    verge: [0.5; 2],
                    lanes: d.lanes,
                    oneway: d.oneway,
                    marks: if d.give_way { marks::GIVE_WAY } else { 0 },
                    hidden: false,
                },
            });
            if i % 2 == 0 {
                stub_flat.push(FlatPoint {
                    x,
                    n,
                    target: y - 0.2,
                    flat: d.half_width + flare + 0.5,
                    blend: 10.0,
                    route: false,
                });
            }
        }
        stubs.push(secs);
    }
    for p in stub_flat {
        flat.add(p);
    }
    let net = RoadNetwork {
        stubs,
        roundabouts: rings,
        flat,
        samples,
    };
    (path, net)
}

impl RoadNetwork {
    /// Amorces, anneaux et îlots, regroupés par cases de 300 m pour que Godot
    /// n'affiche que ceux qui sont dans le champ.
    pub fn meshes(&self, base: f64) -> Vec<MeshData> {
        let mut cells: HashMap<(i64, i64), MeshData> = HashMap::new();
        let key = |x: f64, n: f64| ((x / 300.0).floor() as i64, (n / 300.0).floor() as i64);
        for s in &self.stubs {
            if let Some(f) = s.first() {
                push_strip(cells.entry(key(f.x, f.n)).or_default(), s, base);
            }
        }
        for r in &self.roundabouts {
            let mesh = cells.entry(key(r.center.0, r.center.1)).or_default();
            let steps = (TAU * r.radius).ceil().max(24.0) as usize;
            let secs: Vec<Section> = (0..=steps)
                .map(|i| {
                    let a = i as f64 / steps as f64 * TAU;
                    let (c, s) = (a.cos(), a.sin());
                    Section {
                        x: r.center.0 + r.radius * c,
                        n: r.center.1 + r.radius * s,
                        elevation: r.elevation,
                        // Sens antihoraire : la droite est vers l'extérieur.
                        right: (c, s),
                        s: a * r.radius,
                        profile: Profile {
                            half_width: r.half_width,
                            sidewalk: [r.apron, 0.0],
                            verge: [0.0, 1.2],
                            lanes: r.lanes,
                            oneway: true,
                            marks: marks::EDGE_LEFT,
                            hidden: false,
                        },
                    }
                })
                .collect();
            push_strip(mesh, &secs, base);
            island(mesh, r, base);
        }
        let mut keys: Vec<_> = cells.keys().copied().collect();
        keys.sort();
        keys.into_iter()
            .filter_map(|k| cells.remove(&k))
            .filter(|m| !m.is_empty())
            .collect()
    }

    /// Arbustes sur les îlots des ronds-points (tampon MultiMesh).
    pub fn island_bushes(&self, base: f64) -> Instances {
        let mut out = Instances::default();
        for (k, r) in self.roundabouts.iter().enumerate() {
            let ri = r.island();
            if ri < 2.0 {
                continue;
            }
            let count = ((ri * ri * 0.25) as usize).clamp(3, 24);
            for i in 0..count {
                let h = |c: u32| crate::raster::rand01(k as i64, i as i64, 900 + i64::from(c));
                let a = h(1) * TAU;
                let d = (ri - 1.2) * h(2).sqrt();
                let y = dome(r, d) + r.elevation - base;
                let size = 0.8 + h(3) * 1.0;
                out.push(
                    [
                        r.center.0 + d * a.cos(),
                        y - 0.1,
                        -(r.center.1 + d * a.sin()),
                    ],
                    h(4) * TAU,
                    size * 1.3,
                    size,
                    [1.0, 1.0, 0.9, 1.0],
                );
            }
        }
        out
    }
}

/// Hauteur de l'îlot au-dessus de la chaussée à `d` mètres du centre.
fn dome(r: &Roundabout, d: f64) -> f64 {
    let ri = r.island();
    let t = (d / ri).clamp(0.0, 1.0);
    0.05 + KERB + (ri * 0.06).min(0.9) * (1.0 - t * t)
}

/// Îlot central : pelouse bombée bordée de pavés.
fn island(mesh: &mut MeshData, r: &Roundabout, base: f64) {
    let ri = r.island();
    let steps = (TAU * ri / 1.5).ceil().max(16.0) as usize;
    let rings = [0.0, 0.35, 0.7, 1.0];
    let first = mesh.positions.len() as i32;
    for (ring_i, &f) in rings.iter().enumerate() {
        let count = if ring_i == 0 { 1 } else { steps };
        for i in 0..count {
            let a = i as f64 / steps as f64 * TAU;
            let d = ri * f;
            let (x, n) = (r.center.0 + d * a.cos(), r.center.1 + d * a.sin());
            mesh.positions.push([
                x as f32,
                (r.elevation - base + dome(r, d)) as f32,
                -n as f32,
            ]);
            mesh.normals.push([0.0, 1.0, 0.0]);
            mesh.uvs.push([x as f32, n as f32]);
            mesh.uv2s.push([0.0, 1.0]);
            mesh.colors
                .push([0.0, 0.0, 0.0, f32::from(marks::GRASS) / 255.0]);
        }
    }
    let up = [0.0, 1.0, 0.0];
    let s = steps as i32;
    for i in 0..s {
        mesh.push_tri(first, first + 1 + i, first + 1 + (i + 1) % s, up);
    }
    for ring_i in 1..rings.len() as i32 - 1 {
        let a = first + 1 + (ring_i - 1) * s;
        let b = a + s;
        for i in 0..s {
            let j = (i + 1) % s;
            mesh.push_tri(a + i, b + i, a + j, up);
            mesh.push_tri(a + j, b + i, b + j, up);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::LocalProjection;

    const R: f64 = 6_371_008.8;

    fn ll(x: f64, n: f64) -> (f64, f64) {
        (
            45.0 + (n / R).to_degrees(),
            6.0 + (x / (R * 45f64.to_radians().cos())).to_degrees(),
        )
    }

    fn geometry(pts: &[(f64, f64)]) -> String {
        let v: Vec<String> = pts
            .iter()
            .map(|&(x, n)| {
                let (lat, lon) = ll(x, n);
                format!(r#"{{"lat":{lat:.8},"lon":{lon:.8}}}"#)
            })
            .collect();
        format!("[{}]", v.join(","))
    }

    /// Route nord-sud de 400 m (GPX décalé de 4 m), une rue qui en part
    /// vers l'est à 150 m, et un rond-point de 15 m de rayon à 300 m.
    fn scene() -> (Route, OsmData) {
        let main: Vec<(f64, f64)> = (0..=57).map(|i| (0.0, i as f64 * 5.0)).collect();
        let side = [(0.0, 150.0), (80.0, 150.0)];
        let ring: Vec<(f64, f64)> = (0..=24)
            .map(|i| {
                let a = -std::f64::consts::FRAC_PI_2 + i as f64 / 24.0 * TAU;
                (15.0 * a.cos(), 300.0 + 15.0 * a.sin())
            })
            .collect();
        let exit: Vec<(f64, f64)> = (0..=20).map(|i| (0.0, 315.0 + i as f64 * 5.0)).collect();
        let json = format!(
            r#"{{"elements":[
            {{"type":"way","id":1,"tags":{{"highway":"secondary","sidewalk":"right"}},"geometry":{}}},
            {{"type":"way","id":2,"tags":{{"highway":"residential"}},"geometry":{}}},
            {{"type":"way","id":3,"tags":{{"highway":"secondary","junction":"roundabout"}},"geometry":{}}},
            {{"type":"way","id":4,"tags":{{"highway":"secondary"}},"geometry":{}}}
            ]}}"#,
            geometry(&main),
            geometry(&side),
            geometry(&ring),
            geometry(&exit)
        );
        let mut raw = Vec::new();
        for i in 0..=80 {
            let n = i as f64 * 5.0;
            // Le GPX passe à 4 m à l'ouest de la route, et coupe l'anneau
            // (parcouru par l'est, sens antihoraire).
            let x = if (285.0..=315.0).contains(&n) {
                (15.0f64 * 15.0 - (n - 300.0) * (n - 300.0)).max(0.0).sqrt() * 0.6
            } else {
                -4.0
            };
            let (lat, lon) = ll(x, n);
            raw.push((lat, lon, 500.0));
        }
        let route = Route::from_lat_lon_ele(None, &raw, 5.0, 40.0).unwrap();
        let proj = LocalProjection::for_route(&route);
        let osm = OsmData::from_overpass_json(&json, &proj).unwrap();
        (route, osm)
    }

    #[test]
    fn snaps_to_the_osm_road_and_reads_its_profile() {
        let (route, osm) = scene();
        let (path, _) = build(&route, &osm, &|_, _| 500.0);
        let p = path.samples.iter().find(|s| s.d > 80.0).unwrap();
        let road_x = osm.lines[0].points[3].0;
        assert!(
            (p.x - road_x).abs() < 0.5,
            "axe recalé : {} contre {}",
            p.x,
            road_x
        );
        // Secondaire : 2 × 3,25 m, axe discontinu, trottoir à droite.
        assert!((p.profile.half_width - 3.25).abs() < 0.05);
        assert_eq!(p.profile.lanes, 2);
        assert!(p.profile.marks & marks::CENTRE_DASH != 0);
        assert!(p.profile.sidewalk[1] > 1.5 && p.profile.sidewalk[0] == 0.0);
        assert!(p.profile.marks & marks::EDGE_RIGHT == 0);
    }

    #[test]
    fn side_street_gets_a_short_stub_and_a_gap() {
        let (route, osm) = scene();
        let (path, net) = build(&route, &osm, &|_, _| 500.0);
        let (sx, sn) = osm.lines[1].points[0];
        let east: Vec<_> = net
            .stubs
            .iter()
            .filter(|s| s.last().unwrap().x > sx + 10.0 && (s[0].n - sn).abs() < 3.0)
            .collect();
        assert_eq!(east.len(), 1, "{:?}", net.stubs.len());
        let stub = east[0];
        let len = stub.last().unwrap().s;
        assert!((len - (STUB_LEN)).abs() < 1.5, "{len}");
        assert!(stub[0].profile.marks & marks::GIVE_WAY != 0);
        assert!(stub[0].profile.half_width > stub.last().unwrap().profile.half_width + 2.0);
        let mouth = path
            .samples
            .iter()
            .min_by(|a, b| (a.n - sn).abs().total_cmp(&(b.n - sn).abs()))
            .unwrap();
        assert!(mouth.profile.marks & marks::GAP_RIGHT != 0);
        assert_eq!(
            mouth.profile.sidewalk[1], 0.0,
            "trottoir interrompu au débouché"
        );
        // Le sol est aplani sous l'amorce.
        let (w, _) = net.flat.weight(sx + 20.0, sn).unwrap();
        assert!(w > 0.99);
        assert!(net.flat.near_other(sx + 20.0, sn, 0.5));
    }

    #[test]
    fn finds_the_roundabout_and_hides_the_route_inside() {
        let (route, osm) = scene();
        let (path, net) = build(&route, &osm, &|_, _| 500.0);
        assert_eq!(net.roundabouts.len(), 1);
        let r = &net.roundabouts[0];
        assert!((r.radius - 15.0).abs() < 0.8, "{}", r.radius);
        assert!(r.island() > 3.0);
        assert!(path.samples.iter().any(|p| p.profile.hidden));
        assert!(path
            .samples
            .iter()
            .filter(|p| p.d < 250.0)
            .all(|p| !p.profile.hidden));
        // Le cycliste suit l'anneau, pas la corde du GPX.
        let pose = path.rider_at(path.samples.iter().find(|s| s.n > r.center.1).unwrap().d);
        let d = (pose.x - r.center.0).hypot(pose.n - r.center.1);
        assert!(d > 10.0 && d < 20.0, "{d}");
        assert!(!net.meshes(500.0).is_empty());
        assert!(net.island_bushes(500.0).count() >= 3);
    }

    #[test]
    fn runs_are_cleaned() {
        let mut v = vec![
            false, true, true, false, true, true, true, true, true, false, false, true,
        ];
        clean_runs(&mut v, 2, 4);
        assert_eq!(
            v,
            vec![false, true, true, true, true, true, true, true, true, false, false, false]
        );
    }
}
