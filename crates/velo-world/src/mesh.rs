//! Maillages prêts pour Godot (x est, y altitude, z sud), relatifs au
//! centre de leur morceau de décor.

/// Tableaux d'un `ArrayMesh` Godot. Les faces avant tournent dans le sens
/// horaire vues de l'extérieur, comme l'attend Godot.
#[derive(Debug, Clone, Default)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Second jeu d'UV (facultatif : vide ou un par sommet).
    pub uv2s: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    /// CUSTOM0 en octets (facultatif : vide ou un par sommet).
    pub customs: Vec<[u8; 4]>,
    pub indices: Vec<i32>,
}

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(a: V3) -> V3 {
    let l = dot(a, a).sqrt().max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

fn f3(a: V3) -> [f32; 3] {
    [a[0] as f32, a[1] as f32, a[2] as f32]
}

impl MeshData {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Face plane colorée (polygone convexe ou triangle) dont la normale est
    /// orientée du côté de `outward`.
    pub fn flat_face(&mut self, pts: &[V3], outward: V3, color: [f32; 4]) {
        if pts.len() < 3 {
            return;
        }
        let mut n = normalize(cross(sub(pts[1], pts[0]), sub(pts[2], pts[0])));
        if dot(n, outward) < 0.0 {
            n = [-n[0], -n[1], -n[2]];
        }
        let base = self.positions.len() as i32;
        for &p in pts {
            self.positions.push(f3(p));
            self.normals.push(f3(n));
            self.colors.push(color);
        }
        for k in 1..pts.len() - 1 {
            self.push_tri(base, base + k as i32, base + k as i32 + 1, n);
        }
    }

    /// Ajoute un triangle en corrigeant son sens pour que `normal` soit l'avant.
    pub fn push_tri(&mut self, a: i32, b: i32, c: i32, normal: V3) {
        let p = |i: i32| {
            let v = self.positions[i as usize];
            [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])]
        };
        let g = cross(sub(p(b), p(a)), sub(p(c), p(a)));
        // Godot : sens horaire = avant, donc produit vectoriel opposé à la normale.
        if dot(g, normal) <= 0.0 {
            self.indices.extend([a, b, c]);
        } else {
            self.indices.extend([a, c, b]);
        }
    }
}

