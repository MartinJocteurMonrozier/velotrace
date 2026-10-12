extends RefCounted
## Mobilier de course comme sur le Tour : arche gonflable bleue au départ,
## banderole à pois au pied des ascensions et arche à pois du grand prix de
## la montagne au sommet, banderoles « 3 KM » et « 2 KM », flamme rouge,
## portique d'arrivée en treillis avec chronomètre et ligne peinte, barrières
## habillées de banderoles et spectateurs (positions et textes calculés par
## velo-world). Aucune marque réelle : inscriptions génériques.

const ARCH_SHADER := preload("res://shaders/arch.gdshader")
const BANNER_SHADER := preload("res://shaders/race_banner.gdshader")
const Atlas := preload("res://scripts/race_atlas.gd")
const Clock := preload("res://scripts/race_clock.gd")

enum Kind { START, CLIMB_START, CLIMB_TOP, FINISH, FLAMME_ROUGE, KM_TO_GO }

## Motifs de l'atlas (race_atlas.gd) utilisés hors des barrières.
enum Design { YELLOW, BLUE, RED, GREEN, TRICOLOUR, NAVY, POLKA, ORANGE, SKY, CHEQUER }

const PILLAR_RADIUS := 0.6
const PILLAR_HEIGHT := 7.6
const BEAM_Y := 6.9
const BANNER_Y := 5.6
const BANNER_H := 1.5
## Portique d'arrivée : côté du treillis, axe de la poutre.
const TRUSS_W := 0.8
const GANTRY_BEAM_Y := 6.9
## Barrières et spectateurs : carrés de 64 m, masqués au-delà de leur portée.
const CHUNK_M := 64.0
const BARRIER_REACH := 450.0
const CROWD_REACH := 260.0

const RED := Color(0.84, 0.07, 0.1)
const WHITE := Color(0.97, 0.97, 0.95)
const BLUE := Color(0.06, 0.25, 0.6)
const YELLOW := Color(0.99, 0.83, 0.06)
const DARK := Color(0.07, 0.07, 0.08)
const METAL := Color(0.74, 0.76, 0.79)

static var _bold: FontVariation
static var _cylinder: Array


static func build(ride: Object, base_elev: float, quality: int) -> Node3D:
	var root := Node3D.new()
	root.name = "Course"
	var atlas := Atlas.new()
	root.add_child(atlas)
	for a: Dictionary in ride.route_arches():
		root.add_child(_arch(a, base_elev, atlas))
	var no_shadow := quality == 0
	var buf: PackedFloat32Array = ride.route_barriers(base_elev)
	var barrier_meshes: Array[Mesh] = [_barrier_frame(), _barrier_panel(_banner_material(atlas, -1))]
	_chunked(root, barrier_meshes, buf, BARRIER_REACH, no_shadow)
	var crowd: PackedFloat32Array = ride.route_spectators(base_elev)
	_chunked(root, [_spectator()], crowd, CROWD_REACH, true)
	return root


## Répartit un tampon MultiMesh en carrés de CHUNK_M mètres, chacun masqué
## au-delà de `reach` : on ne dessine que les barrières et la foule proches.
static func _chunked(root: Node3D, meshes: Array[Mesh], buf: PackedFloat32Array, reach: float, no_shadow: bool) -> void:
	var cells := {}
	for i: int in buf.size() / 16:
		var key := Vector2i(floori(buf[i * 16 + 3] / CHUNK_M), floori(buf[i * 16 + 11] / CHUNK_M))
		if not cells.has(key):
			cells[key] = PackedInt32Array()
		var list: PackedInt32Array = cells[key]
		list.append(i)
		cells[key] = list
	for key: Vector2i in cells:
		var list: PackedInt32Array = cells[key]
		var center := Vector3((key.x + 0.5) * CHUNK_M, 0.0, (key.y + 0.5) * CHUNK_M)
		var part := PackedFloat32Array()
		part.resize(list.size() * 16)
		for k: int in list.size():
			for j: int in 16:
				part[k * 16 + j] = buf[list[k] * 16 + j]
			part[k * 16 + 3] -= center.x
			part[k * 16 + 11] -= center.z
		for mesh: Mesh in meshes:
			var mm := MultiMesh.new()
			mm.transform_format = MultiMesh.TRANSFORM_3D
			mm.use_colors = true
			mm.mesh = mesh
			mm.instance_count = list.size()
			mm.buffer = part
			var mmi := MultiMeshInstance3D.new()
			mmi.multimesh = mm
			mmi.position = center
			mmi.visibility_range_end = reach
			mmi.visibility_range_end_margin = 10.0
			if no_shadow:
				mmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			root.add_child(mmi)


