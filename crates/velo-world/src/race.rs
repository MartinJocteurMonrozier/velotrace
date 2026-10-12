//! Mobilier de course, comme sur les grands tours : arche gonflable au
//! départ, au pied et au sommet de chaque ascension et à l'arrivée, barrières
//! le long de la route à leurs abords.

use crate::mesh::Instances;
use crate::road::{marks, RoadPath};
use velo_core::route::Route;

/// Deux arches plus proches que ça n'en font qu'une (m).
const MERGE_M: f64 = 300.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchKind {
    Start,
    ClimbStart,
    ClimbTop,
    Finish,
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
        let summit = format!("{} m d'altitude", fr(c.top_elevation_m, 0));
        if c.start_m - start_d < MERGE_M {
            plan[0].3 = format!("Ascension : {about}");
        } else {
            plan.push((
                c.start_m,
                ArchKind::ClimbStart,
                "PIED DE L'ASCENSION".into(),
                about,
            ));
        }
        if finish_d - c.end_m < MERGE_M {
            finish_sub = format!("Sommet · {summit}");
        } else {
            plan.push((c.end_m, ArchKind::ClimbTop, "SOMMET".into(), summit));
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
    kept.into_iter()
        .filter_map(|(d, kind, title, subtitle)| {
            // Pas au milieu d'un rond-point : un peu plus loin.
            let mut d = d;
            while path.sample_at(d).is_some_and(|p| p.profile.hidden) && d < len {
                d += 10.0;
            }
            let p = path.sample_at(d.min(len))?;
            let pr = &p.profile;
            Some(Arch {
                kind,
                d,
                x: p.x,
                n: p.n,
                elevation: p.elevation,
                heading: p.tangent,
                left: pr.half_width + pr.sidewalk[0] + 0.9,
                right: pr.half_width + pr.sidewalk[1] + 0.9,
                title,
                subtitle,
            })
        })
        .collect()
}

/// Barrières de police le long des deux bords : à la sortie du départ, aux
/// abords des arches d'ascension et sur les 300 derniers mètres (tampon
/// MultiMesh, une barrière de 2,5 m par instance, dont la couleur est celle
/// de son panneau).
pub fn barriers(path: &RoadPath, arches: &[Arch], base: f64) -> Instances {
    let mut out = Instances::default();
    let ranges: Vec<(f64, f64, ArchKind)> = arches
        .iter()
        .map(|a| match a.kind {
            ArchKind::Start => (0.0, a.d + 150.0, a.kind),
            ArchKind::Finish => (a.d - 300.0, a.d + 10.0, a.kind),
            _ => (a.d - 60.0, a.d + 60.0, a.kind),
        })
        .collect();
    let palette: [[f32; 4]; 5] = [
        [0.98, 0.82, 0.10, 1.0],
        [0.10, 0.45, 0.80, 1.0],
        [0.85, 0.12, 0.12, 1.0],
        [0.15, 0.60, 0.30, 1.0],
        [0.95, 0.95, 0.95, 1.0],
    ];
    let ss = &path.samples;
    let mut count = 0usize;
    let mut t = 0.0;
    while t <= path.length() {
        let i = ss
            .partition_point(|p| p.s <= t)
            .saturating_sub(1)
            .min(ss.len() - 2);
        t += 2.5;
        let (a, b) = (&ss[i], &ss[i + 1]);
        let f = ((t - 2.5 - a.s) / (b.s - a.s).max(1e-9)).clamp(0.0, 1.0);
        let lerp = |u: f64, v: f64| u + (v - u) * f;
        let d = lerp(a.d, b.d);
        let Some(&(_, _, kind)) = ranges.iter().find(|r| (r.0..=r.1).contains(&d)) else {
            continue;
        };
        let p = if f < 0.5 { a } else { b };
        // Pas contre les piles d'une arche.
        if arches.iter().any(|a| (a.d - d).abs() < 2.0) || p.profile.hidden {
            continue;
        }
        let tan = (
            lerp(a.tangent.0, b.tangent.0),
            lerp(a.tangent.1, b.tangent.1),
        );
        let l = tan.0.hypot(tan.1).max(1e-9);
        let tan = (tan.0 / l, tan.1 / l);
        let (rx, rn) = (tan.1, -tan.0);
        let (px, pn, pe) = (
            lerp(a.x, b.x),
            lerp(a.n, b.n),
            lerp(a.elevation, b.elevation),
        );
        let yaw = tan.1.atan2(tan.0);
        let pr = &p.profile;
        for side in 0..2 {
            if pr.marks & [marks::GAP_LEFT, marks::GAP_RIGHT][side] != 0 {
                continue;
            }
            let sg = if side == 0 { -1.0 } else { 1.0 };
            let off = sg * (pr.half_width + pr.sidewalk[side] + 0.45);
            let (x, n) = (px + rx * off, pn + rn * off);
            let color = match kind {
                ArchKind::ClimbStart | ArchKind::ClimbTop => {
                    [[0.85, 0.12, 0.12, 1.0], [0.95, 0.95, 0.95, 1.0]][count % 2]
                }
                ArchKind::Start => [[0.15, 0.60, 0.30, 1.0], [0.95, 0.95, 0.95, 1.0]][count % 2],
                ArchKind::Finish => palette[(count / 2) % palette.len()],
            };
            // Sur le trottoir s'il y en a un.
            let kerb = if pr.sidewalk[side] > 0.05 { 0.14 } else { 0.0 };
            out.push([x, pe - base - 0.05 + kerb, -n], yaw, 1.0, 1.0, color);
            count += 1;
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

    #[test]
    fn arches_at_start_climb_and_finish() {
        let r = route();
        let path = RoadPath::new(&r);
        let a = arches(&r, &path);
        let kinds: Vec<ArchKind> = a.iter().map(|a| a.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ArchKind::Start,
                ArchKind::ClimbStart,
                ArchKind::ClimbTop,
                ArchKind::Finish
            ]
        );
        assert!((a[1].d - 1000.0).abs() < 120.0, "{}", a[1].d);
        assert!((a[2].d - 5000.0).abs() < 120.0, "{}", a[2].d);
        assert!(
            a[1].subtitle.starts_with("4,") && a[1].subtitle.contains(" km à 5,"),
            "{}",
            a[1].subtitle
        );
        assert!(a[0].left > 3.0 && a[0].right > 3.0);
        assert!(a[0].heading.1 > 0.99);
        let b = barriers(&path, &a, 500.0);
        // 150 m après le départ, 2 × 120 m par arche d'ascension, 300 m
        // avant l'arrivée ; une barrière de 2,5 m de chaque côté.
        let expected = (165.0 + 240.0 + 300.0) / 2.5 * 2.0;
        assert!(
            (b.count() as f64 - expected).abs() < expected * 0.1,
            "{} contre {expected}",
            b.count()
        );
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
        assert_eq!(kinds, vec![ArchKind::Start, ArchKind::Finish]);
        assert!(a[0].subtitle.starts_with("Ascension"), "{}", a[0].subtitle);
        assert!(a[1].subtitle.starts_with("Sommet"), "{}", a[1].subtitle);
    }
}
