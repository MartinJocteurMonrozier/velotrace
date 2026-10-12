//! Données OpenStreetMap au format JSON d'Overpass (`out geom`) : forêts,
//! eau, bâtiments, routes, cours d'eau et occupation du sol.

use crate::geo::LocalProjection;
use serde_json::Value;

/// Occupation du sol, dans l'ordre où on la peint (la suivante recouvre).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Cover {
    None = 0,
    Meadow,
    Farmland,
    Orchard,
    Urban,
    Rock,
    Glacier,
    Scrub,
    ForestBroad,
    ForestMixed,
    ForestNeedle,
    Water,
    RoadUnpaved,
    RoadPaved,
    Building,
}

impl Cover {
    pub fn from_u8(v: u8) -> Self {
        use Cover::*;
        [
            None,
            Meadow,
            Farmland,
            Orchard,
            Urban,
            Rock,
            Glacier,
            Scrub,
            ForestBroad,
            ForestMixed,
            ForestNeedle,
            Water,
            RoadUnpaved,
            RoadPaved,
            Building,
        ]
        .get(v as usize)
        .copied()
        .unwrap_or(None)
    }

    pub fn is_forest(self) -> bool {
        matches!(
            self,
            Cover::ForestBroad | Cover::ForestMixed | Cover::ForestNeedle
        )
    }

    /// Là où aucun arbre ne pousse.
    pub fn blocks_trees(self) -> bool {
        matches!(
            self,
            Cover::Water | Cover::RoadPaved | Cover::RoadUnpaved | Cover::Building | Cover::Glacier
        )
    }
}

pub type Ring = Vec<(f64, f64)>;

/// Surface (forêt, lac, prairie…) : anneaux remplis en pair-impair, ce qui
/// gère les trous des multipolygones (clairières).
#[derive(Debug, Clone)]
pub struct Area {
    pub cover: Cover,
    pub rings: Vec<Ring>,
    /// Lac ou étang (on pose un plan d'eau), par opposition au lit d'une rivière.
    pub still_water: bool,
    pub bbox: BBox,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineKind {
    Road { paved: bool, major: bool },
    Path,
    Waterway,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub kind: LineKind,
    pub width_m: f64,
    pub points: Vec<(f64, f64)>,
    pub bbox: BBox,
}

#[derive(Debug, Clone)]
pub struct Building {
    pub id: u64,
    pub ring: Ring,
    pub height_m: f64,
    pub bbox: BBox,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBox {
    pub min: (f64, f64),
    pub max: (f64, f64),
}

impl BBox {
    fn of(points: impl IntoIterator<Item = (f64, f64)>) -> Self {
        let mut b = BBox {
            min: (f64::MAX, f64::MAX),
            max: (f64::MIN, f64::MIN),
        };
        for (x, y) in points {
            b.min = (b.min.0.min(x), b.min.1.min(y));
            b.max = (b.max.0.max(x), b.max.1.max(y));
        }
        b
    }

    pub fn intersects(&self, min: (f64, f64), max: (f64, f64)) -> bool {
        self.min.0 <= max.0 && self.max.0 >= min.0 && self.min.1 <= max.1 && self.max.1 >= min.1
    }

