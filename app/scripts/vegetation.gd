extends RefCounted
## Modèles simples mais soignés du bord de route : sapins, feuillus, buissons
## et balises. Hauteur 1 et diamètre 1 : chaque instance est mise à l'échelle.

const VEGETATION_SHADER := preload("res://shaders/vegetation.gdshader")

static var _cache := {}


static func conifer() -> Mesh:
	return _cached("sapin", _build_conifer)


static func broadleaf() -> Mesh:
	return _cached("feuillu", _build_broadleaf)


static func conifer_low() -> Mesh:
	return _cached("sapin_loin", _build_conifer_low)


static func broadleaf_low() -> Mesh:
	return _cached("feuillu_loin", _build_broadleaf_low)


static func bush() -> Mesh:
	return _cached("buisson", _build_bush)


static func post() -> Mesh:
	return _cached("balise", _build_post)


static func material() -> ShaderMaterial:
	if not _cache.has("materiau"):
		var m := ShaderMaterial.new()
		m.shader = VEGETATION_SHADER
		_cache["materiau"] = m
	return _cache["materiau"]


static func _cached(key: String, build: Callable) -> Mesh:
	if not _cache.has(key):
		_cache[key] = build.call()
	return _cache[key]


static func _build_conifer() -> Mesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	_cone(st, 0.0, 0.22, 0.045, 0.03, Color(0.33, 0.24, 0.16), Color(0.28, 0.20, 0.13), 6)
	# Étages de branches qui retombent, plus étroits vers la cime.
	var tiers := 4
	for k in tiers:
		var t := float(k) / tiers
		var y0 := 0.14 + t * 0.66
		var r := lerpf(0.5, 0.16, t)
		var dark := Color(0.09, 0.20, 0.11).lerp(Color(0.12, 0.25, 0.13), t)
		var light := Color(0.16, 0.32, 0.17).lerp(Color(0.20, 0.38, 0.20), t)
		_cone(st, y0, y0 + 0.34 - t * 0.08, r, 0.0, dark, light, 8, -0.06)
	_cone(st, 0.86, 1.0, 0.1, 0.0, Color(0.13, 0.27, 0.14), Color(0.22, 0.40, 0.21), 7)
	st.generate_normals()
	var m := st.commit()
	m.surface_set_material(0, material())
	return m


## Versions allégées pour le lointain (~30 et ~50 triangles).
static func _build_conifer_low() -> Mesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	_cone(st, 0.0, 0.2, 0.05, 0.05, Color(0.30, 0.21, 0.14), Color(0.30, 0.21, 0.14), 4)
	_cone(st, 0.12, 0.62, 0.5, 0.0, Color(0.10, 0.21, 0.11), Color(0.15, 0.30, 0.16), 6)
	_cone(st, 0.48, 1.0, 0.32, 0.0, Color(0.12, 0.25, 0.13), Color(0.19, 0.36, 0.19), 6)
	st.generate_normals()
	var m := st.commit()
	m.surface_set_material(0, material())
	return m


static func _build_broadleaf_low() -> Mesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	_cone(st, 0.0, 0.45, 0.06, 0.04, Color(0.34, 0.25, 0.17), Color(0.34, 0.25, 0.17), 4)
	var sphere := SphereMesh.new()
	sphere.radial_segments = 7
	sphere.rings = 4
	var c := Color(0.18, 0.31, 0.10)
	_append(st, sphere, Transform3D(Basis().scaled(Vector3(0.9, 0.75, 0.9)), Vector3(0.0, 0.64, 0.0)), c, c.lightened(0.25))
	st.generate_normals()
	var m := st.commit()
	m.surface_set_material(0, material())
	return m


static func _build_broadleaf() -> Mesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	_cone(st, 0.0, 0.45, 0.06, 0.035, Color(0.36, 0.27, 0.18), Color(0.30, 0.22, 0.15), 6)
	# Houppier : grappe de boules, plus claires au sommet.
	var blobs := [
		[Vector3(0.0, 0.62, 0.0), 0.34, Color(0.17, 0.30, 0.10)],
		[Vector3(0.17, 0.55, 0.06), 0.24, Color(0.20, 0.34, 0.11)],
		[Vector3(-0.15, 0.57, -0.08), 0.25, Color(0.18, 0.31, 0.10)],
		[Vector3(0.03, 0.56, -0.18), 0.22, Color(0.21, 0.35, 0.12)],
		[Vector3(-0.05, 0.80, 0.05), 0.22, Color(0.25, 0.40, 0.14)],
	]
	var sphere := SphereMesh.new()
	sphere.radial_segments = 8
	sphere.rings = 4
	for b in blobs:
		var xf := Transform3D(Basis().scaled(Vector3.ONE * b[1] * 2.0), b[0])
		_append(st, sphere, xf, b[2], b[2].lightened(0.25))
	st.generate_normals()
	var m := st.commit()
	m.surface_set_material(0, material())
	return m


