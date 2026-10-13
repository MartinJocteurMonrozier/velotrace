#!/usr/bin/env python3
"""Construit le cycliste de velotrace : app/assets/rider/rider.bin.

Sources : le corps vient de MakeHuman (maillage de base hm08, cibles de
morphologie, squelette par défaut et poids de peau), téléchargés depuis le
dépôt makehumancommunity/makehuman au commit MH_COMMIT et gardés en cache.
Ces fichiers sont placés sous licence CC0 1.0 par l'équipe MakeHuman (voir
LICENSE.md du dépôt, section C, « The license for the bundled assets ») : le
maillage produit est libre de droits. Le casque, les lunettes, les chaussures
et les cales sont construits ici, de façon procédurale, autour de la tête et
des pieds du corps.

Étapes :
  1. maillage de base et cibles « macro » : homme d'environ 25 ans, sec et
     musclé, 1,80 m ;
  2. squelette MakeHuman (163 os) ramené à 24 os, poids de peau fusionnés
     (4 influences au plus par sommet), mains refermées en position de prise
     du cintre, pieds retirés sous la tige des chaussures ;
  3. simplification du corps (quadriques) vers 11 000 triangles ;
  4. champs par sommet pour la tenue (maillot, cuissard, chaussettes, gants),
     calculés dans la pose de repos et seuillés dans le shader : les limites
     restent nettes quelle que soit la finesse du maillage ;
  5. casque à aérations, lunettes, chaussures et cales, liés à la tête et aux
     pieds ;
  6. écriture d'un fichier binaire lu par app/scripts/rider_body.gd.

Repère de sortie : celui de Godot, en mètres, avant vers -Z, droite du
cycliste vers +X, haut vers +Y, pieds au sol (y = 0).

Utilisation (numpy et fast-simplification requis) :
    pip install numpy fast-simplification
    python3 tools/rider/build_rider.py [--cache DOSSIER] [--out FICHIER]
"""

import argparse
import gzip
import json
import math
import os
import struct
import sys
import urllib.request

import numpy as np

try:
    import fast_simplification
except ImportError:  # pragma: no cover
    sys.exit("fast-simplification manquant : pip install fast-simplification")

MH_COMMIT = "a8bc2d54ff0ac92e78ff71431b1023eda42bf482"
MH_RAW = "https://raw.githubusercontent.com/makehumancommunity/makehuman/%s/makehuman/" % MH_COMMIT

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))

# Morphologie (curseurs MakeHuman entre 0 et 1).
GENDER = 1.0
MUSCLE = 0.8
WEIGHT = 0.3
HEIGHT = 0.6
PROPORTIONS = 0.9
# Taille finale (m).
STATURE = 1.80
# Cibles de détail : (fichier sous data/targets, poids).
DETAIL_TARGETS = [
    ("armslegs/{s}-lowerleg-scale-vert-incr.target", 0.35),
    ("armslegs/{s}-upperarm-scale-horiz-incr.target", 0.5),
    ("armslegs/{s}-lowerarm-scale-horiz-incr.target", 0.4),
    ("armslegs/{s}-upperleg-muscle-incr.target", 0.7),
    ("armslegs/{s}-lowerleg-muscle-incr.target", 0.5),
    ("armslegs/{s}-upperarm-fat-decr.target", 0.4),
    ("stomach/stomach-pregnant-decr.target", 0.4),
    ("torso/torso-vshape-incr.target", 0.35),
    ("buttocks/buttocks-volume-decr.target", 0.3),
]

TRI_TARGET = 11000

# Os conservés, dans l'ordre (parents avant enfants), avec leur parent.
BONES = [
    ("root", None),
    ("spine05", "root"), ("spine04", "spine05"), ("spine03", "spine04"),
    ("spine02", "spine03"), ("spine01", "spine02"),
    ("neck", "spine01"), ("head", "neck"),
]
for _s in ("L", "R"):
    BONES += [
        ("clavicle." + _s, "spine01"), ("upperarm." + _s, "clavicle." + _s),
        ("lowerarm01." + _s, "upperarm." + _s), ("lowerarm02." + _s, "lowerarm01." + _s),
        ("hand." + _s, "lowerarm02." + _s),
        ("thigh." + _s, "root"), ("shin." + _s, "thigh." + _s), ("foot." + _s, "shin." + _s),
    ]
BONE_INDEX = {n: i for i, (n, _p) in enumerate(BONES)}


def merged_bone(name, mh_bones):
    """Os conservé qui reçoit les poids de l'os MakeHuman `name`."""
    direct = {
        "root": "root", "pelvis.L": "root", "pelvis.R": "root",
        "spine05": "spine05", "spine04": "spine04", "spine03": "spine03",
        "spine02": "spine02", "spine01": "spine01",
        "breast.L": "spine02", "breast.R": "spine02",
        "neck01": "neck", "neck02": "neck", "neck03": "neck", "head": "head",
    }
    n = name
    while n is not None:
        if n in direct:
            return direct[n]
        base, _, side = n.partition(".")
        table = {
            "clavicle": "clavicle", "shoulder01": "clavicle",
            "upperarm01": "upperarm", "upperarm02": "upperarm",
            "lowerarm01": "lowerarm01", "lowerarm02": "lowerarm02", "wrist": "hand",
            "upperleg01": "thigh", "upperleg02": "thigh",
            "lowerleg01": "shin", "lowerleg02": "shin", "foot": "foot",
        }
        if base in table and side:
            return table[base] + "." + side
        n = mh_bones[n]["parent"]
    return "root"


# --- Lecture des fichiers MakeHuman --------------------------------------------

def fetch(cache, rel):
    path = os.path.join(cache, rel)
    if not os.path.exists(path):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        print("téléchargement", rel)
        data = urllib.request.urlopen(MH_RAW + rel, timeout=120).read()
        with open(path + ".part", "wb") as f:
            f.write(data)
        os.replace(path + ".part", path)
    return path


def load_obj(path):
    verts, faces = [], []
    group = None
    with open(path) as f:
        for line in f:
            if line.startswith("v "):
                verts.append([float(x) for x in line.split()[1:4]])
            elif line.startswith("g "):
                group = line.split()[1]
            elif line.startswith("f ") and group == "body":
                faces.append([int(t.split("/")[0]) - 1 for t in line.split()[1:]])
    return np.array(verts, dtype=np.float64), faces