/// Grille de relief de `cells` × `cells` cases. `height(i, j)` est l'altitude
/// relative du nœud (i vers l'est, j vers le sud), définie de -1 à cells + 1
/// pour calculer des normales continues d'un morceau à l'autre. `skirt_m` ajoute
/// une jupe verticale qui masque les fentes avec un voisin moins détaillé.
pub fn terrain_grid(
    extent_m: f64,
    cells: usize,
    height: &dyn Fn(i64, i64) -> f64,
    skirt_m: f64,
) -> MeshData {
    let n = cells + 1;
    let step = extent_m / cells as f64;
    let half = extent_m / 2.0;
    // Hauteurs avec une bordure d'un nœud.
    let w = n + 2;
    let mut h = vec![0.0; w * w];
    for j in 0..w {
        for i in 0..w {
            h[j * w + i] = height(i as i64 - 1, j as i64 - 1);
        }
    }
    let at = |i: usize, j: usize| h[(j + 1) * w + (i + 1)];
    let mut m = MeshData::default();
    for j in 0..n {
        for i in 0..n {
            let y = at(i, j);
            let dhdx = (h[(j + 1) * w + i + 2] - h[(j + 1) * w + i]) / (2.0 * step);
            let dhdz = (h[(j + 2) * w + i + 1] - h[j * w + i + 1]) / (2.0 * step);
            m.positions
                .push(f3([i as f64 * step - half, y, j as f64 * step - half]));
            m.normals.push(f3(normalize([-dhdx, 1.0, -dhdz])));
            m.uvs
                .push([i as f32 / cells as f32, j as f32 / cells as f32]);
        }
    }
    for j in 0..cells {
        for i in 0..cells {
            let a = (j * n + i) as i32;
            let n = n as i32;
            m.indices.extend([a, a + 1, a + n, a + 1, a + n + 1, a + n]);
        }
    }
    if skirt_m > 0.0 {
        // Bords dans l'ordre : nord, est, sud, ouest.
        let edges: [Vec<usize>; 4] = [
            (0..n).collect(),
            (0..n).map(|j| j * n + cells).collect(),
            (0..n).map(|i| cells * n + i).collect(),
            (0..n).map(|j| j * n).collect(),
        ];
        let outward = [
            [0.0, 0.0, -1.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [-1.0, 0.0, 0.0],
        ];
        for (edge, out) in edges.iter().zip(outward) {
            let base = m.positions.len() as i32;
            for &k in edge {
                let p = m.positions[k];
                m.positions.push([p[0], p[1] - skirt_m as f32, p[2]]);
                m.normals.push(m.normals[k]);
                m.uvs.push(m.uvs[k]);
            }
            for s in 0..edge.len() - 1 {
                let (top0, top1) = (edge[s] as i32, edge[s + 1] as i32);
                let (bot0, bot1) = (base + s as i32, base + s as i32 + 1);
                m.push_tri(top0, top1, bot0, out);
                m.push_tri(top1, bot1, bot0, out);
            }
        }
    }
    m
}

/// Ruban drapé sur le sol (route, chemin, rivière). `ground(x, z)` donne
/// l'altitude relative au point ; `lift_m` le décolle légèrement du sol.
pub fn ribbon(
    m: &mut MeshData,
    points: &[(f64, f64)],
    width_m: f64,
    lift_m: f64,
    ground: &dyn Fn(f64, f64) -> f64,
    color: [f32; 4],
) {
    if points.len() < 2 {
        return;
    }
    let base = m.positions.len() as i32;
    let half = width_m / 2.0;
    for (k, &(x, z)) in points.iter().enumerate() {
        let (px, pz) = points[k.saturating_sub(1)];
        let (nx, nz) = points[(k + 1).min(points.len() - 1)];
        let (tx, tz) = (nx - px, nz - pz);
        let l = (tx * tx + tz * tz).sqrt().max(1e-9);
        // Normale horizontale au tracé.
        let (rx, rz) = (-tz / l * half, tx / l * half);
        for (sx, sz) in [(x - rx, z - rz), (x + rx, z + rz)] {
            m.positions.push(f3([sx, ground(sx, sz) + lift_m, sz]));
            m.normals.push([0.0, 1.0, 0.0]);
            m.colors.push(color);
        }
    }
    for k in 0..points.len() as i32 - 1 {
        let a = base + k * 2;
        m.push_tri(a, a + 2, a + 1, [0.0, 1.0, 0.0]);
        m.push_tri(a + 1, a + 2, a + 3, [0.0, 1.0, 0.0]);
    }
}

/// Route drapée sur le sol, pour le shader de chaussée : UV (écart à l'axe,
/// abscisse), UV2 (demi-largeur, voies ; 0 voie pour une piste non revêtue)
/// et marquages dans l'alpha de la couleur (voir `road::push_strip`).
pub fn road_ribbon(
    m: &mut MeshData,
    points: &[(f64, f64)],
    width_m: f64,
    lift_m: f64,
    ground: &dyn Fn(f64, f64) -> f64,
    lanes: f32,
    marks: u8,
) {
    let first = m.positions.len();
    ribbon(
        m,
        points,
        width_m,
        lift_m,
        ground,
        [0.0, 0.0, 0.0, f32::from(marks) / 255.0],
    );
    if m.positions.len() == first {
        return;
    }
    let half = (width_m / 2.0) as f32;
    let mut s = 0.0;
    for (k, &(x, z)) in points.iter().enumerate() {
        if k > 0 {
            let (px, pz) = points[k - 1];
            s += ((x - px).powi(2) + (z - pz).powi(2)).sqrt();
        }
        for u in [-half, half] {
            m.uvs.push([u, s as f32]);
            m.uv2s.push([half, lanes]);
        }
    }
}

/// Surface plane horizontale (plan d'eau) à l'altitude `y`, avec trous.
pub fn flat_polygon(m: &mut MeshData, rings: &[Vec<(f64, f64)>], y: f64, color: [f32; 4]) {
    let mut coords = Vec::new();
    let mut holes = Vec::new();
    for (r, ring) in rings.iter().enumerate() {
        if r > 0 {
            holes.push(coords.len() / 2);
        }
        // L'anneau OSM est fermé : on retire le doublon final.
        for &(x, z) in &ring[..ring.len().saturating_sub(1)] {
            coords.extend([x, z]);
        }
    }
    let Ok(tris) = earcutr::earcut(&coords, &holes, 2) else {
        return;
    };
    let base = m.positions.len() as i32;
    for p in coords.chunks_exact(2) {
        m.positions.push(f3([p[0], y, p[1]]));
        m.normals.push([0.0, 1.0, 0.0]);
        m.colors.push(color);
    }
    for t in tris.chunks_exact(3) {
        m.push_tri(
            base + t[0] as i32,
            base + t[1] as i32,
            base + t[2] as i32,
            [0.0, 1.0, 0.0],
        );
    }
}

/// Bâtiment : murs extrudés et toit plat, ou à deux pans pour une petite
/// maison rectangulaire. `ring` est fermé, en coordonnées (x, z) relatives.
pub fn building(
    m: &mut MeshData,
    ring: &[(f64, f64)],
    base_y: f64,
    top_y: f64,
    wall: [f32; 4],
    roof: [f32; 4],
) {
    let pts = &ring[..ring.len() - 1];
    if pts.len() < 3 {
        return;
    }
    let (cx, cz) = (
        pts.iter().map(|p| p.0).sum::<f64>() / pts.len() as f64,
        pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64,
    );
    let mut gable = None;
    if pts.len() == 4 {
        let d = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1);
        let (l01, l12) = (d(pts[0], pts[1]), d(pts[1], pts[2]));
        let area = l01 * l12;
        if area < 450.0 {
            // Faîtage parallèle aux longs côtés.
            let p: Vec<(f64, f64)> = if l01 >= l12 {
                pts.to_vec()
            } else {
                vec![pts[1], pts[2], pts[3], pts[0]]
            };
            let short = l01.min(l12);
            gable = Some((p, short * 0.35));
        }
    }
    for k in 0..pts.len() {
        let (a, b) = (pts[k], pts[(k + 1) % pts.len()]);
        let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        let out = [mid.0 - cx, 0.0, mid.1 - cz];
        m.flat_face(
            &[
                [a.0, base_y, a.1],
                [b.0, base_y, b.1],
                [b.0, top_y, b.1],
                [a.0, top_y, a.1],
            ],
            out,
            wall,
        );
    }
    match gable {
        Some((p, rise)) => {
            let ridge_y = top_y + rise;
            let mid = |a: (f64, f64), b: (f64, f64)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            // Côtés longs : p0-p1 et p2-p3 ; pignons sur p1-p2 et p3-p0.
            let r1 = mid(p[1], p[2]);
            let r0 = mid(p[3], p[0]);
            let v = |q: (f64, f64), y: f64| [q.0, y, q.1];
            let side = |q: (f64, f64)| [q.0 - cx, 0.0, q.1 - cz];
            let up = |q: (f64, f64)| {
                let s = side(q);
                [s[0], 3.0, s[2]]
            };
            m.flat_face(
                &[
                    v(p[0], top_y),
                    v(p[1], top_y),
                    v(r1, ridge_y),
                    v(r0, ridge_y),
                ],
                up(mid(p[0], p[1])),
                roof,
            );
            m.flat_face(
                &[
                    v(p[2], top_y),
                    v(p[3], top_y),
                    v(r0, ridge_y),
                    v(r1, ridge_y),
                ],
                up(mid(p[2], p[3])),
                roof,
            );
            m.flat_face(
                &[v(p[1], top_y), v(p[2], top_y), v(r1, ridge_y)],
                side(r1),
                wall,
            );
            m.flat_face(
                &[v(p[3], top_y), v(p[0], top_y), v(r0, ridge_y)],
                side(r0),
                wall,
            );
        }
        None => {
            let mut coords = Vec::with_capacity(pts.len() * 2);
            for &(x, z) in pts {
                coords.extend([x, z]);
            }
            let Ok(tris) = earcutr::earcut(&coords, &[], 2) else {
                return;
            };
            let base = m.positions.len() as i32;
            for &(x, z) in pts {
                m.positions.push(f3([x, top_y, z]));
                m.normals.push([0.0, 1.0, 0.0]);
                m.colors.push(roof);
            }
            for t in tris.chunks_exact(3) {
                m.push_tri(
                    base + t[0] as i32,
                    base + t[1] as i32,
                    base + t[2] as i32,
                    [0.0, 1.0, 0.0],
                );
            }
        }
    }
}

