extends Node3D
## Vélo de route aéro moderne, en maillages procéduraux construits une seule
## fois et partagés par tous les vélos à l'écran : cadre carbone à tubes
## profilés raccordés par des congés (modelage SDF de l'extension Rust, voir
## `VeloSdf`), cockpit monobloc, leviers et cocottes, roues hautes de 50 mm à
## disque et rayons plats, transmission 2×12 électronique (plateaux dentés,
## cassette, chaîne animée, dérailleurs), selle, bidons et compteur.
##
## Repère : l'avant pointe vers -Z, la droite du cycliste vers +X, le sol sous
## les pneus est à y = 0. Les points de contact du cycliste (pédales, selle,
## cocottes, bas du cintre) sont exposés par les fonctions statiques plus bas.
## Seuls la peinture et les bidons changent d'une tenue à l'autre.

const WHEEL_RADIUS := 0.338
## Pédalier 7 cm sous les axes de roues.
const BB_Y := WHEEL_RADIUS - 0.07
const BB_Z := 0.06
## Axes des roues : avancée et hauteur par rapport au pédalier.
const REAR_F := -0.405
const FRONT_F := 0.585
const AXLE_H := 0.07
## Centre du cintre (avancée, hauteur) par rapport au pédalier, demi-écart des cocottes.
const BAR_F := 0.486
const BAR_H := 0.574
const BAR_X := 0.205
const CRANK := 0.17
## Écart latéral du centre des pédales.
const PEDAL_X := 0.112
## Dessus de la selle au-dessus du pédalier et avancée du point d'appui.
const SADDLE_H := 0.700
const SADDLE_F := -0.25

## Cadre en vue de côté (avancée, hauteur par rapport au pédalier) : axe de
## direction (73°), bas et longueur de la douille, axe du tube de selle (73,5°).
const HEAD_AXIS := Vector2(-0.2924, 0.9563)
const HEAD_BOTTOM := Vector2(0.437, 0.415)
const HEAD_LEN := 0.155
const SEAT_AXIS := Vector2(-0.284, 0.959)

## Transmission : plateaux, cassette 11-30 en 12 vitesses, pignon sous la
## chaîne, plan de la chaîne, pas de chaîne.
const RINGS := [52, 36]
const RING_X := [0.047, 0.0398]
const COGS := [11, 12, 13, 14, 15, 16, 17, 19, 21, 24, 27, 30]
const COG_X0 := 0.0635
const COG_STEP := 0.00345
const CHAIN_COG := 5
const CHAIN_X := 0.0465
const LINK := 0.0127
## Galets du dérailleur (centres par rapport à l'axe arrière), 12 dents.
const PULLEY_UP := Vector2(-0.022, -0.067)
const PULLEY_LOW := Vector2(-0.004, -0.145)
const PULLEY_TEETH := 12
## Disques de 160 mm : plan de chaque disque (côté gauche).
const ROTOR_R := 0.08
const ROTOR_X_FRONT := -0.0425
const ROTOR_X_REAR := -0.057

## Erreurs (m) des niveaux de détail des pièces modelées en SDF.
const LOD_ERRORS := [0.0012, 0.004]
## Les maillages sont longs à construire (quelques secondes) : on les garde
## dans le dossier utilisateur. Changer CACHE_VERSION si le modelage de
## l'extension (VeloSdf) change.
const CACHE_DIR := "user://cache"
const CACHE_VERSION := 2
static var use_cache := true

# Matériaux partagés (indices des surfaces).
enum { M_PAINT, M_CARBON, M_RIM, M_PLAIN, M_METAL, M_TYRE, M_TAPE, M_CHAIN }

# Primitives et opérations de VeloSdf.
const RBOX := 0
const ELLIPSOID := 1
const CYL := 2
const GEAR := 3
const HOLES := 4
const POLYGON := 5
const UNION := 0
const SUBTRACT := 1

const BLACK_GLOSS := Color(0.03, 0.03, 0.033, 0.25)
const RUBBER := Color(0.045, 0.045, 0.048, 0.78)
const ALU_DARK := Color(0.16, 0.16, 0.17, 0.32)
const ALU_GREY := Color(0.42, 0.42, 0.44, 0.3)
const SILVER := Color(0.86, 0.86, 0.88, 0.2)
const STEEL := Color(0.7, 0.7, 0.72, 0.26)
const SPOKE := Color(0.08, 0.08, 0.085, 0.35)
const GUM := Color(0.64, 0.47, 0.3, 1.0)
const TREAD := Color(0.07, 0.07, 0.07, 1.0)
const KIT_MARK := Color(1.0, 0.0, 1.0, 0.35)
const WHITE := Color(0.92, 0.92, 0.93, 0.3)

## Couleurs de la tenue, à fixer avant l'ajout à la scène.
var frame_color := Color(0.72, 0.04, 0.06)
var accent_color := Color(0.95, 0.95, 0.95)
var bottle_color := Color(0.06, 0.26, 0.70)

## Maillages partagés par tous les vélos, matériaux par jeu de couleurs.
static var _meshes := {}
static var _materials := {}
static var _kit_materials := {}
static var _chain_scale := 1.0

var _frame: MeshInstance3D
var _wheels: Array[MeshInstance3D] = []
var _crankset: MeshInstance3D
var _pedals: Array[MeshInstance3D] = []
var _chain: MeshInstance3D
var _crank_angle := 0.0


func _ready() -> void:
	_build_meshes()
	var mats := _kit(frame_color, accent_color, bottle_color)
	_frame = _instance(_meshes["frame"], mats)
	add_child(_frame)
	for key: String in ["wheel_rear", "wheel_front"]:
		var w := _instance(_meshes[key], mats)
		w.position = P(REAR_F if key == "wheel_rear" else FRONT_F, AXLE_H)
		add_child(w)
		_wheels.append(w)
	_crankset = _instance(_meshes["crank"], mats)
	_crankset.position = P(0.0, 0.0)
	add_child(_crankset)
	for i in 2:
		var p := _instance(_meshes["pedal"], mats)
		add_child(p)
		_pedals.append(p)
	_chain = _instance(_meshes["chain"], mats)
	add_child(_chain)
	set_crank(0.0)
	for i in 2:
		set_pedal(i, Basis())


# --- Points de contact du cycliste --------------------------------------------

## Point du vélo : avancée et hauteur par rapport au pédalier, écart latéral.
static func P(f: float, h: float, x := 0.0) -> Vector3:
	return Vector3(x, BB_Y + h, BB_Z - f)


static func PV(v: Vector2, x := 0.0) -> Vector3:
	return P(v.x, v.y, x)


## Direction 3D d'un vecteur de la vue de côté (avancée, hauteur).
static func D(v: Vector2) -> Vector3:
	return Vector3(0.0, v.y, -v.x)


## Signe du côté : 0 gauche (-X), 1 droite (+X).
static func _side(side: int) -> float:
	return -1.0 if side == 0 else 1.0


## Axe de la pédale `side` (0 gauche, 1 droite) pour l'angle de pédalier
## `crank_angle` (0 : manivelle gauche en haut ; le pédalage l'augmente).
static func pedal_position(side: int, crank_angle: float) -> Vector3:
	var ang := crank_angle + (0.0 if side == 0 else PI)
	return Vector3(_side(side) * PEDAL_X, BB_Y + CRANK * cos(ang), BB_Z - CRANK * sin(ang))


## Creux de la main posée sur la cocotte `side`.
static func hood_point(side: int) -> Vector3:
	return P(BAR_F + 0.10, BAR_H + 0.045, _side(side) * BAR_X)


## Creux de la main dans le bas du cintre, côté `side`.
static func drop_point(side: int) -> Vector3:
	return P(BAR_F + 0.025, BAR_H - 0.112, _side(side) * (BAR_X + 0.005))


## Milieu de la zone d'appui sur le dessus de la selle.
static func saddle_point() -> Vector3:
	return P(SADDLE_F, SADDLE_H)


static func bottom_bracket() -> Vector3:
	return P(0.0, 0.0)


static func rear_axle() -> Vector3:
	return P(REAR_F, AXLE_H)


static func front_axle() -> Vector3:
	return P(FRONT_F, AXLE_H)


# --- Animation -----------------------------------------------------------------

## Tourne le pédalier (même convention que `pedal_position`) et fait défiler la chaîne.
func set_crank(angle: float) -> void:
	_crank_angle = angle
	if _crankset == null:
		return
	_crankset.rotation.x = -angle
	# Le brin supérieur avance quand on pédale : le motif recule le long du trajet.
	var travel := angle * _pitch_radius(RINGS[0])
	_chain.set_instance_shader_parameter("chain_offset", -fposmod(travel, 2.0 * LINK * 64.0))


## Tourne les roues (angle autour de l'axe X, négatif en avançant).
func set_wheel(angle: float) -> void:
	for w in _wheels:
		w.rotation.x = angle


## Oriente la pédale `side` sur son axe, au bout de la manivelle.
func set_pedal(side: int, basis: Basis) -> void:
	if side < _pedals.size():
		_pedals[side].transform = Transform3D(basis, pedal_position(side, _crank_angle))


# --- Matériaux -------------------------------------------------------------------

static func _shader_mat(path: String, params := {}) -> ShaderMaterial:
	var m := ShaderMaterial.new()
	m.shader = load(path)
	for k: String in params:
		m.set_shader_parameter(k, params[k])
	return m


static func _shared_materials() -> void:
	if not _materials.is_empty():
		return
	_materials[M_CARBON] = _shader_mat("res://shaders/bike_carbon.gdshader", {"weave_strength": 0.55})
	_materials[M_RIM] = _shader_mat("res://shaders/bike_carbon.gdshader", {"roughness": 0.5, "clearcoat": 0.25})
	_materials[M_PLAIN] = _shader_mat("res://shaders/bike_generic.gdshader")
	_materials[M_METAL] = _shader_mat("res://shaders/bike_generic.gdshader", {"metallic": 0.9})
	_materials[M_TYRE] = _shader_mat("res://shaders/bike_tyre.gdshader")
	_materials[M_TAPE] = _shader_mat("res://shaders/bike_tape.gdshader")
	_materials[M_CHAIN] = _shader_mat("res://shaders/bike_chain.gdshader")
	_materials[M_PAINT] = _shader_mat("res://shaders/bike_paint.gdshader")


## Matériaux d'un jeu de couleurs : peinture (inscriptions contrastées) et bidons.
static func _kit(frame: Color, accent: Color, bottle: Color) -> Dictionary:
	_shared_materials()
	var key := "%s/%s/%s" % [frame.to_html(), accent.to_html(), bottle.to_html()]
	if not _kit_materials.has(key):
		var mats := _materials.duplicate()
		var light := frame.get_luminance() > 0.55
		mats[M_PAINT] = _shader_mat("res://shaders/bike_paint.gdshader", {
			"frame_color": frame,
			"decal_color": Color(0.05, 0.05, 0.06) if light else Color(0.95, 0.95, 0.95),
			"accent_color": accent,
		})
		mats[M_PLAIN] = _shader_mat("res://shaders/bike_generic.gdshader", {"kit_color": bottle})
		_kit_materials[key] = mats
	return _kit_materials[key]


func _instance(mesh: ArrayMesh, mats: Dictionary) -> MeshInstance3D:
	var mi := MeshInstance3D.new()
	mi.mesh = mesh
	var ids: PackedInt32Array = mesh.get_meta("materials")
	for i in ids.size():
		mi.set_surface_override_material(i, mats[ids[i]])
	return mi


# --- Construction ------------------------------------------------------------------

## Fichier de cache des maillages (`key` : nom du maillage). Le nom dépend de
## CACHE_VERSION et du source de ce script : toute retouche le renouvelle.
static func _cache_file(key: String) -> String:
	var src := FileAccess.get_file_as_string("res://scripts/bike.gd")
	return "%s/velo_%d_%08x_%s.res" % [CACHE_DIR, CACHE_VERSION, src.hash() & 0xffffffff, key]


