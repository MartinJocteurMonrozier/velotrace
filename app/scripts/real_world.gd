extends Node3D
## Décor réel fourni par velo-world : grands carrés lointains construits une
## fois, carrés détaillés (relief fin, image, arbres, bâtiments, routes, eau)
## chargés autour de la caméra et libérés quand on s'éloigne.

const TERRAIN_SHADER := preload("res://shaders/terrain.gdshader")
const BUILDING_SHADER := preload("res://shaders/buildings.gdshader")
const ROAD_SHADER := preload("res://shaders/road.gdshader")
const Vegetation := preload("res://scripts/vegetation.gd")
## Distance de chargement des carrés détaillés selon la qualité.
const LOAD_RADIUS := [900.0, 1200.0, 1600.0]
## Distance d'affichage des arbres et des bâtiments selon la qualité.
const TREE_VIEW := [300.0, 480.0, 800.0]
const BUILDING_VIEW := [900.0, 1300.0, 1800.0]
const BUSH_VIEW := [150.0, 250.0, 400.0]
## Distance (au centre d'une cellule de 128 m) jusqu'à laquelle les arbres sont détaillés.
const TREE_DETAIL_VIEW := [110.0, 170.0, 260.0]
const UNLOAD_EXTRA := 350.0
const MAX_PER_FRAME := 2

var ride: Object
var quality := 1
var focus := Vector3.ZERO

var _near_centers := PackedVector3Array()
var _loaded := {}  # index -> Node3D
var _mask_image: Image
var _mask_texture: ImageTexture
var _mask_origin := Vector2.ZERO
var _mask_dirty := false
var _far_materials: Array[ShaderMaterial] = []
var _plan_timer := 0.0
var _near_size := 512.0
var _building_mat: ShaderMaterial
var _far_building_mat: ShaderMaterial
var _detail: NoiseTexture2D
var _road_mat: ShaderMaterial
var _water_mat: StandardMaterial3D


func setup(p_ride: Object, p_quality: int) -> void:
	ride = p_ride
	quality = clampi(p_quality, 0, 2)
	_detail = _detail_texture()
	_building_mat = ShaderMaterial.new()
	_building_mat.shader = BUILDING_SHADER
	_road_mat = ShaderMaterial.new()
	_road_mat.shader = ROAD_SHADER
	_water_mat = _vertex_color_material(0.08)
	_water_mat.metallic = 0.2
	_near_centers = ride.world_near_centers()
	_build_far()


## Nombre de carrés détaillés nécessaires autour de `pos` et combien sont prêts.
func readiness(pos: Vector3) -> Vector2i:
	var wanted := _wanted(pos)
	var ready := 0
	for i in wanted:
		if _loaded.has(i):
			ready += 1
	return Vector2i(ready, wanted.size())


func _process(delta: float) -> void:
	_plan_timer -= delta
	if _plan_timer <= 0.0:
		_plan_timer = 0.3
		_plan()
	for d: Dictionary in ride.world_take(MAX_PER_FRAME):
		_integrate(d)
	if _mask_dirty:
		_mask_dirty = false
		_mask_texture.update(_mask_image)


func _wanted(pos: Vector3) -> PackedInt32Array:
	var r: float = LOAD_RADIUS[quality]
	var list := []
	for i in _near_centers.size():
		var c := _near_centers[i]
		var d := Vector2(c.x - pos.x, c.z - pos.z).length()
		if d < r:
			list.append([d, i])
	list.sort_custom(func(a, b): return a[0] < b[0])
	var out := PackedInt32Array()
	for e in list:
		out.append(e[1])
	return out


func _plan() -> void:
	var wanted := _wanted(focus)
	var missing := PackedInt32Array()
	for i in wanted:
		if not _loaded.has(i):
			missing.append(i)
	ride.world_want(missing)
	var limit: float = LOAD_RADIUS[quality] + UNLOAD_EXTRA
	for i in _loaded.keys():
		var c := _near_centers[i]
		if Vector2(c.x - focus.x, c.z - focus.z).length() > limit:
			_loaded[i].queue_free()
			_loaded.erase(i)
			_set_mask(i, false)


