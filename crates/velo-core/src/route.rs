//! Parcours issu d'un GPX : projection locale, rééchantillonnage, pentes lissées.

use std::io::Read;

#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("GPX illisible : {0}")]
    Gpx(#[from] gpx::errors::GpxError),
    #[error("le GPX ne contient pas assez de points (au moins 2)")]
    TooShort,
}

/// Point du parcours en coordonnées locales (mètres, x vers l'est, y vers le nord).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoutePoint {
    pub lat: f64,
    pub lon: f64,
    pub x: f64,
    pub y: f64,
    pub elevation_m: f64,
    /// Distance cumulée depuis le départ.
    pub distance_m: f64,
}

#[derive(Debug, Clone)]
pub struct Route {
    pub name: Option<String>,
    pub points: Vec<RoutePoint>,
    /// Pente lissée à chaque point (fraction).
    pub grades: Vec<f64>,
}

const EARTH_RADIUS_M: f64 = 6_371_008.8;

impl Route {
    /// Lit un GPX (traces puis routes) et prépare le parcours.
    ///
    /// L'altitude vient pour l'instant du GPX ; elle sera remplacée par celle
    /// du MNT (IGN ou Copernicus) à l'import, l'altitude GPS étant bruitée.
    pub fn from_gpx<R: Read>(reader: R, step_m: f64, smoothing_m: f64) -> Result<Self, RouteError> {
        let doc = gpx::read(reader)?;
        let mut raw: Vec<(f64, f64, f64)> = Vec::new();
        for track in &doc.tracks {
            for seg in &track.segments {
                raw.extend(seg.points.iter().map(|p| {
                    let pt = p.point();
                    (pt.y(), pt.x(), p.elevation.unwrap_or(0.0))
                }));
            }
        }
        if raw.is_empty() {
            for r in &doc.routes {
                raw.extend(r.points.iter().map(|p| {
                    let pt = p.point();
                    (pt.y(), pt.x(), p.elevation.unwrap_or(0.0))
                }));
            }
        }
        let name = doc
            .tracks
            .first()
            .and_then(|t| t.name.clone())
            .or_else(|| doc.metadata.as_ref().and_then(|m| m.name.clone()));
        Self::from_lat_lon_ele(name, &raw, step_m, smoothing_m)
    }

    pub fn from_lat_lon_ele(
        name: Option<String>,
        raw: &[(f64, f64, f64)],
        step_m: f64,
        smoothing_m: f64,
    ) -> Result<Self, RouteError> {
        if raw.len() < 2 {
            return Err(RouteError::TooShort);
        }
        // Projection équirectangulaire autour du premier point : erreur
        // négligeable sur quelques centaines de km, à remplacer par UTM si besoin.
        let (lat0, lon0, _) = raw[0];
        let cos0 = lat0.to_radians().cos();
        let project = |lat: f64, lon: f64| {
            (
                (lon - lon0).to_radians() * EARTH_RADIUS_M * cos0,
                (lat - lat0).to_radians() * EARTH_RADIUS_M,
            )
        };

        let mut dense: Vec<RoutePoint> = Vec::with_capacity(raw.len());
        for &(lat, lon, ele) in raw {
            let (x, y) = project(lat, lon);
            let distance_m = match dense.last() {
                Some(prev) => prev.distance_m + (x - prev.x).hypot(y - prev.y),
                None => 0.0,
            };
            // Les points GPS dupliqués n'apportent rien.
            if dense
                .last()
                .is_some_and(|prev| distance_m - prev.distance_m < 0.01)
            {
                continue;
            }
            dense.push(RoutePoint {
                lat,
                lon,
                x,
                y,
                elevation_m: ele,
                distance_m,
            });
        }
        if dense.len() < 2 {
            return Err(RouteError::TooShort);
        }

        let total = dense.last().unwrap().distance_m;
        let n = (total / step_m).floor() as usize;
        let mut points: Vec<RoutePoint> = (0..=n)
            .map(|i| interpolate(&dense, i as f64 * step_m))
            .collect();
        // Le dernier point tombe exactement sur l'arrivée, sans doublon quasi nul.
        if total - points.last().unwrap().distance_m < step_m * 0.01 {
            points.pop();
        }
        points.push(interpolate(&dense, total));

        let mut route = Self {
            name,
            points,
            grades: Vec::new(),
        };
        route.recompute_grades(smoothing_m);
        Ok(route)
    }