/// Instances d'arbres au format du tampon `MultiMesh` de Godot
/// (transformée 3×4 ligne par ligne, puis couleur RGBA).
#[derive(Debug, Clone, Default)]
pub struct Instances {
    pub buffer: Vec<f32>,
}

impl Instances {
    pub const STRIDE: usize = 16;

    pub fn count(&self) -> usize {
        self.buffer.len() / Self::STRIDE
    }

    pub fn push(&mut self, pos: [f64; 3], yaw: f64, width: f64, height: f64, color: [f32; 4]) {
        let (s, c) = yaw.sin_cos();
        let (w, h) = (width as f32, height as f32);
        let (s, c) = (s as f32, c as f32);
        // Colonnes de la base : X = (c, 0, -s)·w, Y = (0, 1, 0)·h, Z = (s, 0, c)·w.
        self.buffer.extend([
            c * w,
            0.0,
            s * w,
            pos[0] as f32,
            0.0,
            h,
            0.0,
            pos[1] as f32,
            -s * w,
            0.0,
            c * w,
            pos[2] as f32,
        ]);
        self.buffer.extend(color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn front_normal(m: &MeshData, t: usize) -> V3 {
        let p = |k: usize| {
            let v = m.positions[m.indices[t * 3 + k] as usize];
            [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])]
        };
        let g = cross(sub(p(1), p(0)), sub(p(2), p(0)));
        [-g[0], -g[1], -g[2]]
    }