def load_target(path):
    idx, delta = [], []
    with open(path) as f:
        for line in f:
            if line.startswith("#") or not line.strip():
                continue
            p = line.split()
            idx.append(int(p[0]))
            delta.append([float(x) for x in p[1:4]])
    return np.array(idx, dtype=np.int64), np.array(delta, dtype=np.float64).reshape(-1, 3)


def split3(v):
    """Poids (min, moyen, max) d'un curseur MakeHuman."""
    if v < 0.5:
        return 1.0 - 2.0 * v, 2.0 * v, 0.0
    return 0.0, 2.0 - 2.0 * v, 2.0 * v - 1.0


def macro_targets():
    out = [("macrodetails/caucasian-male-young.target", GENDER)]
    m = dict(zip(("minmuscle", "averagemuscle", "maxmuscle"), split3(MUSCLE)))
    w = dict(zip(("minweight", "averageweight", "maxweight"), split3(WEIGHT)))
    for mk, mw in m.items():
        for wk, ww in w.items():
            k = mw * ww
            if k <= 1e-6:
                continue
            out.append(("macrodetails/universal-male-young-%s-%s.target" % (mk, wk), k))
            if HEIGHT > 0.5:
                out.append(("macrodetails/height/male-young-%s-%s-maxheight.target" % (mk, wk), k * (HEIGHT - 0.5) * 2.0))
            if PROPORTIONS > 0.5:
                out.append(("macrodetails/proportions/male-young-%s-%s-idealproportions.target" % (mk, wk),
                            k * (PROPORTIONS - 0.5) * 2.0))
    for name, k in DETAIL_TARGETS:
        if "{s}" in name:
            out += [(name.format(s=s), k) for s in ("l", "r")]
        else:
            out.append((name, k))
    return out


# --- Géométrie ------------------------------------------------------------------

def normalize(v):
    v = np.asarray(v, dtype=np.float64)
    n = np.linalg.norm(v, axis=-1, keepdims=True)
    return v / np.maximum(n, 1e-12)


def rotation(axis, angle):
    a = normalize(axis)
    x, y, z = a
    c, s = math.cos(angle), math.sin(angle)
    C = 1.0 - c
    return np.array([
        [c + x * x * C, x * y * C - z * s, x * z * C + y * s],
        [y * x * C + z * s, c + y * y * C, y * z * C - x * s],
        [z * x * C - y * s, z * y * C + x * s, c + z * z * C]])


def affine(R, pivot):
    M = np.eye(4)
    M[:3, :3] = R
    M[:3, 3] = pivot - R @ pivot
    return M


def vertex_normals(V, T):
    """Normales sortantes de triangles en sens horaire (convention de Godot)."""
    n = np.zeros_like(V)
    fn = np.cross(V[T[:, 2]] - V[T[:, 0]], V[T[:, 1]] - V[T[:, 0]])
    for k in range(3):
        np.add.at(n, T[:, k], fn)
    return normalize(n)


def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


