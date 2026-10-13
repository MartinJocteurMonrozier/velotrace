//! Mobilier de course, comme sur les grands tours : arche au départ, au pied
//! et au sommet de chaque ascension (grand prix de la montagne), flamme
//! rouge à 1 km de l'arrivée, banderoles à 3 et 2 km, portique d'arrivée,
//! barrières habillées de banderoles et spectateurs à leurs abords.

use crate::mesh::Instances;
use crate::raster::rand01;
use crate::road::{marks, Profile, RoadPath};
use std::f64::consts::PI;
use velo_core::route::Route;

/// Deux arches plus proches que ça n'en font qu'une (m).
const MERGE_M: f64 = 300.0;
/// Distance minimale entre une banderole (flamme rouge, kilomètres) et une
/// autre arche (m).
const BANNER_GAP_M: f64 = 150.0;
/// Longueur d'une barrière (m).
const BARRIER_M: f64 = 2.5;
/// Les barrières sont posées à cette distance au-delà de la chaussée et des
/// trottoirs (m).
const BARRIER_OFFSET: f64 = 0.45;

/// Le numéro est celui transmis à Godot (`as i64`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchKind {
    Start,
    ClimbStart,
    ClimbTop,
    Finish,
    /// Triangle rouge du dernier kilomètre.
    FlammeRouge,
    /// Banderole « 3 KM », « 2 KM ».
    KmToGo,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arch {
    pub kind: ArchKind,
    /// Distance sur le parcours.
    pub d: f64,
    pub x: f64,
    pub n: f64,
    pub elevation: f64,
    /// Sens de la route (est, nord).
    pub heading: (f64, f64),
    /// Distance de l'axe aux piles de gauche et de droite (m), au-delà de la
    /// chaussée et des trottoirs.
    pub left: f64,
    pub right: f64,
    /// Pente de la route sous l'arche (fraction, positive en montée).
    pub grade: f64,
    pub title: String,
    pub subtitle: String,
}

/// « 12,4 », « 1 784 » : nombre à la française.
fn fr(x: f64, decimals: usize) -> String {
    let s = format!("{x:.decimals$}");
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let digits: Vec<char> = int.chars().collect();
    let mut out = String::new();
    for (i, c) in digits.iter().enumerate() {
        if i > 0
            && (digits.len() - i) % 3 == 0
            && c.is_ascii_digit()
            && digits[i - 1].is_ascii_digit()
        {
            out.push(' ');
        }
        out.push(*c);
    }
    if !frac.is_empty() {
        out.push(',');
        out.push_str(frac);
    }
    out
}