static func _material(base: Color, second: Color, pattern: int, scale: Vector2, local := false) -> ShaderMaterial:
	var m := ShaderMaterial.new()
	m.shader = ARCH_SHADER
	m.set_shader_parameter("base_color", base)
	m.set_shader_parameter("second_color", second)
	m.set_shader_parameter("pattern", pattern)
	m.set_shader_parameter("uv_scale", scale)
	m.set_shader_parameter("use_local", local)
	return m


static func _banner_material(atlas: Atlas, design: int, vertical := false) -> ShaderMaterial:
	var m := ShaderMaterial.new()
	m.shader = BANNER_SHADER
	m.set_shader_parameter("design", design)
	m.set_shader_parameter("vertical", vertical)
	atlas.use(m)
	return m


static func _metal() -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = METAL
	m.metallic = 0.75
	m.roughness = 0.35
	return m


static func _font() -> FontVariation:
	if _bold == null:
		_bold = FontVariation.new()
		_bold.base_font = ThemeDB.fallback_font
		_bold.variation_embolden = 0.9
	return _bold


## Arche selon son type, dans un repère où X va vers la droite de la route,
## Y vers le haut et -Z dans le sens de la course.
static func _arch(a: Dictionary, base_elev: float, atlas: Atlas) -> Node3D:
	var kind: int = a.kind
	var h: Vector2 = a.heading
	var pos: Vector3 = a.position
	var node := Node3D.new()
	node.name = "Arche"
	var x_axis := Vector3(h.y, 0.0, h.x)
	var z_axis := Vector3(-h.x, 0.0, h.y)
	node.transform = Transform3D(Basis(x_axis, Vector3.UP, z_axis), Vector3(pos.x, pos.y - base_elev, pos.z))
	match kind:
		Kind.START:
			_start(node, a)
		Kind.CLIMB_START:
			_climb_start(node, a)
		Kind.CLIMB_TOP:
			_climb_top(node, a)
		Kind.FINISH:
			_finish(node, a, atlas)
		Kind.FLAMME_ROUGE:
			_flamme_rouge(node, a)
		_:
			_km_to_go(node, a)
	return node


## Départ : arche gonflable bleue à bandes blanches.
static func _start(node: Node3D, a: Dictionary) -> void:
	var left: float = a.left
	var right: float = a.right
	var pillar := _material(BLUE, WHITE, 3, Vector2(1.0, 5.0))
	_inflatable(node, left, right, pillar, _material(BLUE, WHITE, 0, Vector2.ONE))
	var w := left + right - 2.0 * PILLAR_RADIUS - 0.3
	var cx := (right - left) / 2.0
	_board(node, Vector3(cx, BANNER_Y, 0.0), Vector2(w, BANNER_H), WHITE, a.title, a.subtitle, BLUE, DARK)
	_edges(node, Vector3(cx, BANNER_Y, 0.0), Vector2(w, BANNER_H), _material(BLUE, BLUE, 0, Vector2.ONE))
	_board(node, Vector3(cx, BANNER_Y - BANNER_H / 2.0 - 0.22, 0.0), Vector2(w * 0.6, 0.44), YELLOW, "VELOTRACE", "", BLUE, DARK)


## Pied d'une ascension : banderole à pois tendue entre deux mâts.
static func _climb_start(node: Node3D, a: Dictionary) -> void:
	var left: float = a.left
	var right: float = a.right
	var y := 4.9
	_poles(node, left, right, y + 0.7)
	var w := left + right
	var cx := (right - left) / 2.0
	_board(node, Vector3(cx, y, 0.0), Vector2(w, 1.3), WHITE, a.title, a.subtitle, RED, DARK)
	var dots := _material(WHITE, RED, 1, Vector2(1.0, 1.0) / 0.2, true)
	for sy: float in [1.0, -1.0]:
		_box(node, Vector3(cx, y + sy * 0.75, 0.0), Vector3(w, 0.22, 0.2), dots)