class Shape:
    """Accessoire lié rigidement à un os : grilles de points assemblées en
    triangles (sens horaire vu de l'extérieur), une matière par grille et des
    coordonnées de motif (CUSTOM1) par sommet. Les grilles ne partagent pas
    leurs sommets : arête vive entre deux grilles, normales lisses à
    l'intérieur d'une grille."""

    def __init__(self, bone):
        self.bone = BONE_INDEX[bone]
        self.V, self.C, self.T, self.M = [], [], [], []
        self.n = 0

    def grid(self, P, mat, center, outward=True, wrap=False, coords=None):
        P = np.asarray(P, dtype=np.float64)
        R, C = P.shape[:2]
        cols = C if wrap else C - 1
        base = self.n
        tris = []
        for i in range(R - 1):
            for j in range(cols):
                j1 = (j + 1) % C
                a, b, c, d = base + i * C + j, base + i * C + j1, base + (i + 1) * C + j1, base + (i + 1) * C + j
                tris += [(a, b, c), (a, c, d)]
        self.V.append(P.reshape(-1, 3))
        self.C.append(np.zeros((R * C, 4)) if coords is None else np.asarray(coords, dtype=np.float64).reshape(R * C, 4))
        self.M.append(np.full(R * C, mat))
        self.n += R * C
        T = np.array(tris)
        allV = np.concatenate(self.V)
        fn = np.cross(allV[T[:, 2]] - allV[T[:, 0]], allV[T[:, 1]] - allV[T[:, 0]])
        cen = allV[T].mean(axis=1)
        # Centre unique, ou un centre par rangée (tubes courbes).
        center = np.asarray(center, dtype=np.float64)
        if center.ndim == 2:
            center = center[(T[:, 0] - base) // C]
        score = np.einsum("ij,ij->i", fn, cen - center).sum()
        if (score < 0.0) == outward:
            T = T[:, [0, 2, 1]]
        self.T.append(T)

    def build(self):
        """(os, positions, normales, matières, motifs, triangles), sommets partagés."""
        V = np.concatenate(self.V)
        T = np.concatenate(self.T)
        area = np.linalg.norm(np.cross(V[T[:, 1]] - V[T[:, 0]], V[T[:, 2]] - V[T[:, 0]]), axis=1)
        T = T[area > 1e-10]
        return self.bone, V, vertex_normals(V, T), np.concatenate(self.M), np.concatenate(self.C), T


def ring_smooth(a, passes=2, wrap=True):
    a = np.asarray(a, dtype=np.float64)
    for _ in range(passes):
        if wrap:
            a = (np.roll(a, 1, axis=0) + 2.0 * a + np.roll(a, -1, axis=0)) / 4.0
        else:
            b = a.copy()
            b[1:-1] = (a[:-2] + 2.0 * a[1:-1] + a[2:]) / 4.0
            a = b
    return a


# --- Construction ---------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--cache", default=os.path.join(os.path.expanduser("~"), ".cache", "velotrace", "makehuman"))
    ap.add_argument("--out", default=os.path.join(ROOT, "app", "assets", "rider", "rider.bin"))
    args = ap.parse_args()
    cache = args.cache

    V, quads = load_obj(fetch(cache, "data/3dobjs/base.obj"))
    for rel, k in macro_targets():
        idx, d = load_target(fetch(cache, "data/targets/" + rel))
        V[idx] += d * k

    skel = json.load(open(fetch(cache, "data/rigs/default.mhskel")))
    mhw = json.load(open(fetch(cache, "data/rigs/default_weights.mhw")))
    mh_bones = skel["bones"]

    # Repère Godot en mètres, pieds au sol.
    body_ids = np.unique(np.concatenate([np.array(q) for q in quads]))
    lo, hi = V[body_ids, 1].min(), V[body_ids, 1].max()
    scale = STATURE / (hi - lo)
    V = np.stack([-V[:, 0], V[:, 1] - lo, -V[:, 2]], axis=1) * scale

    joints = {k: V[v].mean(axis=0) for k, v in skel["joints"].items()}
    head_of = {b: joints[d["head"]] for b, d in mh_bones.items()}
    tail_of = {b: joints[d["tail"]] for b, d in mh_bones.items()}

    # Poids complets (163 os) pour refermer les mains.
    names = list(mh_bones.keys())
    col = {n: i for i, n in enumerate(names)}
    nv = V.shape[0]
    W = np.zeros((nv, len(names)))
    for b, lst in mhw["weights"].items():
        if b in col:
            for vi, w in lst:
                W[vi, col[b]] += w
    W /= np.maximum(W.sum(axis=1, keepdims=True), 1e-9)

    grips = bake_grip(V, W, names, col, mh_bones, head_of, tail_of)

    # Poids fusionnés sur les os conservés.
    Wr = np.zeros((nv, len(BONES)))
    for n in names:
        Wr[:, BONE_INDEX[merged_bone(n, mh_bones)]] += W[:, col[n]] * (0.5 if n.startswith("shoulder01") else 1.0)
        if n.startswith("shoulder01"):
            Wr[:, BONE_INDEX["upperarm" + n[len("shoulder01"):]]] += W[:, col[n]] * 0.5

    # Articulations des os conservés (pose de repos).
    J = {
        "root": head_of["spine05"],
        "spine05": head_of["spine05"], "spine04": head_of["spine04"], "spine03": head_of["spine03"],
        "spine02": head_of["spine02"], "spine01": head_of["spine01"],
        "neck": head_of["neck01"], "head": head_of["head"],
    }
    T = {
        "root": head_of["spine05"] + np.array([0.0, 0.1, 0.0]),
        "spine05": tail_of["spine05"], "spine04": tail_of["spine04"], "spine03": tail_of["spine03"],
        "spine02": tail_of["spine02"], "spine01": tail_of["spine01"],
        "neck": tail_of["neck03"], "head": tail_of["head"],
    }
    for s in ("L", "R"):
        J["clavicle." + s] = head_of["clavicle." + s]
        T["clavicle." + s] = head_of["upperarm01." + s]
        J["upperarm." + s] = head_of["upperarm01." + s]
        T["upperarm." + s] = head_of["lowerarm01." + s]
        J["lowerarm01." + s] = head_of["lowerarm01." + s]
        T["lowerarm01." + s] = head_of["lowerarm02." + s]
        J["lowerarm02." + s] = head_of["lowerarm02." + s]
        T["lowerarm02." + s] = head_of["wrist." + s]
        J["hand." + s] = head_of["wrist." + s]
        T["hand." + s] = head_of["finger3-1." + s]
        J["thigh." + s] = head_of["upperleg01." + s]
        T["thigh." + s] = head_of["lowerleg01." + s]
        J["shin." + s] = head_of["lowerleg01." + s]
        T["shin." + s] = head_of["foot." + s]
        J["foot." + s] = head_of["foot." + s]
        T["foot." + s] = head_of["toe3-1." + s]

    # Corps : faces du maillage de base, pieds retirés sous la tige des chaussures.
    Q = np.array(quads, dtype=np.int64)
    foot_w = np.zeros(nv)
    for s in ("L", "R"):
        foot_w += Wr[:, BONE_INDEX["foot." + s]]
    ankle_y = J["foot.L"][1]
    cut_y = ankle_y - 0.028
    inside = (foot_w > 0.3) & (V[:, 1] < cut_y)
    keep = ~inside[Q].all(axis=1)
    Q = Q[keep]
    tris = np.concatenate([Q[:, [0, 1, 2]], Q[:, [0, 2, 3]]])
    # Godot : faces avant dans le sens horaire.
    tris = tris[:, [0, 2, 1]] if not front_is_clockwise(V, tris) else tris
    used = np.unique(tris)
    remap = -np.ones(nv, dtype=np.int64)
    remap[used] = np.arange(used.size)
    BV = V[used]
    BT = remap[tris]
    BW = Wr[used]
    print("corps : %d sommets, %d triangles avant simplification" % (BV.shape[0], BT.shape[0]))

    reduction = 1.0 - TRI_TARGET / BT.shape[0]
    SV, ST = fast_simplification.simplify(BV.astype(np.float32), BT.astype(np.int32), target_reduction=reduction, agg=5)
    SV = SV.astype(np.float64)
    print("corps : %d sommets, %d triangles après simplification" % (SV.shape[0], ST.shape[0]))
    # Poids et masques du sommet d'origine le plus proche.
    nearest = nearest_vertices(BV, SV)
    SW = BW[nearest]
    lips = lips_mask(Wr, W, names, col)[used][nearest]
    distal = sum(W[:, col["finger%d-%d.%s" % (f, k, s)]] for f in range(1, 6) for k in (2, 3) for s in "LR")
    distal = np.clip(distal, 0.0, 1.0)[used][nearest]
    for s in ("L", "R"):
        J["eye." + s] = head_of["eye." + s]
    fields = kit_fields(SV, SW, J, lips, distal)
    if not front_is_clockwise(SV, ST):
        ST = ST[:, [0, 2, 1]]
    # Normales du maillage complet : ombrage lisse malgré la simplification.
    SN = normalize(vertex_normals(BV, BT)[nearest] * 0.75 + vertex_normals(SV, ST) * 0.25)

    for s in ("L", "R"):
        J["eye." + s] = head_of["eye." + s]
    Vb, Wb = V[body_ids], Wr[body_ids]
    parts = shoes(Vb, Wb, J) + helmet(Vb, Wb, J) + glasses(Vb, Wb, J)

    write(args.out, SV, SN, ST, SW, fields, parts, J, T, grips, scale)


def front_is_clockwise(V, T):
    """Vrai si les normales géométriques (règle de Godot, sens horaire) pointent
    vers l'extérieur : on le teste sur le volume signé."""
    a, b, c = V[T[:, 0]], V[T[:, 1]], V[T[:, 2]]
    vol = np.einsum("ij,ij->i", a, np.cross(b, c)).sum()
    # Volume positif en sens trigonométrique : normales sortantes en CCW.
    return vol < 0.0


def nearest_vertices(src, dst, chunk=2048):
    out = np.empty(dst.shape[0], dtype=np.int64)
    for i in range(0, dst.shape[0], chunk):
        d = ((dst[i:i + chunk, None, :] - src[None, :, :]) ** 2).sum(axis=2)
        out[i:i + chunk] = d.argmin(axis=1)
    return out


def lips_mask(Wr, W, names, col):
    m = np.zeros(W.shape[0])
    for n in names:
        if n.startswith("oris"):
            m += W[:, col[n]]
    return np.clip(m, 0.0, 1.0)


# --- Mains : prise du cintre ----------------------------------------------------

FINGER_FLEX = {1: (0.8, 1.0, 0.6), 2: (0.75, 1.05, 0.65), 3: (0.8, 1.1, 0.65), 4: (0.85, 1.1, 0.65), 5: (0.9, 1.05, 0.6)}


def bake_grip(V, W, names, col, mh_bones, head_of, tail_of):
    """Referme les doigts autour d'un tube (prise des cocottes ou du bas du
    cintre), en modifiant V sur place. Rend, pour chaque main, le repère de
    prise (centre du tube, axe du petit doigt vers l'index, dos de la main)."""
    grips = {}
    D = {n: np.eye(4) for n in names}
    for s in ("L", "R"):
        wrist = head_of["wrist." + s]
        idx = head_of["finger2-1." + s]
        pinky = head_of["finger5-1." + s]
        thumb_tip = tail_of["finger1-3." + s]
        n = normalize(np.cross(pinky - wrist, idx - wrist))
        mid = (idx + pinky) * 0.5
        if np.dot(thumb_tip - mid, n) < 0.0:
            n = -n
        # n : normale côté paume.
        for f in range(2, 6):
            for k in range(1, 4):
                b = "finger%d-%d.%s" % (f, k, s)
                h, t = head_of[b], tail_of[b]
                d = normalize(t - h)
                axis = np.cross(d, n)
                R = rotation(axis, FINGER_FLEX[f][k - 1])
                parent = mh_bones[b]["parent"]
                D[b] = D[parent] @ affine(R, h)
        # Pouce : opposition vers la paume puis flexion.
        b1 = "finger1-1." + s
        h1 = head_of[b1]
        d1 = normalize(tail_of[b1] - h1)
        R1 = rotation(np.cross(d1, n), 0.45)
        D[b1] = D[mh_bones[b1]["parent"]] @ affine(R1, h1)
        for k in (2, 3):
            b = "finger1-%d.%s" % (k, s)
            h, t = head_of[b], tail_of[b]
            d = normalize(t - h)
            R = rotation(np.cross(d, n), 0.5)
            D[b] = D[mh_bones[b]["parent"]] @ affine(R, h)
    # Peau : mélange linéaire des transformations.
    Vh = np.concatenate([V, np.ones((V.shape[0], 1))], axis=1)
    out = np.zeros_like(V)
    for n_, M in D.items():
        w = W[:, col[n_]]
        nz = w > 0
        if not nz.any():
            continue
        out[nz] += w[nz, None] * (Vh[nz] @ M.T)[:, :3]
    moved = W.sum(axis=1) > 0
    V[moved] = out[moved]
    for s in ("L", "R"):
        def posed(b, end="head"):
            p = (head_of if end == "head" else tail_of)[b]
            return (D[b] @ np.append(p, 1.0))[:3]
        pts = []
        for f in range(2, 6):
            pts += [posed("finger%d-1.%s" % (f, s)), posed("finger%d-3.%s" % (f, s)), posed("finger%d-3.%s" % (f, s), "tail")]
        c = np.mean(pts, axis=0)
        a = normalize(head_of["finger2-1." + s] - head_of["finger5-1." + s])
        wrist = head_of["wrist." + s]
        idx = head_of["finger2-1." + s]
        pinky = head_of["finger5-1." + s]
        n = normalize(np.cross(pinky - wrist, idx - wrist))
        if np.dot(tail_of["finger1-3." + s] - (idx + pinky) * 0.5, n) < 0.0:
            n = -n
        dorsal = normalize(-n - a * np.dot(-n, a))
        grips[s] = {"center": c, "axis": a, "dorsal": dorsal}
    return grips


# --- Tenue : champs par sommet --------------------------------------------------

def kit_fields(V, Wb, J, lips, distal):
    """CUSTOM0 = (maillot, cuissard, chaussette, gant), positifs à l'intérieur ;
    CUSTOM1 = (côté x/r, avant z/r, hauteur relative au torse, cheveux ou -lèvres).
    Chaque membre a son propre plan de coupe (manche, bas du cuissard, haut de
    chaussette, poignet) ; on passe d'un champ à l'autre là où les deux sont du
    même signe, pour que la limite reste nette."""
    n = V.shape[0]
    w = lambda b: Wb[:, BONE_INDEX[b]]
    arm = {s: w("upperarm." + s) + w("lowerarm01." + s) + w("lowerarm02." + s) + w("hand." + s) for s in "LR"}
    leg = {s: w("thigh." + s) + w("shin." + s) + w("foot." + s) for s in "LR"}
    lower = {s: w("shin." + s) + w("foot." + s) for s in "LR"}
    head = w("head")

    hip_y = J["thigh.L"][1]
    sh_y = J["upperarm.L"][1]
    back = smoothstep(-0.02, 0.1, V[:, 2] - J["root"][2])
    # Maillot : bas à hauteur des hanches, remonté sur le haut des fessiers ; col.
    f_torso = V[:, 1] - (hip_y + 0.005 + 0.035 * back)
    collar = J["neck"][1] + 0.035 - 0.03 * smoothstep(-0.12, -0.04, -(V[:, 2] - J["neck"][2]))
    f_torso = np.minimum(f_torso, collar - V[:, 1])
    f_jersey = f_torso.copy()
    f_short = (hip_y + 0.15) - V[:, 1]
    f_glove = np.full(n, -0.3)
    for s in "LR":
        on_arm = arm[s] > 0.5
        sh, el = J["upperarm." + s], J["lowerarm01." + s]
        d = normalize(el - sh)
        f_jersey = np.where(on_arm, (sh + (el - sh) * 0.58 - V) @ d, f_jersey)
        f_short = np.where(on_arm, -0.3, f_short)
        # Mitaines : du poignet à mi-hauteur des premières phalanges.
        dw = normalize(J["hand." + s] - J["lowerarm02." + s])
        f_c = (V - (J["hand." + s] + dw * 0.012)) @ dw
        f_glove = np.where(on_arm, np.minimum(f_c, (0.5 - distal) * 0.03), f_glove)
    f_sock = np.full(n, -0.3)
    for s in "LR":
        hip, knee, ankle = J["thigh." + s], J["shin." + s], J["foot." + s]
        d = normalize(knee - hip)
        f_short = np.where(leg[s] > 0.5, (knee - d * 0.13 - V) @ d, f_short)
        ds = normalize(ankle - knee)
        f_sock = np.where(lower[s] > 0.5, (V - (ankle - ds * 0.14)) @ ds, f_sock)

    # Coordonnées de décor du maillot.
    c = J["root"]
    rel = V - c
    r = np.sqrt(rel[:, 0] ** 2 + rel[:, 2] ** 2)
    sx = rel[:, 0] / np.maximum(r, 1e-6)
    cz = rel[:, 2] / np.maximum(r, 1e-6)
    hy = (V[:, 1] - hip_y) / (sh_y - hip_y)
    # Cheveux courts : arrière et côtés du crâne, sous le casque.
    hj = J["head"]
    eye_z = J["eye.L"][2]
    hair = smoothstep(0.5, 0.8, head) * smoothstep(hj[1] - 0.01, hj[1] + 0.015, V[:, 1]) \
        * smoothstep(eye_z + 0.07, eye_z + 0.09, V[:, 2])
    extra = np.where(lips > 0.35, -lips, hair)
    c0 = np.stack([f_jersey, f_short, f_sock, f_glove], axis=1)
    c1 = np.stack([sx, cz, hy, extra], axis=1)
    arm_all = np.clip(smoothstep(0.3, 0.8, arm["L"]) + smoothstep(0.3, 0.8, arm["R"]), 0.0, 1.0)
    return c0, c1, arm_all


# --- Accessoires ------------------------------------------------------------------

# Matières des accessoires (CUSTOM2.r), lues par app/shaders/rider.gdshader.
MAT_HELMET = 1
MAT_HELMET_DARK = 4
MAT_SHOE = 5
MAT_LENS = 7
MAT_FRAME = 8
MAT_CLEAT = 9
MAT_STRAP = 10

# Chaussures : la cale touche le dessus de la pédale 9,65 cm sous la cheville et
# 12 cm devant elle (ANKLE_UP et ANKLE_BACK de cyclist.gd, moins l'épaisseur du
# dessus de pédale) ; semelle 1,1 cm au-dessus.
SOLE_BELOW_ANKLE = 0.0855
CLEAT_BELOW_ANKLE = 0.0965
CLEAT_AHEAD = 0.12


def shoes(V, Wb, J):
    out = []
    up = np.array([0.0, 1.0, 0.0])
    for s in ("L", "R"):
        ankle = J["foot." + s]
        fw = Wb[:, BONE_INDEX["foot." + s]] + Wb[:, BONE_INDEX["shin." + s]]
        P = V[(fw > 0.5) & (V[:, 1] < ankle[1] + 0.03) & (np.abs(V[:, 0] - ankle[0]) < 0.09)]
        # Axe du pied (horizontal), vers l'avant (-Z).
        xz = P[:, [0, 2]] - P[:, [0, 2]].mean(axis=0)
        f2 = np.linalg.svd(xz, full_matrices=False)[2][0]
        f = np.array([f2[0], 0.0, f2[1]])
        if f[2] > 0.0:
            f = -f
        lat = np.cross(f, up)
        rel = P - ankle
        t = rel @ f
        x = rel @ lat
        t0, t1 = t.min() - 0.006, t.max() + 0.014
        n = 22
        ts = np.linspace(t0, t1, n)
        hw = np.zeros(n)
        xc = np.zeros(n)
        top = np.zeros(n)
        for i, ti in enumerate(ts):
            m = np.abs(t - ti) < 0.015
            if m.sum() < 3:
                m = np.argsort(np.abs(t - ti))[:6]
            hw[i] = (x[m].max() - x[m].min()) * 0.5
            xc[i] = (x[m].max() + x[m].min()) * 0.5
            top[i] = rel[m, 1].max()
        hw = ring_smooth(hw, 3, wrap=False) + 0.007
        xc = ring_smooth(xc, 3, wrap=False)
        top = np.minimum(ring_smooth(top, 2, wrap=False) + 0.008, -0.012)
        bottom = -SOLE_BELOW_ANKLE + 0.008 * smoothstep(t1 - 0.06, t1, ts) + 0.004 * smoothstep(t0 + 0.03, t0, ts)
        # Bouts arrondis : talon et pointe.
        u = (ts - t0) / (t1 - t0)
        end = np.sqrt(np.clip(1.0 - (np.clip(0.12 - u, 0, None) / 0.12) ** 2, 0.05, 1.0)) \
            * np.sqrt(np.clip(1.0 - (np.clip(u - 0.78, 0, None) / 0.22) ** 2, 0.02, 1.0))
        hw *= 0.35 + 0.65 * end
        top = bottom + (top - bottom) * (0.55 + 0.45 * np.sqrt(end))
        k = 16
        ang = np.linspace(0.0, 2.0 * math.pi, k, endpoint=False)
        ca, sa = np.cos(ang), np.sin(ang)
        cx = np.sign(ca) * np.abs(ca) ** (2.0 / 2.8)
        sy = np.where(sa < 0.0, -np.abs(sa) ** (2.0 / 5.0), np.abs(sa) ** (2.0 / 2.4))
        rings, coords = [], []
        side = 1.0 if s == "R" else -1.0
        for i in range(n):
            mid = (top[i] + bottom[i]) * 0.5
            hh = (top[i] - bottom[i]) * 0.5
            yy = mid + hh * sy
            rings.append(ankle + f * ts[i] + np.outer(xc[i] + hw[i] * cx, lat) + np.outer(yy, up))
            coords.append(np.stack([np.full(k, u[i]), (yy - bottom[i]) / max(top[i] - bottom[i], 1e-3),
                                    cx * side, yy - bottom[i]], axis=1))
        # Talon et pointe fermés.
        rings = [np.repeat(rings[0].mean(axis=0, keepdims=True), k, axis=0)] + rings \
            + [np.repeat(rings[-1].mean(axis=0, keepdims=True), k, axis=0)]
        coords = [coords[0]] + coords + [coords[-1]]
        sh = Shape("foot." + s)
        sh.grid(np.array(rings), MAT_SHOE, ankle + f * (t0 + t1) * 0.5 - up * 0.06, wrap=True, coords=np.array(coords))
        # Cale sous l'avant du pied, alignée sur la pédale (-Z).
        cy0 = ankle[1] - SOLE_BELOW_ANKLE + 0.001
        cy1 = ankle[1] - CLEAT_BELOW_ANKLE
        cz = ankle[2] - CLEAT_AHEAD
        cl = []
        for y, sc in ((cy0, 1.0), (cy1 + 0.002, 0.95), (cy1, 0.85)):
            cl.append([[ankle[0] + dx * sc, y, cz + dz * sc] for dx, dz in
                       ((-0.026, 0.028), (0.026, 0.028), (0.03, 0.0), (0.012, -0.034), (-0.012, -0.034), (-0.03, 0.0))])
        cl.append([[ankle[0], cy1, cz - 0.002]] * 6)
        sh.grid(np.array(cl), MAT_CLEAT, np.array([ankle[0], cy0 + 0.01, cz]), wrap=True)
        out.append(sh)
    return out


def head_frame(V, Wb, J):
    """Sommets de la tête et repères : centre, yeux, haut des oreilles."""
    hw = Wb[:, BONE_INDEX["head"]] + Wb[:, BONE_INDEX["neck"]] * 0.5
    H = V[hw > 0.5]
    eye = (J["eye.L"] + J["eye.R"]) * 0.5
    c = np.array([0.0, eye[1] + 0.012, (H[:, 2].min() + H[:, 2].max()) * 0.5 + 0.004])
    side = H[np.abs(H[:, 0]) > np.abs(H[:, 0]).max() - 0.012]
    ear_top = side[:, 1].max()
    return H, c, eye, ear_top


def sphere_radius(H, c, dirs, cone_deg=9.0, exclude=None):
    """Rayon de la tête depuis c dans chaque direction (max dans un cône)."""
    rel = H - c
    r = np.linalg.norm(rel, axis=1)
    u = rel / r[:, None]
    if exclude is not None:
        r, u = r[~exclude], u[~exclude]
    cosc = math.cos(math.radians(cone_deg))
    out = np.full(dirs.shape[0], np.nan)
    dots = dirs @ u.T
    for i in range(dirs.shape[0]):
        m = dots[i] > cosc
        if m.any():
            out[i] = r[m].max()
    return out


def helmet(V, Wb, J):
    H, c, eye, ear_top = head_frame(V, Wb, J)
    brow = eye[1] + 0.034
    back_y = ear_top - 0.032
    nu, nv = 40, 15
    phis = np.linspace(-math.pi, math.pi, nu, endpoint=False)
    thetas = np.linspace(0.0, math.radians(140.0), 60)
    ears = (np.abs(H[:, 0]) > 0.06) & (H[:, 1] < ear_top + 0.004) & (H[:, 1] > eye[1] - 0.06)

    def direction(th, ph):
        return np.stack([np.sin(th) * np.sin(ph), np.cos(th), -np.sin(th) * np.cos(ph)], axis=-1)

    # Rayon de la tête sur une grille fine, lissé, puis coque décalée.
    TH, PH = np.meshgrid(thetas, phis, indexing="ij")
    rh = sphere_radius(H, c, direction(TH, PH).reshape(-1, 3), exclude=ears).reshape(TH.shape)
    for i in range(rh.shape[0]):
        row = rh[i]
        if np.isnan(row).all():
            rh[i] = rh[i - 1]
        elif np.isnan(row).any():
            idx = np.arange(row.size)
            ok = ~np.isnan(row)
            rh[i] = np.interp(idx, idx[ok], row[ok], period=row.size)
    for _ in range(4):
        rh = ring_smooth(rh.T, 1).T
        rh = ring_smooth(rh, 1, wrap=False)
    # Coque : 2,4 cm au-dessus du crâne, queue allongée vers l'arrière.
    back = np.clip(-np.cos(PH), 0.0, 1.0) ** 1.5 * smoothstep(0.7, 1.8, TH)
    front = np.clip(np.cos(PH), 0.0, 1.0) ** 2 * smoothstep(0.6, 1.3, TH)
    ro = rh + 0.028 - 0.007 * smoothstep(0.3, 1.6, TH) + 0.026 * back + 0.005 * front
    ri = rh + 0.006

    # Bord inférieur : au-dessus des sourcils, des oreilles, sur la nuque.
    # Bord : front, tempes plus basses, au-dessus de l'oreille, nuque.
    edge_y = np.interp(np.degrees(np.abs(phis)), [0.0, 40.0, 65.0, 95.0, 125.0, 180.0],
                       [brow + 0.004, brow, brow - 0.004, ear_top + 0.012, ear_top - 0.004, back_y])
    th_edge = np.zeros(nu)
    for j in range(nu):
        y = c[1] + ro[:, j] * np.cos(thetas)
        k = max(int(np.argmax(y < edge_y[j])), 1)
        th_edge[j] = np.interp(edge_y[j], [y[k], y[k - 1]], [thetas[k], thetas[k - 1]])
    th_edge = ring_smooth(th_edge, 2)

    def surf(radius, frac):
        rows, crd = [], []
        for v in frac:
            th = th_edge * v
            rr = np.array([np.interp(th[j], thetas, radius[:, j]) for j in range(nu)])
            rows.append(c + direction(th, phis) * rr[:, None])
            crd.append(np.stack([phis, th, np.full(nu, v), np.zeros(nu)], axis=1))
        return np.array(rows), np.array(crd)

    sh = Shape("head")
    outer, oc = surf(ro, np.linspace(0.0, 1.0, nv) ** 0.85)
    sh.grid(outer, MAT_HELMET, c, wrap=True, coords=oc)
    inner, ic = surf(ri, np.array([0.55, 0.75, 0.9, 1.0]))
    sh.grid(inner, MAT_HELMET_DARK, c, outward=False, wrap=True, coords=ic)
    # Mousse vue à travers les aérations (la coque y est percée par le shader).
    liner, lc = surf(ro - 0.009, np.linspace(0.0, 1.0, 7) ** 0.85)
    sh.grid(liner, MAT_HELMET_DARK, c, wrap=True, coords=lc)
    sh.grid(np.array([outer[-1], inner[-1]]), MAT_HELMET_DARK, c + np.array([0.0, 0.1, 0.0]), wrap=True)

    # Sangles : devant et derrière l'oreille, jusqu'à la boucle sous le menton.
    chin = H[np.argmin(H[:, 1] + 0.4 * H[:, 2])]
    for sgn in (-1.0, 1.0):
        jf = int(np.argmin(np.abs(phis - sgn * math.radians(72.0))))
        jb = int(np.argmin(np.abs(phis - sgn * math.radians(118.0))))
        a1 = outer[-1][jf] * 0.5 + inner[-1][jf] * 0.5
        a2 = outer[-1][jb] * 0.5 + inner[-1][jb] * 0.5
        knot = np.array([sgn * 0.068, ear_top - 0.07, c[2] + 0.01])
        buckle = np.array([sgn * 0.01, chin[1] - 0.006, chin[2] + 0.035])
        for path in ([a1, (a1 + knot) * 0.5, knot], [a2, (a2 + knot) * 0.5, knot], [knot, knot * 0.6 + buckle * 0.4, buckle]):
            strap(sh, H, np.array(path), c)
    return [sh]


def strap(sh, H, ctrl, c, width=0.0075, thick=0.0018, gap=0.0035):
    """Sangle plate le long d'un chemin, plaquée sur la peau."""
    pts = []
    for i in range(len(ctrl) - 1):
        for t in np.linspace(0.0, 1.0, 4, endpoint=False):
            pts.append(ctrl[i] * (1 - t) + ctrl[i + 1] * t)
    pts.append(ctrl[-1])
    P, N = [], []
    for p in pts:
        d = np.linalg.norm(H - p, axis=1)
        k = np.argsort(d)[:8]
        q = H[k].mean(axis=0)
        nrm = normalize(p - q) if np.linalg.norm(p - q) > 1e-4 else normalize(p - c)
        nrm = normalize(nrm + 0.5 * normalize(p - c))
        P.append(q + nrm * max(gap, float(np.dot(p - q, nrm))))
        N.append(nrm)
    P, N = np.array(P), np.array(N)
    tang = normalize(np.gradient(P, axis=0))
    side = normalize(np.cross(tang, N))
    rings = [[p - sd * width * 0.5, p + sd * width * 0.5, p + sd * width * 0.5 + nn * thick, p - sd * width * 0.5 + nn * thick]
             for p, nn, sd in zip(P, N, side)]
    sh.grid(np.array(rings), MAT_STRAP, c, wrap=True)


def glasses(V, Wb, J):
    H, c, eye, ear_top = head_frame(V, Wb, J)
    eye_y = eye[1]
    g = np.array([0.0, eye_y, eye[2] + 0.075])
    sh = Shape("head")
    nu = 25
    phis = np.radians(np.linspace(-82.0, 82.0, nu))
    # Visage au niveau des yeux et des sourcils, sans le nez.
    band = (H[:, 1] > eye_y - 0.012) & (H[:, 1] < eye_y + 0.03) & ((np.abs(H[:, 0]) > 0.022) | (H[:, 1] > eye_y + 0.012))
    rel = H[band] - g
    az = np.arctan2(rel[:, 0], -rel[:, 2])
    rad = np.hypot(rel[:, 0], rel[:, 2])
    rf = np.array([rad[np.abs(az - p) < math.radians(7.0)].max() for p in phis])
    rf = ring_smooth(np.maximum(rf, rf[::-1]), 3, wrap=False)
    rl = rf + 0.007 + 0.003 * (np.abs(phis) / phis.max())
    top = eye_y + 0.021 - 0.008 * (phis / phis.max()) ** 2
    bot = eye_y - 0.022 + 0.01 * (phis / phis.max()) ** 2 + 0.013 * np.exp(-(np.degrees(phis) / 10.0) ** 2)
    rows, crd = [], []
    for v in np.linspace(0.0, 1.0, 5):
        y = bot + (top - bot) * v
        r = rl - 0.15 * np.clip(eye_y - y, 0.0, None)
        rows.append(np.stack([g[0] + r * np.sin(phis), y, g[2] - r * np.cos(phis)], axis=1))
        crd.append(np.stack([phis, np.full(nu, v), np.zeros(nu), np.zeros(nu)], axis=1))
    lens = np.array(rows)
    sh.grid(lens, MAT_LENS, g, wrap=False, coords=np.array(crd))
    # Dos des verres, sombre.
    sh.grid(lens, MAT_FRAME, g, outward=False, wrap=False)
    # Monture : barre au-dessus des verres puis branches jusqu'aux oreilles.
    tube(sh, lens[-1] + np.array([0.0, 0.002, 0.0]), 0.0032, MAT_FRAME, flat=1.6)
    for sgn, j in ((-1.0, 0), (1.0, nu - 1)):
        start = (lens[-1][j] + lens[2][j]) * 0.5
        path = [start]
        for ph, dy in ((95.0, 0.002), (112.0, -0.002), (128.0, -0.008)):
            p = math.radians(ph) * sgn
            m = np.abs(H[:, 1] - (start[1] + dy)) < 0.02
            relh = H[m] - c
            azh = np.arctan2(relh[:, 0], -relh[:, 2])
            radh = np.hypot(relh[:, 0], relh[:, 2])
            near = np.abs(azh - p) < math.radians(8.0)
            r = radh[near].max() + 0.004 if near.any() else 0.08
            path.append(np.array([c[0] + r * math.sin(p), start[1] + dy, c[2] - r * math.cos(p)]))
        tube(sh, np.array(path), 0.0025, MAT_FRAME, flat=2.0)
    return [sh]


def tube(sh, path, radius, mat, flat=1.0, seg=6):
    """Tube (section elliptique) le long d'un chemin, bouts fermés."""
    tang = normalize(np.gradient(path, axis=0))
    rings = []
    for p, t in zip(path, tang):
        a = normalize(np.cross(t, [0.0, 1.0, 0.0])) if abs(t[1]) < 0.9 else normalize(np.cross(t, [1.0, 0.0, 0.0]))
        b = np.cross(t, a)
        ang = np.linspace(0.0, 2.0 * math.pi, seg, endpoint=False)
        rings.append([p + radius * (a * math.cos(q) * flat + b * math.sin(q)) for q in ang])
    P = np.array([[path[0]] * seg] + rings + [[path[-1]] * seg])
    centers = np.concatenate([path[:1], path, path[-1:]])
    sh.grid(P, mat, centers, wrap=True)


# --- Écriture ---------------------------------------------------------------------

def top4(Wb):
    idx = np.argsort(-Wb, axis=1)[:, :4]
    w = np.take_along_axis(Wb, idx, axis=1)
    w /= np.maximum(w.sum(axis=1, keepdims=True), 1e-9)
    return idx.astype(np.int32), w.astype(np.float32)


# Niveaux de détail : taille de maille du regroupement (m). Godot passe au
# niveau suivant quand cette erreur fait moins d'un pixel à l'écran.
LOD_CELLS = (0.022, 0.05)


def lod_indices(P, N, B, M, I, cell):
    """Index simplifiés qui réutilisent les sommets existants (le squelette et
    la tenue restent ceux du maillage complet) : les sommets d'une même maille
    de côté cell, du même os dominant, de la même matière et tournés du même
    côté sont fusionnés sur le plus proche du centre de la maille."""
    side = np.argmax(np.abs(N), axis=1) * 2 + (np.take_along_axis(N, np.argmax(np.abs(N), axis=1)[:, None], 1)[:, 0] > 0)
    g = np.floor(P / cell).astype(np.int64)
    key = np.stack([g[:, 0], g[:, 1], g[:, 2], B, M, side], axis=1)
    _, cl = np.unique(key, axis=0, return_inverse=True)
    cl = cl.ravel()
    k = cl.max() + 1
    cnt = np.bincount(cl, minlength=k)
    mean = np.stack([np.bincount(cl, P[:, a], k) for a in range(3)], axis=1) / cnt[:, None]
    d = np.linalg.norm(P - mean[cl], axis=1)
    order = np.lexsort((d, cl))
    first = np.ones(len(order), dtype=bool)
    first[1:] = cl[order][1:] != cl[order][:-1]
    rep = np.empty(k, dtype=np.int64)
    rep[cl[order][first]] = order[first]
    T = rep[cl[I]]
    ok = (T[:, 0] != T[:, 1]) & (T[:, 1] != T[:, 2]) & (T[:, 0] != T[:, 2])
    T = T[ok]
    _, keep = np.unique(np.sort(T, axis=1), axis=0, return_index=True)
    return T[np.sort(keep)]


def write(out, SV, SN, ST, SW, fields, parts, J, T, grips, scale):
    c0, c1, arm = fields
    bi, bw = top4(SW)
    mat = np.zeros((SV.shape[0], 4), dtype=np.uint8)
    mat[:, 1] = np.round(arm * 255.0).astype(np.uint8)
    pos = [SV]
    nrm = [SN]
    bones = [bi]
    weights = [bw]
    cus0 = [c0]
    cus1 = [c1]
    mats = [mat]
    idx = [ST]
    base = SV.shape[0]
    for sh in parts:
        bone, P, N, M, Cc, Ts = sh.build()
        n = P.shape[0]
        pos.append(P)
        nrm.append(N)
        b = np.zeros((n, 4), dtype=np.int32)
        b[:, 0] = bone
        w = np.zeros((n, 4), dtype=np.float32)
        w[:, 0] = 1.0
        bones.append(b)
        weights.append(w)
        cus0.append(np.full((n, 4), -1.0))
        cus1.append(Cc)
        m = np.zeros((n, 4), dtype=np.uint8)
        m[:, 0] = M.astype(np.uint8)
        mats.append(m)
        idx.append(Ts + base)
        base += n
    P = np.concatenate(pos)
    I = np.concatenate(idx)
    B = np.concatenate(bones)[:, 0]
    M = np.concatenate(mats)[:, 0]
    lods = [(cell, lod_indices(P, np.concatenate(nrm), B, M, I, cell)) for cell in LOD_CELLS]
    arrays = [
        ("vertex", np.concatenate(pos).astype("<f4")),
        ("normal", np.concatenate(nrm).astype("<f4")),
        ("bones", np.concatenate(bones).astype("<i4")),
        ("weights", np.concatenate(weights).astype("<f4")),
        ("custom0", np.concatenate(cus0).astype("<f4")),
        ("custom1", np.concatenate(cus1).astype("<f4")),
        ("custom2", np.concatenate(mats).astype("u1")),
        ("index", I.astype("<i4")),
    ]
    meta = {"bones": [], "arrays": {}, "grips": {}, "vertex_count": int(base), "lods": []}
    for n, (cell, L) in enumerate(lods):
        arrays.append(("lod%d" % n, L.astype("<i4")))
        meta["lods"].append([cell, "lod%d" % n])
        print("niveau de détail %d (maille %.0f mm) : %d triangles" % (n + 1, cell * 1000, L.shape[0]))
    for name, parent in BONES:
        meta["bones"].append({"name": name, "parent": parent, "head": J[name].round(5).tolist(),
                              "tail": T[name].round(5).tolist()})
    for s, g in grips.items():
        meta["grips"][s] = {k: np.asarray(v).round(5).tolist() for k, v in g.items()}
    blob = bytearray()
    for name, a in arrays:
        while len(blob) % 4:
            blob.append(0)
        meta["arrays"][name] = [len(blob), int(a.size)]
        blob += a.tobytes()
    meta["blob_size"] = len(blob)
    packed = gzip.compress(bytes(blob), 9, mtime=0)
    js = json.dumps(meta, separators=(",", ":")).encode()
    js += b" " * ((4 - len(js) % 4) % 4)
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "wb") as f:
        f.write(b"VTRB")
        f.write(struct.pack("<II", 1, len(js)))
        f.write(js)
        f.write(packed)
    print("écrit %s : %d sommets, %d triangles, %.0f ko" % (out, base, sum(i.shape[0] for i in idx), os.path.getsize(out) / 1024))


if __name__ == "__main__":
    main()
