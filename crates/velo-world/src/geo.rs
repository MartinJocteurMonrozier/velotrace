//! Projections : repère local du parcours et tuiles Web Mercator.

use velo_core::route::Route;

pub const EARTH_RADIUS_M: f64 = 6_371_008.8;
/// Circonférence équatoriale utilisée par les tuiles Web Mercator.
const MERCATOR_EARTH_M: f64 = 40_075_016.686;

/// Repère local du parcours : x vers l'est, n vers le nord, en mètres.
///
/// Identique à la projection de `Route` (équirectangulaire autour du départ),
/// pour que le décor et la route tombent exactement au même endroit.
#[derive(Debug, Clone, Copy)]
pub struct LocalProjection {
    lat0: f64,
    lon0: f64,
    cos0: f64,
}

impl LocalProjection {
    pub fn new(lat0: f64, lon0: f64) -> Self {
        Self {
            lat0,
            lon0,
            cos0: lat0.to_radians().cos(),
        }
    }

    pub fn for_route(route: &Route) -> Self {
        let p = &route.points[0];
        Self::new(p.lat, p.lon)
    }

    pub fn to_local(&self, lat: f64, lon: f64) -> (f64, f64) {
        (
            (lon - self.lon0).to_radians() * EARTH_RADIUS_M * self.cos0,
            (lat - self.lat0).to_radians() * EARTH_RADIUS_M,
        )
    }

    pub fn lon_at(&self, x: f64) -> f64 {
        self.lon0 + (x / (EARTH_RADIUS_M * self.cos0)).to_degrees()
    }

    pub fn lat_at(&self, n: f64) -> f64 {
        self.lat0 + (n / EARTH_RADIUS_M).to_degrees()
    }

    pub fn to_lat_lon(&self, x: f64, n: f64) -> (f64, f64) {
        (self.lat_at(n), self.lon_at(x))
    }

    pub fn latitude(&self) -> f64 {
        self.lat0
    }
}

/// Position en pixels de tuile Web Mercator (256 px par tuile) au zoom `z`.
/// La longitude ne donne que x et la latitude que y.
pub fn mercator_px_x(lon: f64, z: u8) -> f64 {
    (lon + 180.0) / 360.0 * 256.0 * f64::from(1u32 << z)
}

pub fn mercator_px_y(lat: f64, z: u8) -> f64 {
    let lat = lat.clamp(-85.05, 85.05).to_radians();
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0;
    y * 256.0 * f64::from(1u32 << z)
}

/// Taille d'un pixel de tuile (m) à cette latitude et ce zoom.
pub fn meters_per_px(lat: f64, z: u8) -> f64 {
    MERCATOR_EARTH_M * lat.to_radians().cos() / (256.0 * f64::from(1u32 << z))
}

/// Plus petit zoom dont les pixels mesurent au plus `mpp` mètres.
pub fn zoom_for(lat: f64, mpp: f64, max_zoom: u8) -> u8 {
    (0..=max_zoom)
        .find(|&z| meters_per_px(lat, z) <= mpp)
        .unwrap_or(max_zoom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_projection_round_trips() {
        let p = LocalProjection::new(45.05, 6.03);
        let (x, n) = p.to_local(45.06, 6.05);
        let (lat, lon) = p.to_lat_lon(x, n);
        assert!((lat - 45.06).abs() < 1e-9 && (lon - 6.05).abs() < 1e-9);
        assert!((n - 1111.95).abs() < 0.1, "{n}");
    }

    #[test]
    fn mercator_matches_known_tile() {
        // Le Bourg-d'Oisans tombe dans la tuile 13/4233/2945.
        let x = mercator_px_x(6.03, 13) / 256.0;
        let y = mercator_px_y(45.055, 13) / 256.0;
        assert_eq!((x.floor(), y.floor()), (4233.0, 2945.0));
        assert!((meters_per_px(45.0, 13) - 13.5).abs() < 0.1);
        assert_eq!(zoom_for(45.0, 2.0, 19), 16);
    }
}
