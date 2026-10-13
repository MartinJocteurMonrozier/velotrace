//! Modelage SDF pour Godot (voir `velo_world::sdf`) : le script décrit un
//! objet par primitives (tubes profilés, boîtes, cylindres, disques dentés,
//! perçages, polygones), `build` renvoie ses surfaces prêtes pour un `ArrayMesh`.

use godot::prelude::*;
use velo_world::sdf::{Frame, Op, Sdf, Section, Shape};

#[derive(GodotClass)]
#[class(init, base = RefCounted)]
pub struct VeloSdf {
    sdf: Sdf,
}

fn v(p: Vector3) -> [f32; 3] {
    [p.x, p.y, p.z]
}

fn op_of(op: i64) -> Op {
    match op {
        1 => Op::Subtract,
        2 => Op::Intersect,
        _ => Op::Union,
    }
}

fn frame_of(xf: Transform3D) -> Frame {
    let b = xf.basis;
    Frame {
        origin: v(xf.origin),
        axes: [
            v(b.col_a().normalized()),
            v(b.col_b().normalized()),
            v(b.col_c().normalized()),
        ],
    }
}

#[godot_api]
impl VeloSdf {
    /// Primitive : union.
    #[constant]
    const UNION: i64 = 0;
    /// Primitive : soustraction.
    #[constant]
    const SUBTRACT: i64 = 1;
    /// Primitive : intersection.
    #[constant]
    const INTERSECT: i64 = 2;
    /// Boîte arrondie : [demi-x, demi-y, demi-z, arrondi].
    #[constant]
    const RBOX: i64 = 0;
    /// Ellipsoïde : [demi-x, demi-y, demi-z].
    #[constant]
    const ELLIPSOID: i64 = 1;
    /// Cylindre d'axe y : [rayon, demi-hauteur, arrondi].
    #[constant]
    const CYL: i64 = 2;
    /// Disque denté d'axe y : [dents, r intérieur, r sommet, r primitif,
    /// r creux, demi-épaisseur, chanfrein].
    #[constant]
    const GEAR: i64 = 3;
    /// Perçages répétés d'axe y : [nombre, r centres, r trou, demi-épaisseur,
    /// phase, demi-angle d'arc].
    #[constant]
    const HOLES: i64 = 4;
    /// Polygone du plan xz extrudé selon y : [demi-épaisseur, arrondi, x0, z0, x1, z1…].
    #[constant]
    const POLYGON: i64 = 5;

    #[func]
    fn clear(&mut self) {
        self.sdf.clear();
    }

    /// Tube le long de `points`. `sections` : 7 nombres par point (centre et
    /// rayon du cercle avant u, v, r ; du cercle arrière u, v, r ; troncature),
    /// `chords` : direction de la corde (vers l'avant du profil) par point.
    #[func]
    #[allow(clippy::too_many_arguments)]
    fn tube(
        &mut self,
        points: PackedVector3Array,
        sections: PackedFloat32Array,
        chords: PackedVector3Array,
        op: i64,
        k: f64,
        mat: i64,
        color: Color,
    ) -> bool {
        let pts: Vec<[f32; 3]> = points.as_slice().iter().map(|p| v(*p)).collect();
        let s = sections.as_slice();
        let secs: Vec<Section> = s
            .chunks_exact(7)
            .map(|c| Section {
                c1: [c[0], c[1]],
                r1: c[2],
                c2: [c[3], c[4]],
                r2: c[5],
                cut: c[6],
            })
            .collect();
        let ch: Vec<[f32; 3]> = chords.as_slice().iter().map(|p| v(*p)).collect();
        match self.sdf.tube(&pts, &secs, &ch) {
            Some(shape) => {
                self.sdf.push(
                    shape,
                    op_of(op),
                    k as f32,
                    mat as u32,
                    [color.r, color.g, color.b, color.a],
                );
                true
            }
            None => false,
        }
    }