func _integrate(d: Dictionary) -> void:
	var i: int = d.index
	var c: Vector3 = d.center
	if _loaded.has(i) or Vector2(c.x - focus.x, c.z - focus.z).length() > LOAD_RADIUS[quality] + UNLOAD_EXTRA:
		return
	var node := Node3D.new()
	node.position = c
	add_child(node)
	var terrain := MeshInstance3D.new()
	terrain.mesh = d.terrain
	var mat := ShaderMaterial.new()
	mat.shader = TERRAIN_SHADER
	mat.set_shader_parameter("albedo_tex", d.texture)
	mat.set_shader_parameter("detail_tex", _detail)
	terrain.material_override = mat
	terrain.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	node.add_child(terrain)
	var half_diag := 362.0
	var tree_end: float = TREE_VIEW[quality] + half_diag
	# Arbres par cellules de 128 m : modèle détaillé près de la caméra,
	# modèle allégé plus loin.
	var tree_shadow := GeometryInstance3D.SHADOW_CASTING_SETTING_ON if quality == 2 else GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	for cell: Dictionary in d.get("tree_cells", []):
		var cnode := Node3D.new()
		cnode.position = cell.center
		node.add_child(cnode)
		for key in ["conifers", "broadleaves"]:
			var buf: PackedFloat32Array = cell[key]
			if buf.is_empty():
				continue
			var conifer: bool = key == "conifers"
			var hi := _instances(buf, Vegetation.conifer() if conifer else Vegetation.broadleaf())
			hi.visibility_range_end = TREE_DETAIL_VIEW[quality]
			hi.cast_shadow = tree_shadow
			cnode.add_child(hi)
			var lo := _instances(buf, Vegetation.conifer_low() if conifer else Vegetation.broadleaf_low())
			lo.visibility_range_begin = TREE_DETAIL_VIEW[quality]
			lo.visibility_range_end = TREE_VIEW[quality] + 90.0
			lo.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			cnode.add_child(lo)
	var bushes: PackedFloat32Array = d.get("bushes", PackedFloat32Array())
	if not bushes.is_empty():
		var b := _instances(bushes, Vegetation.bush())
		b.visibility_range_end = BUSH_VIEW[quality] + half_diag
		b.cast_shadow = tree_shadow
		node.add_child(b)
	for key in ["buildings", "roads", "water"]:
		if not d.has(key):
			continue
		var mi := MeshInstance3D.new()
		mi.mesh = d[key]
		match key:
			"buildings":
				mi.material_override = _building_mat
				mi.visibility_range_end = BUILDING_VIEW[quality] + half_diag
				mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF if quality == 0 else GeometryInstance3D.SHADOW_CASTING_SETTING_ON
			"roads":
				mi.material_override = _road_mat
				mi.visibility_range_end = BUILDING_VIEW[quality] + half_diag
				mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			"water":
				mi.material_override = _water_mat
				mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		node.add_child(mi)
	_loaded[i] = node
	_set_mask(i, true)


static func _instances(buf: PackedFloat32Array, mesh: Mesh) -> MultiMeshInstance3D:
	var mm := MultiMesh.new()
	mm.transform_format = MultiMesh.TRANSFORM_3D
	mm.use_colors = true
	mm.mesh = mesh
	mm.instance_count = buf.size() / 16
	mm.buffer = buf
	var mmi := MultiMeshInstance3D.new()
	mmi.multimesh = mm
	return mmi


func _build_far() -> void:
	var n: int = ride.world_far_count()
	var far_size: float = ride.world_ranges().get("far_size", 2048.0)
	var near_size: float = ride.world_ranges().get("near_size", 512.0)
	_near_size = near_size
	# Masque couvrant tous les grands carrés, un texel par carré détaillé.
	var lo := Vector2(INF, INF)
	var hi := Vector2(-INF, -INF)
	var datas := []
	for k in n:
		var d: Dictionary = ride.world_far(k)
		datas.append(d)
		var c: Vector3 = d.center
		lo = Vector2(minf(lo.x, c.x), minf(lo.y, c.z))
		hi = Vector2(maxf(hi.x, c.x), maxf(hi.y, c.z))
	if n == 0:
		return
	_mask_origin = lo - Vector2(far_size, far_size) * 0.5
	var per_far := int(round(far_size / near_size))
	var cells := Vector2i(((hi - lo) / near_size).round()) + Vector2i(per_far, per_far)
	_mask_image = Image.create(cells.x, cells.y, false, Image.FORMAT_R8)
	_mask_texture = ImageTexture.create_from_image(_mask_image)
	var rect := Vector4(_mask_origin.x, _mask_origin.y, cells.x * near_size, cells.y * near_size)
	_far_building_mat = ShaderMaterial.new()
	_far_building_mat.shader = BUILDING_SHADER
	_far_building_mat.set_shader_parameter("use_mask", true)
	_far_building_mat.set_shader_parameter("near_mask", _mask_texture)
	_far_building_mat.set_shader_parameter("mask_rect", rect)
	for d: Dictionary in datas:
		if d.has("buildings"):
			var bi := MeshInstance3D.new()
			bi.position = d.center
			bi.mesh = d.buildings
			bi.material_override = _far_building_mat
			bi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			add_child(bi)
		var mi := MeshInstance3D.new()
		mi.position = d.center
		mi.mesh = d.mesh
		var mat := ShaderMaterial.new()
		mat.shader = TERRAIN_SHADER
		mat.set_shader_parameter("albedo_tex", d.texture)
		mat.set_shader_parameter("detail_tex", _detail)
		mat.set_shader_parameter("use_mask", true)
		mat.set_shader_parameter("near_mask", _mask_texture)
		mat.set_shader_parameter("mask_rect", rect)
		mi.material_override = mat
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		_far_materials.append(mat)
		add_child(mi)


func _set_mask(i: int, on: bool) -> void:
	if _mask_image == null:
		return
	var c := _near_centers[i]
	var px := int(floor((c.x - _mask_origin.x) / _near_size))
	var pz := int(floor((c.z - _mask_origin.y) / _near_size))
	if px >= 0 and pz >= 0 and px < _mask_image.get_width() and pz < _mask_image.get_height():
		_mask_image.set_pixel(px, pz, Color(1, 1, 1) if on else Color(0, 0, 0))
		_mask_dirty = true


static func _vertex_color_material(roughness: float) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.vertex_color_use_as_albedo = true
	m.vertex_color_is_srgb = true
	m.roughness = roughness
	return m


## Grain de détail du sol (bruit sans couture), partagé par tous les carrés.
static func _detail_texture() -> NoiseTexture2D:
	var noise := FastNoiseLite.new()
	noise.noise_type = FastNoiseLite.TYPE_SIMPLEX_SMOOTH
	noise.frequency = 0.03
	noise.fractal_octaves = 4
	var tex := NoiseTexture2D.new()
	tex.width = 256
	tex.height = 256
	tex.seamless = true
	tex.generate_mipmaps = true
	tex.noise = noise
	return tex