static func _build_meshes() -> void:
	if not _meshes.is_empty():
		return
	var t0 := Time.get_ticks_msec()
	_shared_materials()
	var keys := ["frame", "wheel_rear", "wheel_front", "crank", "pedal", "chain"]
	var cached := use_cache
	for key: String in keys:
		cached = cached and ResourceLoader.exists(_cache_file(key))
	if cached:
		for key: String in keys:
			_meshes[key] = ResourceLoader.load(_cache_file(key), "ArrayMesh", ResourceLoader.CACHE_MODE_IGNORE)
		_chain_scale = _meshes["chain"].get_meta("scale", 1.0)
		(_materials[M_CHAIN] as ShaderMaterial).set_shader_parameter("scale", _chain_scale)
		print("velo: vélo chargé du cache en %d ms" % [Time.get_ticks_msec() - t0])
		return
	if not ClassDB.class_exists("VeloSdf"):
		push_error("Extension velotrace absente : vélo sans cadre.")
	_meshes["frame"] = _frame_mesh()
	_meshes["wheel_rear"] = _wheel_mesh(true)
	_meshes["wheel_front"] = _wheel_mesh(false)
	_meshes["crank"] = _crank_mesh()
	_meshes["pedal"] = _pedal_mesh()
	_meshes["chain"] = _chain_mesh()
	_meshes["chain"].set_meta("scale", _chain_scale)
	if use_cache and ClassDB.class_exists("VeloSdf"):
		DirAccess.make_dir_recursive_absolute(CACHE_DIR)
		# Les anciennes versions ne servent plus.
		for f in DirAccess.get_files_at(CACHE_DIR):
			if f.begins_with("velo_"):
				DirAccess.remove_absolute(CACHE_DIR.path_join(f))
		for key: String in keys:
			ResourceSaver.save(_meshes[key], _cache_file(key), ResourceSaver.FLAG_COMPRESS)
	var tris := 0
	for k: String in _meshes:
		var m: ArrayMesh = _meshes[k]
		var n := 0
		for s in m.get_surface_count():
			n += m.surface_get_array_index_len(s) / 3
		tris += n * (2 if k == "pedal" else 1)
	print("velo: vélo construit en %d ms, %d triangles" % [Time.get_ticks_msec() - t0, tris])


static func _sdf() -> Object:
	if ClassDB.class_exists("VeloSdf"):
		return ClassDB.instantiate("VeloSdf")
	return null


## Sections de tubes (7 nombres : cercle avant u, v, r ; cercle arrière u, v,
## r ; troncature), u latéral, v le long de la corde.
static func _sec_round(r: float) -> PackedFloat32Array:
	return PackedFloat32Array([0.0, 0.0, r, 0.0, 0.0, r, 0.0])


## Ovale de demi-largeur `w` (u) et demi-profondeur `d` (v).
static func _sec_oval(w: float, d: float) -> PackedFloat32Array:
	if w >= d:
		return PackedFloat32Array([-(w - d), 0.0, d, w - d, 0.0, d, 0.0])
	return PackedFloat32Array([0.0, d - w, w, 0.0, -(d - w), w, 0.0])


## Profil aéro : demi-largeur `w`, bord d'attaque à `nose` devant l'axe, bord
## de fuite à `tail` derrière (arrondi `tail_r`), tronqué à `cut` si > 0.
static func _sec_aero(w: float, nose: float, tail: float, tail_r: float, cut := 0.0) -> PackedFloat32Array:
	return PackedFloat32Array([0.0, nose - w, w, 0.0, -(tail - tail_r), tail_r, cut])


static func _catmull(p0: Vector3, p1: Vector3, p2: Vector3, p3: Vector3, t: float) -> Vector3:
	var t2 := t * t
	var t3 := t2 * t
	return 0.5 * ((2.0 * p1) + (-p0 + p2) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)


## Courbe de Catmull-Rom passant par `ctrl`, `per` pas par intervalle ;
## `params` reçoit le paramètre (indice de contrôle fractionnaire) de chaque point.
static func _smooth(ctrl: Array, per: int, params: PackedFloat32Array = PackedFloat32Array()) -> PackedVector3Array:
	var out := PackedVector3Array()
	var n := ctrl.size()
	for i in n - 1:
		var p0: Vector3 = ctrl[maxi(i - 1, 0)]
		var p1: Vector3 = ctrl[i]
		var p2: Vector3 = ctrl[i + 1]
		var p3: Vector3 = ctrl[mini(i + 2, n - 1)]
		if i == 0:
			p0 = p1 * 2.0 - p2
		if i + 2 > n - 1:
			p3 = p2 * 2.0 - p1
		var steps := per if i < n - 2 else per + 1
		for j in steps:
			var t := float(j) / per
			out.append(_catmull(p0, p1, p2, p3, t))
			params.append(i + t)
	return out


## Tube SDF lissé par les points de contrôle, sections et cordes interpolées.
static func _tube(sdf: Object, ctrl: Array, secs: Array, chords: Variant, k: float, mat: int, col: Color, per := 6, op := UNION) -> void:
	if sdf == null:
		return
	var params := PackedFloat32Array()
	var pts := _smooth(ctrl, per if ctrl.size() > 2 else 1, params)
	var sec := PackedFloat32Array()
	var ch := PackedVector3Array()
	for t: float in params:
		var i := mini(int(t), ctrl.size() - 2)
		var f := t - i
		var a: PackedFloat32Array = secs[mini(i, secs.size() - 1)]
		var b: PackedFloat32Array = secs[mini(i + 1, secs.size() - 1)]
		for q in 7:
			sec.append(lerpf(a[q], b[q], f))
		if chords is Vector3:
			ch.append(chords)
		else:
			var ca: Vector3 = chords[mini(i, chords.size() - 1)]
			var cb: Vector3 = chords[mini(i + 1, chords.size() - 1)]
			ch.append(ca.lerp(cb, f).normalized())
	sdf.tube(pts, sec, ch, op, k, mat, col)


## Base orthonormée dont l'axe Y suit `dir`.
static func _basis_y(dir: Vector3, hint := Vector3.FORWARD) -> Basis:
	var y := dir.normalized()
	var x := hint.cross(y)
	if x.length() < 1e-4:
		x = Vector3.UP.cross(y)
	x = x.normalized()
	return Basis(x, y, x.cross(y).normalized())


## Repère d'extrusion latérale : polygones donnés en (avancée, -hauteur).
static func _side_xf(x: float) -> Transform3D:
	return Transform3D(Basis(Vector3(0, 0, -1), Vector3(1, 0, 0), Vector3(0, -1, 0)), P(0.0, 0.0, x))


static func _shape(sdf: Object, kind: int, xf: Transform3D, params: Array, k: float, mat: int, col: Color, op := UNION) -> void:
	if sdf != null:
		sdf.shape(kind, xf, PackedFloat32Array(params), op, k, mat, col)


## Cylindre SDF d'axe a → b.
static func _cyl(sdf: Object, a: Vector3, b: Vector3, r: float, round: float, k: float, mat: int, col: Color, op := UNION) -> void:
	_shape(sdf, CYL, Transform3D(_basis_y(b - a), (a + b) * 0.5), [r, a.distance_to(b) * 0.5, round], k, mat, col, op)


## Contour convexe de deux cercles (vue de côté), `n` points par demi-tour.
static func _hull2(c1: Vector2, r1: float, c2: Vector2, r2: float, n := 12) -> PackedVector2Array:
	var base := (c2 - c1).angle()
	var a := PI / 2.0 - asin(clampf((r1 - r2) / c1.distance_to(c2), -1.0, 1.0))
	var out := PackedVector2Array()
	for j in n + 1:
		out.append(c1 + Vector2.from_angle(base + a + (TAU - 2.0 * a) * j / n) * r1)
	for j in n + 1:
		out.append(c2 + Vector2.from_angle(base - a + 2.0 * a * j / n) * r2)
	return out


## Polygone de la vue de côté, extrudé latéralement autour de x.
static func _side_poly(sdf: Object, pts: PackedVector2Array, x: float, half_t: float, round: float, k: float, mat: int, col: Color, op := UNION) -> void:
	var params := [half_t, round]
	for p in pts:
		params.append(p.x)
		params.append(-p.y)
	_shape(sdf, POLYGON, _side_xf(x), params, k, mat, col, op)


static func _pitch_radius(teeth: int) -> float:
	return LINK / (2.0 * sin(PI / teeth))


# --- Cadre, fourche, cockpit, selle, transmission fixe ---------------------------------