## Sommet : arche gonflable à pois du grand prix de la montagne.
static func _climb_top(node: Node3D, a: Dictionary) -> void:
	var left: float = a.left
	var right: float = a.right
	var span := left + right
	var pillar := _material(WHITE, RED, 1, Vector2(6.0, 12.0))
	var beam := _material(WHITE, RED, 1, Vector2(6.0, (span + 1.3) / 0.6))
	_inflatable(node, left, right, pillar, beam)
	var w := span - 2.0 * PILLAR_RADIUS - 0.3
	var cx := (right - left) / 2.0
	_board(node, Vector3(cx, BANNER_Y, 0.0), Vector2(w, BANNER_H), WHITE, a.title, a.subtitle, RED, DARK)
	_edges(node, Vector3(cx, BANNER_Y, 0.0), Vector2(w, BANNER_H), _material(RED, RED, 0, Vector2.ONE))
	_board(node, Vector3(cx, BANNER_Y - BANNER_H / 2.0 - 0.22, 0.0), Vector2(w * 0.6, 0.44), RED, "VELOTRACE", "", WHITE, WHITE)


## Flamme rouge : arche gonflable rouge et triangle suspendu.
static func _flamme_rouge(node: Node3D, a: Dictionary) -> void:
	var left: float = a.left
	var right: float = a.right
	var red := _material(RED, RED, 0, Vector2.ONE)
	_inflatable(node, left, right, red, red)
	var w := left + right - 2.0 * PILLAR_RADIUS - 0.3
	var cx := (right - left) / 2.0
	_board(node, Vector3(cx, BANNER_Y + 0.2, 0.0), Vector2(w, 1.0), RED, a.title, "", WHITE, WHITE)
	var top := BANNER_Y - 0.3
	var tri := _triangle(Vector2(3.0, 2.2))
	var flag := MeshInstance3D.new()
	flag.mesh = tri
	flag.material_override = _material(RED.darkened(0.05), RED, 0, Vector2.ONE)
	flag.position = Vector3(cx, top, 0.0)
	node.add_child(flag)


## Banderole « 3 KM » entre deux mâts.
static func _km_to_go(node: Node3D, a: Dictionary) -> void:
	var left: float = a.left
	var right: float = a.right
	var y := 5.0
	_poles(node, left, right, y + 0.6)
	var w := left + right
	var cx := (right - left) / 2.0
	_board(node, Vector3(cx, y, 0.0), Vector2(w, 1.1), WHITE, a.title, "", BLUE, DARK)
	_edges(node, Vector3(cx, y, 0.0), Vector2(w, 1.1), _material(YELLOW, YELLOW, 0, Vector2.ONE))


