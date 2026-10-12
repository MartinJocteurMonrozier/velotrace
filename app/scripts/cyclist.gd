extends Node3D
## Cycliste et vélo en formes simples, pédalage synchronisé sur la cadence.
## L'avant du vélo pointe vers -Z.

const WHEEL_RADIUS := 0.35
const CRANK := 0.17
const THIGH := 0.46
const SHIN := 0.46

var _crank_angle := 0.0
var _wheel_angle := 0.0
var _wheels: Array[Node3D] = []
var _legs := {}
var _cranks: Array[MeshInstance3D] = []

var _bb := Vector3(0.0, 0.30, 0.05)
var _hip := Vector3(0.0, 1.0, 0.20)

var _frame_mat := _mat(Color(0.80, 0.12, 0.10))
var _dark_mat := _mat(Color(0.08, 0.08, 0.09))
var _jersey_mat := _mat(Color(0.10, 0.35, 0.80))
var _skin_mat := _mat(Color(0.86, 0.66, 0.52))
var _metal_mat := _mat(Color(0.7, 0.7, 0.72))


func _ready() -> void:
	# Roues : pneu, et rayons pour voir la rotation.
	for z in [0.52, -0.52]:
		var pivot := Node3D.new()
		pivot.position = Vector3(0.0, WHEEL_RADIUS, z)
		var tire := MeshInstance3D.new()
		var torus := TorusMesh.new()
		torus.inner_radius = WHEEL_RADIUS - 0.035
		torus.outer_radius = WHEEL_RADIUS
		tire.mesh = torus
		tire.material_override = _dark_mat
		tire.rotation_degrees = Vector3(0.0, 0.0, 90.0)
		pivot.add_child(tire)
		for k in 3:
			var spoke := _box(Vector3(0.01, (WHEEL_RADIUS - 0.03) * 2.0, 0.015), _metal_mat)
			spoke.rotation = Vector3(k * PI / 3.0, 0.0, 0.0)
			pivot.add_child(spoke)
		add_child(pivot)
		_wheels.append(pivot)

	# Cadre.
	var rear := Vector3(0.0, WHEEL_RADIUS, 0.52)
	var front := Vector3(0.0, WHEEL_RADIUS, -0.52)
	var seat := Vector3(0.0, 0.92, 0.17)
	var head_top := Vector3(0.0, 0.88, -0.40)
	var head_bottom := Vector3(0.0, 0.70, -0.45)
	for pair in [[_bb, seat], [seat, head_top], [_bb, head_bottom], [head_top, head_bottom],
			[_bb, rear], [seat, rear], [head_bottom, front]]:
		add_child(_bar(pair[0], pair[1], 0.035, _frame_mat))
	add_child(_bar(Vector3(0.0, 0.88, -0.40), Vector3(0.0, 0.95, -0.50), 0.03, _dark_mat))
	add_child(_bar(Vector3(-0.21, 0.95, -0.50), Vector3(0.21, 0.95, -0.50), 0.03, _dark_mat))
	add_child(_bar(Vector3(0.0, 0.97, 0.26), Vector3(0.0, 0.97, 0.08), 0.06, _dark_mat))

	# Haut du corps.
	var shoulder := Vector3(0.0, 1.36, -0.25)
	add_child(_bar(_hip, shoulder, 0.30, _jersey_mat))
	var head := MeshInstance3D.new()
	var sphere := SphereMesh.new()
	sphere.radius = 0.12
	sphere.height = 0.24
	head.mesh = sphere
	head.material_override = _skin_mat
	head.position = Vector3(0.0, 1.50, -0.38)
	add_child(head)
	var helmet := MeshInstance3D.new()
	var dome := SphereMesh.new()
	dome.radius = 0.135
	dome.height = 0.18
	dome.is_hemisphere = true
	helmet.mesh = dome
	helmet.material_override = _jersey_mat
	helmet.position = Vector3(0.0, 1.53, -0.37)
	add_child(helmet)
	for side in [-1.0, 1.0]:
		add_child(_bar(shoulder + Vector3(side * 0.17, 0.0, 0.0), Vector3(side * 0.20, 0.96, -0.49), 0.07, _skin_mat))

	# Jambes et manivelles, recalculées à chaque image.
	for side in [-1.0, 1.0]:
		var thigh := _box(Vector3(0.13, 0.13, THIGH), _dark_mat)
		var shin := _box(Vector3(0.10, 0.10, SHIN), _skin_mat)
		var crank := _box(Vector3(0.02, 0.02, CRANK), _metal_mat)
		add_child(thigh)
		add_child(shin)
		add_child(crank)
		_legs[side] = [thigh, shin]
		_cranks.append(crank)
	_update_legs()


## À appeler à chaque image avec la vitesse (m/s) et la cadence (rpm).
func animate(speed_ms: float, cadence_rpm: float, delta: float) -> void:
	_wheel_angle -= speed_ms / WHEEL_RADIUS * delta
	for w in _wheels:
		w.rotation.x = _wheel_angle
	if speed_ms > 0.3:
		_crank_angle += cadence_rpm / 60.0 * TAU * delta
	_update_legs()


func _update_legs() -> void:
	var i := 0
	for side in [-1.0, 1.0]:
		var a := _crank_angle + (0.0 if side < 0.0 else PI)
		var pedal := _bb + Vector3(side * 0.12, CRANK * cos(a), -CRANK * sin(a))
		var hip := _hip + Vector3(side * 0.10, 0.0, 0.0)
		var knee := _knee(hip, pedal)
		_place(_legs[side][0], hip, knee)
		_place(_legs[side][1], knee, pedal)
		_place(_cranks[i], _bb + Vector3(side * 0.08, 0.0, 0.0), pedal)
		i += 1


## Genou par cinématique inverse à deux segments, plié vers l'avant.
func _knee(hip: Vector3, foot: Vector3) -> Vector3:
	var d := Vector2(foot.z - hip.z, foot.y - hip.y)
	var reach := clampf(d.length(), 0.05, THIGH + SHIN - 0.01)
	var cos_a := (THIGH * THIGH + reach * reach - SHIN * SHIN) / (2.0 * THIGH * reach)
	var a := acos(clampf(cos_a, -1.0, 1.0))
	var dir := d.normalized()
	var k1 := dir.rotated(a) * THIGH
	var k2 := dir.rotated(-a) * THIGH
	var k := k1 if k1.x < k2.x else k2
	return Vector3(lerpf(hip.x, foot.x, 0.5), hip.y + k.y, hip.z + k.x)


func _place(node: Node3D, a: Vector3, b: Vector3) -> void:
	var dir := b - a
	var up := Vector3.UP if absf(dir.normalized().dot(Vector3.UP)) < 0.99 else Vector3.RIGHT
	node.transform = Transform3D(Basis.looking_at(dir, up), (a + b) * 0.5)


func _bar(a: Vector3, b: Vector3, thickness: float, mat: Material) -> MeshInstance3D:
	var mi := _box(Vector3(thickness, thickness, a.distance_to(b)), mat)
	_place(mi, a, b)
	return mi


func _box(size: Vector3, mat: Material) -> MeshInstance3D:
	var mi := MeshInstance3D.new()
	var box := BoxMesh.new()
	box.size = size
	mi.mesh = box
	mi.material_override = mat
	return mi


static func _mat(color: Color) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = color
	m.roughness = 0.6
	return m
