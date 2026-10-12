//! La route d'après OpenStreetMap : tracé recalé sur la vraie route, largeur,
//! voies, marquages et trottoirs, amorces des routes qui débouchent sur le
//! parcours, ronds-points traversés (avec leur vraie forme), et murs de
//! soutènement ou parapets là où le relief est raide au bord de la route.

use crate::mesh::{Instances, MeshData};
use crate::osm::{join_rings, BBox, Line, LineKind, OsmData};
use crate::road::{marks, push_strip, PathSample, Profile, RoadPath, Section, KERB, VERGE};
use std::collections::{HashMap, HashSet};
use std::f64::consts::TAU;
use velo_core::route::Route;

/// Écart maximal entre le GPX et la route OSM sur laquelle on le recale (m).
const MATCH_RADIUS: f64 = 14.0;
/// Longueur des amorces des routes qui croisent le parcours, comptée depuis
/// le bord de la chaussée (m).
pub const STUB_LEN: f64 = 30.0;
/// Évasement des amorces au débouché (m de plus de chaque côté, à angle droit).
const FLARE: f64 = 2.5;
/// Accotement réduit entre trottoir et façades.
const TOWN_VERGE: f64 = 0.6;
/// Une route suivie par le parcours en fait partie tant qu'elle reste à
/// moins de cette marge du bord de la chaussée (m) : pas d'amorce parasite
/// quand son tracé coupe un lacet un peu autrement que le nôtre.
const FOLLOW_MARGIN: f64 = 4.0;
/// Dénivelé du relief sur 10 m au-delà de l'accotement qui appelle un mur
/// de soutènement (en montée) ou un parapet (au-dessus du vide).
const WALL_RISE: f64 = 6.0;
/// Au-delà (plus de 45°), la montagne est taillée dans le rocher : une
/// falaise plutôt qu'un mur.
const CLIFF_RISE: f64 = 10.0;
/// Replat caché derrière un mur de soutènement, sous sa terrasse (m) : plus
/// large qu'une maille du relief détaillé, pour que la pente ne passe jamais
/// devant le mur.
const WALL_FLAT: f64 = 7.0;
/// Profondeur de la terrasse au-dessus d'un mur de soutènement (m).
const WALL_DEPTH: f64 = WALL_FLAT + 4.0;
/// Fondu court du relief au-delà d'un mur ou d'un parapet (m) : talus raide,
/// rocheux, plutôt qu'une longue pente douce.
const WALL_BLEND: f64 = 8.0;
/// Replat au pied d'une falaise, au-delà de l'accotement (m) : la paroi
/// sculptée se dresse devant le talus du relief, qui ne la traverse pas.
const CLIFF_FLAT: f64 = 3.0;
/// Fruit de la paroi : recul par mètre de hauteur.
const CLIFF_BATTER: f64 = 0.14;
/// Écart vertical entre deux rangées de sommets de la paroi (m).
const CLIFF_ROW: f64 = 0.5;

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
    /// Hauteur visée du sol au point, et sa pente (m/m vers l'est, le nord)
    /// pour suivre une chaussée inclinée.
    target: f64,
    slope: (f64, f64),
    /// Vers la droite : rayon du replat et largeur du fondu vers le relief,
    /// à gauche et à droite.
    right: (f64, f64),
    flat: [f64; 2],
    blend: [f64; 2],
    route: bool,
}

impl FlatPoint {
    /// Replat circulaire et horizontal.
    fn round(p: (f64, f64), target: f64, flat: f64, blend: f64) -> Self {
        FlatPoint {
            x: p.0,
            n: p.1,
            target,
            slope: (0.0, 0.0),
            right: (1.0, 0.0),
            flat: [flat; 2],
            blend: [blend; 2],
            route: false,
        }
    }

    fn side(&self, x: f64, n: f64) -> usize {
        usize::from((x - self.x) * self.right.0 + (n - self.n) * self.right.1 >= 0.0)
    }
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
        for s in 0..2 {
            self.reach = self.reach.max(p.flat[s] + p.blend[s]);
        }
        self.grid.insert(p.x, p.n, self.pts.len() as u32);
        self.pts.push(p);
    }

    /// Poids (1 : sous une chaussée) et hauteur visée du sol en (x, n).
    pub fn weight(&self, x: f64, n: f64) -> Option<(f64, f64)> {
        let mut best: Option<(f64, f64, f64)> = None;
        self.grid.near(x, n, self.reach, |i| {
            let p = &self.pts[i as usize];
            let d = (p.x - x).hypot(p.n - n);
            let s = p.side(x, n);
            if d >= p.flat[s] + p.blend[s] {
                return;
            }
            let w = 1.0 - smoothstep(p.flat[s], p.flat[s] + p.blend[s], d);
            let better = match best {
                None => true,
                Some((bw, bd, _)) => w > bw + 1e-9 || (w > bw - 1e-9 && d < bd),
            };
            if better {
                let t = p.target + p.slope.0 * (x - p.x) + p.slope.1 * (n - p.n);
                best = Some((w, d, t));
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
            if !p.route && (p.x - x).hypot(p.n - n) < p.flat[p.side(x, n)] + margin {
                hit = true;
            }
        });
        hit
    }
}

/// Rond-point traversé ou longé par le parcours, avec la forme de son
/// anneau dans OSM (souvent ovale) et sa pente.
#[derive(Debug, Clone, PartialEq)]
pub struct Roundabout {
    /// Centre de gravité de l'anneau.
    pub center: (f64, f64),
    /// Axe de l'anneau : polyligne fermée (sans répéter le premier point),
    /// dans le sens antihoraire, un point par mètre environ.
    pub ring: Vec<(f64, f64)>,
    /// Distance moyenne et maximale du centre à l'axe (m).
    pub radius: f64,
    pub rmax: f64,
    pub half_width: f64,
    /// Bande pavée franchissable autour de l'îlot.
    pub apron: f64,
    /// Altitude de la chaussée au centre, et sa pente (m/m vers l'est, le nord).
    pub elevation: f64,
    pub slope: (f64, f64),
    pub lanes: u8,
    /// Débouchés (point du bord extérieur, demi-largeur) : pas d'accotement.
    mouths: Vec<((f64, f64), f64)>,
}

impl Roundabout {
    /// Rayon moyen de l'îlot central.
    pub fn island(&self) -> f64 {
        (self.radius - self.half_width - self.apron).max(0.5)
    }

    /// Altitude de la chaussée en (x, n).
    pub fn height(&self, x: f64, n: f64) -> f64 {
        self.elevation + self.slope.0 * (x - self.center.0) + self.slope.1 * (n - self.center.1)
    }

    /// Vecteur unitaire vers l'extérieur de l'anneau au point `i`.
    fn outward(&self, i: usize) -> (f64, f64) {
        let m = self.ring.len();
        let (a, b) = (self.ring[(i + m - 1) % m], self.ring[(i + 1) % m]);
        let t = unit((b.0 - a.0, b.1 - a.1));
        (t.1, -t.0)
    }

    /// Point à `off` mètres de l'axe vers l'extérieur (négatif : vers l'îlot).
    fn edge(&self, i: usize, off: f64) -> (f64, f64) {
        let (q, o) = (self.ring[i], self.outward(i));
        (q.0 + o.0 * off, q.1 + o.1 * off)
    }

    fn inside(&self, p: (f64, f64)) -> bool {
        let m = self.ring.len();
        let mut inside = false;
        for i in 0..m {
            let (a, b) = (self.ring[i], self.ring[(i + 1) % m]);
            if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
                inside = !inside;
            }
        }
        inside
    }

    /// Distance à l'axe de l'anneau, négative à l'intérieur (minorant
    /// grossier loin du rond-point).
    pub fn signed(&self, p: (f64, f64)) -> f64 {
        let dc = dist(p, self.center);
        if dc > self.rmax + 60.0 {
            return dc - self.rmax;
        }
        let m = self.ring.len();
        let d = (0..m)
            .map(|i| project(p, self.ring[i], self.ring[(i + 1) % m]).0)
            .fold(f64::MAX, f64::min);
        if self.inside(p) {
            -d
        } else {
            d
        }
    }

    /// Tangente (sens antihoraire) au point de l'axe le plus proche de `p`.
    fn tangent_near(&self, p: (f64, f64)) -> (f64, f64) {
        let i = (0..self.ring.len())
            .min_by(|&a, &b| dist(p, self.ring[a]).total_cmp(&dist(p, self.ring[b])))
            .unwrap_or(0);
        let o = self.outward(i);
        (-o.1, o.0)
    }

    /// Plan de la chaussée ajusté (moindres carrés, pente modérée) sur les
    /// altitudes du parcours qui traverse le rond-point et l'approche.
    fn fit_plane(&mut self, samples: &[PathSample]) {
        let c = self.center;
        let reach = self.rmax + self.half_width + 12.0;
        let mut a = [[0.0; 3]; 3];
        let mut b = [0.0; 3];
        let mut nearest: Option<(f64, f64)> = None;
        for p in samples {
            let (dx, dn) = (p.x - c.0, p.n - c.1);
            let d = dx.hypot(dn);
            if nearest.map_or(true, |(bd, _)| d < bd) {
                nearest = Some((d, p.elevation));
            }
            if d > reach {
                continue;
            }
            let v = [1.0, dx, dn];
            for (row, &vi) in a.iter_mut().zip(&v) {
                for (cell, &vj) in row.iter_mut().zip(&v) {
                    *cell += vi * vj;
                }
            }
            for (bi, &vi) in b.iter_mut().zip(&v) {
                *bi += vi * p.elevation;
            }
        }
        self.elevation = nearest.map_or(0.0, |(_, e)| e);
        self.slope = (0.0, 0.0);
        if a[0][0] < 3.0 {
            return;
        }
        // Rappel vers l'horizontale : le parcours ne dit rien de la pente
        // en travers de sa direction.
        let lambda = a[0][0] * 10.0;
        a[1][1] += lambda;
        a[2][2] += lambda;
        if let Some(x) = solve3(a, b) {
            self.elevation = x[0];
            let g = x[1].hypot(x[2]);
            let k = if g > 0.1 { 0.1 / g } else { 1.0 };
            self.slope = (x[1] * k, x[2] * k);
        }
    }
}

