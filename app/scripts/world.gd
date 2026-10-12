extends RefCounted
## Construit le décor autour du parcours : ciel, soleil, relief, route, arbres.

const ROAD_HALF_WIDTH := 3.0
const TILE_SIZE := 200.0
const TILE_CELLS := 20
const TERRAIN_MARGIN := 450.0
const ROAD_SHADER := preload("res://shaders/road.gdshader")


static func build(ride: Object, parent: Node3D, base_elev: float) -> void:
	parent.add_child(_environment())
	parent.add_child(_sun())
	parent.add_child(_terrain(ride, base_elev))
	parent.add_child(_road(ride.route_points(), ride.route_distances(), base_elev))
	parent.add_child(_trees(ride, base_elev))


static func _environment() -> WorldEnvironment:
	var sky_mat := ProceduralSkyMaterial.new()
	sky_mat.sky_top_color = Color(0.32, 0.52, 0.85)
	sky_mat.sky_horizon_color = Color(0.72, 0.80, 0.90)
	sky_mat.ground_horizon_color = Color(0.72, 0.80, 0.90)
	sky_mat.ground_bottom_color = Color(0.25, 0.32, 0.25)
	var sky := Sky.new()
	sky.sky_material = sky_mat
	var env := Environment.new()
	env.background_mode = Environment.BG_SKY
	env.sky = sky
	env.ambient_light_source = Environment.AMBIENT_SOURCE_SKY
	env.ambient_light_energy = 0.9
	env.tonemap_mode = Environment.TONE_MAPPER_FILMIC
	env.fog_enabled = true
	env.fog_light_color = Color(0.70, 0.78, 0.88)
	env.fog_density = 0.00035
	env.fog_sky_affect = 0.0
	var we := WorldEnvironment.new()
	we.environment = env
	return we


static func _sun() -> DirectionalLight3D:
	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-48.0, -35.0, 0.0)
	sun.light_energy = 1.2
	sun.shadow_enabled = true
	sun.directional_shadow_max_distance = 250.0
	return sun


static func _terrain(ride: Object, base_elev: float) -> Node3D:
	var root := Node3D.new()
	root.name = "Relief"
	var mat := StandardMaterial3D.new()
	mat.vertex_color_use_as_albedo = true
	mat.roughness = 1.0
	var step := TILE_SIZE / TILE_CELLS
	var n := TILE_CELLS + 1
	for origin: Vector2 in ride.terrain_tiles(TILE_SIZE, TERRAIN_MARGIN):
		var heights: PackedFloat32Array = ride.terrain_tile_heights(origin, TILE_SIZE, TILE_CELLS)
		var st := SurfaceTool.new()
		st.begin(Mesh.PRIMITIVE_TRIANGLES)
		for j in n:
			for i in n:
				var x := origin.x + i * step
				var z := origin.y + j * step
				var y := heights[j * n + i] - base_elev
				# Herbe plus ou moins claire selon une variation douce.
				var v := 0.5 + 0.25 * sin(x * 0.013 + z * 0.007) + 0.25 * sin(x * 0.031 - z * 0.023)
				st.set_color(Color(0.13, 0.30, 0.10).lerp(Color(0.30, 0.42, 0.15), clampf(v, 0.0, 1.0)))
				st.set_uv(Vector2(i, j))
				st.add_vertex(Vector3(x, y, z))
		for j in TILE_CELLS:
			for i in TILE_CELLS:
				var a := j * n + i
				st.add_index(a)
				st.add_index(a + 1)
				st.add_index(a + n)
				st.add_index(a + 1)
				st.add_index(a + n + 1)
				st.add_index(a + n)
		st.generate_normals()
		var mi := MeshInstance3D.new()
		mi.mesh = st.commit()
		mi.material_override = mat
		root.add_child(mi)
	return root


static func _road(points: PackedVector3Array, distances: PackedFloat32Array, base_elev: float) -> MeshInstance3D:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	var count := points.size()
	for i in count:
		var prev := points[maxi(i - 1, 0)]
		var next := points[mini(i + 1, count - 1)]
		var t := Vector3(next.x - prev.x, 0.0, next.z - prev.z).normalized()
		var right := Vector3(-t.z, 0.0, t.x)
		var p := points[i] - Vector3(0.0, base_elev - 0.05, 0.0)
		st.set_normal(Vector3.UP)
		st.set_uv(Vector2(0.0, distances[i]))
		st.add_vertex(p - right * ROAD_HALF_WIDTH)
		st.set_normal(Vector3.UP)
		st.set_uv(Vector2(1.0, distances[i]))
		st.add_vertex(p + right * ROAD_HALF_WIDTH)
	for i in count - 1:
		var a := i * 2
		st.add_index(a)
		st.add_index(a + 2)
		st.add_index(a + 1)
		st.add_index(a + 1)
		st.add_index(a + 2)
		st.add_index(a + 3)
	var mat := ShaderMaterial.new()
	mat.shader = ROAD_SHADER
	var mi := MeshInstance3D.new()
	mi.name = "Route"
	mi.mesh = st.commit()
	mi.material_override = mat
	mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	return mi


static func _trees(ride: Object, base_elev: float) -> Node3D:
	var root := Node3D.new()
	root.name = "Arbres"
	var rng := RandomNumberGenerator.new()
	rng.seed = 42
	var spots: Array[Transform3D] = []
	var points: PackedVector3Array = ride.route_points()
	for i in range(0, points.size(), 3):
		for k in 3:
			var p := points[i]
			var angle := rng.randf() * TAU
			var dist := lerpf(9.0, 220.0, pow(rng.randf(), 1.6))
			var x := p.x + cos(angle) * dist
			var z := p.z + sin(angle) * dist
			if ride.distance_to_route(x, z, 12.0) < 9.0:
				continue
			var y: float = ride.ground_height(x, z) - base_elev
			var s := rng.randf_range(0.7, 1.4)
			spots.append(Transform3D(Basis().scaled(Vector3(s, s * rng.randf_range(0.8, 1.3), s)), Vector3(x, y, z)))
	root.add_child(_multimesh(_cone(Color(0.12, 0.30, 0.12), 2.4, 7.0, 4.5), spots))
	root.add_child(_multimesh(_cone(Color(0.33, 0.22, 0.12), 0.25, 2.2, 1.1), spots))
	return root


static func _cone(color: Color, radius: float, height: float, lift: float) -> Mesh:
	var mesh := CylinderMesh.new()
	mesh.top_radius = 0.0 if radius > 1.0 else radius
	mesh.bottom_radius = radius
	mesh.height = height
	mesh.radial_segments = 8
	var mat := StandardMaterial3D.new()
	mat.albedo_color = color
	mat.roughness = 1.0
	mesh.material = mat
	# Décale le maillage pour que sa base repose au sol.
	var st := SurfaceTool.new()
	st.create_from(mesh, 0)
	var arrays := st.commit_to_arrays()
	var verts: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
	for v in verts.size():
		verts[v].y += lift
	arrays[Mesh.ARRAY_VERTEX] = verts
	var out := ArrayMesh.new()
	out.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	out.surface_set_material(0, mat)
	return out


static func _multimesh(mesh: Mesh, spots: Array[Transform3D]) -> MultiMeshInstance3D:
	var mm := MultiMesh.new()
	mm.transform_format = MultiMesh.TRANSFORM_3D
	mm.mesh = mesh
	mm.instance_count = spots.size()
	for i in spots.size():
		mm.set_instance_transform(i, spots[i])
	var mmi := MultiMeshInstance3D.new()
	mmi.multimesh = mm
	return mmi