    #[test]
    fn terrain_faces_up_and_skirts_face_out() {
        let m = terrain_grid(100.0, 4, &|_, _| 0.0, 5.0);
        assert_eq!(m.positions.len(), 25 + 4 * 5);
        for t in 0..32 {
            assert!(front_normal(&m, t)[1] > 0.0, "face {t}");
        }
        // Première jupe : bord nord, z négatif.
        assert!(front_normal(&m, 32)[2] < 0.0);
    }

    #[test]
    fn building_faces_point_outward() {
        let mut m = MeshData::default();
        let ring = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 6.0), (0.0, 6.0), (0.0, 0.0)];
        building(&mut m, &ring, 0.0, 5.0, [1.0; 4], [0.5; 4]);
        for t in 0..m.indices.len() / 3 {
            let n = front_normal(&m, t);
            let c = m.positions[m.indices[t * 3] as usize];
            let c2 = m.positions[m.indices[t * 3 + 1] as usize];
            let c3 = m.positions[m.indices[t * 3 + 2] as usize];
            let center = [
                f64::from(c[0] + c2[0] + c3[0]) / 3.0 - 5.0,
                f64::from(c[1] + c2[1] + c3[1]) / 3.0 - 2.5,
                f64::from(c[2] + c2[2] + c3[2]) / 3.0 - 3.0,
            ];
            assert!(dot(n, center) > 0.0, "face {t} tournée vers l'intérieur");
        }
    }

    #[test]
    fn instance_buffer_layout() {
        let mut i = Instances::default();
        i.push([1.0, 2.0, 3.0], 0.0, 2.0, 3.0, [1.0; 4]);
        assert_eq!(i.count(), 1);
        assert_eq!(
            &i.buffer[..12],
            &[2.0, 0.0, 0.0, 1.0, 0.0, 3.0, 0.0, 2.0, 0.0, 0.0, 2.0, 3.0]
        );
    }
}