static func _frame_mesh() -> ArrayMesh:
	var parts := Parts.new()
	var A := HEAD_AXIS
	var HB := HEAD_BOTTOM
	var HT := HB + A * HEAD_LEN
	var SA := SEAT_AXIS
	var R := Vector2(REAR_F, AXLE_H)
	var F := Vector2(FRONT_F, AXLE_H)
	var fwd_st := Vector2(SA.y, -SA.x)
	var paint := Color(0, 0, 0, 0)
	var FWD := Vector3(0, 0, -1)

	# Cadre.
	var s := _sdf()
	_shape(s, CYL, Transform3D(_basis_y(Vector3.RIGHT), P(0.0, 0.0)), [0.0235, 0.0435, 0.004], 0.0, M_PAINT, paint)
	# Tube diagonal : profil tronqué, bord d'attaque vers la roue avant.
	var dt0 := Vector2(0.004, 0.012)
	var dt1 := HB + A * 0.045 + Vector2(-0.014, 0.0)
	var dd := (dt1 - dt0).normalized()
	var d_lead := Vector2(dd.y, -dd.x)
	_tube(s, [PV(dt0), PV(dt0.lerp(dt1, 0.35)), PV(dt0.lerp(dt1, 0.75)), PV(dt1)],
		[_sec_aero(0.025, 0.029, 0.031, 0.011, 0.025), _sec_aero(0.0222, 0.026, 0.034, 0.009, 0.029),
		_sec_aero(0.021, 0.025, 0.036, 0.009, 0.031), _sec_aero(0.0205, 0.025, 0.037, 0.009, 0.032)],
		D(d_lead), 0.022, M_PAINT, paint)
	# Douille de direction conique et profilée.
	_tube(s, [PV(HB - A * 0.004), PV(HT)], [_sec_aero(0.0285, 0.031, 0.05, 0.012), _sec_aero(0.0215, 0.024, 0.042, 0.01)],
		FWD, 0.02, M_PAINT, paint)
	# Tube de selle : profil tronqué qui épouse la roue arrière.
	var st_ctrl := []
	var st_secs := []
	for sv: float in [-0.01, 0.07, 0.14, 0.21, 0.28, 0.35, 0.42, 0.49, 0.565]:
		var p := SA * sv + fwd_st * _seat_bow(sv)
		var gap := p.distance_to(R) - WHEEL_RADIUS - 0.0075
		var tail := clampf(gap, 0.014, 0.036)
		st_ctrl.append(PV(p))
		st_secs.append(_sec_aero(lerpf(0.021, 0.0176, sv / 0.565), 0.0165, tail, 0.0065, tail - 0.0022))
	_tube(s, st_ctrl, st_secs, D(fwd_st), 0.018, M_PAINT, paint, 3)
	# Tube supérieur incliné, aplati.
	var tt0 := HB + A * 0.128 + Vector2(-0.016, 0.0)
	var tt1 := SA * 0.505 + fwd_st * 0.004
	_tube(s, [PV(tt0), PV(tt0.lerp(tt1, 0.5) + Vector2(0.0, -0.005)), PV(tt1)],
		[_sec_oval(0.0168, 0.0125), _sec_oval(0.0145, 0.011), _sec_oval(0.0128, 0.0105)], Vector3.UP, 0.02, M_PAINT, paint)
	# Haubans abaissés et bases asymétriques.
	var ss0 := SA * 0.37 + fwd_st * (_seat_bow(0.37) - 0.004)
	var ss1 := R + Vector2(0.01, 0.016)
	var sd := (ss1 - ss0).normalized()
	var ss_lead := Vector2(-sd.y, sd.x)
	for sx: float in [-1.0, 1.0]:
		var ctrl := []
		for u: float in [0.0, 0.12, 0.3, 0.6, 1.0]:
			var x := 0.008 + 0.0585 * (1.0 - pow(1.0 - u, 2.2))
			ctrl.append(PV(ss0.lerp(ss1, u), sx * x))
		_tube(s, ctrl, [_sec_aero(0.0072, 0.008, 0.013, 0.004), _sec_aero(0.0068, 0.0076, 0.012, 0.0038),
			_sec_aero(0.0064, 0.007, 0.0105, 0.0036), _sec_aero(0.006, 0.0065, 0.009, 0.0035)], D(ss_lead), 0.012, M_PAINT, paint, 4)
		var drive := sx > 0.0
		var cs := [P(-0.004, 0.0, sx * (0.028 if drive else 0.03)), P(-0.09, 0.011, sx * (0.029 if drive else 0.034)),
			P(-0.2, 0.034, sx * (0.047 if drive else 0.05)), P(-0.31, 0.055, sx * 0.062), P(R.x + 0.006, R.y - 0.001, sx * 0.0675)]
		var secs := [_sec_oval(0.0095, 0.017), _sec_oval(0.009, 0.0155), _sec_oval(0.0085, 0.0125), _sec_oval(0.0075, 0.0105), _sec_oval(0.0062, 0.009)] if drive \
			else [_sec_oval(0.0105, 0.0135), _sec_oval(0.0105, 0.013), _sec_oval(0.009, 0.0115), _sec_oval(0.0078, 0.0098), _sec_oval(0.0062, 0.009)]
		_tube(s, cs, secs, Vector3.UP, 0.015, M_PAINT, paint, 4)
		_shape(s, RBOX, Transform3D(Basis(Vector3.RIGHT, 0.35), PV(R + Vector2(0.004, 0.004), sx * 0.0685)), [0.0042, 0.017, 0.016, 0.0035], 0.008, M_PAINT, paint)
	parts.sdf(s, 0.0019, 0.00045)
	_frame_decals(parts, s, dt0, dt1)

	# Fourche : té intégré sous la douille, fourreaux profilés.
	var fk := _sdf()
	_tube(fk, [PV(HB - A * 0.0065), PV(HB - A * 0.026)], [_sec_aero(0.03, 0.033, 0.042, 0.014), _sec_aero(0.034, 0.034, 0.036, 0.016)],
		FWD, 0.0, M_PAINT, paint)
	for sx: float in [-1.0, 1.0]:
		var top := HB - A * 0.021 + Vector2(0.004, 0.0)
		var end := F + Vector2(-0.002, 0.012)
		_tube(fk, [PV(top, sx * 0.034), PV(top.lerp(end, 0.35) + Vector2(0.004, 0.0), sx * 0.043),
			PV(top.lerp(end, 0.7) + Vector2(0.006, 0.0), sx * 0.051), PV(end, sx * 0.0565)],
			[_sec_aero(0.0125, 0.017, 0.028, 0.004), _sec_aero(0.0105, 0.015, 0.024, 0.0035), _sec_aero(0.0085, 0.012, 0.019, 0.003),
			_sec_aero(0.0068, 0.0095, 0.014, 0.003)], FWD, 0.018, M_PAINT, paint)
		_shape(fk, RBOX, Transform3D(Basis(Vector3.RIGHT, 0.3), PV(F + Vector2(0.0, 0.004), sx * 0.0565)), [0.0042, 0.012, 0.014, 0.0035], 0.006, M_PAINT, paint)
	parts.sdf(fk, 0.0016, 0.00045)

	# Tige de selle profilée et chariot.
	var sp := _sdf()
	_tube(sp, [PV(SA * 0.49 + fwd_st * 0.001), PV(SA * 0.672)], [_sec_aero(0.0158, 0.0145, 0.03, 0.0055, 0.027)], D(fwd_st), 0.0, M_PAINT, paint)
	_shape(sp, RBOX, Transform3D(Basis(), P(-0.197, 0.657)), [0.02, 0.0075, 0.03, 0.006], 0.012, M_PAINT, paint)
	parts.sdf(sp, 0.0015, 0.00045)

	_cockpit(parts)
	_saddle(parts)
	_drivetrain(parts)
	_brakes_and_axles(parts)
	_bottles(parts, dt0, dt1)
	return parts.mesh()


## Cambrure du tube de selle vers l'avant pour contourner la roue.
static func _seat_bow(sv: float) -> float:
	return 0.008 * sin(PI * clampf(sv / 0.42, 0.0, 1.0))


## Inscription « velotrace » sur les flancs du tube diagonal.
static func _frame_decals(parts: Parts, s: Object, dt0: Vector2, dt1: Vector2) -> void:
	if s == null:
		return
	var dd := (dt1 - dt0).normalized()
	var up := D(Vector2(-dd.y, dd.x))
	var along := D(dd)
	var mid := PV(dt0.lerp(dt1, 0.53) + Vector2(-dd.y, dd.x) * 0.0)
	var g := Geo.new()
	for sx: float in [-1.0, 1.0]:
		var dir := along * sx
		var out := Vector3(sx, 0.0, 0.0)
		var h := 0.022
		var strokes := Glyphs.text("velotrace")
		var width := Glyphs.width("velotrace") * h
		var origin := mid - dir * width * 0.5 - up * h * 0.62
		Glyphs.decal(g, strokes, h, 0.2, func(q: Vector2) -> Vector3:
			return origin + dir * q.x + up * q.y + out * 0.04, s, Color(1, 1, 1, 1))
	parts.geo(M_PAINT, g)


## Potence et cintre monobloc en carbone, cocottes, leviers, guidoline,
## support et compteur.
static func _cockpit(parts: Parts) -> void:
	var A := HEAD_AXIS
	var HT := HEAD_BOTTOM + A * HEAD_LEN
	var FWD := Vector3(0, 0, -1)
	var c := _sdf()
	var carbon := Color(1, 1, 1, 0)
	# Capot de jeu de direction et collier de potence profilés.
	_tube(c, [PV(HT + A * 0.0012), PV(HT + A * 0.0062)], [_sec_aero(0.0213, 0.0238, 0.0415, 0.0098)], FWD, 0.0, M_CARBON, carbon)
	_tube(c, [PV(HT + A * 0.0068), PV(HT + A * 0.041)], [_sec_aero(0.0195, 0.0215, 0.024, 0.012), _sec_aero(0.019, 0.021, 0.022, 0.012)], FWD, 0.0, M_CARBON, carbon)
	_cyl(c, PV(HT + A * 0.04), PV(HT + A * 0.0435), 0.0165, 0.0012, 0.004, M_CARBON, carbon)
	var s0 := HT + A * 0.024
	_tube(c, [PV(s0 + Vector2(0.008, 0.0)), PV(Vector2(BAR_F, BAR_H))], [_sec_oval(0.0145, 0.0175), _sec_oval(0.0125, 0.0135)], Vector3.UP, 0.012, M_CARBON, carbon)
	for sx: float in [-1.0, 1.0]:
		var path := _bar_ctrl(sx)
		var ctrl := path.slice(0, 7)
		var secs := []
		var chords := []
		for i in ctrl.size():
			secs.append(_sec_aero(0.0085, 0.013, 0.03, 0.0035) if i < 3 else _sec_round(0.0118))
			chords.append(FWD)
		_tube(c, ctrl, secs, chords, 0.012, M_CARBON, carbon, 6)
	# Support avancé et compteur.
	_tube(c, [P(BAR_F + 0.01, BAR_H - 0.004), P(BAR_F + 0.07, BAR_H - 0.011)], [_sec_oval(0.011, 0.0045)], Vector3.UP, 0.008, M_CARBON, carbon)
	_shape(c, RBOX, Transform3D(Basis(), P(BAR_F + 0.074, BAR_H - 0.011)), [0.0165, 0.0045, 0.016, 0.003], 0.006, M_CARBON, carbon)
	parts.sdf(c, 0.0012, 0.00025)

	# Compteur : boîtier sombre incliné vers le cycliste, écran clair.
	var cu := _sdf()
	var tilt := Basis(Vector3.RIGHT, 0.12)
	var cxf := Transform3D(tilt, P(BAR_F + 0.08, BAR_H + 0.002))
	_shape(cu, RBOX, cxf, [0.0255, 0.0085, 0.0415, 0.006], 0.0, M_PLAIN, Color(0.11, 0.11, 0.12, 0.4))
	parts.sdf(cu, 0.001, 0.00025)
	var scr := Geo.new()
	var top := cxf.origin + tilt.y * 0.0087
	var cols := [Color(0.62, 0.66, 0.62, 0.15), Color(0.08, 0.08, 0.09, 0.15), Color(0.62, 0.66, 0.62, 0.15)]
	var nrows := 6
	for i in nrows:
		for j in 2:
			var u0 := lerpf(-0.0205, 0.0205, j / 2.0)
			var u1 := lerpf(-0.0205, 0.0205, (j + 1) / 2.0)
			var v0 := lerpf(-0.033, 0.033, float(i) / nrows)
			var v1 := lerpf(-0.033, 0.033, float(i + 1) / nrows)
			var col: Color = cols[0]
			scr.quad([top + tilt.x * u0 + tilt.z * v0, top + tilt.x * u1 + tilt.z * v0, top + tilt.x * u1 + tilt.z * v1, top + tilt.x * u0 + tilt.z * v1], tilt.y, col)
			# Chiffres : blocs sombres au centre de chaque champ.
			var cu0 := lerpf(u0, u1, 0.22)
			var cu1 := lerpf(u0, u1, 0.78 if i % 2 == 0 else 0.6)
			var cv0 := lerpf(v0, v1, 0.25)
			var cv1 := lerpf(v0, v1, 0.75)
			var lift := tilt.y * 0.00015
			scr.quad([top + lift + tilt.x * cu0 + tilt.z * cv0, top + lift + tilt.x * cu1 + tilt.z * cv0, top + lift + tilt.x * cu1 + tilt.z * cv1,
				top + lift + tilt.x * cu0 + tilt.z * cv1], tilt.y, cols[1])
	parts.geo(M_PLAIN, scr)

	# Cocottes en caoutchouc et leviers en carbone.
	var hd := _sdf()
	var lv := _sdf()
	for sx: float in [-1.0, 1.0]:
		var x := sx * BAR_X
		var hood := [Vector2(0.05, -0.004), Vector2(0.08, 0.002), Vector2(0.112, 0.006), Vector2(0.138, 0.012), Vector2(0.155, 0.024), Vector2(0.163, 0.04)]
		var ctrl := []
		for v: Vector2 in hood:
			ctrl.append(P(BAR_F + v.x, BAR_H + v.y, x))
		_tube(hd, ctrl, [_sec_oval(0.0128, 0.0145), _sec_oval(0.0148, 0.0215), _sec_oval(0.0148, 0.022), _sec_oval(0.0135, 0.019),
			_sec_oval(0.011, 0.014), _sec_oval(0.0075, 0.007)], Vector3.UP, 0.0, M_PLAIN, RUBBER, 5)
		_shape(hd, ELLIPSOID, Transform3D(Basis(), P(BAR_F + 0.118, BAR_H - 0.016, x)), [0.0118, 0.017, 0.021], 0.012, M_PLAIN, RUBBER)
		var blade := [Vector2(0.15, -0.01), Vector2(0.161, -0.04), Vector2(0.159, -0.075), Vector2(0.147, -0.108), Vector2(0.128, -0.132)]
		var bctrl := []
		for i in blade.size():
			bctrl.append(P(BAR_F + blade[i].x, BAR_H + blade[i].y, sx * (BAR_X + 0.0028 * i)))
		_tube(lv, bctrl, [_sec_oval(0.0058, 0.0095), _sec_oval(0.0052, 0.0085), _sec_oval(0.0048, 0.0075), _sec_oval(0.0045, 0.0068),
			_sec_oval(0.004, 0.006)], FWD, 0.0, M_CARBON, carbon, 5)
		var paddle := [P(BAR_F + 0.141, BAR_H - 0.022, x - sx * 0.002), P(BAR_F + 0.146, BAR_H - 0.05, x - sx * 0.003), P(BAR_F + 0.139, BAR_H - 0.08, x - sx * 0.002)]
		_tube(lv, paddle, [_sec_oval(0.0035, 0.0068), _sec_oval(0.0032, 0.006), _sec_oval(0.003, 0.0052)], FWD, 0.0, M_PLAIN, BLACK_GLOSS, 5)
	parts.sdf(hd, 0.0012, 0.0004)
	parts.sdf(lv, 0.0009, 0.0003)

	# Guidoline du virage jusqu'au bout du cintre, embouts.
	var tape := Geo.new()
	for sx: float in [-1.0, 1.0]:
		var path := _bar_ctrl(sx).slice(3)
		var pts := _smooth(path, 5)
		pts.remove_at(0)
		tape.sweep(pts, PackedFloat32Array([0.0128]), 10, Color(0.035, 0.035, 0.038, 1.0), true, Vector3.UP)
	parts.geo(M_TAPE, tape)


