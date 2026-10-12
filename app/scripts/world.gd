extends RefCounted
## Construit le décor autour du parcours : ciel, soleil, route, puis soit le
## décor réel (relief, images aériennes, OSM via velo-world), soit un décor
## simplifié procédural quand rien n'est disponible.

const ROAD_HALF_WIDTH := 3.0
const TILE_SIZE := 200.0
const TILE_CELLS := 20
const TERRAIN_MARGIN := 450.0
const ROAD_SHADER := preload("res://shaders/road.gdshader")
const WALLS_SHADER := preload("res://shaders/walls.gdshader")
const CLIFF_SHADER := preload("res://shaders/cliff.gdshader")
const RealWorld := preload("res://scripts/real_world.gd")
const Vegetation := preload("res://scripts/vegetation.gd")
const Race := preload("res://scripts/race.gd")
## Ombres du soleil selon la qualité : distance maximale et découpage.
const SHADOW_DISTANCE := [60.0, 120.0, 220.0]
## Brouillard : distance à laquelle le décor disparaît presque.
const FOG_REACH := [3500.0, 5000.0, 7000.0]


## Construit le décor. Avec `real`, renvoie le nœud du décor réel (qui
## charge les carrés détaillés autour de son `focus`), sinon `null`.
static func build(ride: Object, parent: Node3D, base_elev: float, quality := 1, real := false) -> Node3D:
	parent.add_child(_environment(quality))
	parent.add_child(_sun(quality))
	parent.add_child(_road(ride, base_elev, quality))
	parent.add_child(Race.build(ride, base_elev, quality))
	if real:
		var rw := RealWorld.new()
		rw.name = "DecorReel"
		parent.add_child(rw)
		rw.setup(ride, quality)
		return rw
	parent.add_child(_terrain(ride, base_elev))
	parent.add_child(_trees(ride, base_elev))
	return null


static func _environment(quality: int) -> WorldEnvironment:
	var sky_mat := ProceduralSkyMaterial.new()
	sky_mat.sky_top_color = Color(0.32, 0.52, 0.85)
	sky_mat.sky_horizon_color = Color(0.72, 0.80, 0.90)
	sky_mat.ground_horizon_color = Color(0.72, 0.80, 0.90)
	sky_mat.ground_bottom_color = Color(0.25, 0.32, 0.25)
	var sky := Sky.new()
	sky.sky_material = sky_mat
	# Ciel fixe : une petite carte de radiance suffit et coûte peu.
	sky.radiance_size = Sky.RADIANCE_SIZE_64 if quality == 0 else Sky.RADIANCE_SIZE_128
	var env := Environment.new()
	env.background_mode = Environment.BG_SKY
	env.sky = sky
	env.ambient_light_source = Environment.AMBIENT_SOURCE_SKY
	env.ambient_light_energy = 0.9
	env.tonemap_mode = Environment.TONE_MAPPER_FILMIC
	env.fog_enabled = true
	env.fog_light_color = Color(0.70, 0.78, 0.88)
	env.fog_density = 1.7 / FOG_REACH[quality]
	env.fog_sky_affect = 0.0
	var we := WorldEnvironment.new()
	we.environment = env
	return we


static func _sun(quality: int) -> DirectionalLight3D:
	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-48.0, -35.0, 0.0)
	sun.light_energy = 1.2
	sun.shadow_enabled = true
	sun.directional_shadow_max_distance = SHADOW_DISTANCE[quality]
	sun.directional_shadow_mode = DirectionalLight3D.SHADOW_PARALLEL_4_SPLITS if quality == 2 else DirectionalLight3D.SHADOW_PARALLEL_2_SPLITS
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


## Chaussée lissée (un point par mètre) en tronçons, amorces des routes
## adjacentes, ronds-points, murs et falaises le long des talus raides, et
## balises de bord.
static func _road(ride: Object, base_elev: float, quality: int) -> Node3D:
	var root := Node3D.new()
	root.name = "Route"
	var mat := ShaderMaterial.new()
	mat.shader = ROAD_SHADER
	for mesh: Mesh in ride.road_meshes(base_elev):
		var mi := MeshInstance3D.new()
		mi.mesh = mesh
		mi.material_override = mat
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		root.add_child(mi)
	var walls: Array = ride.road_walls(base_elev)
	if not walls.is_empty():
		var wall_mat := ShaderMaterial.new()
		wall_mat.shader = WALLS_SHADER
		for mesh: Mesh in walls:
			var mi := MeshInstance3D.new()
			mi.mesh = mesh
			mi.material_override = wall_mat
			root.add_child(mi)
	var cliffs: Array = ride.road_cliffs(base_elev)
	if not cliffs.is_empty():
		var cliff_mat := ShaderMaterial.new()
		cliff_mat.shader = CLIFF_SHADER
		for mesh: Mesh in cliffs:
			var mi := MeshInstance3D.new()
			mi.mesh = mesh
			mi.material_override = cliff_mat
			root.add_child(mi)
	var bushes: PackedFloat32Array = ride.road_bushes(base_elev)
	if not bushes.is_empty():
		root.add_child(RealWorld._instances(bushes, Vegetation.bush()))
	var posts: PackedFloat32Array = ride.road_posts(base_elev)
	if not posts.is_empty():
		var mm := MultiMesh.new()
		mm.transform_format = MultiMesh.TRANSFORM_3D
		mm.use_colors = true
		mm.mesh = Vegetation.post()
		mm.instance_count = posts.size() / 16
		mm.buffer = posts
		var mmi := MultiMeshInstance3D.new()
		mmi.multimesh = mm
		mmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF if quality == 0 else GeometryInstance3D.SHADOW_CASTING_SETTING_ON
		root.add_child(mmi)
	return root


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
	var scaled: Array[Transform3D] = []
	for t in spots:
		scaled.append(Transform3D(t.basis.scaled(Vector3(4.6, 11.0, 4.6)), t.origin))
	root.add_child(_multimesh(Vegetation.conifer(), scaled))
	return root


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