## Arrivée : portique en treillis d'aluminium, panneaux sur les piles,
## grande banderole « ARRIVÉE » soulignée d'un damier, chronomètre au
## sommet et ligne peinte sur la chaussée.
static func _finish(node: Node3D, a: Dictionary, atlas: Atlas) -> void:
	var left: float = a.left
	var right: float = a.right
	var xl := -left - 0.1
	var xr := right + 0.1
	var cx := (xr + xl) / 2.0
	var hw := TRUSS_W / 2.0
	var top := GANTRY_BEAM_Y + hw

	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	for x: float in [xl, xr]:
		_truss(st, Vector3(x, 0.15, 0.0), Vector3(x, top, 0.0), Vector3.RIGHT, Vector3.BACK, TRUSS_W)
	_truss(st, Vector3(xl - hw, GANTRY_BEAM_Y, 0.0), Vector3(xr + hw, GANTRY_BEAM_Y, 0.0), Vector3.UP, Vector3.BACK, TRUSS_W)
	var frame := MeshInstance3D.new()
	frame.name = "Treillis"
	frame.mesh = st.commit()
	frame.material_override = _metal()
	node.add_child(frame)

	var ballast := _material(Color(0.32, 0.33, 0.35), DARK, 0, Vector2.ONE)
	for x: float in [xl, xr]:
		_box(node, Vector3(x, 0.08, 0.0), Vector3(1.3, 0.3, 1.3), ballast)
		# Panneaux publicitaires sur les faces avant, arrière et intérieure.
		var inner := 1.0 if x < cx else -1.0
		var design: int = Design.YELLOW if x < cx else Design.BLUE
		for face: float in [1.0, -1.0]:
			var p := _quad(Vector2(TRUSS_W + 0.04, 3.2), _banner_material(atlas, design, true))
			p.position = Vector3(x, 2.4, face * (hw + 0.03))
			p.rotation.y = 0.0 if face > 0.0 else PI
			node.add_child(p)
		var side := _quad(Vector2(TRUSS_W + 0.04, 3.2), _banner_material(atlas, Design.RED, true))
		side.position = Vector3(x + inner * (hw + 0.03), 2.4, 0.0)
		side.rotation.y = inner * PI / 2.0
		node.add_child(side)

	# Banderole principale et damier, accrochés sous la poutre.
	var w := xr - xl - TRUSS_W - 0.1
	var by := GANTRY_BEAM_Y - hw - 0.85
	_board(node, Vector3(cx, by, 0.0), Vector2(w, 1.6), WHITE, a.title, a.subtitle, DARK, Color(0.25, 0.25, 0.28), 0.14)
	var chequer := _material(WHITE, DARK, 2, Vector2(1.0, 1.0) / 0.2, true)
	_box(node, Vector3(cx, by - 0.8 - 0.2, 0.0), Vector3(w, 0.4, 0.14), chequer)
	_box(node, Vector3(cx, by + 0.8 + 0.06, 0.0), Vector3(w, 0.12, 0.16), _material(YELLOW, YELLOW, 0, Vector2.ONE))

	# Chronomètre sur la poutre.
	var cw := minf(4.2, w * 0.45)
	var cy := top + 0.6
	_box(node, Vector3(cx, cy, 0.0), Vector3(cw, 1.2, 0.5), _material(DARK, DARK, 0, Vector2.ONE))
	_board(node, Vector3(cx, cy + 0.6 + 0.22, 0.0), Vector2(cw, 0.44), YELLOW, "VELOTRACE", "", DARK, DARK, 0.5)
	for face: float in [1.0, -1.0]:
		var clock := Label3D.new()
		clock.set_script(Clock)
		clock.text = "0:00:00"
		clock.font = _font()
		clock.font_size = 96
		clock.modulate = Color(1.0, 0.78, 0.15)
		clock.outline_size = 0
		clock.double_sided = false
		clock.shaded = false
		clock.pixel_size = minf(0.95 / 96.0, (cw - 0.4) / _text_width("0:00:00", 96))
		clock.position = Vector3(cx, cy - 0.02, face * 0.26)
		clock.rotation.y = 0.0 if face > 0.0 else PI
		node.add_child(clock)

	# Ligne d'arrivée : bande blanche et damier, inclinés comme la route
	# (chaussée à 5 cm au-dessus de l'axe du tracé).
	var road_w := left + right - 1.8
	var ground := Node3D.new()
	ground.position = Vector3((right - left) / 2.0, 0.085, 0.0)
	var grade: float = a.get("grade", 0.0)
	ground.rotation.x = atan(grade)
	node.add_child(ground)
	var line := _flat(Vector2(road_w, 0.5), _material(WHITE, WHITE, 0, Vector2.ONE))
	line.position = Vector3(0.0, 0.0, 0.0)
	ground.add_child(line)
	var check := _flat(Vector2(road_w, 0.75), _material(WHITE, DARK, 2, Vector2(1.0, 1.0) / 0.25, true))
	check.position = Vector3(0.0, 0.0, 0.625)
	ground.add_child(check)
	for m: MeshInstance3D in [line, check]:
		(m.material_override as ShaderMaterial).set_shader_parameter("roughness", 0.9)
		m.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## Deux piles et une poutre gonflables posées sur des lests.