## Points de contrôle de l'axe du cintre, côté `sx` : plat, virage, cocottes,
## courbe du bas et bas du cintre (avancée, hauteur relatives au centre, écart).
static func _bar_ctrl(sx: float) -> Array:
	var pts := [Vector3(0.0, 0.0, 0.0), Vector3(0.0, 0.0, 0.07), Vector3(0.0, 0.0, 0.13), Vector3(0.008, -0.001, 0.163),
		Vector3(0.03, -0.004, 0.19), Vector3(0.056, -0.008, 0.203), Vector3(0.076, -0.017, 0.2055), Vector3(0.087, -0.04, 0.2065),
		Vector3(0.086, -0.07, 0.2075), Vector3(0.072, -0.1, 0.2085), Vector3(0.048, -0.119, 0.2095), Vector3(0.02, -0.125, 0.21),
		Vector3(-0.02, -0.125, 0.21), Vector3(-0.07, -0.122, 0.21)]
	var out := []
	for v: Vector3 in pts:
		out.append(P(BAR_F + v.x, BAR_H + v.y, sx * v.z))
	return out


## Selle à bec classique, canal central, rails en carbone.
static func _saddle(parts: Parts) -> void:
	var g := Geo.new()
	var stations := 30
	var ring_n := 36
	var rings: Array[PackedVector3Array] = []
	var ring_cols: Array[PackedColorArray] = []
	var widths := [Vector2(0.0, 0.05), Vector2(0.03, 0.066), Vector2(0.1, 0.0715), Vector2(0.2, 0.068), Vector2(0.32, 0.054),
		Vector2(0.45, 0.034), Vector2(0.58, 0.023), Vector2(0.72, 0.019), Vector2(0.86, 0.017), Vector2(0.96, 0.0125), Vector2(1.0, 0.004)]
	for i in stations + 1:
		var u := float(i) / stations
		u = u * u * (3.0 - 2.0 * u) * 0.35 + u * 0.65
		# Arrière arrondi : largeur et épaisseur se referment sur les derniers millimètres.
		var cap := 0.3 + 0.7 * sqrt(clampf(u / 0.06, 0.0, 1.0))
		var w: float = _interp(widths, u) * cap
		var f := lerpf(-0.335, -0.065, u)
		var top := SADDLE_H + 0.003 * (1.0 - smoothstep(0.0, 0.12, u)) - 0.004 * smoothstep(0.8, 1.0, u)
		var thick := (lerpf(0.026, 0.015, smoothstep(0.3, 1.0, u)) * (1.0 - 0.6 * smoothstep(0.93, 1.0, u)) + 0.002) * (0.5 + 0.5 * cap)
		var droop := lerpf(0.013, 0.004, smoothstep(0.25, 0.75, u))
		var groove := 0.0035 * smoothstep(0.1, 0.22, u) * (1.0 - smoothstep(0.62, 0.78, u))
		var ring := PackedVector3Array()
		var cols := PackedColorArray()
		for j in ring_n:
			var th := TAU * j / ring_n
			var c := cos(th)
			var sn := sin(th)
			var e := 2.0 / 3.2
			var x := w * signf(c) * pow(absf(c), e)
			var yr := signf(sn) * pow(absf(sn), e)
			var v := x / maxf(w, 1e-4)
			var y := top - thick * 0.5 + yr * thick * 0.5
			if sn > 0.0:
				y -= droop * pow(absf(v), 3.0) + groove * exp(-pow(x / 0.009, 2.0))
			else:
				y += 0.004 * (1.0 - v * v) * smoothstep(0.1, 0.4, u)
			ring.append(P(f, y, x))
			# Coque sombre dessous, dessus noir, liseré blanc sur les flancs.
			var col := Color(0.03, 0.03, 0.032, 0.6) if sn > -0.15 else Color(0.07, 0.07, 0.075, 0.35)
			if absf(sn) < 0.25 and u > 0.06 and u < 0.3:
				col = Color(0.9, 0.9, 0.9, 0.5)
			cols.append(col)
		rings.append(ring)
		ring_cols.append(cols)
	g.loft(rings, ring_cols, true)
	parts.geo(M_PLAIN, g)
	var rails := Geo.new()
	for sx: float in [-1.0, 1.0]:
		var pts: Array = [P(-0.09, 0.688, sx * 0.008), P(-0.112, 0.678, sx * 0.015), P(-0.142, 0.664, sx * 0.021), P(-0.25, 0.662, sx * 0.021),
			P(-0.285, 0.668, sx * 0.027), P(-0.31, 0.684, sx * 0.034)]
		rails.sweep(_smooth(pts, 4), PackedFloat32Array([0.0036]), 8, Color(0.06, 0.06, 0.065, 0.3), true, Vector3.UP, 1.25)
	parts.geo(M_PLAIN, rails)


static func _interp(table: Array, u: float) -> float:
	for i in table.size() - 1:
		var a: Vector2 = table[i]
		var b: Vector2 = table[i + 1]
		if u <= b.x:
			var t := (u - a.x) / maxf(b.x - a.x, 1e-6)
			t = t * t * (3.0 - 2.0 * t)
			return lerpf(a.y, b.y, t)
	return table[table.size() - 1].y


## Trajet de la chaîne en vue de côté (avancée, hauteur) dans le sens du
## défilement : plateau, galet bas, galet haut, pignon.
static func _chain_path() -> PackedVector2Array:
	var R := Vector2(REAR_F, AXLE_H)
	var rp := _pitch_radius(PULLEY_TEETH)
	# Centre, rayon primitif et sens (+1 trigonométrique).
	var circles := [[Vector2.ZERO, _pitch_radius(RINGS[0]), -1.0], [R + PULLEY_LOW, rp, -1.0], [R + PULLEY_UP, rp, 1.0],
		[R, _pitch_radius(COGS[CHAIN_COG]), -1.0]]
	var n := circles.size()
	var outs: Array[Vector2] = []
	var ins: Array[Vector2] = []
	outs.resize(n)
	ins.resize(n)
	for i in n:
		var a: Array = circles[i]
		var b: Array = circles[(i + 1) % n]
		var c1: Vector2 = a[0]
		var c2: Vector2 = b[0]
		var d := c2 - c1
		var dl := d.length()
		var k: float = b[2] * b[1] - a[2] * a[1]
		var beta := asin(clampf(-k / dl, -1.0, 1.0))
		var dh := d / dl
		var dp := Vector2(-dh.y, dh.x)
		var u := dh * cos(beta) + dp * sin(beta)
		var nl := Vector2(-u.y, u.x)
		outs[i] = c1 - nl * (a[2] * a[1])
		ins[(i + 1) % n] = c2 - nl * (b[2] * b[1])
	var path := PackedVector2Array()
	for i in n:
		var c: Array = circles[i]
		var center: Vector2 = c[0]
		var r: float = c[1]
		var sg: float = c[2]
		var a0 := (ins[i] - center).angle()
		var a1 := (outs[i] - center).angle()
		var sweep := fposmod(a1 - a0, TAU) if sg > 0.0 else -fposmod(a0 - a1, TAU)
		var steps := maxi(2, int(absf(sweep) * r / 0.0035))
		for j in steps + 1:
			path.append(center + Vector2.from_angle(a0 + sweep * j / steps) * r)
	path.append(path[0])
	return path


## Dérailleurs avant et arrière (électroniques, sans câble), cassette fixée à la roue.
static func _drivetrain(parts: Parts) -> void:
	var R := Vector2(REAR_F, AXLE_H)
	var U := R + PULLEY_UP
	var L := R + PULLEY_LOW
	var rd := _sdf()
	# Patte (UDH), chape fixe, parallélogramme, batterie, chape mobile.
	_shape(rd, RBOX, Transform3D(Basis(Vector3.RIGHT, 0.5), PV(R + Vector2(-0.004, -0.012), 0.0745)), [0.0025, 0.013, 0.009, 0.002], 0.0, M_METAL, ALU_DARK)
	var bk := R + Vector2(-0.013, -0.026)
	_cyl(rd, PV(bk, 0.071), PV(bk, 0.087), 0.0085, 0.002, 0.004, M_METAL, ALU_DARK)
	_tube(rd, [PV(bk + Vector2(-0.004, -0.004), 0.08), PV(U + Vector2(-0.006, 0.008), 0.073)], [_sec_oval(0.0085, 0.012), _sec_oval(0.008, 0.0105)],
		D(Vector2(1.0, 0.6)), 0.006, M_METAL, ALU_DARK)
	_shape(rd, RBOX, Transform3D(Basis(Vector3.RIGHT, 0.75), PV(R + Vector2(-0.033, -0.034), 0.085)), [0.0075, 0.014, 0.019, 0.0045], 0.004, M_PLAIN, BLACK_GLOSS)
	_cyl(rd, PV(U, 0.066), PV(U, 0.078), 0.0095, 0.002, 0.005, M_METAL, ALU_DARK)
	# Flasques de chape : pincées entre les galets, ajourées au milieu.
	var plate := _hull2(U, 0.0128, L, 0.0142, 10)
	var mid := (U + L) * 0.5
	_side_poly(rd, plate, CHAIN_X + 0.0068, 0.0011, 0.0006, 0.0, M_CARBON, Color(1, 1, 1, 0))
	_side_poly(rd, plate, CHAIN_X - 0.0068, 0.001, 0.0005, 0.0, M_METAL, ALU_DARK)
	var along := L - U
	var win := Basis(Vector3.RIGHT, atan2(-along.x, along.y))
	for sx: float in [-1.0, 1.0]:
		_shape(rd, ELLIPSOID, Transform3D(win, PV(mid, CHAIN_X + sx * 0.0068)), [0.004, 0.0125, 0.0042], 0.0, M_METAL, ALU_DARK, SUBTRACT)
	for c: Vector2 in [U, L]:
		_cyl(rd, PV(c, CHAIN_X - 0.0075), PV(c, CHAIN_X + 0.0082), 0.0042, 0.0008, 0.0, M_METAL, SILVER)
	parts.sdf(rd, 0.0009, 0.0006)
	var pl := _sdf()
	for c: Vector2 in [U, L]:
		var rp := _pitch_radius(PULLEY_TEETH)
		_shape(pl, GEAR, Transform3D(_basis_y(Vector3.RIGHT), PV(c, CHAIN_X)), [PULLEY_TEETH, 0.004, rp + 0.0026, rp, 0.0039, 0.0014, 0.0005], 0.0, M_PLAIN, Color(0.06, 0.06, 0.065, 0.45))
		_cyl(pl, PV(c, CHAIN_X - 0.0045), PV(c, CHAIN_X + 0.0045), 0.008, 0.001, 0.002, M_PLAIN, Color(0.06, 0.06, 0.065, 0.45))
	parts.sdf(pl, 0.0007, 0.0004)

	# Dérailleur avant : moteur sur la patte du tube de selle, chape au-dessus du grand plateau.
	var fd := _sdf()
	var rr := _pitch_radius(RINGS[0]) + 0.0068
	var outer := PackedVector2Array()
	var inner := PackedVector2Array()
	var a0 := deg_to_rad(76.0)
	var a1 := deg_to_rad(124.0)
	for j in 13:
		var a := lerpf(a0, a1, j / 12.0)
		outer.append(Vector2.from_angle(a) * rr)
		inner.append(Vector2.from_angle(a) * rr)
	for j in 13:
		var a := lerpf(a1, a0, j / 12.0)
		outer.append(Vector2.from_angle(a) * (rr + 0.016 - 0.006 * smoothstep(0.6, 1.0, 1.0 - j / 12.0)))
		inner.append(Vector2.from_angle(a) * (rr + 0.009))
	_side_poly(fd, outer, 0.0575, 0.0011, 0.0006, 0.0, M_METAL, ALU_GREY)
	_side_poly(fd, inner, 0.0383, 0.0009, 0.0004, 0.0, M_METAL, ALU_DARK)
	var top := Vector2.from_angle(deg_to_rad(112.0)) * (rr + 0.013)
	_shape(fd, RBOX, Transform3D(Basis(Vector3.RIGHT, -0.35), PV(top, 0.048)), [0.0105, 0.0045, 0.011, 0.002], 0.004, M_METAL, ALU_DARK)
	var body := Vector2.from_angle(deg_to_rad(106.0)) * (rr + 0.038)
	_shape(fd, RBOX, Transform3D(Basis(Vector3.RIGHT, -0.3), PV(body, 0.039)), [0.012, 0.017, 0.019, 0.0055], 0.006, M_PLAIN, BLACK_GLOSS)
	_tube(fd, [PV(body, 0.042), PV(top, 0.05)], [_sec_oval(0.004, 0.008)], D(Vector2(1, 0)), 0.005, M_METAL, ALU_DARK)
	var mount := SEAT_AXIS * 0.2 + Vector2(SEAT_AXIS.y, -SEAT_AXIS.x) * 0.012
	_shape(fd, RBOX, Transform3D(Basis(), PV(mount.lerp(body, 0.5), 0.022)), [0.012, 0.012, 0.008, 0.003], 0.006, M_METAL, ALU_DARK)
	parts.sdf(fd, 0.001, 0.0003)