    /// Autre primitive (voir les constantes pour `kind` et `params`).
    #[func]
    #[allow(clippy::too_many_arguments)]
    fn shape(
        &mut self,
        kind: i64,
        xf: Transform3D,
        params: PackedFloat32Array,
        op: i64,
        k: f64,
        mat: i64,
        color: Color,
    ) -> bool {
        let f = frame_of(xf);
        let p = params.as_slice();
        let need = [4, 3, 3, 7, 6, 2];
        if kind < 0 || kind as usize >= need.len() || p.len() < need[kind as usize] {
            return false;
        }
        let shape = match kind {
            0 => Shape::RBox {
                f,
                half: [p[0], p[1], p[2]],
                round: p[3],
            },
            1 => Shape::Ellipsoid {
                f,
                radii: [p[0], p[1], p[2]],
            },
            2 => Shape::Cyl {
                f,
                r: p[0],
                half_h: p[1],
                round: p[2],
            },
            3 => Shape::Gear {
                f,
                n: p[0],
                r_in: p[1],
                r_tip: p[2],
                r_pitch: p[3],
                r_notch: p[4],
                half_t: p[5],
                chamfer: p[6],
            },
            4 => Shape::Holes {
                f,
                n: p[0],
                r_center: p[1],
                r_hole: p[2],
                half_t: p[3],
                phase: p[4],
                arc: p[5],
            },
            _ => {
                let pts: Vec<[f32; 2]> = p[2..].chunks_exact(2).map(|c| [c[0], c[1]]).collect();
                if pts.len() < 3 {
                    return false;
                }
                Shape::Polygon {
                    f,
                    pts,
                    half_t: p[0],
                    round: p[1],
                }
            }
        };
        self.sdf.push(
            shape,
            op_of(op),
            k as f32,
            mat as u32,
            [color.r, color.g, color.b, color.a],
        );
        true
    }

    /// Maillage : pas de grille `cell` (m), erreur de simplification `error`
    /// (m), niveaux de détail aux erreurs `lod_errors`. Renvoie une liste de
    /// dictionnaires { mat, arrays (tableaux d'`ArrayMesh`), lods }.
    #[func]
    fn build(&self, cell: f64, error: f64, lod_errors: PackedFloat32Array) -> VarArray {
        let surfaces = self
            .sdf
            .build(cell as f32, error as f32, lod_errors.as_slice());
        let mut out = VarArray::new();
        for s in surfaces {
            let m = &s.mesh;
            let mut arrays = VarArray::new();
            arrays.resize(13, &Variant::nil());
            let pos: PackedVector3Array = m
                .positions
                .iter()
                .map(|p| Vector3::new(p[0], p[1], p[2]))
                .collect();
            let nrm: PackedVector3Array = m
                .normals
                .iter()
                .map(|p| Vector3::new(p[0], p[1], p[2]))
                .collect();
            let col: PackedColorArray = m
                .colors
                .iter()
                .map(|c| Color::from_rgba(c[0], c[1], c[2], c[3]))
                .collect();
            arrays.set(0, &pos.to_variant());
            arrays.set(1, &nrm.to_variant());
            arrays.set(3, &col.to_variant());
            arrays.set(
                12,
                &PackedInt32Array::from(m.indices.as_slice()).to_variant(),
            );
            let mut lods = VarDictionary::new();
            for (e, idx) in &s.lods {
                lods.set(*e, &PackedInt32Array::from(idx.as_slice()).to_variant());
            }
            let mut d = VarDictionary::new();
            d.set("mat", s.mat as i64);
            d.set("arrays", &arrays.to_variant());
            d.set("lods", &lods.to_variant());
            out.push(&d.to_variant());
        }
        out
    }

    /// Niveaux de détail d'un maillage déjà construit (sommets `points`,
    /// triangles `indices`) : dictionnaire { erreur : indices } prêt pour
    /// `ArrayMesh.add_surface_from_arrays`.
    #[func]
    fn lods(
        &self,
        points: PackedVector3Array,
        indices: PackedInt32Array,
        errors: PackedFloat32Array,
    ) -> VarDictionary {
        let pts: Vec<[f32; 3]> = points.as_slice().iter().map(|p| v(*p)).collect();
        let idx: Vec<u32> = indices.as_slice().iter().map(|&i| i as u32).collect();
        let mut out = VarDictionary::new();
        for (e, l) in velo_world::sdf::lods(&pts, &idx, errors.as_slice()) {
            let li: Vec<i32> = l.iter().map(|&i| i as i32).collect();
            out.set(e, &PackedInt32Array::from(li.as_slice()).to_variant());
        }
        out
    }

    /// Ramène `points` sur la surface décalée de `offset` (m) : [points, normales].
    #[func]
    fn project(&self, points: PackedVector3Array, offset: f64) -> VarArray {
        let pts: Vec<[f32; 3]> = points.as_slice().iter().map(|p| v(*p)).collect();
        let (p, n) = self.sdf.project(&pts, offset as f32);
        let to = |a: &[[f32; 3]]| -> PackedVector3Array {
            a.iter().map(|q| Vector3::new(q[0], q[1], q[2])).collect()
        };
        let mut out = VarArray::new();
        out.push(&to(&p).to_variant());
        out.push(&to(&n).to_variant());
        out
    }
}