static func _inflatable(node: Node3D, left: float, right: float, pillar_mat: Material, beam_mat: Material) -> void:
	var foot_mat := _material(DARK, DARK, 0, Vector2.ONE)
	for x: float in [-left, right]:
		var pillar := MeshInstance3D.new()
		var cap := CapsuleMesh.new()
		cap.radius = PILLAR_RADIUS
		cap.height = PILLAR_HEIGHT
		pillar.mesh = cap
		pillar.material_override = pillar_mat
		pillar.position = Vector3(x, PILLAR_HEIGHT / 2.0 - 0.6, 0.0)
		node.add_child(pillar)
		_box(node, Vector3(x, 0.0, 0.0), Vector3(1.5, 0.35, 1.5), foot_mat)
	var span := left + right
	var beam := MeshInstance3D.new()
	var bcap := CapsuleMesh.new()
	bcap.radius = PILLAR_RADIUS + 0.05
	bcap.height = span + 2.0 * PILLAR_RADIUS + 0.1
	beam.mesh = bcap
	beam.material_override = beam_mat
	beam.rotation = Vector3(0.0, 0.0, PI / 2.0)
	beam.position = Vector3((right - left) / 2.0, BEAM_Y, 0.0)
	node.add_child(beam)


## Deux mâts métalliques haubanés au sol.
static func _poles(node: Node3D, left: float, right: float, height: float) -> void:
	var metal := _metal()
	for x: float in [-left, right]:
		var pole := MeshInstance3D.new()
		var cyl := CylinderMesh.new()
		cyl.top_radius = 0.08
		cyl.bottom_radius = 0.1
		cyl.height = height
		cyl.radial_segments = 10
		pole.mesh = cyl
		pole.material_override = metal
		pole.position = Vector3(x, height / 2.0, 0.0)
		node.add_child(pole)
		_box(node, Vector3(x, 0.05, 0.0), Vector3(0.6, 0.12, 0.6), _material(DARK, DARK, 0, Vector2.ONE))


## Panneau rigide (boîte) avec titre et sous-titre lisibles des deux côtés.
static func _board(node: Node3D, center: Vector3, size: Vector2, bg: Color, title: String, subtitle: String, title_color: Color, sub_color: Color, depth := 0.16) -> void:
	_box(node, center, Vector3(size.x, size.y, depth), _material(bg, bg, 0, Vector2.ONE))
	var inner := size.x - 0.5
	for face: float in [1.0, -1.0]:
		var z := center.z + face * (depth / 2.0 + 0.01)
		var turn := 0.0 if face > 0.0 else PI
		var title_h := size.y * (0.56 if subtitle != "" else 0.72)
		var ty := center.y + (size.y * 0.14 if subtitle != "" else 0.0)
		var t := _label(title, 96, inner, title_h, title_color)
		t.position = Vector3(center.x, ty, z)
		t.rotation.y = turn
		node.add_child(t)
		if subtitle != "":
			var s := _label(subtitle, 48, inner, size.y * 0.22, sub_color)
			s.position = Vector3(center.x, center.y - size.y * 0.3, z)
			s.rotation.y = turn
			node.add_child(s)


## Liserés colorés en haut et en bas d'un panneau.
static func _edges(node: Node3D, center: Vector3, size: Vector2, mat: Material) -> void:
	for sy: float in [1.0, -1.0]:
		_box(node, center + Vector3(0.0, sy * (size.y / 2.0 + 0.06), 0.0), Vector3(size.x, 0.12, 0.2), mat)


static func _box(node: Node3D, center: Vector3, size: Vector3, mat: Material) -> MeshInstance3D:
	var mi := MeshInstance3D.new()
	var box := BoxMesh.new()
	box.size = size
	mi.mesh = box
	mi.material_override = mat
	mi.position = center
	node.add_child(mi)
	return mi


## Rectangle vertical face à +Z (UV : (0, 0) en haut à gauche).
static func _quad(size: Vector2, mat: Material) -> MeshInstance3D:
	var q := QuadMesh.new()
	q.size = size
	var mi := MeshInstance3D.new()
	mi.mesh = q
	mi.material_override = mat
	return mi


## Rectangle posé à plat (X en travers, Z le long de la route).
static func _flat(size: Vector2, mat: Material) -> MeshInstance3D:
	var mi := _quad(size, mat)
	mi.rotation.x = -PI / 2.0
	return mi


## Triangle pointe en bas, haut au point d'attache, visible des deux côtés.
static func _triangle(size: Vector2) -> ArrayMesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	var pts := [Vector3(-size.x / 2.0, 0.0, 0.0), Vector3(size.x / 2.0, 0.0, 0.0), Vector3(0.0, -size.y, 0.0)]
	for face: float in [1.0, -1.0]:
		var z := face * 0.02
		var order := [0, 1, 2] if face < 0.0 else [0, 2, 1]
		for k: int in order:
			var p: Vector3 = pts[k]
			st.set_normal(Vector3(0.0, 0.0, face))
			st.add_vertex(Vector3(p.x, p.y, z))
	return st.commit()


