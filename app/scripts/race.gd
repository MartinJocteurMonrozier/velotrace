extends RefCounted
## Mobilier de course comme sur les grands tours : arches gonflables au
## départ, au pied et au sommet des ascensions et à l'arrivée (positions et
## textes calculés par velo-world), barrières de police à leurs abords.

const ARCH_SHADER := preload("res://shaders/arch.gdshader")

enum Kind { START, CLIMB_START, CLIMB_TOP, FINISH }

const PILLAR_RADIUS := 0.6
const PILLAR_HEIGHT := 7.6
const BEAM_Y := 6.9
const BANNER_Y := 5.6
const BANNER_H := 1.5


static func build(ride: Object, base_elev: float, quality: int) -> Node3D:
	var root := Node3D.new()
	root.name = "Course"
	for a: Dictionary in ride.route_arches():
		root.add_child(_arch(a, base_elev))
	var buf: PackedFloat32Array = ride.route_barriers(base_elev)
	if not buf.is_empty():
		for mesh: Mesh in [_barrier_frame(), _barrier_panel()]:
			var mm := MultiMesh.new()
			mm.transform_format = MultiMesh.TRANSFORM_3D
			mm.use_colors = true
			mm.mesh = mesh
			mm.instance_count = buf.size() / 16
			mm.buffer = buf
			var mmi := MultiMeshInstance3D.new()
			mmi.multimesh = mm
			if quality == 0:
				mmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			root.add_child(mmi)
	return root


static func _material(base: Color, second: Color, pattern: int, scale: Vector2, local := false) -> ShaderMaterial:
	var m := ShaderMaterial.new()
	m.shader = ARCH_SHADER
	m.set_shader_parameter("base_color", base)
	m.set_shader_parameter("second_color", second)
	m.set_shader_parameter("pattern", pattern)
	m.set_shader_parameter("uv_scale", scale)
	m.set_shader_parameter("use_local", local)
	return m


## Arche : deux piles et une poutre gonflables, un bandeau avec le texte
## lisible en arrivant (et de dos en repartant).
static func _arch(a: Dictionary, base_elev: float) -> Node3D:
	var kind: int = a.kind
	var left: float = a.left
	var right: float = a.right
	var h: Vector2 = a.heading
	var pos: Vector3 = a.position
	var node := Node3D.new()
	node.name = "Arche"
	# X vers la droite de la route, -Z dans le sens de la course.
	var x_axis := Vector3(h.y, 0.0, h.x)
	var z_axis := Vector3(-h.x, 0.0, h.y)
	node.transform = Transform3D(Basis(x_axis, Vector3.UP, z_axis), Vector3(pos.x, pos.y - base_elev, pos.z))

	var red := Color(0.84, 0.08, 0.1)
	var white := Color(0.97, 0.97, 0.95)
	var green := Color(0.1, 0.55, 0.28)
	var dark := Color(0.08, 0.08, 0.09)
	var span := left + right
	var inflate: ShaderMaterial
	var beam_mat: ShaderMaterial
	var text_color := dark
	match kind:
		Kind.START:
			inflate = _material(green, white, 0, Vector2.ONE)
			beam_mat = inflate
			text_color = green.darkened(0.2)
		Kind.FINISH:
			inflate = _material(red, white, 0, Vector2.ONE)
			beam_mat = inflate
		_:
			# Maillot à pois du meilleur grimpeur.
			inflate = _material(white, red, 1, Vector2(6.0, 12.0))
			beam_mat = _material(white, red, 1, Vector2(6.0, (span + 1.3) / 0.6))
			text_color = red

	for side: float in [-1.0, 1.0]:
		var x := -left if side < 0.0 else right
		var pillar := MeshInstance3D.new()
		var cap := CapsuleMesh.new()
		cap.radius = PILLAR_RADIUS
		cap.height = PILLAR_HEIGHT
		pillar.mesh = cap
		pillar.material_override = inflate
		pillar.position = Vector3(x, PILLAR_HEIGHT / 2.0 - 0.6, 0.0)
		node.add_child(pillar)
		# Lest au pied.
		var foot := MeshInstance3D.new()
		var box := BoxMesh.new()
		box.size = Vector3(1.5, 0.35, 1.5)
		foot.mesh = box
		foot.material_override = _material(dark, dark, 0, Vector2.ONE)
		foot.position = Vector3(x, 0.0, 0.0)
		node.add_child(foot)

	var beam := MeshInstance3D.new()
	var bcap := CapsuleMesh.new()
	bcap.radius = PILLAR_RADIUS + 0.05
	bcap.height = span + 2.0 * PILLAR_RADIUS + 0.1
	beam.mesh = bcap
	beam.material_override = beam_mat
	beam.rotation = Vector3(0.0, 0.0, PI / 2.0)
	beam.position = Vector3((right - left) / 2.0, BEAM_Y, 0.0)
	node.add_child(beam)

	var banner_w := span - 2.0 * PILLAR_RADIUS - 0.3
	var banner := MeshInstance3D.new()
	var bbox := BoxMesh.new()
	bbox.size = Vector3(banner_w, BANNER_H, 0.16)
	banner.mesh = bbox
	banner.material_override = _material(white, white, 0, Vector2.ONE)
	banner.position = Vector3((right - left) / 2.0, BANNER_Y, 0.0)
	node.add_child(banner)
	if kind == Kind.FINISH:
		# Damier en haut et en bas du bandeau.
		for y: float in [BANNER_Y + BANNER_H / 2.0, BANNER_Y - BANNER_H / 2.0]:
			var strip := MeshInstance3D.new()
			var sbox := BoxMesh.new()
			sbox.size = Vector3(banner_w, 0.22, 0.18)
			strip.mesh = sbox
			strip.material_override = _material(white, dark, 2, Vector2(1.0 / 0.22, 1.0 / 0.11), true)
			strip.position = Vector3((right - left) / 2.0, y, 0.0)
			node.add_child(strip)

	var title: String = a.title
	var subtitle: String = a.subtitle
	for face: float in [1.0, -1.0]:
		var z := 0.09 * face
		var turn := 0.0 if face > 0.0 else PI
		var t := _label(title, 80, banner_w - 0.4, text_color)
		t.position = Vector3((right - left) / 2.0, BANNER_Y + (0.22 if subtitle != "" else 0.0), z)
		t.rotation.y = turn
		node.add_child(t)
		if subtitle != "":
			var s := _label(subtitle, 34, banner_w - 0.4, dark)
			s.position = Vector3((right - left) / 2.0, BANNER_Y - 0.45, z)
			s.rotation.y = turn
			node.add_child(s)
	return node