    pub fn center(&self) -> (f64, f64) {
        (
            (self.min.0 + self.max.0) / 2.0,
            (self.min.1 + self.max.1) / 2.0,
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct OsmData {
    pub areas: Vec<Area>,
    pub lines: Vec<Line>,
    pub buildings: Vec<Building>,
}

impl OsmData {
    pub fn is_empty(&self) -> bool {
        self.areas.is_empty() && self.lines.is_empty() && self.buildings.is_empty()
    }

    /// Lit une réponse Overpass (`[out:json] … out geom;`).
    pub fn from_overpass_json(text: &str, proj: &LocalProjection) -> Result<Self, String> {
        let v: Value =
            serde_json::from_str(text).map_err(|e| format!("JSON OSM illisible : {e}"))?;
        let elements = v
            .get("elements")
            .and_then(Value::as_array)
            .ok_or("JSON OSM sans « elements »")?;
        let mut data = OsmData::default();
        for el in elements {
            let tags = el.get("tags").cloned().unwrap_or(Value::Null);
            match el.get("type").and_then(Value::as_str) {
                Some("way") => {
                    let Some(pts) = geometry(el.get("geometry"), proj) else {
                        continue;
                    };
                    let id = el.get("id").and_then(Value::as_u64).unwrap_or(0);
                    data.add_way(id, &tags, pts);
                }
                Some("relation") => {
                    let (outer, inner) = relation_rings(el, proj);
                    if outer.is_empty() {
                        continue;
                    }
                    if let Some((cover, still)) = area_cover(&tags) {
                        let rings: Vec<Ring> = outer.into_iter().chain(inner).collect();
                        let bbox = BBox::of(rings.iter().flatten().copied());
                        data.areas.push(Area {
                            cover,
                            rings,
                            still_water: still,
                            bbox,
                        });
                    } else if is_building(&tags) {
                        let id = el.get("id").and_then(Value::as_u64).unwrap_or(0);
                        for ring in outer {
                            data.push_building(id, &tags, ring);
                        }
                    }
                }
                _ => {}
            }
        }
        // Les grandes surfaces d'abord, pour que les petites (clairières,
        // prés) restent visibles par-dessus à priorité égale.
        data.areas.sort_by(|a, b| {
            a.cover.cmp(&b.cover).then(
                area_of(b)
                    .partial_cmp(&area_of(a))
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });
        Ok(data)
    }

    fn add_way(&mut self, id: u64, tags: &Value, pts: Vec<(f64, f64)>) {
        let closed = pts.len() >= 4 && pts.first() == pts.last();
        if closed && is_building(tags) {
            self.push_building(id, tags, pts);
            return;
        }
        if let Some((kind, width)) = line_kind(tags) {
            let bbox = BBox::of(pts.iter().copied());
            self.lines.push(Line {
                kind,
                width_m: width,
                points: pts,
                bbox,
            });
            return;
        }
        if closed {
            if let Some((cover, still)) = area_cover(tags) {
                let bbox = BBox::of(pts.iter().copied());
                self.areas.push(Area {
                    cover,
                    rings: vec![pts],
                    still_water: still,
                    bbox,
                });
            }
        }
    }

    fn push_building(&mut self, id: u64, tags: &Value, ring: Ring) {
        if ring.len() < 4 {
            return;
        }
        let bbox = BBox::of(ring.iter().copied());
        self.buildings.push(Building {
            id,
            height_m: building_height(tags),
            ring,
            bbox,
        });
    }

    /// Requête Overpass pour le couloir autour du parcours (points lat/lon).
    pub fn overpass_query(path: &[(f64, f64)], radius_m: f64) -> String {
        let coords: Vec<String> = path
            .iter()
            .map(|(lat, lon)| format!("{lat:.5},{lon:.5}"))
            .collect();
        let around = format!("(around:{radius_m:.0},{})", coords.join(","));
        format!(
            "[out:json][timeout:180];(\
             way[building]{a};\
             way[highway]{a};\
             way[waterway~\"^(river|stream|canal|ditch|drain)$\"]{a};\
             way[natural~\"^(wood|water|scrub|heath|grassland|bare_rock|scree|glacier)$\"]{a};\
             way[landuse~\"^(forest|meadow|grass|farmland|orchard|vineyard|residential|commercial|industrial|retail|reservoir)$\"]{a};\
             relation[type=multipolygon][natural~\"^(wood|water|scrub|bare_rock|scree|glacier)$\"]{a};\
             relation[type=multipolygon][landuse~\"^(forest|meadow|farmland|residential|reservoir)$\"]{a};\
             );out geom qt;",
            a = around
        )
    }
}

fn area_of(a: &Area) -> f64 {
    (a.bbox.max.0 - a.bbox.min.0) * (a.bbox.max.1 - a.bbox.min.1)
}

fn tag<'a>(tags: &'a Value, key: &str) -> Option<&'a str> {
    tags.get(key).and_then(Value::as_str)
}

fn geometry(g: Option<&Value>, proj: &LocalProjection) -> Option<Vec<(f64, f64)>> {
    let arr = g?.as_array()?;
    let pts: Vec<(f64, f64)> = arr
        .iter()
        .filter_map(|p| {
            let lat = p.get("lat")?.as_f64()?;
            let lon = p.get("lon")?.as_f64()?;
            Some(proj.to_local(lat, lon))
        })
        .collect();
    (pts.len() >= 2).then_some(pts)
}

/// Assemble les chemins membres d'une relation en anneaux fermés.
fn relation_rings(el: &Value, proj: &LocalProjection) -> (Vec<Ring>, Vec<Ring>) {
    let mut outer = Vec::new();
    let mut inner = Vec::new();
    for m in el
        .get("members")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if m.get("type").and_then(Value::as_str) != Some("way") {
            continue;
        }
        let Some(pts) = geometry(m.get("geometry"), proj) else {
            continue;
        };
        match m.get("role").and_then(Value::as_str) {
            Some("inner") => inner.push(pts),
            _ => outer.push(pts),
        }
    }
    (join_rings(outer), join_rings(inner))
}

fn join_rings(mut parts: Vec<Vec<(f64, f64)>>) -> Vec<Ring> {
    let same = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 0.05 && (a.1 - b.1).abs() < 0.05;
    let mut rings = Vec::new();
    while let Some(mut cur) = parts.pop() {
        loop {
            if cur.len() >= 3 && same(cur[0], *cur.last().unwrap()) {
                break;
            }
            let end = *cur.last().unwrap();
            let Some(i) = parts
                .iter()
                .position(|p| same(p[0], end) || same(*p.last().unwrap(), end))
            else {
                break;
            };
            let mut next = parts.swap_remove(i);
            if !same(next[0], end) {
                next.reverse();
            }
            cur.extend(next.into_iter().skip(1));
        }
        if cur.len() >= 4 {
            rings.push(cur);
        }
    }
    rings
}

fn is_building(tags: &Value) -> bool {
    tag(tags, "building").is_some_and(|b| b != "no")
}

fn area_cover(tags: &Value) -> Option<(Cover, bool)> {
    let leaf = || match tag(tags, "leaf_type") {
        Some("needleleaved") => Cover::ForestNeedle,
        Some("broadleaved") => Cover::ForestBroad,
        _ => Cover::ForestMixed,
    };
    if let Some(n) = tag(tags, "natural") {
        let c = match n {
            "wood" => leaf(),
            "water" => {
                let still = !matches!(
                    tag(tags, "water"),
                    Some("river" | "stream" | "canal" | "stream_pool")
                );
                return Some((Cover::Water, still));
            }
            "scrub" | "heath" => Cover::Scrub,
            "grassland" => Cover::Meadow,
            "bare_rock" | "scree" => Cover::Rock,
            "glacier" => Cover::Glacier,
            _ => return None,
        };
        return Some((c, false));
    }
    if let Some(l) = tag(tags, "landuse") {
        let c = match l {
            "forest" => leaf(),
            "meadow" | "grass" => Cover::Meadow,
            "farmland" => Cover::Farmland,
            "orchard" | "vineyard" => Cover::Orchard,
            "residential" | "commercial" | "industrial" | "retail" => Cover::Urban,
            "reservoir" | "basin" => return Some((Cover::Water, true)),
            _ => return None,
        };
        return Some((c, false));
    }
    if tag(tags, "waterway") == Some("riverbank") {
        return Some((Cover::Water, false));
    }
    None
}

fn line_kind(tags: &Value) -> Option<(LineKind, f64)> {
    if tag(tags, "area") == Some("yes") {
        return None;
    }
    if let Some(h) = tag(tags, "highway") {
        let unpaved_surface = matches!(
            tag(tags, "surface"),
            Some(
                "gravel"
                    | "dirt"
                    | "ground"
                    | "unpaved"
                    | "grass"
                    | "fine_gravel"
                    | "compacted"
                    | "earth"
            )
        );
        let (width, major) = match h {
            "motorway" | "trunk" | "primary" | "motorway_link" | "trunk_link" | "primary_link" => {
                (9.0, true)
            }
            "secondary" | "secondary_link" => (7.5, true),
            "tertiary" | "tertiary_link" => (6.5, true),
            "unclassified" | "residential" | "living_street" | "road" => (5.5, false),
            "service" => (3.5, false),
            "track" => (3.0, false),
            "path" | "footway" | "cycleway" | "bridleway" | "steps" | "pedestrian" => {
                return Some((LineKind::Path, 1.5))
            }
            _ => return None,
        };
        let paved = h != "track" && !unpaved_surface;
        let width = tag(tags, "width")
            .and_then(parse_meters)
            .filter(|w| (1.0..30.0).contains(w))
            .unwrap_or(width);
        return Some((LineKind::Road { paved, major }, width));
    }
    let w = match tag(tags, "waterway")? {
        "river" => 14.0,
        "canal" => 8.0,
        "stream" => 3.0,
        "ditch" | "drain" => 1.5,
        _ => return None,
    };
    Some((LineKind::Waterway, w))
}

fn parse_meters(s: &str) -> Option<f64> {
    s.trim()
        .trim_end_matches('m')
        .trim()
        .replace(',', ".")
        .parse()
        .ok()
}

fn building_height(tags: &Value) -> f64 {
    if let Some(h) = tag(tags, "height").and_then(parse_meters) {
        if (2.0..300.0).contains(&h) {
            return h;
        }
    }
    if let Some(l) = tag(tags, "building:levels").and_then(|s| s.trim().parse::<f64>().ok()) {
        if (1.0..80.0).contains(&l) {
            return l * 3.0 + 1.0;
        }
    }
    match tag(tags, "building").unwrap_or("yes") {
        "garage" | "garages" | "shed" | "hut" | "cabin" | "roof" | "carport" => 3.0,
        "apartments" | "hotel" | "commercial" | "office" => 12.0,
        "church" | "cathedral" | "chapel" => 14.0,
        "industrial" | "warehouse" | "retail" | "supermarket" => 8.0,
        _ => 6.5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"elements":[
      {"type":"way","id":1,"tags":{"building":"house","building:levels":"2"},
       "geometry":[{"lat":45.0,"lon":6.0},{"lat":45.0001,"lon":6.0},{"lat":45.0001,"lon":6.0001},{"lat":45.0,"lon":6.0}]},
      {"type":"way","id":2,"tags":{"highway":"track"},
       "geometry":[{"lat":45.0,"lon":6.0},{"lat":45.001,"lon":6.0}]},
      {"type":"way","id":3,"tags":{"landuse":"forest","leaf_type":"needleleaved"},
       "geometry":[{"lat":45.0,"lon":6.0},{"lat":45.01,"lon":6.0},{"lat":45.01,"lon":6.01},{"lat":45.0,"lon":6.0}]},
      {"type":"relation","id":4,"tags":{"type":"multipolygon","natural":"water"},"members":[
        {"type":"way","role":"outer","geometry":[{"lat":45.0,"lon":6.0},{"lat":45.001,"lon":6.0}]},
        {"type":"way","role":"outer","geometry":[{"lat":45.0,"lon":6.001},{"lat":45.001,"lon":6.0}]},
        {"type":"way","role":"outer","geometry":[{"lat":45.0,"lon":6.001},{"lat":45.0,"lon":6.0}]}]}
    ]}"#;

    #[test]
    fn classifies_features() {
        let proj = LocalProjection::new(45.0, 6.0);
        let d = OsmData::from_overpass_json(SAMPLE, &proj).unwrap();
        assert_eq!(d.buildings.len(), 1);
        assert_eq!(d.buildings[0].height_m, 7.0);
        assert_eq!(d.lines.len(), 1);
        assert_eq!(
            d.lines[0].kind,
            LineKind::Road {
                paved: false,
                major: false
            }
        );
        assert_eq!(d.areas.len(), 2);
        assert_eq!(d.areas[0].cover, Cover::ForestNeedle);
        let lake = &d.areas[1];
        assert_eq!(lake.cover, Cover::Water);
        assert!(lake.still_water);
        // Les trois morceaux forment un seul anneau fermé.
        assert_eq!(lake.rings.len(), 1);
        assert_eq!(lake.rings[0].len(), 4);
    }

    #[test]
    fn reads_the_alpe_fixture() {
        let text = include_str!("../../../data/osm-alpe-d-huez.json");
        let proj = LocalProjection::new(45.0548, 6.0314);
        let d = OsmData::from_overpass_json(text, &proj).unwrap();
        assert!(d.buildings.len() > 2000, "{}", d.buildings.len());
        assert!(d.lines.len() > 1000);
        assert!(d.areas.iter().any(|a| a.cover.is_forest()));
    }

    #[test]
    fn query_mentions_the_corridor() {
        let q = OsmData::overpass_query(&[(45.0, 6.0), (45.1, 6.1)], 800.0);
        assert!(q.contains("around:800,45.00000,6.00000,45.10000,6.10000"));
        assert!(q.ends_with("out geom qt;"));
    }
}