/// Arches du parcours, dans l'ordre.
pub fn arches(route: &Route, path: &RoadPath) -> Vec<Arch> {
    let len = route.length_m();
    if len < 100.0 || path.samples.len() < 2 {
        return Vec::new();
    }
    let start_d = (len * 0.05).min(15.0);
    let finish_d = len - (len * 0.02).min(4.0);
    let mut plan: Vec<(f64, ArchKind, String, String)> =
        vec![(start_d, ArchKind::Start, "DÉPART".into(), String::new())];
    let mut finish_sub = String::new();
    for c in route.climbs() {
        let about = format!(
            "{} km à {} %",
            fr(c.length_m() / 1000.0, 1),
            fr(c.grade() * 100.0, 1)
        );
        let cat = c.category();
        let summit = format!("{} m d'altitude", fr(c.top_elevation_m, 0));
        if c.start_m - start_d < MERGE_M {
            plan[0].3 = format!("Ascension : {about} · {cat}");
        } else {
            plan.push((
                c.start_m,
                ArchKind::ClimbStart,
                "PIED DE L'ASCENSION".into(),
                format!("{about} · {cat}"),
            ));
        }
        if finish_d - c.end_m < MERGE_M {
            finish_sub = format!("Sommet · {summit} · {cat}");
        } else {
            plan.push((
                c.end_m,
                ArchKind::ClimbTop,
                "GRAND PRIX DE LA MONTAGNE".into(),
                format!("Sommet · {summit} · {cat}"),
            ));
        }
    }
    plan.push((finish_d, ArchKind::Finish, "ARRIVÉE".into(), finish_sub));
    // Pas deux arches l'une sur l'autre : l'arrivée l'emporte, sinon la première.
    let mut kept: Vec<(f64, ArchKind, String, String)> = Vec::new();
    for a in plan {
        if let Some(prev) = kept.last() {
            if a.0 - prev.0 < 150.0 {
                if a.1 == ArchKind::Finish && prev.1 != ArchKind::Start {
                    kept.pop();
                } else {
                    continue;
                }
            }
        }
        kept.push(a);
    }
    // Flamme rouge à 1 km, banderoles à 3 et 2 km : seulement là où elles ne
    // gênent aucune arche.
    let mut banners: Vec<(f64, ArchKind, String, String)> = Vec::new();
    if len > 1500.0 {
        banners.push((
            finish_d - 1000.0,
            ArchKind::FlammeRouge,
            "DERNIER KILOMÈTRE".into(),
            String::new(),
        ));
    }
    for k in [2u32, 3] {
        let d = finish_d - f64::from(k) * 1000.0;
        if d > start_d + 500.0 {
            banners.push((d, ArchKind::KmToGo, format!("{k} KM"), String::new()));
        }
    }
    for b in banners {
        if kept.iter().all(|a| (a.0 - b.0).abs() >= BANNER_GAP_M) {
            kept.push(b);
        }
    }
    kept.sort_by(|a, b| a.0.total_cmp(&b.0));
    kept.into_iter()
        .filter_map(|(d, kind, title, subtitle)| {
            // Pas au milieu d'un rond-point : un peu plus loin.
            let mut d = d;
            while path.sample_at(d).is_some_and(|p| p.profile.hidden) && d < len {
                d += 10.0;
            }
            let p = path.sample_at(d.min(len))?;
            let pr = &p.profile;
            let (lo, hi) = (path.sample_at(d - 5.0)?, path.sample_at(d + 5.0)?);
            let grade = (hi.elevation - lo.elevation) / (hi.s - lo.s).max(1.0);
            Some(Arch {
                kind,
                d,
                x: p.x,
                n: p.n,
                elevation: p.elevation,
                heading: p.tangent,
                left: pr.half_width + pr.sidewalk[0] + 0.9,
                right: pr.half_width + pr.sidewalk[1] + 0.9,
                grade,
                title,
                subtitle,
            })
        })
        .collect()
}

/// Point du tracé lissé, interpolé.
#[derive(Clone, Copy)]
struct Spot {
    d: f64,
    x: f64,
    n: f64,
    elevation: f64,
    tangent: (f64, f64),
    profile: Profile,
}

/// Points du tracé lissé tous les `step` mètres.
fn spots(path: &RoadPath, step: f64) -> Vec<Spot> {
    let ss = &path.samples;
    let mut out = Vec::new();
    if ss.len() < 2 {
        return out;
    }
    let mut k = 0u32;
    loop {
        let t = f64::from(k) * step;
        if t > path.length() {
            break;
        }
        k += 1;
        let i = ss
            .partition_point(|p| p.s <= t)
            .saturating_sub(1)
            .min(ss.len() - 2);
        let (a, b) = (&ss[i], &ss[i + 1]);
        let f = ((t - a.s) / (b.s - a.s).max(1e-9)).clamp(0.0, 1.0);
        let lerp = |u: f64, v: f64| u + (v - u) * f;
        let tan = (
            lerp(a.tangent.0, b.tangent.0),
            lerp(a.tangent.1, b.tangent.1),
        );
        let l = tan.0.hypot(tan.1).max(1e-9);
        out.push(Spot {
            d: lerp(a.d, b.d),
            x: lerp(a.x, b.x),
            n: lerp(a.n, b.n),
            elevation: lerp(a.elevation, b.elevation),
            tangent: (tan.0 / l, tan.1 / l),
            profile: if f < 0.5 { a.profile } else { b.profile },
        });
    }
    out
}

/// Zone habillée autour de chaque arche : (début, fin, type).
fn zones(arches: &[Arch], spectators: bool) -> Vec<(f64, f64, ArchKind)> {
    arches
        .iter()
        .filter_map(|a| {
            let (lo, hi) = match (a.kind, spectators) {
                (ArchKind::Start, false) => (-a.d, 150.0),
                (ArchKind::Start, true) => (-10.0, 40.0),
                (ArchKind::Finish, false) => (-300.0, 30.0),
                (ArchKind::Finish, true) => (-250.0, 25.0),
                (ArchKind::FlammeRouge, false) => (-40.0, 40.0),
                (ArchKind::FlammeRouge, true) => (-20.0, 20.0),
                (ArchKind::ClimbTop, true) => (-50.0, 50.0),
                (ArchKind::ClimbStart, true) => (-25.0, 25.0),
                (ArchKind::ClimbStart | ArchKind::ClimbTop, false) => (-60.0, 60.0),
                (ArchKind::KmToGo, _) => return None,
            };
            Some((a.d + lo, a.d + hi, a.kind))
        })
        .collect()
}