## Étriers de frein à disque (montage flat mount) et axes traversants.
static func _brakes_and_axles(parts: Parts) -> void:
	var R := Vector2(REAR_F, AXLE_H)
	var F := Vector2(FRONT_F, AXLE_H)
	var b := _sdf()
	_caliper(b, F, deg_to_rad(145.0), ROTOR_X_FRONT, F + Vector2(-0.02, 0.045), -0.05)
	_caliper(b, R, deg_to_rad(18.0), ROTOR_X_REAR, R + Vector2(0.075, 0.004), -0.064)
	parts.sdf(b, 0.0011, 0.00035)
	var g := Geo.new()
	for c: Array in [[F, -0.064, 0.058], [R, -0.079, 0.075]]:
		var center: Vector2 = c[0]
		var x0: float = c[1]
		var x1: float = c[2]
		var o := PV(center)
		var prof := PackedVector2Array([Vector2(0.0, x0 - 0.009), Vector2(0.0108, x0 - 0.009), Vector2(0.0118, x0 - 0.007),
			Vector2(0.0118, x0), Vector2(0.0064, x0), Vector2(0.0064, x1), Vector2(0.0085, x1), Vector2(0.0085, x1 + 0.006), Vector2(0.0, x1 + 0.006)])
		g.revolve_x(o, prof, 18, Callable(), ALU_DARK, 1.0)
		# Levier du blocage, à plat contre la patte.
		g.box(Transform3D(Basis(Vector3.RIGHT, 0.4), o + Vector3(x0 - 0.0115, 0.0, 0.0)), Vector3(0.004, 0.006, 0.05), ALU_DARK)
	parts.geo(M_METAL, g)


## Étrier autour du bord du disque (centre de roue `c`, angle `theta`), relié
## au point d'attache `mount` (vue de côté) à l'écart latéral `mount_x`.
static func _caliper(s: Object, c: Vector2, theta: float, rx: float, mount: Vector2, mount_x: float) -> void:
	var rdir := Vector2.from_angle(theta)
	var tdir := Vector2(-rdir.y, rdir.x)
	var center := c + rdir * (ROTOR_R - 0.006)
	var basis := Basis(Vector3.RIGHT, D(rdir), Vector3.RIGHT.cross(D(rdir)))
	for side: float in [-1.0, 1.0]:
		_shape(s, RBOX, Transform3D(basis, PV(center, rx + side * 0.0085)), [0.0062, 0.0145, 0.026, 0.0055], 0.004, M_METAL, ALU_DARK)
	_shape(s, RBOX, Transform3D(basis, PV(center + rdir * 0.0115, rx)), [0.0145, 0.0048, 0.021, 0.0035], 0.004, M_METAL, ALU_DARK)
	_tube(s, [PV(center - rdir * 0.004, rx - 0.009), PV(mount, mount_x)], [_sec_oval(0.006, 0.009)], D(tdir), 0.006, M_METAL, ALU_DARK)
	for t: float in [-1.0, 1.0]:
		_cyl(s, PV(center + tdir * t * 0.017, rx - 0.016), PV(center + tdir * t * 0.017, rx - 0.012), 0.0045, 0.001, 0.003, M_METAL, SILVER)


## Bidons et porte-bidons sur le tube diagonal et le tube de selle.
static func _bottles(parts: Parts, dt0: Vector2, dt1: Vector2) -> void:
	var dd := (dt1 - dt0).normalized()
	var dup := Vector2(-dd.y, dd.x)
	var fwd_st := Vector2(SEAT_AXIS.y, -SEAT_AXIS.x)
	var bottle := Geo.new()
	var cage := Geo.new()
	var placements := [
		[PV(dt0 + dd * 0.2 + dup * 0.0735), D(dd), D(-dup)],
		[PV(SEAT_AXIS * 0.13 + fwd_st * 0.0635), D(SEAT_AXIS), D(-fwd_st)],
	]
	for pl: Array in placements:
		var base: Vector3 = pl[0]
		var axis: Vector3 = pl[1]
		var to_tube: Vector3 = pl[2]
		var lat := axis.cross(to_tube).normalized()
		var prof := PackedVector2Array([Vector2(0.0, 0.0), Vector2(0.03, 0.0), Vector2(0.0355, 0.004), Vector2(0.0365, 0.012), Vector2(0.0365, 0.07),
			Vector2(0.0345, 0.09), Vector2(0.0345, 0.125), Vector2(0.0365, 0.145), Vector2(0.0365, 0.19), Vector2(0.033, 0.203), Vector2(0.0225, 0.206),
			Vector2(0.0225, 0.222), Vector2(0.019, 0.225), Vector2(0.0105, 0.227), Vector2(0.009, 0.238), Vector2(0.0045, 0.241), Vector2(0.0, 0.241)])
		bottle.revolve_axis(base, axis, to_tube, prof, 24, func(t: float) -> Color:
			if t > 0.204:
				return Color(0.05, 0.05, 0.055, 0.4)
			if t > 0.095 and t < 0.12:
				return Color(0.92, 0.92, 0.92, 0.35)
			return KIT_MARK)
		# Cage : dos le long du tube, deux ailes autour du bidon, crochet en bas.
		var r := 0.0395
		var cage_col := Color(0.03, 0.03, 0.033, 0.3)
		var spine := PackedVector3Array([base + axis * 0.012 + to_tube * r, base + axis * 0.17 + to_tube * r])
		cage.sweep(spine, PackedFloat32Array([0.0032]), 8, cage_col, true, lat, 2.4)
		for sg: float in [-1.0, 1.0]:
			var wing := PackedVector3Array()
			for q: Vector2 in [Vector2(0.03, 15.0), Vector2(0.065, 70.0), Vector2(0.11, 108.0), Vector2(0.15, 80.0), Vector2(0.175, 25.0)]:
				var a := deg_to_rad(q.y)
				wing.append(base + axis * q.x + (to_tube * cos(a) + lat * sg * sin(a)) * r)
			cage.sweep(_smooth(Array(wing), 4), PackedFloat32Array([0.0028]), 6, cage_col, true, axis, 1.8)
		var hook := PackedVector3Array()
		for k in 7:
			var a := deg_to_rad(lerpf(-55.0, 55.0, k / 6.0))
			hook.append(base - axis * 0.002 + (to_tube * cos(a) + lat * sin(a)) * (r - 0.01))
		cage.sweep(hook, PackedFloat32Array([0.0026]), 6, cage_col, true, axis, 1.6)
	parts.geo(M_PLAIN, bottle)
	parts.geo(M_PLAIN, cage)


# --- Roues ---------------------------------------------------------------------------

## Profil de la jante de 50 mm (rayon, demi-largeur) du talon au fond.
const RIM := [Vector2(0.3115, 0.0117), Vector2(0.3088, 0.0128), Vector2(0.3005, 0.0137), Vector2(0.29, 0.0144), Vector2(0.28, 0.0146),
	Vector2(0.272, 0.0141), Vector2(0.2665, 0.0124), Vector2(0.2632, 0.0097), Vector2(0.2614, 0.006), Vector2(0.2606, 0.0)]


static func _rim_x(r: float) -> float:
	for i in RIM.size() - 1:
		var a: Vector2 = RIM[i]
		var b: Vector2 = RIM[i + 1]
		if r <= a.x and r >= b.x:
			return lerpf(a.y, b.y, (a.x - r) / maxf(a.x - b.x, 1e-6))
	return 0.0


static func _wheel_mesh(rear: bool) -> ArrayMesh:
	var parts := Parts.new()
	var o := Vector3.ZERO
	var seg := 64
	# Pneu de 28 mm : bande de roulement noire, flancs beiges.
	var tyre := Geo.new()
	var tr := 0.0148
	var tc := WHEEL_RADIUS - tr
	var prof := PackedVector2Array()
	var vs := PackedFloat32Array()
	var tcols := []
	var rows := 22
	for k in rows + 1:
		var ph := deg_to_rad(lerpf(-132.0, 132.0, float(k) / rows))
		prof.append(Vector2(tc + tr * cos(ph), tr * sin(ph) * 0.96))
		vs.append(float(k) / rows)
		tcols.append(TREAD if absf(ph) < deg_to_rad(52.0) else GUM)
	tyre.revolve_x(o, prof, seg, func(k: int) -> Color: return tcols[k], Color(), 1.0, vs, true)
	parts.geo(M_TYRE, tyre)
	# Jante carbone : flancs, fond de jante, bord du talon.
	var rim := Geo.new()
	var rp := PackedVector2Array()
	for i in RIM.size():
		var v: Vector2 = RIM[i]
		rp.append(Vector2(v.x, v.y))
	for i in range(RIM.size() - 2, -1, -1):
		var v: Vector2 = RIM[i]
		rp.append(Vector2(v.x, -v.y))
	rp.append(Vector2(0.3078, -0.0098))
	rp.append(Vector2(0.3078, 0.0098))
	rp.append(rp[0])
	rp.reverse()
	rim.revolve_x(o, rp, seg, Callable(), Color(1, 1, 1, 0), -1.0)
	_rim_decals(rim)
	parts.geo(M_RIM, rim)

	# Moyeu, rayons plats droits, disque, cassette.
	var metal := Geo.new()
	var hub := PackedVector2Array()
	if rear:
		hub = PackedVector2Array([Vector2(0.0074, -0.0705), Vector2(0.0115, -0.0705), Vector2(0.0115, -0.066), Vector2(0.023, -0.063), Vector2(0.023, -0.056),
			Vector2(0.027, -0.0365), Vector2(0.027, -0.0315), Vector2(0.017, -0.03), Vector2(0.0155, 0.0), Vector2(0.018, 0.0165), Vector2(0.0275, 0.0175),
			Vector2(0.0275, 0.0215), Vector2(0.0165, 0.023), Vector2(0.0165, 0.066), Vector2(0.0115, 0.068), Vector2(0.0115, 0.0705), Vector2(0.0074, 0.0705)])
	else:
		hub = PackedVector2Array([Vector2(0.0074, -0.0498), Vector2(0.0115, -0.0498), Vector2(0.0115, -0.0465), Vector2(0.022, -0.044),
			Vector2(0.022, -0.0365), Vector2(0.0245, -0.0335), Vector2(0.0245, -0.0285), Vector2(0.016, -0.027), Vector2(0.0145, 0.0), Vector2(0.016, 0.0285),
			Vector2(0.0235, 0.0305), Vector2(0.0235, 0.0355), Vector2(0.0125, 0.037), Vector2(0.0115, 0.0498), Vector2(0.0074, 0.0498)])
	metal.revolve_x(o, hub, 28, Callable(), ALU_DARK, 1.0)
	# Rayons : 24 par roue, 12 par côté ; deux croisements, radiaux à droite devant.
	var flanges := [[-0.031, 0.0245, 2], [0.033, 0.0235, 0]] if not rear else [[-0.034, 0.027, 2], [0.0195, 0.0275, 2]]
	for k in 24:
		var th := TAU * k / 24.0
		var fl: Array = flanges[k % 2]
		var cross: int = fl[2]
		var hub_th := th
		if cross > 0:
			# Écart trou de jante / trou de moyeu : croisements × 720° / rayons.
			hub_th += (1.0 if (k / 2) % 2 == 0 else -1.0) * cross * TAU / 12.0
		var hx: float = fl[0]
		var hr: float = fl[1]
		var a := Vector3(hx, hr * cos(hub_th), hr * sin(hub_th))
		var b := Vector3(signf(hx) * 0.0025, 0.2625 * cos(th), 0.2625 * sin(th))
		var wide := Vector3.RIGHT
		metal.sweep(PackedVector3Array([a, b]), PackedFloat32Array([0.00105]), 6, SPOKE, false, wide, 0.42)
		# Tête de rayon dans son logement du flasque.
		metal.sweep(PackedVector3Array([a - (b - a).normalized() * 0.003, a + (b - a).normalized() * 0.003]), PackedFloat32Array([0.0022]), 6, ALU_DARK, true, wide)
	var valve_th := TAU * 0.5 / 24.0
	metal.sweep(PackedVector3Array([Vector3(0, 0.255 * cos(valve_th), 0.255 * sin(valve_th)), Vector3(0, 0.272 * cos(valve_th), 0.272 * sin(valve_th))]),
		PackedFloat32Array([0.0022]), 8, SILVER, true)
	if rear:
		_cassette(metal)
	parts.geo(M_METAL, metal)
	_rotor(parts, ROTOR_X_REAR if rear else ROTOR_X_FRONT)
	return parts.mesh()