static func _build_bush() -> Mesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	var sphere := SphereMesh.new()
	sphere.radial_segments = 8
	sphere.rings = 4
	for b in [
		[Vector3(0.0, 0.45, 0.0), Vector3(0.62, 0.9, 0.62), Color(0.18, 0.29, 0.10)],
		[Vector3(0.25, 0.35, 0.1), Vector3(0.5, 0.7, 0.5), Color(0.22, 0.33, 0.12)],
		[Vector3(-0.22, 0.32, -0.08), Vector3(0.48, 0.64, 0.48), Color(0.20, 0.31, 0.11)],
	]:
		_append(st, sphere, Transform3D(Basis().scaled(b[1]), b[0]), b[2], b[2].lightened(0.2))
	st.generate_normals()
	var m := st.commit()
	m.surface_set_material(0, material())
	return m


## Balise J6 : fût blanc, bande noire, catadioptre orange côté trafic.
static func _build_post() -> Mesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	var box := BoxMesh.new()
	box.size = Vector3.ONE
	var white := Color(0.92, 0.92, 0.9)
	_append(st, box, Transform3D(Basis().scaled(Vector3(0.1, 0.78, 0.1)), Vector3(0, 0.39, 0)), white, white)
	_append(st, box, Transform3D(Basis().scaled(Vector3(0.102, 0.2, 0.102)), Vector3(0, 0.88, 0)), Color(0.05, 0.05, 0.05), Color(0.05, 0.05, 0.05))
	_append(st, box, Transform3D(Basis().scaled(Vector3(0.1, 0.05, 0.1)), Vector3(0, 1.0, 0)), white, white)
	var amber := Color(1.0, 0.55, 0.05)
	_append(st, box, Transform3D(Basis().scaled(Vector3(0.05, 0.12, 0.01)), Vector3(0, 0.88, 0.052)), amber, amber)
	st.generate_normals()
	var m := st.commit()
	var mat := StandardMaterial3D.new()
	mat.vertex_color_use_as_albedo = true
	mat.vertex_color_is_srgb = true
	mat.roughness = 0.5
	m.surface_set_material(0, mat)
	return m


## Ajoute une primitive transformée, en dégradé de couleur du bas vers le haut.
static func _append(st: SurfaceTool, mesh: PrimitiveMesh, xf: Transform3D, bottom: Color, top: Color) -> void:
	var arrays := mesh.get_mesh_arrays()
	var verts: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
	var idx: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
	var ymin := INF
	var ymax := -INF
	for v in verts:
		ymin = minf(ymin, v.y)
		ymax = maxf(ymax, v.y)
	for i in idx:
		var v := verts[i]
		st.set_color(bottom.lerp(top, (v.y - ymin) / maxf(ymax - ymin, 1e-4)))
		st.add_vertex(xf * v)


## Tronc de cône à facettes, de couleur `bottom` en bas et `top` en haut.
## `droop` abaisse le bord de la base pour des branches qui retombent.
static func _cone(st: SurfaceTool, y0: float, y1: float, r0: float, r1: float, bottom: Color, top: Color, sides: int, droop := 0.0) -> void:
	for k in sides:
		var a0 := TAU * k / sides
		var a1 := TAU * (k + 1) / sides
		var b0 := Vector3(cos(a0) * r0, y0 + droop, sin(a0) * r0)
		var b1 := Vector3(cos(a1) * r0, y0 + droop, sin(a1) * r0)
		var t0 := Vector3(cos(a0) * r1, y1, sin(a0) * r1)
		var t1 := Vector3(cos(a1) * r1, y1, sin(a1) * r1)
		# Sens horaire vu de l'extérieur (face avant pour Godot).
		for v in [b0, b1, t0]:
			st.set_color(bottom if v.y < y1 - 1e-4 else top)
			st.add_vertex(v)
		if r1 > 0.0:
			for v in [b1, t1, t0]:
				st.set_color(bottom if v.y < y1 - 1e-4 else top)
				st.add_vertex(v)
		# Dessous des étages : ferme le cône pour qu'on ne voie pas au travers.
		if droop != 0.0:
			var c := Vector3(0.0, y0 + 0.04, 0.0)
			for v in [b1, b0, c]:
				st.set_color(bottom.darkened(0.3))
				st.add_vertex(v)