## Texte réduit pour tenir dans `width` × `height` mètres.
static func _label(text: String, size: int, width: float, height: float, color: Color) -> Label3D:
	var l := Label3D.new()
	l.text = text
	l.font = _font()
	l.font_size = size
	l.outline_size = 0
	l.modulate = color
	l.double_sided = false
	l.shaded = true
	l.pixel_size = minf(height / (size * 0.95), width / _text_width(text, size))
	return l


static func _text_width(text: String, size: int) -> float:
	return maxf(1.0, _font().get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, size).x)


## Ajoute un tube de `p` à `q` (cylindre à six pans).
static func _tube(st: SurfaceTool, p: Vector3, q: Vector3, r: float) -> void:
	if _cylinder.is_empty():
		var c := CylinderMesh.new()
		c.top_radius = 0.5
		c.bottom_radius = 0.5
		c.height = 1.0
		c.radial_segments = 6
		c.rings = 0
		c.cap_top = false
		c.cap_bottom = false
		_cylinder = c.get_mesh_arrays()
	var d := q - p
	var length := d.length()
	if length < 1e-4:
		return
	var y := d / length
	var ref := Vector3.UP if absf(y.y) < 0.9 else Vector3.RIGHT
	var x := ref.cross(y).normalized()
	var z := x.cross(y)
	_append(st, _cylinder, Transform3D(Basis(x * 2.0 * r, y * length, z * 2.0 * r), (p + q) / 2.0), METAL)


## Poutre en treillis de `p` à `q` : quatre membrures aux coins d'une
## section carrée de côté `w` (selon `a` et `b`), montants et diagonales en
## zigzag sur chaque face.
static func _truss(st: SurfaceTool, p: Vector3, q: Vector3, a: Vector3, b: Vector3, w: float) -> void:
	var h := w / 2.0
	var corners: Array[Vector3] = [a * h + b * h, -a * h + b * h, -a * h - b * h, a * h - b * h]
	for c: Vector3 in corners:
		_tube(st, p + c, q + c, 0.05)
	var n := maxi(1, roundi(p.distance_to(q) / w))
	for k: int in n + 1:
		var o := p.lerp(q, float(k) / n)
		for j: int in 4:
			_tube(st, o + corners[j], o + corners[(j + 1) % 4], 0.025)
	for k: int in n:
		var o0 := p.lerp(q, float(k) / n)
		var o1 := p.lerp(q, float(k + 1) / n)
		for j: int in 4:
			var c0 := corners[j]
			var c1 := corners[(j + 1) % 4]
			if (k + j) % 2 == 0:
				_tube(st, o0 + c0, o1 + c1, 0.022)
			else:
				_tube(st, o0 + c1, o1 + c0, 0.022)


## Barrière « Vauban » galvanisée de 2,5 m (axe X le long de la route, +Z
## vers la chaussée) : lisses, barreaux, montants et pieds plats.
static func _barrier_frame() -> Mesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	var box := BoxMesh.new()
	box.size = Vector3.ONE
	var arrays := box.get_mesh_arrays()
	var parts := [
		[Vector3(2.44, 0.045, 0.045), Vector3(0.0, 1.1, 0.0)],
		[Vector3(2.44, 0.035, 0.035), Vector3(0.0, 0.16, 0.0)],
		[Vector3(0.045, 1.0, 0.045), Vector3(-1.2, 0.62, 0.0)],
		[Vector3(0.045, 1.0, 0.045), Vector3(1.2, 0.62, 0.0)],
		# Pieds plats en travers, débordant côté public.
		[Vector3(0.07, 0.025, 0.85), Vector3(-1.05, 0.012, -0.12)],
		[Vector3(0.07, 0.025, 0.85), Vector3(1.05, 0.012, -0.12)],
		[Vector3(0.035, 0.16, 0.035), Vector3(-1.05, 0.08, 0.0)],
		[Vector3(0.035, 0.16, 0.035), Vector3(1.05, 0.08, 0.0)],
	]
	for k: int in 13:
		parts.append([Vector3(0.018, 0.92, 0.018), Vector3(-1.08 + k * 0.18, 0.63, 0.0)])
	for p: Array in parts:
		var size: Vector3 = p[0]
		var at: Vector3 = p[1]
		_append(st, arrays, Transform3D(Basis().scaled(size), at), METAL)
	var m := st.commit()
	var mat := StandardMaterial3D.new()
	mat.albedo_color = METAL
	mat.metallic = 0.7
	mat.roughness = 0.4
	m.surface_set_material(0, mat)
	return m