## Inscriptions blanches sur les flancs de la jante, deux par côté.
static func _rim_decals(g: Geo) -> void:
	var h := 0.0115
	var text := "velotrace"
	var strokes := Glyphs.text(text)
	var width := Glyphs.width(text) * h
	var r_mid := 0.2905
	for sx: float in [-1.0, 1.0]:
		for start: float in [0.0, PI]:
			Glyphs.decal(g, strokes, h, 0.18, func(q: Vector2) -> Vector3:
				var r := r_mid - h * 0.5 + q.y
				var th := start - sx * (q.x - width * 0.5) / r_mid
				var x := sx * (_rim_x(r) + 0.00025)
				return Vector3(x, r * cos(th), r * sin(th)), null, Color(0.85, 0.85, 0.86, 1.0), Vector3(sx, 0.0, 0.0))


## Cassette 11-30 : pignons dentés empilés sur le corps de roue libre.
static func _cassette(g: Geo) -> void:
	for i in COGS.size():
		var n: int = COGS[i]
		var x := COG_X0 - i * COG_STEP
		var rp := _pitch_radius(n)
		var r_in := rp - 0.012 if i < COGS.size() - 1 else 0.018
		var col := STEEL if i < 9 else ALU_GREY
		g.gear_x(x, n, rp, 0.0039, rp + 0.0034, r_in, 0.0008, col, i == COGS.size() - 1)
	# Écrou de blocage.
	g.revolve_x(Vector3.ZERO, PackedVector2Array([Vector2(0.0125, COG_X0 + 0.0008), Vector2(0.0175, COG_X0 + 0.0008), Vector2(0.0175, COG_X0 + 0.0042),
		Vector2(0.0125, COG_X0 + 0.0042)]), 24, Callable(), SILVER, 1.0)


## Disque de 160 mm : piste en acier ajourée, araignée en aluminium, bague.
static func _rotor(parts: Parts, x: float) -> void:
	var band := _sdf()
	var xf := Transform3D(_basis_y(Vector3.RIGHT), Vector3(x, 0.0, 0.0))
	_shape(band, GEAR, xf, [0, 0.0615, ROTOR_R, ROTOR_R, 0.0, 0.0009, 0.0003], 0.0, M_METAL, STEEL)
	_shape(band, HOLES, xf, [30, 0.0702, 0.0021, 0.01, 0.0, 0.0], 0.0, M_METAL, STEEL, SUBTRACT)
	_shape(band, HOLES, xf, [30, 0.0758, 0.0016, 0.01, PI / 30.0, 0.0], 0.0, M_METAL, STEEL, SUBTRACT)
	parts.sdf(band, 0.0007, 0.0003)
	var carrier := _sdf()
	var cx := Transform3D(_basis_y(Vector3.RIGHT), Vector3(x + signf(x) * 0.0018, 0.0, 0.0))
	_shape(carrier, GEAR, cx, [0, 0.0175, 0.066, 0.066, 0.0, 0.0016, 0.0005], 0.0, M_METAL, ALU_DARK)
	_shape(carrier, HOLES, cx, [6, 0.042, 0.0095, 0.01, 0.0, 0.32], 0.0, M_METAL, ALU_DARK, SUBTRACT)
	_shape(carrier, HOLES, cx, [6, 0.0635, 0.0028, 0.0028, PI / 6.0, 0.0], 0.0, M_METAL, SILVER)
	var lock := Transform3D(_basis_y(Vector3.RIGHT), Vector3(x + signf(x) * 0.0045, 0.0, 0.0))
	_shape(carrier, GEAR, lock, [12, 0.011, 0.0205, 0.0205, 0.0018, 0.0018, 0.0004], 0.0, M_METAL, SILVER)
	parts.sdf(carrier, 0.0008, 0.0005)


# --- Pédalier, pédales, chaîne ---------------------------------------------------

## Pédalier (origine à l'axe, manivelle droite vers le bas) : manivelles,
## araignée à capteur de puissance, plateaux 52-36 dentés, axes de pédales.
static func _crank_mesh() -> ArrayMesh:
	var parts := Parts.new()
	var arms := _sdf()
	_cyl(arms, Vector3(-0.07, 0, 0), Vector3(0.07, 0, 0), 0.0122, 0.001, 0.0, M_METAL, ALU_DARK)
	for sx: float in [-1.0, 1.0]:
		var dir := -sx
		var ctrl := [Vector3(sx * 0.063, 0.0, 0.0), Vector3(sx * 0.069, dir * 0.06, 0.0), Vector3(sx * 0.0745, dir * 0.12, 0.0), Vector3(sx * 0.0765, dir * CRANK, 0.0)]
		_tube(arms, ctrl, [_sec_oval(0.0088, 0.0165), _sec_oval(0.0082, 0.0135), _sec_oval(0.0078, 0.0118), _sec_oval(0.0074, 0.011)], Vector3(0, 0, -1), 0.0, M_METAL, ALU_DARK, 5)
		_cyl(arms, Vector3(sx * 0.0545, 0, 0), Vector3(sx * 0.0715, 0, 0), 0.0205, 0.004, 0.01, M_METAL, ALU_DARK)
		_cyl(arms, Vector3(sx * 0.0685, dir * CRANK, 0), Vector3(sx * 0.0845, dir * CRANK, 0), 0.0118, 0.003, 0.008, M_METAL, ALU_DARK)
		# Axe de pédale.
		_cyl(arms, Vector3(sx * 0.084, dir * CRANK, 0), Vector3(sx * 0.092, dir * CRANK, 0), 0.0058, 0.0008, 0.0, M_METAL, SILVER)
	# Araignée à quatre bras et capteur de puissance.
	for k in 4:
		var th := PI / 4.0 + TAU * k / 4.0
		var tip := Vector3(0.0525, 0.0555 * cos(th), 0.0555 * sin(th))
		_tube(arms, [Vector3(0.055, 0.0, 0.0), tip], [_sec_oval(0.0042, 0.0095), _sec_oval(0.004, 0.0068)], Vector3(0, -sin(th), cos(th)), 0.006, M_METAL, ALU_DARK)
		_cyl(arms, tip - Vector3(0.006, 0, 0), tip + Vector3(0.0035, 0, 0), 0.0068, 0.0015, 0.004, M_METAL, ALU_DARK)
	_cyl(arms, Vector3(0.049, 0, 0), Vector3(0.061, 0, 0), 0.031, 0.003, 0.008, M_METAL, ALU_DARK)
	_cyl(arms, Vector3(0.0605, 0, 0), Vector3(0.0635, 0, 0), 0.012, 0.0012, 0.003, M_PLAIN, BLACK_GLOSS)
	parts.sdf(arms, 0.001, 0.0004)
	var rings := _sdf()
	for i in RINGS.size():
		var n: int = RINGS[i]
		var rp := _pitch_radius(n)
		var x: float = RING_X[i]
		var xf := Transform3D(_basis_y(Vector3.RIGHT), Vector3(x, 0, 0))
		var r_in := rp - 0.019 if i == 0 else 0.05
		_shape(rings, GEAR, xf, [n, r_in, rp + 0.0043, rp, 0.004, 0.0018, 0.0011], 0.0, M_METAL, ALU_GREY if i == 0 else ALU_DARK)
		for k in 4:
			var th := PI / 4.0 + TAU * k / 4.0
			var tab := Transform3D(Basis(Vector3.RIGHT, th),Vector3(x, 0, 0) + Vector3(0, cos(th), sin(th)) * (0.5 * (0.0555 + r_in + 0.004)))
			_shape(rings, RBOX, tab, [0.0016, (r_in + 0.004 - 0.0555) * 0.5 + 0.007, 0.0085, 0.0012], 0.004, M_METAL, ALU_GREY if i == 0 else ALU_DARK)
	for k in 4:
		var th := PI / 4.0 + TAU * k / 4.0
		var c := Vector3(0, 0.0555 * cos(th), 0.0555 * sin(th))
		_cyl(rings, c + Vector3(0.037, 0, 0), c + Vector3(0.0505, 0, 0), 0.0042, 0.0008, 0.0, M_METAL, SILVER)
	parts.sdf(rings, 0.0007, 0.0003)
	return parts.mesh()


## Pédale automatique (origine à l'axe, avant vers -Z) : corps en composite,
## plaque d'appui en acier, mâchoire arrière.
static func _pedal_mesh() -> ArrayMesh:
	var parts := Parts.new()
	var s := _sdf()
	var body := PackedFloat32Array([0.0055, 0.003, -0.024, -0.05, 0.024, -0.05, 0.034, -0.012, 0.031, 0.026, 0.02, 0.04, -0.02, 0.04, -0.031, 0.026, -0.034, -0.012])
	var params := []
	for v in body:
		params.append(v)
	_shape(s, POLYGON, Transform3D(Basis(), Vector3(0, 0.0005, 0)), params, 0.0, M_PLAIN, BLACK_GLOSS)
	_cyl(s, Vector3(-0.034, 0, 0), Vector3(0.034, 0, 0), 0.0098, 0.003, 0.006, M_PLAIN, BLACK_GLOSS)
	_shape(s, RBOX, Transform3D(Basis(Vector3.RIGHT, 0.4), Vector3(0, 0.006, -0.046)), [0.019, 0.006, 0.008, 0.003], 0.004, M_PLAIN, BLACK_GLOSS)
	_shape(s, RBOX, Transform3D(Basis(Vector3.RIGHT, -0.3), Vector3(0, 0.0075, 0.036)), [0.021, 0.0055, 0.008, 0.003], 0.004, M_PLAIN, BLACK_GLOSS)
	_shape(s, RBOX, Transform3D(Basis(), Vector3(0, 0.0062, -0.006)), [0.021, 0.0012, 0.026, 0.0011], 0.0, M_METAL, SILVER)
	parts.sdf(s, 0.0008, 0.0004)
	return parts.mesh()