    /// Recalcule les pentes après lissage de l'altitude sur `smoothing_m` mètres.
    pub fn recompute_grades(&mut self, smoothing_m: f64) {
        let pts = &self.points;
        let step = if pts.len() > 1 {
            pts[1].distance_m - pts[0].distance_m
        } else {
            1.0
        };
        let half = ((smoothing_m / step) / 2.0).round().max(1.0) as usize;
        let smooth: Vec<f64> = (0..pts.len())
            .map(|i| {
                let a = i.saturating_sub(half);
                let b = (i + half).min(pts.len() - 1);
                pts[a..=b].iter().map(|p| p.elevation_m).sum::<f64>() / (b - a + 1) as f64
            })
            .collect();
        self.grades = (0..pts.len())
            .map(|i| {
                let a = i.saturating_sub(1);
                let b = (i + 1).min(pts.len() - 1);
                let d = pts[b].distance_m - pts[a].distance_m;
                if d > 0.0 {
                    (smooth[b] - smooth[a]) / d
                } else {
                    0.0
                }
            })
            .collect();
    }

    pub fn length_m(&self) -> f64 {
        self.points.last().map_or(0.0, |p| p.distance_m)
    }

    /// Dénivelé positif cumulé (m).
    pub fn elevation_gain_m(&self) -> f64 {
        self.points
            .windows(2)
            .map(|w| (w[1].elevation_m - w[0].elevation_m).max(0.0))
            .sum()
    }

    /// Position interpolée à une distance donnée depuis le départ.
    pub fn point_at(&self, distance_m: f64) -> RoutePoint {
        interpolate(&self.points, distance_m.clamp(0.0, self.length_m()))
    }

    /// Pente lissée à une distance donnée.
    pub fn grade_at(&self, distance_m: f64) -> f64 {
        let (i, t) = locate(&self.points, distance_m.clamp(0.0, self.length_m()));
        let j = (i + 1).min(self.grades.len() - 1);
        self.grades[i] * (1.0 - t) + self.grades[j] * t
    }

    /// Ascensions du parcours : au moins 80 m de dénivelé à 3 % de moyenne,
    /// courts replats compris, du pied (là où la pente prend vraiment) au
    /// sommet.
    pub fn climbs(&self) -> Vec<Climb> {
        let pts = &self.points;
        let m = pts.len();
        if m < 3 {
            return Vec::new();
        }
        let step = (self.length_m() / (m - 1) as f64).max(0.1);
        // Altitude lissée sur ~100 m : le bruit du GPS ne fait pas de bosses.
        let r = (50.0 / step).round().max(1.0) as usize;
        let e: Vec<f64> = (0..m)
            .map(|i| {
                let (a, b) = (i.saturating_sub(r), (i + r).min(m - 1));
                pts[a..=b].iter().map(|p| p.elevation_m).sum::<f64>() / (b - a + 1) as f64
            })
            .collect();
        let d: Vec<f64> = pts.iter().map(|p| p.distance_m).collect();
        // Ça monte : au moins 2,5 % sur les 200 m qui suivent. Les replats
        // et descentes de moins de 400 m ne coupent pas l'ascension.
        let w = (200.0 / step).round().max(1.0) as usize;
        let gap = (400.0 / step).round() as usize;
        let rising = |i: usize| i + w < m && e[i + w] - e[i] >= 0.025 * (d[i + w] - d[i]);
        let mut out = Vec::new();
        let mut i = 0;
        while i < m {
            if !rising(i) {
                i += 1;
                continue;
            }
            // Le pied : là où la pente prend sur les 50 m suivants.
            let q = (w / 4).max(1);
            let mut start = i;
            while start + q < m && e[start + q] - e[start] < 0.025 * (d[start + q] - d[start]) {
                start += 1;
            }
            let mut last = i;
            while i < m && i <= last + gap {
                if rising(i) {
                    last = i;
                }
                i += 1;
            }
            let end = (last..=(last + w).min(m - 1))
                .max_by(|&a, &b| e[a].total_cmp(&e[b]))
                .unwrap_or(last);
            let c = Climb {
                start_m: d[start],
                end_m: d[end],
                gain_m: e[end] - e[start],
                top_elevation_m: e[end],
            };
            if c.gain_m >= 80.0 && c.grade() >= 0.03 && c.length_m() >= 500.0 {
                out.push(c);
            }
            i = last + 1;
        }
        out
    }
}

/// Une ascension du parcours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Climb {
    pub start_m: f64,
    pub end_m: f64,
    pub gain_m: f64,
    pub top_elevation_m: f64,
}

impl Climb {
    pub fn length_m(&self) -> f64 {
        self.end_m - self.start_m
    }

    /// Pente moyenne (fraction).
    pub fn grade(&self) -> f64 {
        self.gain_m / self.length_m().max(1.0)
    }
}