/// Calage d'un objet posé au bord de la route, du côté 0 (gauche) ou 1
/// (droite), à `beyond` mètres au-delà de la chaussée et des trottoirs :
/// position Godot (x, y, z) et cap tourné vers la route (+Z local vers l'axe).
fn roadside(s: &Spot, side: usize, beyond: f64, base: f64) -> ([f64; 3], f64) {
    let pr = &s.profile;
    let (rx, rn) = (s.tangent.1, -s.tangent.0);
    let sg = if side == 0 { -1.0 } else { 1.0 };
    let off = sg * (pr.half_width + pr.sidewalk[side] + beyond);
    let (x, n) = (s.x + rx * off, s.n + rn * off);
    // Sur le trottoir s'il y en a un.
    let kerb = if pr.sidewalk[side] > 0.05 { 0.14 } else { 0.0 };
    let yaw = s.tangent.1.atan2(s.tangent.0) + if side == 0 { 0.0 } else { PI };
    ([x, s.elevation - base - 0.05 + kerb, -n], yaw)
}

/// Motifs de banderoles de barrières (cases de l'atlas dessiné par Godot),
/// par zone : jaune, bleu, rouge, vert, blanc à rayures, marine, à pois,
/// orange, « bonne route », damier.
fn banner_designs(kind: ArchKind) -> &'static [u8] {
    match kind {
        ArchKind::Finish => &[0, 1, 2, 3, 4, 5, 7, 9],
        ArchKind::Start => &[1, 4, 8, 0],
        ArchKind::ClimbStart | ArchKind::ClimbTop => &[6, 2, 6, 4],
        ArchKind::FlammeRouge | ArchKind::KmToGo => &[2, 0],
    }
}

/// Nombre de motifs de l'atlas (le canal alpha de la couleur d'instance
/// porte le numéro du motif : (motif + 0,5) / 16).
pub const BANNER_DESIGNS: u8 = 10;

/// Barrières « Vauban » le long des deux bords, habillées côté route de
/// banderoles imprimées : à la sortie du départ, aux abords des arches
/// d'ascension et de la flamme rouge, sur les 300 derniers mètres (tampon
/// MultiMesh, une barrière de 2,5 m par instance, +Z local vers la route).
/// Quatre barrières de suite portent le même motif, comme une banderole
/// continue.
pub fn barriers(path: &RoadPath, arches: &[Arch], base: f64) -> Instances {
    let mut out = Instances::default();
    let zones = zones(arches, false);
    for s in spots(path, BARRIER_M) {
        let Some(&(_, _, kind)) = zones.iter().find(|r| (r.0..=r.1).contains(&s.d)) else {
            continue;
        };
        // Pas contre les piles d'une arche.
        if arches.iter().any(|a| (a.d - s.d).abs() < 2.0) || s.profile.hidden {
            continue;
        }
        let designs = banner_designs(kind);
        let block = (s.d / (4.0 * BARRIER_M)).floor().max(0.0) as usize;
        for side in 0..2 {
            if s.profile.marks & [marks::GAP_LEFT, marks::GAP_RIGHT][side] != 0 {
                continue;
            }
            let design = designs[(block + side * 3) % designs.len()];
            let (pos, yaw) = roadside(&s, side, BARRIER_OFFSET, base);
            let color = [1.0, 1.0, 1.0, (f32::from(design) + 0.5) / 16.0];
            out.push(pos, yaw, 1.0, 1.0, color);
        }
    }
    out
}