## Chaîne : rubans des plaques et des rouleaux le long du trajet, maillons
## dessinés et animés par le shader.
static func _chain_mesh() -> ArrayMesh:
	var path2 := _chain_path()
	var pts := PackedVector3Array()
	for p in path2:
		pts.append(PV(p, CHAIN_X))
	var g := Geo.new()
	var n := pts.size()
	var dist := PackedFloat32Array()
	var acc := 0.0
	for i in n:
		if i > 0:
			acc += pts[i].distance_to(pts[i - 1])
		dist.append(acc)
	var links := maxi(2, int(round(acc / (2.0 * LINK)))) * 2
	_chain_scale = links * LINK / acc
	# (écart latéral, demi-hauteur ou décalage, genre) des rubans.
	for strip: Array in [[0.0037, 0.0046, 0.0, 0], [-0.0037, 0.0046, 0.0, 0], [0.0029, 0.0043, 0.5, 0], [-0.0029, 0.0043, 0.5, 0],
			[0.0, 0.0036, 1.0, 1], [0.0, -0.0036, 1.0, 1]]:
		var ids: Array[PackedInt32Array] = [PackedInt32Array(), PackedInt32Array()]
		for i in n:
			var t := (pts[mini(i + 1, n - 1)] - pts[maxi(i - 1, 0)]).normalized()
			var up := Vector3.RIGHT.cross(t).normalized()
			var c := Color(strip[2], 0, 0, 1)
			if strip[3] == 0:
				var side := Vector3(strip[0], 0, 0)
				ids[0].append(g.add(pts[i] + side - up * strip[1], Vector3(signf(strip[0]), 0, 0), Vector2(dist[i], -strip[1]), c))
				ids[1].append(g.add(pts[i] + side + up * strip[1], Vector3(signf(strip[0]), 0, 0), Vector2(dist[i], strip[1]), c))
			else:
				var nrm := up * signf(strip[1])
				ids[0].append(g.add(pts[i] + up * strip[1] - Vector3(0.0028, 0, 0), nrm, Vector2(dist[i], 0.0), c))
				ids[1].append(g.add(pts[i] + up * strip[1] + Vector3(0.0028, 0, 0), nrm, Vector2(dist[i], 0.0), c))
		g.grid([ids[0], ids[1]])
	var parts := Parts.new()
	parts.geo(M_CHAIN, g, false)
	var mesh := parts.mesh()
	var mat: ShaderMaterial = _materials[M_CHAIN]
	mat.set_shader_parameter("scale", _chain_scale)
	return mesh


# --- Outils de maillage ------------------------------------------------------------

## Surfaces en construction, par matériau : morceaux (tableaux d'ArrayMesh,
## niveaux de détail éventuels) fusionnés en une surface par matériau.
class Parts:
	var by_mat := {}

	func add(mat: int, arrays: Array, lods: Dictionary) -> void:
		if not by_mat.has(mat):
			by_mat[mat] = []
		by_mat[mat].append([arrays, lods])

	## Pièce tournée ou balayée ; ses niveaux de détail sont calculés par
	## VeloSdf quand l'extension est chargée (`lod` faux : maillage intact).
	func geo(mat: int, g: Geo, lod := true) -> void:
		if g.idx.is_empty():
			return
		var lods := {}
		if lod and ClassDB.class_exists("VeloSdf"):
			lods = ClassDB.instantiate("VeloSdf").lods(g.verts, g.idx, PackedFloat32Array(LOD_ERRORS))
		var arr := []
		arr.resize(Mesh.ARRAY_MAX)
		arr[Mesh.ARRAY_VERTEX] = g.verts
		arr[Mesh.ARRAY_NORMAL] = g.norms
		arr[Mesh.ARRAY_TEX_UV] = g.uvs
		arr[Mesh.ARRAY_COLOR] = g.cols
		arr[Mesh.ARRAY_INDEX] = g.idx
		add(mat, arr, lods)

	func sdf(s: Object, cell: float, error: float) -> void:
		if s == null:
			return
		for d: Dictionary in s.build(cell, error, PackedFloat32Array(LOD_ERRORS)):
			add(d["mat"], d["arrays"], d["lods"])

	func mesh() -> ArrayMesh:
		var m := ArrayMesh.new()
		var ids := PackedInt32Array()
		var keys := by_mat.keys()
		keys.sort()
		for mat: int in keys:
			var verts := PackedVector3Array()
			var norms := PackedVector3Array()
			var uvs := PackedVector2Array()
			var cols := PackedColorArray()
			var idx := PackedInt32Array()
			var lods := {}
			var any_lod := false
			for part: Array in by_mat[mat]:
				var lod_d: Dictionary = part[1]
				if not lod_d.is_empty():
					any_lod = true
			for e: float in LOD_ERRORS:
				lods[e] = PackedInt32Array()
			for part: Array in by_mat[mat]:
				var arr: Array = part[0]
				var base := verts.size()
				var pv: PackedVector3Array = arr[Mesh.ARRAY_VERTEX]
				verts.append_array(pv)
				norms.append_array(arr[Mesh.ARRAY_NORMAL])
				cols.append_array(arr[Mesh.ARRAY_COLOR])
				if arr[Mesh.ARRAY_TEX_UV] != null:
					uvs.append_array(arr[Mesh.ARRAY_TEX_UV])
				else:
					var z := PackedVector2Array()
					z.resize(pv.size())
					uvs.append_array(z)
				var pi: PackedInt32Array = arr[Mesh.ARRAY_INDEX]
				idx.append_array(_offset(pi, base))
				var lod_d: Dictionary = part[1]
				var prev := pi
				for e: float in LOD_ERRORS:
					var li: PackedInt32Array = prev
					for key: float in lod_d:
						if is_equal_approx(key, e):
							li = lod_d[key]
					prev = li
					var acc: PackedInt32Array = lods[e]
					acc.append_array(_offset(li, base))
					lods[e] = acc
			var arrays := []
			arrays.resize(Mesh.ARRAY_MAX)
			arrays[Mesh.ARRAY_VERTEX] = verts
			arrays[Mesh.ARRAY_NORMAL] = norms
			arrays[Mesh.ARRAY_TEX_UV] = uvs
			arrays[Mesh.ARRAY_COLOR] = cols
			arrays[Mesh.ARRAY_INDEX] = idx
			m.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays, [], lods if any_lod else {})
			ids.append(mat)
		m.set_meta("materials", ids)
		return m

	static func _offset(a: PackedInt32Array, base: int) -> PackedInt32Array:
		if base == 0:
			return a
		var out := a.duplicate()
		for i in out.size():
			out[i] += base
		return out