## Banderole imprimée tendue côté route sur la barrière (motif de
## l'instance).
static func _barrier_panel(mat: Material) -> Mesh:
	var q := QuadMesh.new()
	q.size = Vector2(2.5, 0.84)
	q.center_offset = Vector3(0.0, 0.64, 0.04)
	q.material = mat
	return q


## Spectateur sommaire : jambes, maillot de la couleur de l'instance, bras
## et tête (une surface par matière).
static func _spectator() -> Mesh:
	var m := ArrayMesh.new()
	var shirt := StandardMaterial3D.new()
	shirt.vertex_color_use_as_albedo = true
	shirt.roughness = 0.8
	var trousers := StandardMaterial3D.new()
	trousers.albedo_color = Color(0.16, 0.18, 0.24)
	trousers.roughness = 0.9
	var skin := StandardMaterial3D.new()
	skin.albedo_color = Color(0.94, 0.7, 0.55)
	skin.roughness = 0.8

	var torso := CapsuleMesh.new()
	torso.radius = 0.2
	torso.height = 0.72
	torso.radial_segments = 7
	torso.rings = 1
	var arm := CapsuleMesh.new()
	arm.radius = 0.06
	arm.height = 0.62
	arm.radial_segments = 5
	arm.rings = 0
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	_append(st, torso.get_mesh_arrays(), Transform3D(Basis().scaled(Vector3(1.0, 1.0, 0.75)), Vector3(0.0, 1.2, 0.0)), Color.WHITE)
	for s: float in [-1.0, 1.0]:
		var b := Basis(Vector3.BACK, s * 0.25)
		_append(st, arm.get_mesh_arrays(), Transform3D(b, Vector3(s * 0.27, 1.17, 0.0)), Color.WHITE)
	# Casquette assortie, visière vers la route.
	var cap := SphereMesh.new()
	cap.radius = 0.128
	cap.height = 0.128
	cap.is_hemisphere = true
	cap.radial_segments = 7
	cap.rings = 2
	_append(st, cap.get_mesh_arrays(), Transform3D(Basis(), Vector3(0.0, 1.74, 0.0)), Color.WHITE)
	var visor := BoxMesh.new()
	visor.size = Vector3(0.2, 0.02, 0.12)
	_append(st, visor.get_mesh_arrays(), Transform3D(Basis(), Vector3(0.0, 1.75, 0.15)), Color.WHITE)
	st.commit(m)
	m.surface_set_material(0, shirt)

	var leg := CylinderMesh.new()
	leg.top_radius = 0.09
	leg.bottom_radius = 0.075
	leg.height = 0.9
	leg.radial_segments = 5
	leg.rings = 0
	leg.cap_top = false
	st = SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	for s: float in [-1.0, 1.0]:
		_append(st, leg.get_mesh_arrays(), Transform3D(Basis(), Vector3(s * 0.1, 0.45, 0.0)), Color.WHITE)
	st.commit(m)
	m.surface_set_material(1, trousers)

	var head := SphereMesh.new()
	head.radius = 0.12
	head.height = 0.25
	head.radial_segments = 7
	head.rings = 3
	st = SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	_append(st, head.get_mesh_arrays(), Transform3D(Basis(), Vector3(0.0, 1.72, 0.0)), Color.WHITE)
	st.commit(m)
	m.surface_set_material(2, skin)
	return m


static func _append(st: SurfaceTool, arrays: Array, xf: Transform3D, color: Color) -> void:
	var verts: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
	var normals: PackedVector3Array = arrays[Mesh.ARRAY_NORMAL]
	var idx: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
	var nb := xf.basis.inverse().transposed()
	for i: int in idx:
		st.set_color(color)
		st.set_normal((nb * normals[i]).normalized())
		st.add_vertex(xf * verts[i])