## Texte réduit pour tenir dans `width` mètres.
static func _label(text: String, size: int, width: float, color: Color) -> Label3D:
	var l := Label3D.new()
	l.text = text
	l.font_size = size
	l.outline_size = 0
	l.modulate = color
	l.double_sided = false
	l.shaded = true
	# Largeur estimée d'un caractère : ~0,62 em.
	l.pixel_size = minf(0.011, width / maxf(1.0, text.length() * size * 0.62))
	return l


## Barrière de police galvanisée de 2,4 m (axe X le long de la route).
static func _barrier_frame() -> Mesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	var box := BoxMesh.new()
	box.size = Vector3.ONE
	var metal := Color(0.78, 0.8, 0.82)
	var parts := [
		[Vector3(2.4, 0.05, 0.05), Vector3(0.0, 1.08, 0.0)],
		[Vector3(2.4, 0.04, 0.04), Vector3(0.0, 0.14, 0.0)],
		[Vector3(0.05, 1.1, 0.05), Vector3(-1.18, 0.6, 0.0)],
		[Vector3(0.05, 1.1, 0.05), Vector3(1.18, 0.6, 0.0)],
		[Vector3(0.06, 0.04, 0.7), Vector3(-0.95, 0.02, 0.0)],
		[Vector3(0.06, 0.04, 0.7), Vector3(0.95, 0.02, 0.0)],
	]
	for k in 11:
		parts.append([Vector3(0.022, 0.94, 0.022), Vector3(-1.0 + k * 0.2, 0.61, 0.0)])
	for p: Array in parts:
		var size: Vector3 = p[0]
		var at: Vector3 = p[1]
		_append(st, box, Transform3D(Basis().scaled(size), at), metal)
	st.generate_normals()
	var m := st.commit()
	var mat := StandardMaterial3D.new()
	mat.albedo_color = metal
	mat.metallic = 0.6
	mat.roughness = 0.4
	m.surface_set_material(0, mat)
	return m


## Panneau publicitaire accroché à la barrière, de la couleur de l'instance.
static func _barrier_panel() -> Mesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	var box := BoxMesh.new()
	box.size = Vector3.ONE
	_append(st, box, Transform3D(Basis().scaled(Vector3(2.3, 0.72, 0.03)), Vector3(0.0, 0.66, 0.0)), Color.WHITE)
	st.generate_normals()
	var m := st.commit()
	var mat := StandardMaterial3D.new()
	mat.vertex_color_use_as_albedo = true
	mat.roughness = 0.6
	m.surface_set_material(0, mat)
	return m


static func _append(st: SurfaceTool, mesh: PrimitiveMesh, xf: Transform3D, color: Color) -> void:
	var arrays := mesh.get_mesh_arrays()
	var verts: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
	var idx: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
	for i in idx:
		st.set_color(color)
		st.add_vertex(xf * verts[i])