## Accumulateur de triangles indexés : sommets, normales, UV, couleurs.
class Geo:
	var verts := PackedVector3Array()
	var norms := PackedVector3Array()
	var uvs := PackedVector2Array()
	var cols := PackedColorArray()
	var idx := PackedInt32Array()

	func add(p: Vector3, n: Vector3, uv := Vector2.ZERO, c := Color(1, 1, 1, 0)) -> int:
		verts.append(p)
		norms.append(n)
		uvs.append(uv)
		cols.append(c)
		return verts.size() - 1

	## Triangle tourné pour que sa face avant regarde du côté de `n` (Godot :
	## sens horaire vu de face).
	func tri(a: int, b: int, c: int, n: Vector3) -> void:
		var g := (verts[b] - verts[a]).cross(verts[c] - verts[a])
		if g.length_squared() < 1e-20:
			return
		if g.dot(n) <= 0.0:
			idx.append_array([a, b, c])
		else:
			idx.append_array([a, c, b])

	## Grille de sommets déjà ajoutés (rangées d'indices).
	func grid(rows: Array[PackedInt32Array]) -> void:
		for i in rows.size() - 1:
			var r0 := rows[i]
			var r1 := rows[i + 1]
			for j in r0.size() - 1:
				var a := r0[j]
				var b := r0[j + 1]
				var c := r1[j + 1]
				var d := r1[j]
				var n := norms[a] + norms[b] + norms[c] + norms[d]
				tri(a, b, c, n)
				tri(a, c, d, n)

	func quad(p: Array, n: Vector3, c: Color) -> void:
		var ids := PackedInt32Array()
		for q: Vector3 in p:
			ids.append(add(q, n, Vector2.ZERO, c))
		tri(ids[0], ids[1], ids[2], n)
		tri(ids[0], ids[2], ids[3], n)

	func box(xf: Transform3D, size: Vector3, c: Color) -> void:
		var h := size * 0.5
		for nv: Vector3 in [Vector3.RIGHT, Vector3.LEFT, Vector3.UP, Vector3.DOWN, Vector3.BACK, Vector3.FORWARD]:
			var u := Vector3(nv.y, nv.z, nv.x)
			var v := nv.cross(u)
			var ctr := nv * h
			var n := (xf.basis * nv).normalized()
			quad([xf * (ctr - u * h - v * h), xf * (ctr + u * h - v * h), xf * (ctr + u * h + v * h), xf * (ctr - u * h + v * h)], n, c)

	## Surface de révolution autour de l'axe X passant par `o` : profil (rayon,
	## x). Normales du profil tournées de `sgn` ; deux points égaux font une
	## arête vive. UV.x : longueur d'arc au rayon, UV.y : `vs` ou indice.
	## Couleur : `colf(k)` si valide, sinon `col`.
	func revolve_x(o: Vector3, prof: PackedVector2Array, seg: int, colf: Callable, col: Color, sgn: float,
			vs := PackedFloat32Array(), smooth_all := false) -> void:
		var n2 := _profile_normals(prof, sgn, smooth_all)
		var rows: Array[PackedInt32Array] = []
		for k in prof.size():
			var row := PackedInt32Array()
			var c: Color = colf.call(k) if colf.is_valid() else col
			var v := vs[k] if k < vs.size() else float(k)
			for j in seg + 1:
				var th := TAU * j / seg
				var cs := cos(th)
				var sn := sin(th)
				var p := o + Vector3(prof[k].y, prof[k].x * cs, prof[k].x * sn)
				var nn := Vector3(n2[k].y, n2[k].x * cs, n2[k].x * sn).normalized()
				row.append(add(p, nn, Vector2(th * prof[k].x, v), c))
			rows.append(row)
		_grid_creased(rows, prof)

	## Révolution autour d'un axe quelconque (bidons) : profil (rayon, t le
	## long de `axis` depuis `base`), couleur `colf(t)`.
	func revolve_axis(base: Vector3, axis: Vector3, ref: Vector3, prof: PackedVector2Array, seg: int, colf: Callable) -> void:
		var a := axis.normalized()
		var e1 := (ref - a * ref.dot(a)).normalized()
		var e2 := a.cross(e1)
		var n2 := _profile_normals(prof, 1.0, false)
		var rows: Array[PackedInt32Array] = []
		for k in prof.size():
			var row := PackedInt32Array()
			var c: Color = colf.call(prof[k].y)
			for j in seg + 1:
				var th := TAU * j / seg
				var radial := e1 * cos(th) + e2 * sin(th)
				var p := base + a * prof[k].y + radial * prof[k].x
				var nn := (radial * n2[k].x + a * n2[k].y).normalized()
				row.append(add(p, nn, Vector2.ZERO, c))
			rows.append(row)
		_grid_creased(rows, prof)

	func _grid_creased(rows: Array[PackedInt32Array], prof: PackedVector2Array) -> void:
		for k in rows.size() - 1:
			if prof[k].is_equal_approx(prof[k + 1]):
				continue
			grid([rows[k], rows[k + 1]])

	## Normales 2D d'un profil (rayon, x) : tangente tournée d'un quart de
	## tour, orientée par `sgn`. Les points doublés séparent les faces.
	static func _profile_normals(prof: PackedVector2Array, sgn: float, smooth_all: bool) -> PackedVector2Array:
		var out := PackedVector2Array()
		var n := prof.size()
		for k in n:
			var a := maxi(k - 1, 0)
			var b := mini(k + 1, n - 1)
			if not smooth_all:
				if k + 1 < n and prof[k + 1].is_equal_approx(prof[k]):
					b = k
				if k > 0 and prof[k - 1].is_equal_approx(prof[k]):
					a = k
			var t := prof[b] - prof[a]
			if t.length_squared() < 1e-14:
				t = prof[mini(k + 1, n - 1)] - prof[maxi(k - 1, 0)]
			out.append(Vector2(t.y, -t.x).normalized() * sgn)
		return out

	## Tube le long d'un chemin (repères transportés), section elliptique
	## (rayon × `flat` selon le second axe, le premier suit `hint`), bouts
	## fermés si `caps`. UV.x : longueur parcourue, UV.y : tour (0 à 1).
	func sweep(path: PackedVector3Array, radii: PackedFloat32Array, seg: int, c: Color, caps := false, hint := Vector3.UP, flat := 1.0) -> void:
		var n := path.size()
		if n < 2:
			return
		var rows: Array[PackedInt32Array] = []
		var t0 := (path[1] - path[0]).normalized()
		var normal := hint - t0 * hint.dot(t0)
		if normal.length() < 1e-4:
			normal = t0.cross(Vector3.RIGHT if absf(t0.x) < 0.9 else Vector3.UP)
		normal = normal.normalized()
		var dist := 0.0
		for i in n:
			if i > 0:
				dist += path[i].distance_to(path[i - 1])
			var t := (path[mini(i + 1, n - 1)] - path[maxi(i - 1, 0)]).normalized()
			normal = (normal - t * normal.dot(t)).normalized()
			var bin := t.cross(normal)
			var r := radii[mini(i, radii.size() - 1)]
			var row := PackedInt32Array()
			for j in seg + 1:
				var phi := TAU * j / seg
				var d := normal * cos(phi) * r + bin * sin(phi) * r * flat
				var nn := (normal * cos(phi) / r + bin * sin(phi) / (r * flat)).normalized()
				row.append(add(path[i] + d, nn, Vector2(dist, float(j) / seg), c))
			rows.append(row)
		grid(rows)
		if caps:
			for e in [0, n - 1]:
				var t := (path[mini(e + 1, n - 1)] - path[maxi(e - 1, 0)]).normalized() * (-1.0 if e == 0 else 1.0)
				var center := add(path[e], t, Vector2.ZERO, c)
				var ring := rows[0 if e == 0 else n - 1]
				var cap_ids := PackedInt32Array()
				for id in ring:
					cap_ids.append(add(verts[id], t, Vector2.ZERO, c))
				for j in seg:
					tri(center, cap_ids[j], cap_ids[j + 1], t)

	## Surface fermée par anneaux successifs (selle), couvercles aux bouts.
	func loft(rings: Array[PackedVector3Array], ring_cols: Array[PackedColorArray], caps: bool) -> void:
		var ns := rings.size()
		var m := rings[0].size()
		var rows: Array[PackedInt32Array] = []
		for i in ns:
			var ctr := Vector3.ZERO
			for p in rings[i]:
				ctr += p
			ctr /= m
			var row := PackedInt32Array()
			for j in m + 1:
				var jj := j % m
				var along := rings[mini(i + 1, ns - 1)][jj] - rings[maxi(i - 1, 0)][jj]
				var around := rings[i][(jj + 1) % m] - rings[i][(jj - 1 + m) % m]
				var nn := along.cross(around).normalized()
				if nn.dot(rings[i][jj] - ctr) < 0.0:
					nn = -nn
				row.append(add(rings[i][jj], nn, Vector2.ZERO, ring_cols[i][jj]))
			rows.append(row)
		grid(rows)
		if caps:
			for e in [0, ns - 1]:
				var ctr := _centroid(rings[e])
				var axis := (ctr - _centroid(rings[1 if e == 0 else ns - 2])).normalized()
				var c := add(ctr + axis * 0.002, axis, Vector2.ZERO, ring_cols[e][0])
				var ids := PackedInt32Array()
				for j in m + 1:
					var out := (rings[e][j % m] - ctr).normalized()
					ids.append(add(rings[e][j % m], (axis + out * 0.6).normalized(), Vector2.ZERO, ring_cols[e][j % m]))
				for j in m:
					tri(c, ids[j], ids[j + 1], axis)

	static func _centroid(ring: PackedVector3Array) -> Vector3:
		var ctr := Vector3.ZERO
		for p in ring:
			ctr += p
		return ctr / maxi(ring.size(), 1)

	## Pignon denté d'axe X, face extérieure tournée vers +X, épaisseur 2·`half_t`.
	func gear_x(x: float, teeth: int, rp: float, r_notch: float, r_tip: float, r_in: float, half_t: float, c: Color, back: bool) -> void:
		# Profil trapézoïdal : creux, flanc, sommet plat sur deux échantillons,
		# flanc (5 échantillons par dent).
		var per := 5
		var m := teeth * per
		var cell := TAU / teeth
		var r_root := rp - r_notch
		var outline := PackedFloat32Array()
		for i in m:
			var th := TAU * i / m
			var a := th - cell * roundf(th / cell)
			var f := absf(a) / (cell * 0.5)
			outline.append(lerpf(r_root, r_tip, smoothstep(0.2, 0.8, f)))
		var faces := [[x + half_t, 1.0]]
		if back:
			faces.append([x - half_t, -1.0])
		for fc: Array in faces:
			var fx: float = fc[0]
			var nx := Vector3(fc[1], 0, 0)
			var outer := PackedInt32Array()
			var inner := PackedInt32Array()
			for i in m + 1:
				var th := TAU * (i % m) / m
				var dir := Vector3(0, cos(th), sin(th))
				outer.append(add(Vector3(fx, 0, 0) + dir * outline[i % m], nx, Vector2.ZERO, c))
				inner.append(add(Vector3(fx, 0, 0) + dir * r_in, nx, Vector2.ZERO, c))
			grid([outer, inner])
		# Bord denté.
		var e0 := PackedInt32Array()
		var e1 := PackedInt32Array()
		for i in m + 1:
			var th := TAU * (i % m) / m
			var thn := TAU * ((i + 1) % m) / m
			var thp := TAU * ((i - 1 + m) % m) / m
			var p := Vector3(0, cos(th), sin(th)) * outline[i % m]
			var pn := Vector3(0, cos(thn), sin(thn)) * outline[(i + 1) % m]
			var pp := Vector3(0, cos(thp), sin(thp)) * outline[(i - 1 + m) % m]
			var tg := pn - pp
			var nrm := Vector3.RIGHT.cross(tg).normalized()
			if nrm.dot(p) < 0.0:
				nrm = -nrm
			e0.append(add(Vector3(x - half_t, 0, 0) + p, nrm, Vector2.ZERO, c))
			e1.append(add(Vector3(x + half_t, 0, 0) + p, nrm, Vector2.ZERO, c))
		grid([e0, e1])


## Mot-symbole en traits (minuscules italiques grasses) posé sur une surface.
class Glyphs:
	## Traits de chaque lettre, hauteur d'x = 1, et chasse.
	static func glyph(ch: String) -> Array:
		match ch:
			"v":
				return [[[Vector2(0.0, 1.0), Vector2(0.4, 0.0), Vector2(0.8, 1.0)]], 0.8]
			"e":
				var s := [Vector2(0.06, 0.5), Vector2(0.8, 0.5)]
				for k in 15:
					var a := deg_to_rad(lerpf(0.0, 318.0, k / 14.0))
					s.append(Vector2(0.42 + 0.39 * cos(a), 0.5 + 0.5 * sin(a)))
				return [[s], 0.84]
			"l":
				return [[[Vector2(0.12, 1.45), Vector2(0.12, 0.0)]], 0.24]
			"o":
				return [[_ellipse(Vector2(0.42, 0.5), Vector2(0.39, 0.5), 0.0, 360.0, 20)], 0.84]
			"t":
				var s := [Vector2(0.24, 1.32), Vector2(0.24, 0.2)]
				for k in 5:
					var a := deg_to_rad(lerpf(180.0, 270.0, k / 4.0))
					s.append(Vector2(0.44 + 0.2 * cos(a), 0.2 + 0.2 * sin(a)))
				s.append(Vector2(0.56, 0.02))
				return [[s, [Vector2(0.0, 1.0), Vector2(0.52, 1.0)]], 0.56]
			"r":
				return [[[Vector2(0.1, 1.0), Vector2(0.1, 0.0)], [Vector2(0.1, 0.55), Vector2(0.2, 0.84), Vector2(0.38, 0.99), Vector2(0.58, 0.96)]], 0.58]
			"a":
				return [[_ellipse(Vector2(0.4, 0.5), Vector2(0.37, 0.5), 0.0, 360.0, 20), [Vector2(0.77, 1.0), Vector2(0.77, 0.0)]], 0.84]
			"c":
				return [[_ellipse(Vector2(0.42, 0.5), Vector2(0.39, 0.5), 42.0, 318.0, 16)], 0.8]
		return [[], 0.5]

	static func _ellipse(c: Vector2, r: Vector2, a0: float, a1: float, n: int) -> Array:
		var out := []
		for k in n + 1:
			var a := deg_to_rad(lerpf(a0, a1, float(k) / n))
			out.append(c + Vector2(r.x * cos(a), r.y * sin(a)))
		return out

	## Traits du texte (hauteur d'x = 1), penchés en italique.
	static func text(s: String) -> Array:
		var out := []
		var x := 0.0
		for ch in s:
			var g := glyph(ch)
			for stroke: Array in g[0]:
				var pts := PackedVector2Array()
				for p: Vector2 in stroke:
					pts.append(Vector2(x + p.x + p.y * 0.22, p.y))
				out.append(pts)
			x += g[1] + 0.3
		return out

	static func width(s: String) -> float:
		var x := 0.0
		for ch in s:
			x += glyph(ch)[1] + 0.3
		return x - 0.3

	## Pose les traits (hauteur `h`, épaisseur `w` en hauteurs d'x) : `map`
	## donne la position 3D d'un point du plan du texte, ramenée sur la surface
	## de `sdf` (décalée de 0,25 mm) si fourni, sinon normale `nrm`.
	static func decal(g: Geo, strokes: Array, h: float, w: float, map: Callable, sdf: Object, c: Color, nrm := Vector3.ZERO) -> void:
		var hw := w * 0.5 * h
		for stroke: PackedVector2Array in strokes:
			# Rééchantillonnage fin pour suivre la courbure de la surface.
			var pts := PackedVector2Array()
			for i in stroke.size() - 1:
				var a := stroke[i] * h
				var b := stroke[i + 1] * h
				var steps := maxi(1, int(a.distance_to(b) / 0.003))
				for k in steps:
					pts.append(a.lerp(b, float(k) / steps))
			pts.append(stroke[stroke.size() - 1] * h)
			var n := pts.size()
			var left := PackedVector3Array()
			var right := PackedVector3Array()
			for i in n:
				var t := (pts[mini(i + 1, n - 1)] - pts[maxi(i - 1, 0)]).normalized()
				var p := Vector2(-t.y, t.x)
				var q := pts[i]
				# Bouts prolongés d'une demi-épaisseur.
				if i == 0:
					q -= t * hw * 0.6
				elif i == n - 1:
					q += t * hw * 0.6
				left.append(map.call(q + p * hw))
				right.append(map.call(q - p * hw))
			var normals_l := PackedVector3Array()
			var normals_r := PackedVector3Array()
			if sdf != null:
				var pl: Array = sdf.project(left, 0.00025)
				var pr: Array = sdf.project(right, 0.00025)
				left = pl[0]
				normals_l = pl[1]
				right = pr[0]
				normals_r = pr[1]
			var rows: Array[PackedInt32Array] = [PackedInt32Array(), PackedInt32Array()]
			for i in n:
				var nl := normals_l[i] if sdf != null else nrm
				var nr := normals_r[i] if sdf != null else nrm
				rows[0].append(g.add(left[i], nl, Vector2.ZERO, c))
				rows[1].append(g.add(right[i], nr, Vector2.ZERO, c))
			g.grid(rows)