/// Spectateurs massés derrière les barrières, sur une ou deux rangées : à
/// l'arrivée, au sommet des ascensions, à la flamme rouge et au départ
/// (tampon MultiMesh ; la couleur est celle du maillot, la hauteur varie).
pub fn spectators(path: &RoadPath, arches: &[Arch], base: f64) -> Instances {
    let mut out = Instances::default();
    let zones = zones(arches, true);
    let shirts: [[f32; 4]; 10] = [
        [0.98, 0.84, 0.10, 1.0],
        [0.96, 0.96, 0.95, 1.0],
        [0.85, 0.12, 0.12, 1.0],
        [0.12, 0.35, 0.75, 1.0],
        [0.15, 0.58, 0.30, 1.0],
        [0.10, 0.10, 0.12, 1.0],
        [0.95, 0.50, 0.10, 1.0],
        [0.55, 0.20, 0.55, 1.0],
        [0.45, 0.70, 0.90, 1.0],
        [0.98, 0.84, 0.10, 1.0],
    ];
    const STEP: f64 = 0.7;
    for (k, s) in spots(path, STEP).into_iter().enumerate() {
        let Some(&(lo, hi, kind)) = zones.iter().find(|r| (r.0..=r.1).contains(&s.d)) else {
            continue;
        };
        if arches.iter().any(|a| (a.d - s.d).abs() < 2.5) || s.profile.hidden {
            continue;
        }
        // Plus serrés au cœur de la zone (l'arche d'arrivée, le sommet).
        let core = match kind {
            ArchKind::Finish => (s.d - (hi - 25.0)).abs() < 120.0,
            _ => (s.d - (lo + hi) / 2.0).abs() < (hi - lo) / 4.0,
        };
        let rows = if core { 2 } else { 1 };
        for side in 0..2 {
            if s.profile.marks & [marks::GAP_LEFT, marks::GAP_RIGHT][side] != 0 {
                continue;
            }
            for row in 0..rows {
                let r = |c: i64| rand01(k as i64, (side * 2 + row) as i64, c);
                if r(0) < 0.25 {
                    continue;
                }
                let beyond = BARRIER_OFFSET + 0.45 + row as f64 * 0.6 + r(1) * 0.2;
                let mut spot = s;
                let shift = (r(2) - 0.5) * STEP * 0.8;
                spot.x += spot.tangent.0 * shift;
                spot.n += spot.tangent.1 * shift;
                let (pos, yaw) = roadside(&spot, side, beyond, base);
                let shirt = shirts[(r(3) * shirts.len() as f64) as usize % shirts.len()];
                out.push(pos, yaw + (r(4) - 0.5) * 0.8, 1.0, 0.9 + r(5) * 0.2, shirt);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: f64 = 6_371_008.8;

    /// 1 km de plat vers le nord, 4 km à 6 %, 1 km de plat.
    fn route() -> Route {
        let raw: Vec<(f64, f64, f64)> = (0..=600)
            .map(|i| {
                let d = i as f64 * 10.0;
                let e = 500.0 + (d.min(5000.0) - 1000.0).max(0.0) * 0.06;
                (45.0 + (d / R).to_degrees(), 6.0, e)
            })
            .collect();
        Route::from_lat_lon_ele(None, &raw, 5.0, 40.0).unwrap()
    }

    /// Ligne droite plate vers le nord de `len` mètres.
    fn flat(len: f64) -> Route {
        let m = (len / 10.0) as usize;
        let raw: Vec<(f64, f64, f64)> = (0..=m)
            .map(|i| (45.0 + (i as f64 * 10.0 / R).to_degrees(), 6.0, 500.0))
            .collect();
        Route::from_lat_lon_ele(None, &raw, 5.0, 40.0).unwrap()
    }

    #[test]
    fn arches_at_start_climb_and_finish() {
        let r = route();
        let path = RoadPath::new(&r);
        let a = arches(&r, &path);
        let kinds: Vec<ArchKind> = a.iter().map(|a| a.kind).collect();
        // Flamme rouge à 4,996 km : trop près du sommet, pas posée.
        assert_eq!(
            kinds,
            vec![
                ArchKind::Start,
                ArchKind::ClimbStart,
                ArchKind::KmToGo,
                ArchKind::KmToGo,
                ArchKind::ClimbTop,
                ArchKind::Finish
            ]
        );
        assert!((a[1].d - 1000.0).abs() < 120.0, "{}", a[1].d);
        assert!((a[4].d - 5000.0).abs() < 120.0, "{}", a[4].d);
        assert!(
            a[1].subtitle.starts_with("4,") && a[1].subtitle.contains(" km à 5,"),
            "{}",
            a[1].subtitle
        );
        assert!(a[1].subtitle.ends_with("3e catégorie"), "{}", a[1].subtitle);
        assert_eq!(a[4].title, "GRAND PRIX DE LA MONTAGNE");
        assert_eq!((a[2].title.as_str(), a[3].title.as_str()), ("3 KM", "2 KM"));
        assert!(a[0].left > 3.0 && a[0].right > 3.0);
        assert!(a[0].heading.1 > 0.99);
        assert!((a[2].grade - 0.06).abs() < 0.01, "{}", a[2].grade);
        assert!(a[5].grade.abs() < 0.01, "{}", a[5].grade);
        let b = barriers(&path, &a, 500.0);
        // 150 m après le départ, 2 × 120 m par arche d'ascension, 300 m
        // avant l'arrivée (la route s'arrête 4 m après) ; une barrière de
        // 2,5 m de chaque côté.
        let expected = (165.0 + 240.0 + 304.0) / 2.5 * 2.0;
        assert!(
            (b.count() as f64 - expected).abs() < expected * 0.1,
            "{} contre {expected}",
            b.count()
        );
        // Motif de banderole dans le canal alpha.
        assert!(b
            .buffer
            .chunks_exact(Instances::STRIDE)
            .all(|one| one[15] > 0.0 && one[15] * 16.0 < f32::from(BANNER_DESIGNS)));
    }

    #[test]
    fn flamme_rouge_one_km_before_the_finish() {
        let r = flat(4000.0);
        let a = arches(&r, &RoadPath::new(&r));
        let kinds: Vec<ArchKind> = a.iter().map(|a| a.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ArchKind::Start,
                ArchKind::KmToGo,
                ArchKind::KmToGo,
                ArchKind::FlammeRouge,
                ArchKind::Finish
            ]
        );
        let (flamme, finish) = (&a[3], &a[4]);
        assert!(
            (finish.d - flamme.d - 1000.0).abs() < 1.0,
            "{} {}",
            flamme.d,
            finish.d
        );
        // Trop court : ni flamme rouge ni banderoles.
        let r = flat(1200.0);
        let a = arches(&r, &RoadPath::new(&r));
        let kinds: Vec<ArchKind> = a.iter().map(|a| a.kind).collect();
        assert_eq!(kinds, vec![ArchKind::Start, ArchKind::Finish]);
    }

    #[test]
    fn spectators_stand_behind_the_barriers() {
        let r = flat(2000.0);
        let path = RoadPath::new(&r);
        let a = arches(&r, &path);
        let x0 = path.samples[0].x as f32;
        let barrier_x = barriers(&path, &a, 500.0)
            .buffer
            .chunks_exact(Instances::STRIDE)
            .map(|one| (one[3] - x0).abs())
            .fold(0.0f32, f32::max);
        let s = spectators(&path, &a, 500.0);
        assert!(s.count() > 300, "{}", s.count());
        for one in s.buffer.chunks_exact(Instances::STRIDE) {
            assert!((one[3] - x0).abs() > barrier_x + 0.3, "{}", one[3] - x0);
            assert!((one[7] + 0.05).abs() < 0.2, "{}", one[7]);
        }
    }

    #[test]
    fn french_numbers() {
        assert_eq!(fr(1784.4, 0), "1 784");
        assert_eq!(fr(12.46, 1), "12,5");
        assert_eq!(fr(-3.0, 0), "-3");
    }

    #[test]
    fn climb_to_the_finish_shares_its_arch() {
        // Arrivée au sommet : pas d'arche « sommet » en plus.
        let raw: Vec<(f64, f64, f64)> = (0..=300)
            .map(|i| {
                let d = i as f64 * 10.0;
                (45.0 + (d / R).to_degrees(), 6.0, 500.0 + d * 0.08)
            })
            .collect();
        let r = Route::from_lat_lon_ele(None, &raw, 5.0, 40.0).unwrap();
        let a = arches(&r, &RoadPath::new(&r));
        let kinds: Vec<ArchKind> = a.iter().map(|a| a.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ArchKind::Start,
                ArchKind::KmToGo,
                ArchKind::FlammeRouge,
                ArchKind::Finish
            ]
        );
        assert!(a[0].subtitle.starts_with("Ascension"), "{}", a[0].subtitle);
        assert!(a[3].subtitle.starts_with("Sommet"), "{}", a[3].subtitle);
    }
}