fn locate(points: &[RoutePoint], d: f64) -> (usize, f64) {
    let i = points
        .partition_point(|p| p.distance_m <= d)
        .saturating_sub(1);
    let i = i.min(points.len() - 2);
    let (a, b) = (&points[i], &points[i + 1]);
    let span = b.distance_m - a.distance_m;
    let t = if span > 0.0 {
        ((d - a.distance_m) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (i, t)
}

fn interpolate(points: &[RoutePoint], d: f64) -> RoutePoint {
    let (i, t) = locate(points, d);
    let (a, b) = (&points[i], &points[i + 1]);
    let lerp = |u: f64, v: f64| u + (v - u) * t;
    RoutePoint {
        lat: lerp(a.lat, b.lat),
        lon: lerp(a.lon, b.lon),
        x: lerp(a.x, b.x),
        y: lerp(a.y, b.y),
        elevation_m: lerp(a.elevation_m, b.elevation_m),
        distance_m: d,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Montée rectiligne vers le nord : 1 km à 5 %.
    fn climb_gpx() -> String {
        let mut s = String::from(
            r#"<?xml version="1.0"?><gpx version="1.1" creator="test" xmlns="http://www.topografix.com/GPX/1/1"><trk><name>Test</name><trkseg>"#,
        );
        for i in 0..=100 {
            let d = i as f64 * 10.0;
            let lat = 45.0 + (d / EARTH_RADIUS_M).to_degrees();
            s += &format!(
                r#"<trkpt lat="{lat}" lon="6.0"><ele>{}</ele></trkpt>"#,
                500.0 + d * 0.05
            );
        }
        s + "</trkseg></trk></gpx>"
    }

    #[test]
    fn reads_gpx_and_resamples() {
        let r = Route::from_gpx(climb_gpx().as_bytes(), 5.0, 40.0).unwrap();
        assert_eq!(r.name.as_deref(), Some("Test"));
        assert!((r.length_m() - 1000.0).abs() < 0.5, "{}", r.length_m());
        assert_eq!(r.points.len(), 201);
        assert!((r.elevation_gain_m() - 50.0).abs() < 0.5);
    }

    #[test]
    fn grade_is_five_percent() {
        let r = Route::from_gpx(climb_gpx().as_bytes(), 5.0, 40.0).unwrap();
        assert!(
            (r.grade_at(500.0) - 0.05).abs() < 1e-3,
            "{}",
            r.grade_at(500.0)
        );
        let p = r.point_at(250.0);
        assert!((p.y - 250.0).abs() < 0.5 && p.x.abs() < 1e-6);
    }

    #[test]
    fn finds_climbs_and_ignores_bumps() {
        // 2 km de plat, 5 km à 7 %, 1 km de descente à 3 %, 3 km à 4 %,
        // une bosse de 30 m, 1 km de plat.
        let profile = |d: f64| -> f64 {
            let mut e = 500.0;
            let seg = |d: f64, a: f64, b: f64| (d.min(b) - a).max(0.0);
            e += seg(d, 2000.0, 7000.0) * 0.07;
            e -= seg(d, 7000.0, 8000.0) * 0.03;
            e += seg(d, 8000.0, 11000.0) * 0.04;
            e += seg(d, 11500.0, 12000.0) * 0.06 - seg(d, 12000.0, 12500.0) * 0.06;
            e
        };
        let raw: Vec<(f64, f64, f64)> = (0..=1350)
            .map(|i| {
                let d = i as f64 * 10.0;
                (45.0 + (d / EARTH_RADIUS_M).to_degrees(), 6.0, profile(d))
            })
            .collect();
        let r = Route::from_lat_lon_ele(None, &raw, 5.0, 40.0).unwrap();
        let c = r.climbs();
        assert_eq!(c.len(), 2, "{c:?}");
        assert!((c[0].start_m - 2000.0).abs() < 120.0, "{c:?}");
        assert!((c[0].end_m - 7000.0).abs() < 120.0, "{c:?}");
        assert!((c[0].grade() - 0.07).abs() < 0.005, "{c:?}");
        assert!((c[1].start_m - 8000.0).abs() < 120.0, "{c:?}");
        assert!((c[1].end_m - 11000.0).abs() < 120.0, "{c:?}");
        assert!((c[1].top_elevation_m - 945.0).abs() < 8.0, "{c:?}");
    }

    #[test]
    fn rejects_empty_gpx() {
        let gpx = r#"<?xml version="1.0"?><gpx version="1.1" creator="t" xmlns="http://www.topografix.com/GPX/1/1"></gpx>"#;
        assert!(matches!(
            Route::from_gpx(gpx.as_bytes(), 5.0, 40.0),
            Err(RouteError::TooShort)
        ));
    }
}