/// Mur le long du parcours, côté extérieur de l'accotement.
#[derive(Debug, Clone, PartialEq)]
pub struct Wall {
    pub pts: Vec<WallPoint>,
    /// Profondeur du dessus : terrasse d'un mur de soutènement, épaisseur
    /// d'un parapet (m).
    pub depth: f64,
    /// Mur de soutènement (côté montagne) plutôt que parapet (côté vide).
    pub retaining: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallPoint {
    /// Pied de la face avant.
    pub x: f64,
    pub n: f64,
    /// Vecteur unitaire de la route vers le mur.
    pub out: (f64, f64),
    /// Altitudes absolues du pied (enterré) et du haut.
    pub base: f64,
    pub top: f64,
    /// Abscisse le long du mur (m).
    pub s: f64,
}

/// Falaise sculptée le long du parcours, côté montagne : une paroi
/// rocheuse en relief (strates, surplombs, ravines) devant le talus du sol,
/// coiffée d'un rebord qui s'enfonce dans le relief.
#[derive(Debug, Clone, PartialEq)]
pub struct Cliff {
    /// Colonnes de sommets du pied vers le haut puis le long du rebord
    /// (x, altitude absolue, n), toutes du même nombre de points.
    pub columns: Vec<Vec<[f64; 3]>>,
    /// Abscisse le long du parcours de chaque colonne (m).
    pub s: Vec<f64>,
    /// Hauteur au-dessus du pied de chaque point d'une colonne (m).
    pub heights: Vec<Vec<f64>>,
    /// Nombre de rangées de la paroi ; les suivantes forment le rebord.
    pub face_rows: usize,
    /// Vecteur unitaire de la route vers la paroi, par colonne.
    pub out: Vec<(f64, f64)>,
}

/// Route OSM dessinée drapée sur le sol, hors du parcours, des amorces et
/// des ronds-points.
#[derive(Debug, Clone)]
pub struct Ribbon {
    /// Indice de la ligne dans `OsmData::lines`.
    pub line: usize,
    pub points: Vec<(f64, f64)>,
    pub bbox: BBox,
}

impl Ribbon {
    fn new(line: usize, points: Vec<(f64, f64)>) -> Self {
        Ribbon {
            line,
            bbox: BBox::of(points.iter().copied()),
            points,
        }
    }
}

/// Tout ce qui entoure le parcours d'après OSM.
#[derive(Debug, Clone, Default)]
pub struct RoadNetwork {
    /// Amorces : rubans de chaussée, altitude absolue.
    pub stubs: Vec<Vec<Section>>,
    pub roundabouts: Vec<Roundabout>,
    /// Le reste des routes OSM, à draper sur le sol.
    pub ribbons: Vec<Ribbon>,
    pub walls: Vec<Wall>,
    pub cliffs: Vec<Cliff>,
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
    /// Depuis le nœud de raccord (sous la chaussée rejointe).
    pts: Vec<(f64, f64)>,
    start: Hit,
    end: Option<Hit>,
    /// Longueur passée sous la chaussée rejointe, au début et à la fin.
    inner: (f64, f64),
    /// Sinus et cosinus de l'angle avec la chaussée rejointe, au début et à la fin.
    sin: (f64, f64),
    cos: (f64, f64),
    half_width: f64,
    lanes: u8,
    oneway: bool,
    give_way: bool,
    centre: bool,
    /// Hauteur du ruban drapé qui prolonge l'amorce au-dessus du sol.
    lift: f64,
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

/// Hauteur au-dessus du sol du ruban drapé d'une route OSM.
pub fn ribbon_lift(line: &Line) -> f64 {
    match line.kind {
        LineKind::Road { major: true, .. } => 0.14,
        _ => 0.11,
    }
}

/// La route OSM a-t-elle un axe peint (double sens, au moins tertiaire) ?
pub fn has_centre_line(line: &Line) -> bool {
    let (_, lanes) = carriageway(line);
    matches!(line.kind, LineKind::Road { paved: true, .. })
        && !line.info.oneway
        && lanes >= 2
        && line.info.rank >= 3
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

fn det3(m: [[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// Résout `a · x = b` (règle de Cramer).
fn solve3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let d = det3(a);
    if d.abs() < 1e-9 {
        return None;
    }
    let mut x = [0.0; 3];
    for (k, xk) in x.iter_mut().enumerate() {
        let mut m = a;
        for (row, &bi) in m.iter_mut().zip(&b) {
            row[k] = bi;
        }
        *xk = det3(m) / d;
    }
    Some(x)
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

fn polyline_len(pts: &[(f64, f64)]) -> f64 {
    pts.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// Morceau d'une polyligne entre les abscisses `l0` et `l1` (m).
fn sub_polyline(pts: &[(f64, f64)], l0: f64, l1: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut s = 0.0;
    for w in pts.windows(2) {
        let d = dist(w[0], w[1]);
        let at = |l: f64| {
            let t = ((l - s) / d.max(1e-9)).clamp(0.0, 1.0);
            (
                w[0].0 + (w[1].0 - w[0].0) * t,
                w[0].1 + (w[1].1 - w[0].1) * t,
            )
        };
        if out.is_empty() && l0 < s + d {
            out.push(at(l0));
        }
        if !out.is_empty() {
            if l1 <= s + d {
                out.push(at(l1));
                return out;
            }
            out.push(w[1]);
        }
        s += d;
    }
    out
}

fn point_at(pts: &[(f64, f64)], l: f64) -> (f64, f64) {
    let mut s = 0.0;
    for w in pts.windows(2) {
        let d = dist(w[0], w[1]);
        if l <= s + d {
            let t = ((l - s) / d.max(1e-9)).clamp(0.0, 1.0);
            return (
                w[0].0 + (w[1].0 - w[0].0) * t,
                w[0].1 + (w[1].1 - w[0].1) * t,
            );
        }
        s += d;
    }
    *pts.last().unwrap()
}

/// Abscisse du point de la polyligne le plus proche de `q`.
fn arc_of(pts: &[(f64, f64)], q: (f64, f64)) -> f64 {
    let mut best = (f64::MAX, 0.0);
    let mut s = 0.0;
    for w in pts.windows(2) {
        let (d, p) = project(q, w[0], w[1]);
        if d < best.0 {
            best = (d, s + dist(w[0], p));
        }
        s += dist(w[0], w[1]);
    }
    best.1
}

/// Aire signée (positive dans le sens antihoraire) et centre de gravité
/// d'un polygone.
fn area_centroid(ring: &[(f64, f64)]) -> (f64, (f64, f64)) {
    let m = ring.len();
    let (mut a, mut cx, mut cn) = (0.0, 0.0, 0.0);
    for i in 0..m {
        let (p, q) = (ring[i], ring[(i + 1) % m]);
        let c = p.0 * q.1 - q.0 * p.1;
        a += c;
        cx += (p.0 + q.0) * c;
        cn += (p.1 + q.1) * c;
    }
    a /= 2.0;
    if a.abs() < 1e-9 {
        let k = m as f64;
        return (
            0.0,
            (
                ring.iter().map(|p| p.0).sum::<f64>() / k,
                ring.iter().map(|p| p.1).sum::<f64>() / k,
            ),
        );
    }
    (a, (cx / (6.0 * a), cn / (6.0 * a)))
}

/// Anneau fermé rééchantillonné tous les mètres et lissé (angles des nœuds OSM).
fn smooth_ring(ring: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut closed = ring.to_vec();
    closed.push(ring[0]);
    let mut pts: Vec<(f64, f64)> = resample(&closed).into_iter().map(|p| (p.0, p.1)).collect();
    if pts.len() > 2 && dist(pts[0], *pts.last().unwrap()) < 0.5 {
        pts.pop();
    }
    let m = pts.len() as isize;
    let sigma: f64 = 2.0;
    let r = 6isize;
    (0..m)
        .map(|i| {
            let (mut sx, mut sn, mut sw) = (0.0, 0.0, 0.0);
            for k in -r..=r {
                let w = (-((k * k) as f64) / (2.0 * sigma * sigma)).exp();
                let p = pts[(i + k).rem_euclid(m) as usize];
                sx += p.0 * w;
                sn += p.1 * w;
                sw += w;
            }
            (sx / sw, sn / sw)
        })
        .collect()
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
    let route_lines: HashSet<usize> = matched.iter().flatten().map(|m| m.0).collect();

    // 2. Ronds-points : anneaux étiquetés `junction=roundabout`, ou petites
    //    boucles fermées presque circulaires. On garde leur vraie forme.
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
        if pts.len() < 5 || dist(pts[0], *pts.last().unwrap()) > 0.05 {
            continue;
        }
        let mut ring = pts[..pts.len() - 1].to_vec();
        let (area, c) = area_centroid(&ring);
        if area < 0.0 {
            ring.reverse();
        }
        let k = ring.len() as f64;
        let radii: Vec<f64> = ring.iter().map(|&p| dist(p, c)).collect();
        let r = radii.iter().sum::<f64>() / k;
        let rmax = radii.iter().copied().fold(0.0, f64::max);
        let sd = (radii.iter().map(|x| (x - r) * (x - r)).sum::<f64>() / k).sqrt();
        let round_enough = if tagged {
            sd / r < 0.35 && (4.0..80.0).contains(&r)
        } else {
            sd / r < 0.2 && (5.0..45.0).contains(&r)
        };
        if !round_enough || samples.nearest(c.0, c.1, rmax + 30.0).is_none() {
            continue;
        }
        // Les lignes de l'anneau : celles dont tous les points le longent.
        let on_ring: Vec<usize> = lines
            .into_iter()
            .filter(|&li| {
                osm.lines[li]
                    .points
                    .iter()
                    .all(|&p| (dist(p, c) - r).abs() < (rmax - r) + r * 0.5 + 2.0)
            })
            .collect();
        let first = on_ring.first().map(|&li| &osm.lines[li]);
        let lanes = first.and_then(|l| l.info.lanes).unwrap_or(1);
        let mut hw = match first {
            Some(l) if l.info.width_tagged => l.width_m / 2.0,
            _ => (f64::from(lanes) * 3.5).max(7.0) / 2.0,
        };
        let rmin = radii.iter().copied().fold(f64::MAX, f64::min);
        hw = hw.min(r * 0.55).min(rmin * 0.7);
        ring_lines.extend(on_ring);
        let mut rb = Roundabout {
            center: c,
            ring: smooth_ring(&ring),
            radius: r,
            rmax,
            half_width: hw,
            apron: (rmin - hw - 2.0).clamp(0.0, 1.5),
            elevation: 0.0,
            slope: (0.0, 0.0),
            lanes,
            mouths: Vec::new(),
        };
        rb.fit_plane(&path.samples);
        rings.push(rb);
    }

    // 3. Profil de la chaussée sous chaussée échantillon ; dans un rond-point,
    //    l'anneau prend le relais et le parcours rejoint son plan.
    for (p, w) in path.samples.iter_mut().zip(&way) {
        if let Some((li, _)) = *w {
            p.profile = profile_for(&osm.lines[li]);
        }
        for r in &rings {
            let sd = r.signed((p.x, p.n));
            if sd < r.half_width - 0.7 {
                p.profile = Profile {
                    half_width: r.half_width,
                    sidewalk: [0.0; 2],
                    verge: [0.0, 1.2],
                    lanes: r.lanes,
                    oneway: true,
                    marks: marks::EDGE_LEFT,
                    hidden: true,
                };
                p.elevation = r.height(p.x, p.n);
            } else if sd - r.half_width < 25.0 {
                // Raccord en douceur, un rien sous l'anneau là où les
                // chaussées se chevauchent.
                let out = sd - r.half_width;
                let w = 1.0 - smoothstep(0.0, 25.0, out);
                let dip = 0.04 * (1.0 - smoothstep(2.0, 8.0, out));
                p.elevation += (r.height(p.x, p.n) - dip - p.elevation) * w;
            }
        }
    }
    for k in 1..ms {
        let (a, b) = (&path.samples[k - 1], &path.samples[k]);
        if a.profile.hidden == b.profile.hidden {
            continue;
        }
        let (inner, outer) = if a.profile.hidden { (a, b) } else { (b, a) };
        if let Some(r) = rings.iter_mut().min_by(|x, y| {
            x.signed((inner.x, inner.n))
                .total_cmp(&y.signed((inner.x, inner.n)))
        }) {
            r.mouths
                .push(((outer.x, outer.n), outer.profile.half_width));
        }
    }

    // 4. Amorces des routes qui débouchent sur le parcours ou un rond-point.
    //    Une route n'en a une que si elle partage un nœud OSM avec la route
    //    suivie ou l'anneau : celles qui passent à côté sans s'y raccorder
    //    (parallèles, ponts) sont dessinées telles quelles.
    let node_key = |p: (f64, f64)| ((p.0 * 4.0).round() as i64, (p.1 * 4.0).round() as i64);
    let mut nodes: HashSet<(i64, i64)> = HashSet::new();
    for &li in route_lines.iter().chain(&ring_lines) {
        for &p in &osm.lines[li].points {
            nodes.insert(node_key(p));
        }
    }
    let is_node = |p: (f64, f64)| {
        let (i, j) = node_key(p);
        (-1..=1).any(|di| (-1..=1).any(|dj| nodes.contains(&(i + di, j + dj))))
    };
    // De combien `q` dépasse le début ou la fin du parcours (m, négatif sinon).
    let beyond_ends = |j: usize, q: (f64, f64)| -> f64 {
        let p = &path.samples[j];
        let along = (q.0 - p.x) * p.tangent.0 + (q.1 - p.n) * p.tangent.1;
        if j == 0 {
            -along
        } else if j + 1 == ms {
            along
        } else {
            f64::MIN
        }
    };
    let hit_at = |q: (f64, f64), follows: bool| -> Option<Hit> {
        for (k, r) in rings.iter().enumerate() {
            if r.signed(q) < r.half_width + 0.3 {
                return Some(Hit::Ring(k));
            }
        }
        let margin = if follows { FOLLOW_MARGIN } else { 0.6 };
        let (d, j) = samples.nearest(q.0, q.1, 12.0 + margin)?;
        let pr = &path.samples[j].profile;
        (d < pr.half_width + margin && !pr.hidden && beyond_ends(j, q) < 0.5)
            .then_some(Hit::Route(j))
    };
    // Écart (signé) au bord de la chaussée qu'on rejoint, un peu en dedans.
    let edge_gap = |hit: Hit, q: (f64, f64)| -> f64 {
        match hit {
            Hit::Ring(k) => rings[k].signed(q) - (rings[k].half_width - 0.3),
            Hit::Route(j) => {
                let j = samples.nearest(q.0, q.1, 40.0).map_or(j, |(_, k)| k);
                let p = &path.samples[j];
                let (rx, rn) = p.right();
                let lateral =
                    ((q.0 - p.x) * rx + (q.1 - p.n) * rn).abs() - (p.profile.half_width - 0.3);
                lateral.max(beyond_ends(j, q))
            }
        }
    };
    // Longueur parcourue sous la chaussée rejointe avant d'en sortir.
    let exit_len = |pts: &[(f64, f64)], hit: Hit| -> f64 {
        let mut s = 0.0;
        if edge_gap(hit, pts[0]) >= 0.0 {
            return 0.0;
        }
        for w in pts.windows(2) {
            let d = dist(w[0], w[1]);
            if edge_gap(hit, w[1]) >= 0.0 {
                let (mut lo, mut hi) = (0.0, 1.0);
                for _ in 0..12 {
                    let t: f64 = (lo + hi) / 2.0;
                    let q = (
                        w[0].0 + (w[1].0 - w[0].0) * t,
                        w[0].1 + (w[1].1 - w[0].1) * t,
                    );
                    if edge_gap(hit, q) < 0.0 {
                        lo = t;
                    } else {
                        hi = t;
                    }
                }
                return s + d * lo;
            }
            s += d;
        }
        s
    };
    // Sens de la chaussée rejointe près de `q`.
    let main_tangent = |hit: Hit, q: (f64, f64)| -> (f64, f64) {
        match hit {
            Hit::Ring(k) => rings[k].tangent_near(q),
            Hit::Route(j) => {
                let j = samples.nearest(q.0, q.1, 40.0).map_or(j, |(_, k)| k);
                path.samples[j].tangent
            }
        }
    };
    let main_rank = |h: Hit| match h {
        Hit::Route(j) => way[j].map_or(3, |(l, _)| osm.lines[l].info.rank),
        Hit::Ring(_) => 6,
    };
    let mut drafts: Vec<StubDraft> = Vec::new();
    let mut ribbons: Vec<Ribbon> = Vec::new();
    for &li in &roads {
        if ring_lines.contains(&li) {
            continue;
        }
        let line = &osm.lines[li];
        let follows = route_lines.contains(&li);
        let dens = densify(&line.points, 2.0);
        let hits: Vec<Option<Hit>> = dens.iter().map(|&q| hit_at(q, follows)).collect();
        let node: Vec<bool> = dens.iter().map(|&q| is_node(q)).collect();
        if hits.iter().all(Option::is_none) || (!follows && !node.iter().any(|&b| b)) {
            ribbons.push(Ribbon::new(li, line.points.clone()));
            continue;
        }
        let (width, lanes) = carriageway(line);
        let paved = matches!(line.kind, LineKind::Road { paved: true, .. });
        let give_way = |h: Hit| line.info.rank >= 1 && line.info.rank <= main_rank(h);
        // Ajoute l'amorce `pts` (du nœud de départ au nœud d'arrivée s'il y
        // en a un) ; renvoie son dernier point.
        // Un nœud un peu hors de la chaussée : on prolonge l'amorce dessous.
        let tuck = |pts: &mut Vec<(f64, f64)>, h: Hit| {
            let gap = edge_gap(h, pts[0]);
            if gap < 0.0 || pts.len() < 2 {
                return;
            }
            let d = unit((pts[1].0 - pts[0].0, pts[1].1 - pts[0].1));
            let t = main_tangent(h, pts[0]);
            let sin = (d.0 * t.1 - d.1 * t.0).abs().max(0.25);
            let back = ((gap + 1.0) / sin).min(10.0);
            pts.insert(0, (pts[0].0 - d.0 * back, pts[0].1 - d.1 * back));
        };
        let mut add = |mut pts: Vec<(f64, f64)>, s: Hit, e: Option<Hit>| -> Option<(f64, f64)> {
            tuck(&mut pts, s);
            if let Some(h) = e {
                pts.reverse();
                tuck(&mut pts, h);
                pts.reverse();
            }
            let inner_s = exit_len(&pts, s);
            let rev: Vec<(f64, f64)> = pts.iter().rev().copied().collect();
            let inner_e = e.map_or(0.0, |h| exit_len(&rev, h));
            let pts = if e.is_none() {
                sub_polyline(&pts, 0.0, inner_s + STUB_LEN)
            } else {
                pts
            };
            let len = polyline_len(&pts);
            if pts.len() < 2 || len - inner_s - inner_e < 3.0 {
                return None;
            }
            let angle = |pts: &[(f64, f64)], inner: f64, h: Hit| {
                let a = point_at(pts, inner);
                let d = unit({
                    let b = point_at(pts, inner + 6.0);
                    (b.0 - a.0, b.1 - a.1)
                });
                let t = main_tangent(h, a);
                (
                    (d.0 * t.1 - d.1 * t.0).abs().max(0.25),
                    (d.0 * t.0 + d.1 * t.1).abs(),
                )
            };
            let (sin_s, cos_s) = angle(&pts, inner_s, s);
            let rev: Vec<(f64, f64)> = pts.iter().rev().copied().collect();
            let (sin_e, cos_e) = e.map_or((1.0, 0.0), |h| angle(&rev, inner_e, h));
            let last = *pts.last().unwrap();
            drafts.push(StubDraft {
                pts,
                start: s,
                end: e,
                inner: (inner_s, inner_e),
                sin: (sin_s, sin_e),
                cos: (cos_s, cos_e),
                half_width: (width / 2.0).min(6.0),
                lanes: if paved { lanes } else { 0 },
                oneway: line.info.oneway,
                give_way: give_way(s) || e.is_some_and(give_way),
                centre: has_centre_line(line),
                lift: ribbon_lift(line),
            });
            Some(last)
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
            // Nœud de raccord : sous la chaussée rejointe (au plus 30 m avant
            // d'en sortir), ou juste au bord.
            let start_node = start.and_then(|h| {
                let back = || {
                    (a.saturating_sub(16)..a)
                        .rev()
                        .take_while(|&k| hits[k].is_some())
                };
                back()
                    .find(|&k| node[k] && edge_gap(h, dens[k]) < 0.0)
                    .or_else(|| back().find(|&k| node[k]))
                    .or_else(|| (a..(a + 2).min(b + 1)).find(|&k| node[k]))
                    .map(|k| (h, k))
            });
            let end_node = end.and_then(|h| {
                let ahead = || (i..(i + 16).min(dens.len())).take_while(|&k| hits[k].is_some());
                ahead()
                    .find(|&k| node[k] && edge_gap(h, dens[k]) < 0.0)
                    .or_else(|| ahead().find(|&k| node[k]))
                    .or_else(|| (b.saturating_sub(1).max(a)..=b).rev().find(|&k| node[k]))
                    .map(|k| (h, k))
            });
            let run_len = polyline_len(&dens[a..=b]);
            let full = &dens[a.saturating_sub(1)..(b + 2).min(dens.len())];
            let (mut from, mut to) = (None, None);
            match (start_node, end_node) {
                (Some((s, k0)), Some((e, k1))) if run_len <= 2.0 * STUB_LEN + 10.0 => {
                    if add(dens[k0..=k1].to_vec(), s, Some(e)).is_some() {
                        continue;
                    }
                }
                (sn, en) => {
                    if let Some((s, k0)) = sn {
                        from = add(dens[k0..=b].to_vec(), s, None);
                    }
                    if let Some((e, k1)) = en {
                        let rev: Vec<(f64, f64)> = dens[a..=k1].iter().rev().copied().collect();
                        to = add(rev, e, None);
                    }
                }
            }
            // Le reste de la route, drapé sur le sol, à partir du bout des amorces.
            let l0 = from.map_or(0.0, |q| arc_of(full, q));
            let l1 = to.map_or(polyline_len(full), |q| arc_of(full, q));
            if l1 - l0 > 1.0 {
                ribbons.push(Ribbon::new(li, sub_polyline(full, l0, l1)));
            }
        }
    }

    // Débouchés : rive discontinue, trottoir interrompu et accotement réduit
    // sur le parcours ; pas d'accotement sur l'anneau d'un rond-point.
    let mut cut_sidewalk = [vec![false; ms], vec![false; ms]];
    for d in &drafts {
        let rev: Vec<(f64, f64)> = d.pts.iter().rev().copied().collect();
        for (hit, pts, inner, sin) in [
            (Some(d.start), &d.pts, d.inner.0, d.sin.0),
            (d.end, &rev, d.inner.1, d.sin.1),
        ] {
            let Some(hit) = hit else { continue };
            let mouth = point_at(pts, inner);
            let hw = d.half_width + FLARE * sin;
            let j = match hit {
                Hit::Ring(k) => {
                    rings[k].mouths.push((mouth, hw));
                    continue;
                }
                Hit::Route(j) => samples
                    .nearest(mouth.0, mouth.1, 40.0)
                    .map_or(j, |(_, k)| k),
            };
            let p = path.samples[j];
            let b = point_at(pts, inner + 6.0);
            let dir = unit((b.0 - mouth.0, b.1 - mouth.1));
            let (rx, rn) = p.right();
            let side = usize::from(dir.0 * rx + dir.1 * rn > 0.0);
            let reach = (hw / sin).min(18.0) + 1.0;
            let gap = [marks::GAP_LEFT, marks::GAP_RIGHT][side];
            let (lo, hi) = (
                j.saturating_sub(reach as usize + 1),
                (j + reach as usize + 2).min(ms),
            );
            for (q, cut) in path.samples[lo..hi]
                .iter_mut()
                .zip(&mut cut_sidewalk[side][lo..hi])
            {
                if (q.s - p.s).abs() < reach && !q.profile.hidden {
                    q.profile.marks |= gap;
                    q.profile.verge[side] = q.profile.verge[side].min(0.4);
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
            let w = if pr < 0.05 || p.profile.hidden {
                0.0
            } else {
                full * pr
            };
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

    // 7. Talus raides au bord du parcours : mur de soutènement côté
    //    montagne, parapet côté vide.
    let mut stub_grid = Grid::new(16.0);
    let mut stub_pts: Vec<((f64, f64), f64)> = Vec::new();
    for d in &drafts {
        for &q in d.pts.iter().step_by(2) {
            stub_grid.insert(q.0, q.1, stub_pts.len() as u32);
            stub_pts.push((q, d.half_width + FLARE));
        }
    }
    let sides = wall_sides(&path, &samples, &rings, dem, &|q: (f64, f64), r: f64| {
        let mut hit = false;
        stub_grid.near(q.0, q.1, r + 8.0, |id| {
            let (p, hw) = stub_pts[id as usize];
            if dist(p, q) < hw + r {
                hit = true;
            }
        });
        hit
    });

    // 8. Relief aplani sous le parcours et les ronds-points, puis sous les amorces.
    let mut flat = Flatten::new();
    for (k, p) in path.samples.iter().enumerate() {
        if k % 2 != 0 || p.profile.hidden {
            continue;
        }
        let pr = &p.profile;
        let mut fp = FlatPoint::round(
            (p.x, p.n),
            p.elevation - 0.35,
            pr.reach(0).max(pr.reach(1)) + 0.7,
            23.0,
        );
        fp.route = true;
        fp.right = p.right();
        for (side, kinds) in sides.iter().enumerate() {
            match kinds[k] {
                Steep::Up => {
                    fp.flat[side] = pr.reach(side) + 0.2 + WALL_FLAT;
                    fp.blend[side] = WALL_BLEND;
                }
                Steep::Down => {
                    fp.flat[side] = pr.reach(side) + 0.9;
                    fp.blend[side] = WALL_BLEND;
                }
                Steep::Cliff => {
                    fp.flat[side] = pr.reach(side) + CLIFF_FLAT;
                    fp.blend[side] = WALL_BLEND;
                }
                Steep::No => {}
            }
        }
        flat.add(fp);
    }
    for r in &rings {
        for &q in r.ring.iter().step_by(2) {
            let mut fp = FlatPoint::round(q, r.height(q.0, q.1) - 0.35, r.half_width + 1.6, 15.0);
            fp.slope = r.slope;
            flat.add(fp);
        }
        let mut fp = FlatPoint::round(r.center, r.elevation - 0.35, r.rmax, 15.0);
        fp.slope = r.slope;
        flat.add(fp);
    }
    // Altitude et pente de la chaussée rejointe près de `q`.
    let grade = |j: usize| {
        let (a, b) = (j.saturating_sub(3), (j + 3).min(ms - 1));
        let ds = path.samples[b].s - path.samples[a].s;
        if ds > 0.5 {
            (path.samples[b].elevation - path.samples[a].elevation) / ds
        } else {
            0.0
        }
    };
    let level = |h: Hit, q: (f64, f64)| -> (f64, (f64, f64)) {
        match h {
            Hit::Route(j) => {
                let j = samples.nearest(q.0, q.1, 40.0).map_or(j, |(_, k)| k);
                let p = &path.samples[j];
                let g = grade(j);
                let along = (q.0 - p.x) * p.tangent.0 + (q.1 - p.n) * p.tangent.1;
                (p.elevation + g * along, (g * p.tangent.0, g * p.tangent.1))
            }
            Hit::Ring(k) => (rings[k].height(q.0, q.1), rings[k].slope),
        }
    };
    let mut stubs = Vec::with_capacity(drafts.len());
    let mut stub_flat = Vec::new();
    for (si, d) in drafts.iter().enumerate() {
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
        // Le marquage « cédez le passage » là où toute la largeur est sortie.
        let clear = |sin: f64, cos: f64| (d.half_width * cos / sin).min(8.0);
        let mut secs = Vec::with_capacity(pts.len());
        for (i, &(x, n, t)) in pts.iter().enumerate() {
            let a = pts[i.saturating_sub(1)];
            let b = pts[(i + 1).min(pts.len() - 1)];
            let tan = unit((b.0 - a.0, b.1 - a.1));
            let right = (tan.1, -tan.0);
            let ground = flat.apply(dem(x, n), x, n);
            let mut y = ground + d.lift - 0.05;
            let mut bank = 0.0;
            let mut s = t - d.inner.0 - clear(d.sin.0, d.cos.0);
            let mut near = 0.0f64;
            // Raccord à la chaussée rejointe : même altitude et même pente,
            // un rien plus bas pour passer dessous.
            let mut join = |h: Hit, from: f64, sin: f64| -> f64 {
                let (yh, g) = level(h, (x, n));
                let w = 1.0 - smoothstep(2.0, 22.0, from);
                // Chaque amorce à sa hauteur : pas de scintillement là où deux
                // se chevauchent (carrefour en bout de parcours).
                let dip = if from < 0.0 { 0.06 } else { 0.04 } + 0.006 * (si % 4) as f64;
                y += (yh - dip - y) * w;
                bank += (g.0 * right.0 + g.1 * right.1) * w;
                near = near.max(w);
                if from < 0.0 {
                    FLARE * sin
                } else {
                    FLARE * sin * (1.0 - smoothstep(0.0, 7.0, from)).powi(2)
                }
            };
            let mut flare = join(d.start, t - d.inner.0, d.sin.0);
            if let Some(e) = d.end {
                let from = len - t - d.inner.1;
                flare += join(e, from, d.sin.1);
                s = s.min(from - clear(d.sin.1, d.cos.1));
            }
            let mut m = 0;
            if d.give_way {
                m |= marks::GIVE_WAY;
            }
            if d.centre {
                m |= marks::CENTRE_DASH;
            }
            secs.push(Section {
                x,
                n,
                elevation: y,
                right,
                s,
                bank,
                profile: Profile {
                    half_width: d.half_width + flare,
                    sidewalk: [0.0; 2],
                    verge: [0.5; 2],
                    lanes: d.lanes,
                    oneway: d.oneway,
                    marks: m,
                    hidden: false,
                },
            });
            let under = t < d.inner.0 || (d.end.is_some() && len - t < d.inner.1);
            if i % 2 == 0 && !under {
                let mut fp = FlatPoint::round(
                    (x, n),
                    y + 0.05 - d.lift - 0.15 * near,
                    d.half_width + flare + 0.5,
                    10.0,
                );
                fp.slope = (bank * right.0, bank * right.1);
                stub_flat.push(fp);
            }
        }
        stubs.push(secs);
    }
    for p in stub_flat {
        flat.add(p);
    }

    // 9. Les murs et les falaises, une fois le relief aplani connu.
    let walls = build_walls(&path, &sides, &flat, dem);
    let cliffs = build_cliffs(&path, &sides, &flat, dem);
    let net = RoadNetwork {
        stubs,
        roundabouts: rings,
        ribbons,
        walls,
        cliffs,
        flat,
        samples,
    };
    (path, net)
}

/// Relief au-delà de l'accotement d'un côté de la route.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Steep {
    No,
    /// Talus montant : mur de soutènement.
    Up,
    /// Paroi très raide : rocher taillé, sans mur.
    Cliff,
    /// Talus descendant : parapet.
    Down,
}

/// Côtés du parcours (gauche, droite) bordés d'un talus raide, par
/// échantillon. `busy(q, r)` : une amorce passe-t-elle à moins de `r` de `q` ?
fn wall_sides(
    path: &RoadPath,
    samples: &SampleIndex,
    rings: &[Roundabout],
    dem: &dyn Fn(f64, f64) -> f64,
    busy: &dyn Fn((f64, f64), f64) -> bool,
) -> [Vec<Steep>; 2] {
    let ms = path.samples.len();
    let mut out = [vec![Steep::No; ms], vec![Steep::No; ms]];
    for (side, out) in out.iter_mut().enumerate() {
        let sg = if side == 0 { -1.0 } else { 1.0 };
        let mut kind = vec![Steep::No; ms];
        let mut allowed = vec![false; ms];
        for (k, p) in path.samples.iter().enumerate() {
            let pr = &p.profile;
            let gap = [marks::GAP_LEFT, marks::GAP_RIGHT][side];
            if pr.hidden || pr.sidewalk[side] > 0.05 || pr.marks & gap != 0 {
                continue;
            }
            let (rx, rn) = p.right();
            let w0 = pr.reach(side) + 0.2;
            let at = |off: f64| (p.x + rx * sg * off, p.n + rn * sg * off);
            let probe = at(pr.reach(side) + 10.0);
            let rise = dem(probe.0, probe.1) - p.elevation;
            let up = rise > WALL_RISE && rise <= CLIFF_RISE;
            let depth = if up { WALL_DEPTH } else { 1.0 };
            // Pas dans l'intérieur d'un virage trop serré pour la terrasse.
            let inner = if side == 0 { p.curvature } else { -p.curvature };
            if inner > 0.0 && 1.0 / inner < w0 + depth + 5.0 {
                continue;
            }
            // Ni sur une autre portion du parcours (lacets), ni sur une
            // amorce ou un rond-point.
            let clear = [w0, w0 + depth * 0.5, w0 + depth + 4.0].iter().all(|&off| {
                let q = at(off);
                let other = samples.nearest(q.0, q.1, 30.0).is_some_and(|(d, j)| {
                    j.abs_diff(k) > 60 && d < path.samples[j].profile.reach(0).max(4.0) + 3.0
                });
                !other && !busy(q, 3.0) && rings.iter().all(|r| r.signed(q) > r.half_width + 3.0)
            });
            allowed[k] = clear;
            kind[k] = if up {
                Steep::Up
            } else if rise > CLIFF_RISE {
                Steep::Cliff
            } else if -rise > WALL_RISE {
                Steep::Down
            } else {
                Steep::No
            };
        }
        for want in [Steep::Up, Steep::Cliff, Steep::Down] {
            let mut on: Vec<bool> = kind
                .iter()
                .zip(&allowed)
                .map(|(&k, &a)| k == want && a)
                .collect();
            clean_runs(&mut on, 8, 12);
            for (o, a) in on.iter_mut().zip(&allowed) {
                *o &= *a;
            }
            clean_runs(&mut on, 0, 12);
            for (o, &b) in out.iter_mut().zip(&on) {
                if b {
                    *o = want;
                }
            }
        }
    }
    out
}

/// Murs le long des talus raides : la terrasse d'un mur de soutènement
/// rejoint le relief aplani, un parapet fait 0,75 m.
fn build_walls(
    path: &RoadPath,
    sides: &[Vec<Steep>; 2],
    flat: &Flatten,
    dem: &dyn Fn(f64, f64) -> f64,
) -> Vec<Wall> {
    let mut walls = Vec::new();
    for (side, kinds) in sides.iter().enumerate() {
        let sg = if side == 0 { -1.0 } else { 1.0 };
        let mut k = 0;
        while k < kinds.len() {
            let want = kinds[k];
            if want == Steep::No || want == Steep::Cliff {
                k += 1;
                continue;
            }
            let a = k;
            while k < kinds.len() && kinds[k] == want {
                k += 1;
            }
            let retaining = want == Steep::Up;
            let depth = if retaining { WALL_DEPTH } else { 0.45 };
            let mut idx: Vec<usize> = (a..k).step_by(2).chain(std::iter::once(k - 1)).collect();
            idx.dedup();
            let mut pts: Vec<WallPoint> = idx
                .iter()
                .map(|&j| {
                    let p = &path.samples[j];
                    let (rx, rn) = p.right();
                    let out = (rx * sg, rn * sg);
                    let w0 = p.profile.reach(side) + if retaining { 0.2 } else { 0.25 };
                    let top = if retaining {
                        let q = (p.x + out.0 * (w0 + depth), p.n + out.1 * (w0 + depth));
                        (flat.apply(dem(q.0, q.1), q.0, q.1) - p.elevation).clamp(1.0, 4.0)
                    } else {
                        0.75
                    };
                    WallPoint {
                        x: p.x + out.0 * w0,
                        n: p.n + out.1 * w0,
                        out,
                        base: p.elevation - 0.6,
                        top: p.elevation + top,
                        s: p.s,
                    }
                })
                .collect();
            if retaining {
                // Couronnement régulier, abaissé aux extrémités.
                let rel: Vec<f64> = pts.iter().map(|w| w.top - w.base).collect();
                let smooth = gaussian(&rel, 3.0);
                let (s0, s1) = (pts[0].s, pts[pts.len() - 1].s);
                for (w, h) in pts.iter_mut().zip(smooth) {
                    let end = smoothstep(0.0, 8.0, (w.s - s0).min(s1 - w.s));
                    w.top = w.base + 1.4 + (h - 1.4) * end;
                }
            }
            if pts.len() >= 2 {
                walls.push(Wall {
                    pts,
                    depth,
                    retaining,
                });
            }
        }
    }
    walls
}

/// Relief de la paroi d'une falaise (m, positif vers la route) à
/// l'abscisse `s` et à la hauteur `h` : strates inclinées en corniches,
/// bombements, ravines verticales et blocs.
fn cliff_relief(s: f64, h: f64, seed: f64) -> f64 {
    use crate::raster::noise;
    let tilted = h + 0.07 * s + 1.5 * noise(s * 0.02 + seed, 3.0);
    let layer = 1.7 + 0.6 * noise(s * 0.01 + seed, 9.0);
    let f = (tilted / layer).rem_euclid(1.0);
    let ledge = 0.4 * smoothstep(0.0, 0.2, f) * (1.0 - smoothstep(0.7, 1.0, f));
    let bulge = (noise(s * 0.06 + seed, h * 0.08) * 0.65 + noise(s * 0.17, h * 0.2 + seed) * 0.35
        - 0.5)
        * 2.0;
    let g = (s / 9.0 + 0.3 * noise(h * 0.1, seed)).rem_euclid(1.0) - 0.5;
    let gully = -0.9 * (-(g / 0.07).powi(2)).exp();
    let blocks = 0.3 * (noise(s * 0.6 + seed, h * 0.6) - 0.5);
    (ledge + bulge + gully + blocks).clamp(-1.0, 1.3)
}

/// Falaises sculptées le long des talus « Cliff » : la paroi part du replat
/// au pied, monte presque d'aplomb jusqu'au haut du talus, et son rebord
/// rejoint le relief en s'y enfonçant.
fn build_cliffs(
    path: &RoadPath,
    sides: &[Vec<Steep>; 2],
    flat: &Flatten,
    dem: &dyn Fn(f64, f64) -> f64,
) -> Vec<Cliff> {
    let mut cliffs = Vec::new();
    for (side, kinds) in sides.iter().enumerate() {
        let sg = if side == 0 { -1.0 } else { 1.0 };
        let mut k = 0;
        while k < kinds.len() {
            if kinds[k] != Steep::Cliff {
                k += 1;
                continue;
            }
            let a = k;
            while k < kinds.len() && kinds[k] == Steep::Cliff {
                k += 1;
            }
            let idx: Vec<usize> = (a..k).collect();
            if idx.len() < 4 {
                continue;
            }
            let ground = |q: (f64, f64)| flat.apply(dem(q.0, q.1), q.0, q.1);
            // Hauteur visée : le relief au-delà du talus aplani, lissée et
            // abaissée aux extrémités où la falaise sort de la pente.
            let raw: Vec<f64> = idx
                .iter()
                .map(|&j| {
                    let p = &path.samples[j];
                    let (rx, rn) = p.right();
                    let o = p.profile.reach(side) + CLIFF_FLAT + WALL_BLEND;
                    let q = (p.x + rx * sg * o, p.n + rn * sg * o);
                    (ground(q) - p.elevation).clamp(3.0, 30.0)
                })
                .collect();
            let smooth = gaussian(&raw, 4.0);
            let (s0, s1) = (path.samples[a].s, path.samples[k - 1].s);
            let heights: Vec<f64> = idx
                .iter()
                .zip(&smooth)
                .map(|(&j, &h)| {
                    let s = path.samples[j].s;
                    let end = smoothstep(0.0, 12.0, (s - s0).min(s1 - s));
                    1.5 + (h - 1.5) * end
                })
                .collect();
            let hmax = heights.iter().copied().fold(0.0, f64::max);
            let face_rows = (hmax / CLIFF_ROW).ceil() as usize + 1;
            let seed = (side * 37 + a) as f64 * 0.731;
            let mut cliff = Cliff {
                columns: Vec::new(),
                s: Vec::new(),
                heights: Vec::new(),
                face_rows,
                out: Vec::new(),
            };
            for (&j, &hh) in idx.iter().zip(&heights) {
                let p = &path.samples[j];
                let (rx, rn) = p.right();
                let out = (rx * sg, rn * sg);
                let reach = p.profile.reach(side);
                let end = smoothstep(0.0, 12.0, (p.s - s0).min(s1 - p.s));
                // Aux extrémités, le pied recule dans la pente.
                let o0 = reach + 0.8 + (1.0 - end) * 2.5;
                let foot = p.elevation - 0.5;
                let mut col = Vec::with_capacity(face_rows + 4);
                let mut hs = Vec::with_capacity(face_rows + 4);
                let at = |o: f64, h: f64| [p.x + out.0 * o, foot + h, p.n + out.1 * o];
                let mut top_o = o0;
                for r in 0..face_rows {
                    let h = hh * r as f64 / (face_rows - 1) as f64;
                    let d =
                        cliff_relief(p.s, h, seed) * smoothstep(0.3, 1.5, h) * (0.4 + 0.6 * end);
                    let o = (o0 + CLIFF_BATTER * h - d).max(reach + 0.4);
                    col.push(at(o, h));
                    hs.push(h);
                    top_o = o;
                }
                // Rebord arrondi puis talus qui s'enfonce dans le relief.
                let back = reach + CLIFF_FLAT + WALL_BLEND + 2.5;
                let sink = |o: f64| ground((p.x + out.0 * o, p.n + out.1 * o)) - foot;
                for (t, lift) in [(0.12, 0.35), (0.35, 0.2), (0.65, 0.0), (1.0, -1.2)] {
                    let o = top_o + 0.4 + (back - top_o - 0.4).max(1.0) * t;
                    // Sous le relief s'il est plus haut : le rebord ne déborde
                    // jamais de la pente en languette.
                    let h = if t < 1.0 {
                        (hh + lift).max(sink(o) - 0.3)
                    } else {
                        sink(o) + lift
                    };
                    col.push(at(o, h));
                    hs.push(h);
                }
                cliff.columns.push(col);
                cliff.heights.push(hs);
                cliff.s.push(p.s);
                cliff.out.push(out);
            }
            cliffs.push(cliff);
        }
    }
    cliffs
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
            let m = r.ring.len();
            let mut s = 0.0;
            let secs: Vec<Section> = (0..=m)
                .map(|i| {
                    let k = i % m;
                    if i > 0 {
                        s += dist(r.ring[i - 1], r.ring[k]);
                    }
                    let q = r.ring[k];
                    let out = r.outward(k);
                    let rim = r.edge(k, r.half_width);
                    let mouth = r.mouths.iter().any(|&(p, w)| dist(rim, p) < w + 3.0);
                    Section {
                        x: q.0,
                        n: q.1,
                        elevation: r.height(q.0, q.1),
                        // Sens antihoraire : la droite est vers l'extérieur.
                        right: out,
                        s,
                        bank: r.slope.0 * out.0 + r.slope.1 * out.1,
                        profile: Profile {
                            half_width: r.half_width,
                            sidewalk: [r.apron, 0.0],
                            verge: [0.0, if mouth { 0.0 } else { 1.2 }],
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

    /// Murs de soutènement et parapets, par cases de 300 m.
    /// UV : abscisse le long du mur et hauteur (m) ; COLOR.r : 0 parement,
    /// 0,5 couronnement, 1 terrasse plantée.
    pub fn wall_meshes(&self, base: f64) -> Vec<MeshData> {
        let mut cells: HashMap<(i64, i64), MeshData> = HashMap::new();
        for w in &self.walls {
            let f = &w.pts[0];
            let key = ((f.x / 300.0).floor() as i64, (f.n / 300.0).floor() as i64);
            push_wall(cells.entry(key).or_default(), w, base);
        }
        let mut keys: Vec<_> = cells.keys().copied().collect();
        keys.sort();
        keys.into_iter()
            .filter_map(|k| cells.remove(&k))
            .filter(|m| !m.is_empty())
            .collect()
    }

    /// Falaises sculptées, par cases de 300 m. UV : abscisse le long du
    /// parcours et hauteur au-dessus du pied (m) ; COLOR.r : creux de la
    /// paroi (0 saillant, 1 renfoncé), COLOR.g : 1 sur le rebord végétalisé.
    pub fn cliff_meshes(&self, base: f64) -> Vec<MeshData> {
        let mut cells: HashMap<(i64, i64), MeshData> = HashMap::new();
        for c in &self.cliffs {
            let f = c.columns[0][0];
            let key = ((f[0] / 300.0).floor() as i64, (f[2] / 300.0).floor() as i64);
            push_cliff(cells.entry(key).or_default(), c, base);
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
            let m = r.ring.len();
            for i in 0..count {
                let h = |c: u32| crate::raster::rand01(k as i64, i as i64, 900 + i64::from(c));
                let e = r.edge((h(1) * m as f64) as usize % m, -(r.half_width + r.apron));
                let f = h(2).sqrt() * (1.0 - 1.2 / ri).max(0.0);
                let (x, n) = (
                    r.center.0 + (e.0 - r.center.0) * f,
                    r.center.1 + (e.1 - r.center.1) * f,
                );
                let y = dome(r, f) + r.height(x, n) - base;
                let size = 0.8 + h(3) * 1.0;
                out.push(
                    [x, y - 0.1, -n],
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

/// Hauteur de l'îlot au-dessus de la chaussée, à la fraction `f` du centre
/// vers le bord.
fn dome(r: &Roundabout, f: f64) -> f64 {
    let f = f.clamp(0.0, 1.0);
    0.05 + KERB + (r.island() * 0.06).min(0.9) * (1.0 - f * f)
}

/// Îlot central : pelouse bombée qui épouse le contour intérieur de l'anneau,
/// avec une jupe sous son bord.
fn island(mesh: &mut MeshData, r: &Roundabout, base: f64) {
    let m = r.ring.len();
    let c = r.center;
    let edge: Vec<(f64, f64)> = (0..m)
        .map(|i| r.edge(i, -(r.half_width + r.apron - 0.1)))
        .collect();
    let fractions = [0.0, 0.35, 0.7, 1.0];
    let first = mesh.positions.len() as i32;
    let mut vertex = |p: (f64, f64), y: f64| {
        mesh.positions
            .push([p.0 as f32, (y - base) as f32, -p.1 as f32]);
        mesh.normals.push([0.0, 1.0, 0.0]);
        mesh.uvs.push([p.0 as f32, p.1 as f32]);
        mesh.uv2s.push([0.0, 1.0]);
        mesh.colors
            .push([0.0, 0.0, 0.0, f32::from(marks::GRASS) / 255.0]);
    };
    vertex(c, r.height(c.0, c.1) + dome(r, 0.0));
    for &f in &fractions[1..] {
        for e in &edge {
            let p = (c.0 + (e.0 - c.0) * f, c.1 + (e.1 - c.1) * f);
            vertex(p, r.height(p.0, p.1) + dome(r, f));
        }
    }
    for e in &edge {
        vertex(*e, r.height(e.0, e.1) - 0.2);
    }
    let up = [0.0, 1.0, 0.0];
    let s = m as i32;
    for i in 0..s {
        mesh.push_tri(first, first + 1 + i, first + 1 + (i + 1) % s, up);
    }
    for ring_i in 1..fractions.len() as i32 {
        let a = first + 1 + (ring_i - 1) * s;
        let b = a + s;
        for i in 0..s {
            let j = (i + 1) % s;
            let normal = if ring_i == fractions.len() as i32 - 1 {
                let e = edge[i as usize];
                [e.0 - c.0, 0.0, -(e.1 - c.1)]
            } else {
                up
            };
            mesh.push_tri(a + i, b + i, a + j, normal);
            mesh.push_tri(a + j, b + i, b + j, normal);
        }
    }
}

/// Paroi d'une falaise en grille (colonnes × rangées), normales lissées
/// d'après les voisins, faces tournées vers la route.
fn push_cliff(mesh: &mut MeshData, c: &Cliff, base: f64) {
    let cols = c.columns.len();
    let rows = c.columns[0].len();
    if cols < 2 || rows < 2 {
        return;
    }
    let p = |i: usize, r: usize| c.columns[i][r];
    let first = mesh.positions.len() as i32;
    for i in 0..cols {
        let out = c.out[i];
        for r in 0..rows {
            let q = p(i, r);
            let (i0, i1) = (i.saturating_sub(1), (i + 1).min(cols - 1));
            let (r0, r1) = (r.saturating_sub(1), (r + 1).min(rows - 1));
            let along = [
                p(i1, r)[0] - p(i0, r)[0],
                p(i1, r)[1] - p(i0, r)[1],
                p(i1, r)[2] - p(i0, r)[2],
            ];
            let up = [
                p(i, r1)[0] - p(i, r0)[0],
                p(i, r1)[1] - p(i, r0)[1],
                p(i, r1)[2] - p(i, r0)[2],
            ];
            // Repère Godot (x, y, -n) ; la normale doit regarder la route.
            let a = [along[0], along[1], -along[2]];
            let b = [up[0], up[1], -up[2]];
            let mut nrm = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            if nrm[1] - nrm[0] * out.0 + nrm[2] * out.1 < 0.0 {
                nrm = [-nrm[0], -nrm[1], -nrm[2]];
            }
            let l = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2])
                .sqrt()
                .max(1e-9);
            mesh.positions
                .push([q[0] as f32, (q[1] - base) as f32, -q[2] as f32]);
            mesh.normals.push([
                (nrm[0] / l) as f32,
                (nrm[1] / l) as f32,
                (nrm[2] / l) as f32,
            ]);
            mesh.uvs.push([c.s[i] as f32, c.heights[i][r] as f32]);
            // Creux : recul par rapport à la moyenne des voisins.
            let depth = |j: usize, s: usize| p(j, s)[0] * out.0 + p(j, s)[2] * out.1;
            let around = (depth(i0, r) + depth(i1, r) + depth(i, r0) + depth(i, r1)) / 4.0;
            let recess = ((depth(i, r) - around) * 2.0 + 0.5).clamp(0.0, 1.0);
            let cap = if r + 3 >= rows { 1.0 } else { 0.0 };
            mesh.colors.push([recess as f32, cap, 0.0, 1.0]);
        }
    }
    // Même sens de rotation pour toute la grille (les surplombs regardent
    // vers le bas), choisi sur une maille du pied : face vers la route.
    let rows_i = rows as i32;
    let m = cols / 2;
    let g = |q: [f64; 3]| [q[0], q[1], -q[2]];
    let (q0, q1, q2) = (g(p(m - 1, 0)), g(p(m, 0)), g(p(m - 1, 1)));
    let (u, v) = (
        [q1[0] - q0[0], q1[1] - q0[1], q1[2] - q0[2]],
        [q2[0] - q0[0], q2[1] - q0[1], q2[2] - q0[2]],
    );
    let nrm = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let out = c.out[m];
    // Godot : faces avant dans le sens horaire, donc normale géométrique
    // (sens trigonométrique) opposée à la face visible.
    let flip = -nrm[0] * out.0 + nrm[2] * out.1 > 0.0;
    for i in 0..cols - 1 {
        for r in 0..rows - 1 {
            let a = first + i as i32 * rows_i + r as i32;
            let b = a + rows_i;
            if flip {
                mesh.indices
                    .extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
            } else {
                mesh.indices
                    .extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
            }
        }
    }
    // Bouts : le profil de la première et de la dernière colonne, fermé.
    for (i, other) in [(0, 1), (cols - 1, cols - 2)] {
        let t = [
            p(i, 0)[0] - p(other, 0)[0],
            0.0,
            -(p(i, 0)[2] - p(other, 0)[2]),
        ];
        let foot = first + (i * rows) as i32;
        for r in 1..rows - 1 {
            mesh.push_tri(foot, foot + r as i32, foot + r as i32 + 1, t);
        }
    }
}

/// Mur : parement côté route, couronnement, terrasse (soutènement) ou
/// dessus (parapet), et face arrière.
fn push_wall(mesh: &mut MeshData, w: &Wall, base: f64) {
    let coping = 0.5f64.min(w.depth);
    // Profils en travers : (recul depuis la face, haut ?) ; chaque face a
    // ses propres sommets pour des arêtes franches.
    type Edge = (f64, bool);
    let faces: [(Edge, Edge, f32); 4] = [
        ((0.0, false), (0.0, true), 0.0),
        ((0.0, true), (coping, true), 0.5),
        ((coping, true), (w.depth, true), 1.0),
        ((w.depth, true), (w.depth, false), 0.0),
    ];
    for (fi, &(p0, p1, zone)) in faces.iter().enumerate() {
        if fi == 2 && w.depth - coping < 0.05 {
            continue;
        }
        let first = mesh.positions.len() as i32;
        for wp in &w.pts {
            for (back, high) in [p0, p1] {
                let (x, n) = (wp.x + wp.out.0 * back, wp.n + wp.out.1 * back);
                let y = if high { wp.top } else { wp.base };
                mesh.positions
                    .push([x as f32, (y - base) as f32, -n as f32]);
                let v = if fi == 0 || fi == 3 {
                    y - wp.base
                } else {
                    back
                };
                mesh.uvs.push([wp.s as f32, v as f32]);
                mesh.colors.push([zone, 0.0, 0.0, 1.0]);
                mesh.normals.push(match fi {
                    0 => [-wp.out.0 as f32, 0.0, wp.out.1 as f32],
                    3 => [wp.out.0 as f32, 0.0, -wp.out.1 as f32],
                    _ => [0.0, 1.0, 0.0],
                });
            }
        }
        let wp = &w.pts[0];
        let normal = match fi {
            0 => [-wp.out.0, 0.0, wp.out.1],
            3 => [wp.out.0, 0.0, -wp.out.1],
            _ => [0.0, 1.0, 0.0],
        };
        for k in 0..w.pts.len() as i32 - 1 {
            let (a, b) = (first + 2 * k, first + 2 * k + 2);
            mesh.push_tri(a, b, a + 1, normal);
            mesh.push_tri(a + 1, b, b + 1, normal);
        }
    }
    // Bouts du mur.
    let last = w.pts.len() - 1;
    for (wp, other) in [(&w.pts[0], &w.pts[1]), (&w.pts[last], &w.pts[last - 1])] {
        // Face tournée à l'opposé du reste du mur.
        let t = unit((wp.x - other.x, wp.n - other.n));
        let first = mesh.positions.len() as i32;
        for (back, high) in [(0.0, false), (0.0, true), (w.depth, true), (w.depth, false)] {
            let (x, n) = (wp.x + wp.out.0 * back, wp.n + wp.out.1 * back);
            let y = if high { wp.top } else { wp.base };
            mesh.positions
                .push([x as f32, (y - base) as f32, -n as f32]);
            mesh.uvs.push([back as f32, (y - wp.base) as f32]);
            mesh.colors.push([0.0, 0.0, 0.0, 1.0]);
            mesh.normals.push([t.0 as f32, 0.0, -t.1 as f32]);
        }
        let normal = [t.0, 0.0, -t.1];
        mesh.push_tri(first, first + 1, first + 2, normal);
        mesh.push_tri(first, first + 2, first + 3, normal);
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
    /// vers l'est à 150 m, un chemin non raccordé qui frôle la route vers
    /// 60 m, et un rond-point de 15 m de rayon à 300 m.
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
        let lane = [(14.0, 30.0), (2.6, 60.0), (14.0, 90.0)];
        let json = format!(
            r#"{{"elements":[
            {{"type":"way","id":1,"tags":{{"highway":"secondary","sidewalk":"right"}},"geometry":{}}},
            {{"type":"way","id":2,"tags":{{"highway":"residential"}},"geometry":{}}},
            {{"type":"way","id":3,"tags":{{"highway":"secondary","junction":"roundabout"}},"geometry":{}}},
            {{"type":"way","id":4,"tags":{{"highway":"secondary"}},"geometry":{}}},
            {{"type":"way","id":5,"tags":{{"highway":"service"}},"geometry":{}}}
            ]}}"#,
            geometry(&main),
            geometry(&side),
            geometry(&ring),
            geometry(&exit),
            geometry(&lane)
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
        let p = path.samples.iter().find(|s| s.d > 120.0).unwrap();
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
        // Le cycliste reste sur la chaussée, à distance de la bordure.
        for s in &path.samples {
            assert!(
                s.offset <= s.profile.half_width - 0.3 + 1e-9,
                "{}",
                s.offset
            );
        }
    }

    #[test]
    fn side_street_starts_under_the_road_and_continues_as_a_ribbon() {
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
        // Elle part du nœud de jonction, sous la chaussée, un peu plus bas.
        let mouth = path
            .samples
            .iter()
            .min_by(|a, b| (a.n - sn).abs().total_cmp(&(b.n - sn).abs()))
            .unwrap();
        assert!((stub[0].x - mouth.x).abs() < mouth.profile.half_width);
        assert!(stub[0].elevation < mouth.elevation);
        let end = stub.last().unwrap();
        assert!((end.s - STUB_LEN).abs() < 1.5, "{}", end.s);
        assert!(stub[0].profile.marks & marks::GIVE_WAY != 0);
        assert!(stub[0].profile.half_width > end.profile.half_width + 2.0);
        assert!(mouth.profile.marks & marks::GAP_RIGHT != 0);
        assert_eq!(
            mouth.profile.sidewalk[1], 0.0,
            "trottoir interrompu au débouché"
        );
        // La suite de la rue est drapée à partir du bout de l'amorce.
        let rest: Vec<_> = net.ribbons.iter().filter(|r| r.line == 1).collect();
        assert_eq!(rest.len(), 1);
        let first = rest[0].points[0];
        assert!(dist(first, (end.x, end.n)) < 0.5, "{first:?}");
        // Le sol est aplani sous l'amorce.
        let (w, _) = net.flat.weight(sx + 20.0, sn).unwrap();
        assert!(w > 0.99);
        assert!(net.flat.near_other(sx + 20.0, sn, 0.5));
    }

    #[test]
    fn a_road_that_only_passes_by_gets_no_stub() {
        let (route, osm) = scene();
        let (_, net) = build(&route, &osm, &|_, _| 500.0);
        assert!(
            net.stubs
                .iter()
                .all(|s| !(25.0..95.0).contains(&s[0].n) || s[0].x < 0.0),
            "{:?}",
            net.stubs
                .iter()
                .map(|s| (s[0].x, s[0].n))
                .collect::<Vec<_>>()
        );
        let lane: Vec<_> = net.ribbons.iter().filter(|r| r.line == 4).collect();
        assert_eq!(lane.len(), 1);
        assert_eq!(lane[0].points, osm.lines[4].points);
    }

    #[test]
    fn finds_the_roundabout_and_hides_the_route_inside() {
        let (route, osm) = scene();
        let (path, net) = build(&route, &osm, &|_, _| 500.0);
        assert_eq!(net.roundabouts.len(), 1);
        let r = &net.roundabouts[0];
        assert!((r.radius - 15.0).abs() < 0.8, "{}", r.radius);
        assert!(r.ring.len() > 80);
        assert!(r
            .ring
            .iter()
            .all(|&p| (dist(p, r.center) - 15.0).abs() < 0.6));
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
        // Ni l'anneau ni ses entrées ne sont redessinés par-dessus.
        assert!(net.ribbons.iter().all(|r| r.line != 2));
    }

    #[test]
    fn oval_ring_keeps_its_shape_and_slope() {
        let ellipse: Vec<(f64, f64)> = (0..40)
            .map(|i| {
                let a = -(i as f64) / 40.0 * TAU;
                (20.0 * a.cos(), 12.0 * a.sin())
            })
            .collect();
        let (area, c) = area_centroid(&ellipse);
        assert!(area < 0.0, "sens horaire");
        let mut ring = ellipse.clone();
        ring.reverse();
        let mut r = Roundabout {
            center: c,
            ring: smooth_ring(&ring),
            radius: 16.0,
            rmax: 20.0,
            half_width: 3.5,
            apron: 1.0,
            elevation: 0.0,
            slope: (0.0, 0.0),
            lanes: 1,
            mouths: Vec::new(),
        };
        assert!((r.signed((0.0, 0.0)) + 11.9).abs() < 0.3);
        assert!((r.signed((25.0, 0.0)) - 5.0).abs() < 0.3);
        assert!((r.signed((0.0, 15.0)) - 3.0).abs() < 0.3);
        // L'extérieur est à droite du sens de circulation.
        let o = r.outward(0);
        let p = r.ring[0];
        assert!(dist((p.0 + o.0, p.1 + o.1), c) > dist(p, c));
        // Plan ajusté sur un parcours qui monte vers le nord à 5 %.
        let samples: Vec<PathSample> = (0..80)
            .map(|i| {
                let n = -40.0 + i as f64;
                PathSample {
                    x: 2.0,
                    n,
                    elevation: 100.0 + 0.05 * n,
                    d: 0.0,
                    s: 0.0,
                    tangent: (0.0, 1.0),
                    curvature: 0.0,
                    offset: 0.0,
                    profile: Profile::default(),
                }
            })
            .collect();
        r.fit_plane(&samples);
        assert!((r.elevation - 100.0).abs() < 0.1, "{}", r.elevation);
        assert!(r.slope.1 > 0.035 && r.slope.0.abs() < 0.01, "{:?}", r.slope);
    }

    #[test]
    fn steep_slopes_get_retaining_walls_or_parapets() {
        let (route, osm) = scene();
        // Montagne à l'ouest du parcours : mur de soutènement à gauche.
        let x0 = osm.lines[0].points[0].0;
        let up = |x: f64, _: f64| 500.0 + (x0 - x - 6.0).max(0.0) * 0.9;
        let (path, net) = build(&route, &osm, &up);
        let walls: Vec<_> = net.walls.iter().filter(|w| w.retaining).collect();
        assert!(!walls.is_empty());
        let p = path.samples.iter().find(|s| s.d > 100.0).unwrap();
        let w = walls
            .iter()
            .flat_map(|w| &w.pts)
            .min_by(|a, b| (a.n - p.n).abs().total_cmp(&(b.n - p.n).abs()))
            .unwrap();
        assert!(w.x < p.x - p.profile.reach(0), "{}", w.x);
        assert!(w.top - p.elevation > 1.0, "{}", w.top - p.elevation);
        // Le sol reste au niveau de la route juste derrière le mur, caché
        // par la terrasse.
        let behind = w.x - 2.0;
        let g = net.flat.apply(up(behind, w.n), behind, w.n);
        assert!((g - (p.elevation - 0.35)).abs() < 0.05, "{g}");
        assert!(!net.wall_meshes(500.0).is_empty());
        // Pas de mur contre le rond-point.
        let c = net.roundabouts[0].center;
        assert!(walls
            .iter()
            .flat_map(|w| &w.pts)
            .all(|q| dist((q.x, q.n), c) > 21.0));

        // Ravin à l'ouest : parapet.
        let down = |x: f64, _: f64| 500.0 - (x0 - x - 6.0).max(0.0) * 0.9;
        let (_, net) = build(&route, &osm, &down);
        assert!(net.walls.iter().any(|w| !w.retaining));
        assert!(net.walls.iter().all(|w| !w.retaining));
        // Plat : aucun mur.
        let (_, net) = build(&route, &osm, &|_, _| 500.0);
        assert!(net.walls.is_empty());
    }

    #[test]
    fn cliffs_rise_in_front_of_the_slope() {
        let (route, osm) = scene();
        // Paroi à l'ouest, bien plus raide que pour un mur.
        let x0 = osm.lines[0].points[0].0;
        let cliff = |x: f64, _: f64| 500.0 + (x0 - x - 6.0).max(0.0) * 2.5;
        let (path, net) = build(&route, &osm, &cliff);
        assert!(!net.cliffs.is_empty());
        assert!(net.walls.iter().all(|w| !w.retaining));
        let p = path.samples.iter().find(|s| s.d > 100.0).unwrap();
        for c in &net.cliffs {
            let rows = c.columns[0].len();
            assert!(c.face_rows >= 2 && rows > c.face_rows);
            for (col, h) in c.columns.iter().zip(&c.heights) {
                assert_eq!(col.len(), rows);
                assert_eq!(h.len(), rows);
                // La paroi monte, à l'ouest de la chaussée, sans la déborder.
                assert!(h[c.face_rows - 1] >= 1.5);
                for q in &col[..c.face_rows] {
                    assert!(q[0] < p.x - p.profile.reach(0) + 0.5 - 0.4, "{}", q[0]);
                }
            }
        }
        let tallest = net
            .cliffs
            .iter()
            .flat_map(|c| c.heights.iter().map(|h| h[c.face_rows - 1]))
            .fold(0.0, f64::max);
        assert!(tallest > 8.0, "{tallest}");
        let meshes = net.cliff_meshes(500.0);
        assert!(!meshes.is_empty());
        assert!(meshes.iter().all(|m| m.normals.len() == m.positions.len()));
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
