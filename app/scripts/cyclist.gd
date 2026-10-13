extends Node3D
## Cycliste et vélo de route, pédalage synchronisé sur la cadence et posture
## choisie selon la télémétrie : pied à terre, assis sur les cocottes, aéro
## dans le bas du cintre, en danseuse. Le vélo est en maillages procéduraux ;
## le corps (rider_body.gd) suit les cibles articulaires calculées ici.
## L'avant du vélo pointe vers -Z, la droite du cycliste vers +X.

const RiderBody := preload("res://scripts/rider_body.gd")

## Postures, dans l'ordre des tableaux P_* plus bas.
enum Posture { ARRET, ASSIS, AERO, DANSEUSE }

## Seuils de posture, avec hystérésis : on entre au premier seuil, on sort au second.
## Arrêt sous 0,4 m/s, pied de nouveau sur la pédale au-dessus de 1 m/s.
const STOP_IN_MS := 0.4
const STOP_OUT_MS := 1.0
## Sprint en danseuse au-delà de 8 W/kg (sortie sous 6,5 W/kg).
const SPRINT_IN_WKG := 8.0
const SPRINT_OUT_WKG := 6.5
## Relance en danseuse : puissance 1,5 fois la moyenne des dernières secondes
## (sortie sous 1,2 fois), à partir de 5,5 W/kg.
const SURGE_IN_RATIO := 1.5
const SURGE_OUT_RATIO := 1.2
const SURGE_MIN_WKG := 5.5
const SURGE_AVG_S := 8.0
## Montée en danseuse : pente au-delà de 6 % et plus de 4,5 W/kg
## (sortie sous 4,5 % ou sous 3,8 W/kg).
const CLIMB_IN_GRADE := 0.06
const CLIMB_OUT_GRADE := 0.045
const CLIMB_IN_WKG := 4.5
const CLIMB_OUT_WKG := 3.8
## Aéro sur le plat et en descente : pente sous 2 % et plus de 3,8 W/kg
## (sortie au-dessus de 3 % ou sous 3,2 W/kg).
const AERO_IN_GRADE := 0.02
const AERO_OUT_GRADE := 0.03
const AERO_IN_WKG := 3.8
const AERO_OUT_WKG := 3.2
## Aéro aussi en descente rapide sans pédaler : pente sous -4 % et plus de 50 km/h
## (sortie sous 40 km/h).
const TUCK_GRADE := -0.04
const TUCK_IN_MS := 13.9
const TUCK_OUT_MS := 11.0
## Durée minimale d'une posture avant d'en changer (sauf arrêt et départ).
const POSTURE_MIN_S := 1.5
## Constante de temps du fondu entre postures (presque terminé en 1 s).
const BLEND_S := 0.3
## Puissance sous laquelle le cycliste roue libre, pédales à l'horizontale.
const FREEWHEEL_W := 5.0

## Bassin (x, hauteur au-dessus du pédalier, avancée) pour chaque posture.
const P_HIP := [Vector3(-0.07, 0.62, -0.04), Vector3(0.0, 0.79, -0.165), Vector3(0.0, 0.77, -0.13), Vector3(0.0, 0.89, 0.02)]
## Inclinaison du dos par rapport à l'horizontale (degrés).
const P_SPINE := [62.0, 40.0, 16.0, 27.0]
## Mains : 0 sur les cocottes, 1 dans le bas du cintre.
const P_DROPS := [0.0, 0.0, 1.0, 0.0]
## Tête : avancée le long du dos et hauteur au-dessus des épaules.
const P_NECK := [Vector2(0.07, 0.165), Vector2(0.09, 0.135), Vector2(0.12, 0.08), Vector2(0.1, 0.11)]
## Tête penchée vers le bas (degrés).
const P_HEAD_PITCH := [4.0, 10.0, -4.0, 16.0]

## Pied à terre : vélo penché vers le pied gauche, corps un peu moins.
const STOP_TILT := 0.13
const STOP_BODY_TILT := 0.04
## Pédale droite en haut, un peu en avant, prête à repartir (rad depuis le haut).
const STOP_RIGHT_PEDAL := PI / 4.0
## Danseuse : balancement du vélo, contre-mouvement du corps, déplacement des hanches.
const ROCK := 0.14
const BODY_FOLLOW := -0.25
const SWAY := 0.035

const WHEEL_RADIUS := 0.338
## Pédalier 7 cm sous les axes de roues.
const BB_Y := WHEEL_RADIUS - 0.07
const BB_Z := 0.06
const REAR_F := -0.405
const FRONT_F := 0.585
const AXLE_H := 0.07
## Centre du cintre (avancée, hauteur) par rapport au pédalier.
const BAR_F := 0.486
const BAR_H := 0.574
const BAR_X := 0.205
const CRANK := 0.17
const PEDAL_X := 0.112
const THIGH := 0.45
const SHIN := 0.45
const UPPER_ARM := 0.30
## Du coude au creux de la main.
const FOREARM := 0.33
## Des hanches aux épaules.
const SPINE := 0.50
const HIP_HALF := 0.09
const SHOULDER_HALF := 0.17
## Cheville par rapport à l'axe de pédale, dans le repère du pied.
const ANKLE_UP := 0.107
const ANKLE_BACK := 0.12
## Coup de cheville : pointe plus basse en bas et à l'arrière du coup de pédale.
const ANKLE_BASE := 0.26
const ANKLE_AMP := 0.17
const ANKLE_PHASE := 3.5
const STAND_TOE := 0.14
## Le sol, sous les pneus (main.gd soulève le cycliste de 5 cm).
const GROUND_Y := -0.05
const SOLE_TO_ANKLE := 0.093

const WHITE := Color(0.93, 0.93, 0.95)
const CARBON := Color(0.045, 0.045, 0.05)
const TAPE := Color(0.05, 0.05, 0.055)
const SILVER := Color(0.78, 0.78, 0.8)
const STEEL := Color(0.36, 0.36, 0.38)
const DARK_METAL := Color(0.14, 0.14, 0.15)
const TREAD := Color(0.06, 0.06, 0.06)
const GUM := Color(0.6, 0.43, 0.26)
## Tenues et cadres : la première est celle du joueur, les suivantes celles
## des coureurs virtuels (maillot, maillot foncé, liseré, cadre).
const KITS := [
	[Color(0.06, 0.26, 0.70), Color(0.03, 0.12, 0.38), Color(0.86, 0.07, 0.09), Color(0.72, 0.04, 0.06)],
	[Color(0.05, 0.50, 0.24), Color(0.02, 0.24, 0.11), Color(0.95, 0.95, 0.95), Color(0.06, 0.06, 0.07)],
	[Color(0.95, 0.78, 0.05), Color(0.10, 0.10, 0.10), Color(0.10, 0.10, 0.10), Color(0.92, 0.92, 0.94)],
	[Color(0.80, 0.08, 0.10), Color(0.40, 0.03, 0.05), Color(0.95, 0.95, 0.95), Color(0.06, 0.06, 0.07)],
	[Color(0.92, 0.92, 0.94), Color(0.12, 0.12, 0.14), Color(0.05, 0.40, 0.75), Color(0.05, 0.25, 0.55)],
	[Color(0.95, 0.45, 0.06), Color(0.05, 0.10, 0.25), Color(0.05, 0.10, 0.25), Color(0.95, 0.45, 0.06)],
	[Color(0.02, 0.55, 0.60), Color(0.01, 0.25, 0.30), Color(0.98, 0.85, 0.20), Color(0.92, 0.92, 0.94)],
	[Color(0.42, 0.12, 0.62), Color(0.18, 0.05, 0.30), Color(0.98, 0.62, 0.20), Color(0.10, 0.10, 0.12)],
	[Color(0.98, 0.55, 0.70), Color(0.35, 0.10, 0.20), Color(0.10, 0.10, 0.10), Color(0.10, 0.10, 0.12)],
]
## Tenue choisie (indice dans KITS), à fixer avant l'ajout à la scène.
var kit := 0
var _jersey: Color
var _jersey_dark: Color
var _accent: Color
var _frame_col: Color

## Maillages partagés par tous les cyclistes de même tenue.
static var _mesh_cache := {}

var _posture := Posture.ASSIS
var _posture_time := 0.0
var _weights := PackedFloat32Array([0.0, 1.0, 0.0, 0.0])
var _first := true
var _avg_power := 0.0
var _power := 0.0

var _crank_angle := 0.0
var _wheel_angle := 0.0

var _m_paint := _material(0.3, 0.05, true)
var _m_matte := _material(0.8, 0.0)
var _m_gloss := _material(0.25, 0.0)
var _m_metal := _material(0.3, 0.8)

var _bike: Node3D
var _wheels: Array[Node3D] = []
var _crankset: Node3D
var _pedals: Array[Node3D] = []
var _body: RiderBody


func _ready() -> void:
	var k: Array = KITS[posmod(kit, KITS.size())]
	_jersey = k[0]
	_jersey_dark = k[1]
	_accent = k[2]
	_frame_col = k[3]
	_bike = Node3D.new()
	add_child(_bike)
	_bike.add_child(_instance(_cached("_frame_mesh", _frame_mesh)))
	var wheel := _cached("_wheel_mesh", _wheel_mesh)
	for f: float in [REAR_F, FRONT_F]:
		var w := _instance(wheel)
		w.position = _p(f, AXLE_H)
		_bike.add_child(w)
		_wheels.append(w)
	_crankset = _instance(_cached("_crank_mesh", _crank_mesh))
	_crankset.position = _p(0.0, 0.0)
	_bike.add_child(_crankset)
	var pedal := _cached("_pedal_mesh", _pedal_mesh)
	for i in 2:
		var p := _instance(pedal)
		_bike.add_child(p)
		_pedals.append(p)
	_body = RiderBody.new()
	_body.setup(_jersey, _jersey_dark, _accent)
	add_child(_body)
	_update_pose()


func _cached(key: String, build: Callable) -> ArrayMesh:
	var id := "%d/%s" % [posmod(kit, KITS.size()), key]
	if not _mesh_cache.has(id):
		_mesh_cache[id] = build.call()
	return _mesh_cache[id]


## Choisit la posture d'après la télémétrie (vitesse en m/s, puissance en W,
## pente en fraction, masse du cycliste en kg) et la fond avec la précédente.
func set_state(speed_ms: float, power_w: float, grade: float, rider_kg: float, delta: float) -> void:
	_power = power_w
	if _first:
		_avg_power = power_w
	else:
		_avg_power = lerpf(_avg_power, power_w, 1.0 - exp(-delta / SURGE_AVG_S))
	var wanted := _choose(speed_ms, power_w / maxf(rider_kg, 30.0), power_w, grade)
	_posture_time += delta
	if wanted != _posture and (_first or _posture_time >= POSTURE_MIN_S
			or wanted == Posture.ARRET or _posture == Posture.ARRET):
		_posture = wanted
		_posture_time = 0.0
	var k := 1.0 if _first else 1.0 - exp(-delta / BLEND_S)
	for i in _weights.size():
		_weights[i] = lerpf(_weights[i], 1.0 if i == _posture else 0.0, k)
	_first = false


func _choose(speed: float, wkg: float, power: float, grade: float) -> Posture:
	var cur := _posture
	if speed < (STOP_OUT_MS if cur == Posture.ARRET else STOP_IN_MS):
		return Posture.ARRET
	var standing := cur == Posture.DANSEUSE
	var sprint := wkg > (SPRINT_OUT_WKG if standing else SPRINT_IN_WKG)
	var surge := wkg > SURGE_MIN_WKG and power > _avg_power * (SURGE_OUT_RATIO if standing else SURGE_IN_RATIO)
	var climb := grade > (CLIMB_OUT_GRADE if standing else CLIMB_IN_GRADE) \
			and wkg > (CLIMB_OUT_WKG if standing else CLIMB_IN_WKG)
	if sprint or surge or climb:
		return Posture.DANSEUSE
	var aero := cur == Posture.AERO
	var flat := grade < (AERO_OUT_GRADE if aero else AERO_IN_GRADE) \
			and wkg > (AERO_OUT_WKG if aero else AERO_IN_WKG)
	var tuck := grade < TUCK_GRADE and speed > (TUCK_OUT_MS if aero else TUCK_IN_MS)
	if flat or tuck:
		return Posture.AERO
	return Posture.ASSIS


## À appeler à chaque image avec la vitesse (m/s) et la cadence (rpm).
func animate(speed_ms: float, cadence_rpm: float, delta: float) -> void:
	_wheel_angle -= speed_ms / WHEEL_RADIUS * delta
	for w in _wheels:
		w.rotation.x = _wheel_angle
	if _posture == Posture.ARRET:
		# Pédale droite remontée pour repartir, en rétropédalant au besoin.
		_crank_angle = rotate_toward(_crank_angle, STOP_RIGHT_PEDAL - PI, 3.0 * delta)
	elif _power < FREEWHEEL_W:
		# Roue libre : pédales à l'horizontale.
		var level := PI / 2.0 if sin(_crank_angle) >= 0.0 else -PI / 2.0
		_crank_angle = rotate_toward(_crank_angle, level, 2.0 * delta)
	elif speed_ms > 0.3:
		_crank_angle = wrapf(_crank_angle + cadence_rpm / 60.0 * TAU * delta, -PI, PI)
	_update_pose()


func _mix(values: Array) -> Variant:
	var acc: Variant = values[0] * _weights[0]
	for i in range(1, values.size()):
		acc += values[i] * _weights[i]
	return acc


## Place le vélo et le corps : bassin, dos, puis bras et jambes par cinématique
## inverse vers les cocottes et les pédales.
func _update_pose() -> void:
	var hip_p: Vector3 = _mix(P_HIP)
	var spine_a := deg_to_rad(_mix(P_SPINE))
	var drops: float = _mix(P_DROPS)
	var neck: Vector2 = _mix(P_NECK)
	var head_pitch := deg_to_rad(_mix(P_HEAD_PITCH))
	var stop := _weights[Posture.ARRET]
	var stand := _weights[Posture.DANSEUSE]
	var a := _crank_angle
	# > 0 quand la jambe droite appuie : le vélo part à gauche.
	var push_r := sin(a + PI)
	var bike_roll := stop * STOP_TILT + stand * ROCK * push_r
	var bike_xf := Transform3D(Basis(Vector3.BACK, bike_roll), Vector3.ZERO)
	_bike.transform = bike_xf
	_crankset.rotation.x = -a
	var body := Basis(Vector3.BACK, stop * STOP_BODY_TILT + stand * ROCK * BODY_FOLLOW * push_r)
	var right := body.x
	var hip_mid := body * _p(hip_p.z, hip_p.y, hip_p.x + stand * SWAY * push_r)

	# Pieds : sur les pédales, le gauche posé au sol à l'arrêt.
	var ankles: Array[Vector3] = []
	var foot_bases: Array[Basis] = []
	for i in 2:
		var s := -1.0 if i == 0 else 1.0
		var ang := a + (0.0 if i == 0 else PI)
		var spindle := Vector3(s * PEDAL_X, BB_Y + CRANK * cos(ang), BB_Z - CRANK * sin(ang))
		var pitch := ANKLE_BASE + ANKLE_AMP * cos(ang - ANKLE_PHASE) + stand * STAND_TOE
		var pedal_basis := Basis(Vector3.RIGHT, -pitch)
		var fb := bike_xf.basis * pedal_basis
		var ankle := bike_xf * spindle + fb * Vector3(0.0, ANKLE_UP, ANKLE_BACK)
		var unclip := stop if i == 0 else 0.0
		if unclip > 0.0:
			var ground := Vector3(-0.30, GROUND_Y + SOLE_TO_ANKLE, BB_Z - 0.02)
			var lift := sin(PI * unclip)
			ankle = ankle.lerp(ground, unclip) + Vector3(-0.08 * lift, 0.06 * lift, 0.0)
			fb = fb.slerp(Basis(Vector3.UP, 0.25), unclip)
			# Pédale libre : elle bascule nez en l'air.
			pedal_basis = pedal_basis.slerp(Basis(Vector3.RIGHT, 0.35), unclip)
		_pedals[i].transform = Transform3D(pedal_basis, spindle)
		ankles.append(ankle)
		foot_bases.append(fb)

	# Bassin abaissé si une jambe ne peut pas atteindre son pied.
	var leg_max := (THIGH + SHIN) * 0.985
	var drop := 0.0
	for i in 2:
		var s := -1.0 if i == 0 else 1.0
		var d := ankles[i] - (hip_mid + right * s * HIP_HALF)
		var flat2 := d.x * d.x + d.z * d.z
		if d.length() > leg_max and flat2 < leg_max * leg_max:
			drop = maxf(drop, -d.y - sqrt(leg_max * leg_max - flat2))
	hip_mid.y -= drop

	var spine := body * Vector3(0.0, sin(spine_a), -cos(spine_a))
	var back := body * Vector3.UP - spine * spine.dot(body * Vector3.UP)
	var shoulder_mid := hip_mid + spine * SPINE
	var head_up := body * Vector3.UP
	var head_c := shoulder_mid + spine * neck.x + head_up * neck.y
	var head_basis := body * Basis(Vector3.RIGHT, -head_pitch)
	_body.body = body
	_body.hip_mid = hip_mid
	_body.spine = spine
	_body.head = Transform3D(head_basis, head_c)
	_body.bike = bike_xf.basis
	_body.drops = drops

	for i in 2:
		var s := -1.0 if i == 0 else 1.0
		# Jambe.
		var hip := hip_mid + right * s * HIP_HALF
		_body.knees[i] = _ik(hip, ankles[i], THIGH, SHIN, body * Vector3(s * 0.12, 0.25, -1.0))
		_body.ankles[i] = ankles[i]
		_body.feet[i] = foot_bases[i]
		# Bras.
		var shoulder := shoulder_mid + right * s * SHOULDER_HALF - back.normalized() * 0.03
		var hood := bike_xf * _p(BAR_F + 0.10, BAR_H + 0.045, s * BAR_X)
		var low := bike_xf * _p(BAR_F + 0.025, BAR_H - 0.112, s * (BAR_X + 0.005))
		var hand := hood.lerp(low, drops)
		_body.elbows[i] = _ik(shoulder, hand, UPPER_ARM, FOREARM, body * Vector3(s * 0.7, -0.5, 0.6))
		_body.hands[i] = hand
	_body.apply()


## Articulation intermédiaire (genou, coude) d'une chaîne à deux segments,
## pliée du côté de `pole`.
static func _ik(root: Vector3, target: Vector3, l1: float, l2: float, pole: Vector3) -> Vector3:
	var d := target - root
	var dist := clampf(d.length(), 0.01, l1 + l2 - 0.001)
	var dir := d.normalized()
	var cos_a := (l1 * l1 + dist * dist - l2 * l2) / (2.0 * l1 * dist)
	var ang := acos(clampf(cos_a, -1.0, 1.0))
	var p := (pole - dir * pole.dot(dir)).normalized()
	return root + dir * (l1 * cos(ang)) + p * (l1 * sin(ang))


## Point du vélo : avancée et hauteur par rapport au pédalier, écart latéral.
static func _p(f: float, h: float, x := 0.0) -> Vector3:
	return Vector3(x, BB_Y + h, BB_Z - f)


static func _pv(v: Vector2, x := 0.0) -> Vector3:
	return _p(v.x, v.y, x)


## Repère dont l'axe -Z suit `dir`, pour les surfaces de révolution.
static func _axis(origin: Vector3, dir: Vector3, hint := Vector3.RIGHT) -> Transform3D:
	var d := dir.normalized()
	var h := hint
	if absf(d.dot(hint.normalized())) > 0.98:
		h = Vector3.UP if absf(d.y) < 0.9 else Vector3.RIGHT
	return Transform3D(Basis.looking_at(d, h), origin)


## Rangées d'un tube de a vers b, extrémités plates (arêtes vives).
static func _tube_rows(length: float, r0: float, r1: float, oval := 1.0) -> Array[Vector4]:
	return [Vector4(0.0, 0.0, 0.0, 0.0), Vector4(0.0, r0, r0 * oval, 0.0), Vector4(0.0, r0, r0 * oval, 0.0),
		Vector4(length, r1, r1 * oval, 0.0), Vector4(length, r1, r1 * oval, 0.0), Vector4(length, 0.0, 0.0, 0.0)]


static func _tube(mb: Mb, a: Vector3, b: Vector3, r0: float, r1: float, col: Variant, seg := 12, oval := 1.0) -> void:
	mb.revolve(_axis(a, b - a), _tube_rows(a.distance_to(b), r0, r1, oval), seg, col)


func _instance(mesh: Mesh) -> MeshInstance3D:
	var mi := MeshInstance3D.new()
	mi.mesh = mesh
	return mi


static func _material(roughness: float, metallic: float, clearcoat := false) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.vertex_color_use_as_albedo = true
	m.vertex_color_is_srgb = true
	m.roughness = roughness
	m.metallic = metallic
	if clearcoat:
		m.clearcoat_enabled = true
		m.clearcoat = 0.9
		m.clearcoat_roughness = 0.08
	return m


# --- Vélo -------------------------------------------------------------------

func _frame_mesh() -> ArrayMesh:
	var paint := Mb.new(_m_paint)
	var matte := Mb.new(_m_matte)
	var gloss := Mb.new(_m_gloss)
	var metal := Mb.new(_m_metal)
	var head_axis := Vector2(-0.2924, 0.9563)
	var ht_bot := Vector2(0.437, 0.415)
	var ht_top := ht_bot + head_axis * 0.155
	var st_dir := Vector2(-0.284, 0.959)
	var rear := Vector2(REAR_F, AXLE_H)
	var front := Vector2(FRONT_F, AXLE_H)

	# Cadre : tube supérieur incliné, tube diagonal large, haubans fins.
	_tube(paint, _pv(ht_bot - head_axis * 0.01), _pv(ht_top + head_axis * 0.008), 0.032, 0.025, _frame_col, 16)
	_tube(paint, _pv(ht_top - head_axis * 0.03), _pv(st_dir * 0.50), 0.02, 0.016, _frame_col, 12, 1.2)
	var dt_a := Vector2(0.015, 0.02)
	var dt_b := ht_bot + head_axis * 0.035
	var dt_len := (dt_b - dt_a).length()
	_tube(paint, _pv(dt_a), _pv(dt_b), 0.028, 0.024,
		func(t: float, phi: float, _r: float) -> Color:
			# Inscription blanche sur les flancs du tube diagonal.
			var side := absf(sin(phi)) > 0.75
			return WHITE if side and t > dt_len * 0.3 and t < dt_len * 0.75 else _frame_col, 16, 0.85)
	_tube(paint, _pv(Vector2.ZERO), _pv(st_dir * 0.53), 0.019, 0.016, _frame_col, 12, 1.25)
	_tube(paint, _p(0.0, 0.0, -0.043), _p(0.0, 0.0, 0.043), 0.023, 0.023, _frame_col, 14)
	for s: float in [-1.0, 1.0]:
		_tube(paint, _p(-0.01, 0.0, s * 0.035), _pv(rear, s * 0.064), 0.013, 0.008, _frame_col, 8)
		_tube(paint, _pv(st_dir * 0.47, s * 0.017), _pv(rear + Vector2(0.008, 0.012), s * 0.063), 0.0095, 0.007, _frame_col, 8)
		# Fourche, légèrement cintrée vers l'avant.
		var crown := ht_bot - head_axis * 0.03
		var mid := crown.lerp(front, 0.55) + Vector2(0.012, 0.0)
		paint.sweep(PackedVector3Array([_pv(crown, s * 0.034), _pv(mid, s * 0.046), _pv(front + Vector2(0.0, 0.01), s * 0.052)]),
			PackedFloat32Array([0.019, 0.014, 0.009]), 10, _frame_col)
		gloss.box(Transform3D(Basis(), _pv(rear, s * 0.064)), Vector3(0.008, 0.03, 0.03), CARBON)
		gloss.box(Transform3D(Basis(), _pv(front, s * 0.054)), Vector3(0.008, 0.025, 0.025), CARBON)
	paint.revolve(_axis(_pv(ht_bot - head_axis * 0.03, -0.05), Vector3.RIGHT, Vector3.UP),
		[Vector4(0.0, 0.0, 0.0, 0.0), Vector4(0.0, 0.022, 0.03, 0.0), Vector4(0.1, 0.022, 0.03, 0.0), Vector4(0.1, 0.0, 0.0, 0.0)], 10, _frame_col)

	# Jeu de direction, potence, cintre.
	var steer_top := ht_top + head_axis * 0.045
	_tube(gloss, _pv(ht_top), _pv(steer_top), 0.019, 0.018, CARBON, 12)
	var stem_a := ht_top + head_axis * 0.022
	_tube(gloss, _pv(stem_a - Vector2(0.02, 0.0)), _p(BAR_F, BAR_H), 0.019, 0.017, CARBON, 12, 1.15)
	_tube(gloss, _p(BAR_F, BAR_H, -0.16), _p(BAR_F, BAR_H, 0.16), 0.0155, 0.0155, CARBON, 12)
	for s: float in [-1.0, 1.0]:
		var path := PackedVector3Array()
		for k in 7:
			var th := PI / 2.0 * k / 6.0
			path.append(_p(BAR_F + 0.055 * (1.0 - cos(th)), BAR_H, s * (0.15 + 0.055 * sin(th))))
		for k in 13:
			var ps := PI * k / 12.0
			path.append(_p(BAR_F + 0.075 + 0.065 * sin(ps), BAR_H - 0.065 + 0.065 * cos(ps), s * BAR_X))
		path.append(_p(BAR_F - 0.045, BAR_H - 0.13, s * BAR_X))
		matte.sweep(path, PackedFloat32Array([0.0145]), 10, TAPE, true)
		# Cocottes et leviers de frein.
		var hood_xf := _axis(_p(BAR_F + 0.068, BAR_H + 0.006, s * BAR_X), _p(1.0, 0.14) - _p(0.0, 0.0), Vector3.UP)
		matte.revolve(hood_xf, [Vector4(0.0, 0.0, 0.0, 0.0), Vector4(0.004, 0.016, 0.018, 0.0), Vector4(0.025, 0.018, 0.025, 0.008),
			Vector4(0.06, 0.017, 0.027, 0.012), Vector4(0.08, 0.016, 0.026, 0.016), Vector4(0.088, 0.012, 0.016, 0.018),
			Vector4(0.092, 0.0, 0.0, 0.018)], 12, Color(0.08, 0.08, 0.085))
		gloss.sweep(PackedVector3Array([_p(BAR_F + 0.15, BAR_H + 0.02, s * BAR_X), _p(BAR_F + 0.165, BAR_H - 0.04, s * (BAR_X + 0.004)),
			_p(BAR_F + 0.158, BAR_H - 0.095, s * (BAR_X + 0.006)), _p(BAR_F + 0.138, BAR_H - 0.125, s * (BAR_X + 0.006))]),
			PackedFloat32Array([0.009, 0.007, 0.006, 0.005]), 8, CARBON, true)

	# Tige de selle et selle.
	_tube(gloss, _pv(st_dir * 0.49), _pv(st_dir * 0.69), 0.014, 0.014, CARBON, 12, 1.3)
	var saddle_xf := _axis(_p(-0.335, 0.682), Vector3.FORWARD, Vector3.UP)
	matte.revolve(saddle_xf, [Vector4(0.0, 0.0, 0.0, 0.0), Vector4(0.004, 0.05, 0.012, 0.0), Vector4(0.02, 0.068, 0.017, 0.002),
		Vector4(0.06, 0.07, 0.018, 0.002), Vector4(0.11, 0.055, 0.017, 0.0), Vector4(0.16, 0.03, 0.015, 0.0),
		Vector4(0.23, 0.022, 0.014, 0.0), Vector4(0.262, 0.016, 0.011, 0.0), Vector4(0.27, 0.0, 0.0, 0.0)], 16,
		func(t: float, phi: float, _r: float) -> Color:
			return WHITE if absf(cos(phi)) > 0.93 and t < 0.13 else CARBON)
	for s: float in [-1.0, 1.0]:
		metal.sweep(PackedVector3Array([_p(-0.31, 0.672, s * 0.02), _p(-0.25, 0.662, s * 0.02), _p(-0.13, 0.662, s * 0.015), _p(-0.09, 0.672, s * 0.012)]),
			PackedFloat32Array([0.0035]), 6, SILVER)
	gloss.box(Transform3D(Basis(), _pv(st_dir * 0.69 + Vector2(0.0, 0.0))), Vector3(0.05, 0.02, 0.05), CARBON)

	# Transmission : plateaux, cassette, chaîne, dérailleurs.
	var rings := [Vector4(0.0, 0.07, 0.07, 0.0), Vector4(0.0, 0.07, 0.07, 0.0), Vector4(0.0, 0.101, 0.101, 0.0), Vector4(0.0, 0.101, 0.101, 0.0),
		Vector4(0.004, 0.101, 0.101, 0.0), Vector4(0.004, 0.101, 0.101, 0.0), Vector4(0.004, 0.07, 0.07, 0.0), Vector4(0.004, 0.07, 0.07, 0.0)]
	var typed_rings: Array[Vector4] = []
	typed_rings.assign(rings)
	metal.revolve(_axis(_p(0.0, 0.0, 0.041), Vector3.RIGHT, Vector3.UP), typed_rings, 40, Color(0.3, 0.3, 0.32))
	var small: Array[Vector4] = [Vector4(0.0, 0.045, 0.045, 0.0), Vector4(0.0, 0.069, 0.069, 0.0), Vector4(0.0, 0.069, 0.069, 0.0),
		Vector4(0.004, 0.069, 0.069, 0.0), Vector4(0.004, 0.069, 0.069, 0.0), Vector4(0.004, 0.045, 0.045, 0.0)]
	metal.revolve(_axis(_p(0.0, 0.0, 0.034), Vector3.RIGHT, Vector3.UP), small, 32, Color(0.24, 0.24, 0.26))
	var cassette: Array[Vector4] = [Vector4(0.0, 0.0, 0.0, 0.0), Vector4(0.0, 0.055, 0.055, 0.0)]
	for i in 11:
		var x0 := 0.0035 * i
		var r := 0.055 - 0.0029 * i
		cassette.append(Vector4(x0, r, r, 0.0))
		cassette.append(Vector4(x0 + 0.0018, r, r, 0.0))
		cassette.append(Vector4(x0 + 0.0018, r - 0.006, r - 0.006, 0.0))
		cassette.append(Vector4(x0 + 0.0035, r - 0.006, r - 0.006, 0.0))
	cassette.append(Vector4(0.0385, 0.0, 0.0, 0.0))
	metal.revolve(_axis(_pv(rear, 0.027), Vector3.RIGHT, Vector3.UP), cassette, 24, SILVER)
	# Chaîne : brin haut, cassette, chape du dérailleur, brin bas, autour du plateau.
	var cx := 0.043
	var chain := PackedVector3Array()
	var cog_r := 0.041
	for k in 9:
		var b := PI * 1.15 * k / 8.0
		chain.append(_p(rear.x - cog_r * sin(b), rear.y + cog_r * cos(b), cx))
	var jockey_up := rear + Vector2(0.004, -0.06)
	var jockey_lo := rear + Vector2(0.03, -0.12)
	chain.append(_pv(jockey_up + Vector2(-0.012, 0.0), cx))
	chain.append(_pv(jockey_lo + Vector2(-0.012, 0.0), cx))
	chain.append(_pv(jockey_lo + Vector2(0.0, -0.013), cx))
	for k in 13:
		var g := PI * k / 12.0
		chain.append(_p(0.105 * sin(g), -0.105 * cos(g), cx))
	chain.append(chain[0])
	metal.sweep(chain, PackedFloat32Array([0.0045]), 6, Color(0.42, 0.42, 0.44))
	for j: Vector2 in [jockey_up, jockey_lo]:
		var jr: Array[Vector4] = [Vector4(0.0, 0.0, 0.0, 0.0), Vector4(0.0, 0.013, 0.013, 0.0), Vector4(0.0, 0.013, 0.013, 0.0),
			Vector4(0.006, 0.013, 0.013, 0.0), Vector4(0.006, 0.013, 0.013, 0.0), Vector4(0.006, 0.0, 0.0, 0.0)]
		gloss.revolve(_axis(_pv(j, cx - 0.003), Vector3.RIGHT, Vector3.UP), jr, 12, CARBON)
	gloss.sweep(PackedVector3Array([_pv(rear + Vector2(0.0, -0.005), 0.058), _pv(rear + Vector2(-0.025, -0.035), 0.062),
		_pv(jockey_up, 0.052)]), PackedFloat32Array([0.011, 0.012, 0.009]), 8, CARBON, true)
	gloss.box(Transform3D(Basis(Vector3.RIGHT, -0.46), _pv(jockey_up.lerp(jockey_lo, 0.5), 0.05)), Vector3(0.004, 0.024, 0.08), CARBON)
	gloss.box(Transform3D(Basis(Vector3.RIGHT, 0.28), _p(-0.04, 0.125, 0.036)), Vector3(0.012, 0.03, 0.065), CARBON)

	# Freins à disque et bidon.
	gloss.box(Transform3D(Basis(Vector3.RIGHT, 0.3), _pv(front + Vector2(-0.04, 0.045), -0.052)), Vector3(0.022, 0.03, 0.06), CARBON)
	gloss.box(Transform3D(Basis(Vector3.RIGHT, -0.2), _pv(rear + Vector2(0.045, 0.03), -0.055)), Vector3(0.022, 0.03, 0.06), CARBON)
	var dt_dir := (dt_b - dt_a).normalized()
	var dt_up := Vector2(-dt_dir.y, dt_dir.x)
	var bottle_a := dt_a + dt_dir * 0.1 + dt_up * 0.06
	gloss.revolve(_axis(_pv(bottle_a), _pv(bottle_a + dt_dir) - _pv(bottle_a)), [Vector4(0.0, 0.0, 0.0, 0.0),
		Vector4(0.003, 0.033, 0.033, 0.0), Vector4(0.17, 0.035, 0.035, 0.0), Vector4(0.19, 0.03, 0.03, 0.0),
		Vector4(0.2, 0.014, 0.014, 0.0), Vector4(0.2, 0.014, 0.014, 0.0), Vector4(0.225, 0.01, 0.01, 0.0), Vector4(0.23, 0.0, 0.0, 0.0)], 14,
		func(t: float, _phi: float, _r: float) -> Color:
			if t > 0.198:
				return CARBON
			return _jersey if t > 0.06 and t < 0.12 else WHITE)
	return _commit([paint, matte, gloss, metal])


func _wheel_mesh() -> ArrayMesh:
	var rubber := Mb.new(_m_matte)
	var rim := Mb.new(_m_gloss)
	var metal := Mb.new(_m_metal)
	var ax := _axis(Vector3.ZERO, Vector3.RIGHT, Vector3.UP)
	# Pneu de 26 mm : bande de roulement noire, flancs beige.
	var tyre: Array[Vector4] = []
	var tr := 0.0135
	var tc := WHEEL_RADIUS - tr
	for k in 19:
		var ps := deg_to_rad(lerpf(-40.0, 220.0, k / 18.0))
		tyre.append(Vector4(-tr * cos(ps), tc + tr * sin(ps), tc + tr * sin(ps), 0.0))
	rubber.revolve(ax, tyre, 64, func(_t: float, phi: float, r: float) -> Color:
		if r > tc + tr * 0.6:
			return TREAD
		if r > tc - tr * 0.2 and r < tc + tr * 0.3 and fposmod(phi, PI) < 0.35:
			return WHITE
		return GUM)
	# Jante carbone de 50 mm avec inscriptions.
	var prof := [Vector2(0.0115, 0.318), Vector2(0.0125, 0.305), Vector2(0.0138, 0.29), Vector2(0.013, 0.276),
		Vector2(0.0095, 0.266), Vector2(0.0045, 0.2608), Vector2(0.0, 0.2595)]
	var rim_rows: Array[Vector4] = []
	for v: Vector2 in prof:
		rim_rows.append(Vector4(v.x, v.y, v.y, 0.0))
	for i in range(prof.size() - 2, -1, -1):
		rim_rows.append(Vector4(-prof[i].x, prof[i].y, prof[i].y, 0.0))
	rim.revolve(ax, rim_rows, 64, func(t: float, phi: float, r: float) -> Color:
		var decal := absf(t) > 0.009 and r > 0.279 and r < 0.297 and fposmod(phi, PI) > 0.5 and fposmod(phi, PI) < 1.25
		return WHITE if decal else CARBON)
	# Rayons croisés, moyeu, disque de frein.
	var n := 24
	for k in n:
		var s := 1.0 if k % 2 == 0 else -1.0
		var th := TAU * k / n
		var hub_th := th + (0.32 if (k >> 1) % 2 == 0 else -0.32)
		var a := Vector3(s * 0.028, 0.028 * cos(hub_th), 0.028 * sin(hub_th))
		var b := Vector3(s * 0.004, 0.262 * cos(th), 0.262 * sin(th))
		metal.sweep(PackedVector3Array([a, b]), PackedFloat32Array([0.0014]), 4, DARK_METAL)
	var hub := [Vector2(-0.05, 0.0), Vector2(-0.05, 0.009), Vector2(-0.036, 0.012), Vector2(-0.032, 0.017), Vector2(-0.031, 0.031),
		Vector2(-0.025, 0.031), Vector2(-0.023, 0.018), Vector2(0.023, 0.018), Vector2(0.025, 0.031), Vector2(0.031, 0.031),
		Vector2(0.032, 0.017), Vector2(0.036, 0.012), Vector2(0.05, 0.009), Vector2(0.05, 0.0)]
	var hub_rows: Array[Vector4] = []
	for v: Vector2 in hub:
		hub_rows.append(Vector4(v.x, v.y, v.y, 0.0))
	metal.revolve(ax, hub_rows, 16, DARK_METAL)
	var disc: Array[Vector4] = [Vector4(-0.043, 0.02, 0.02, 0.0), Vector4(-0.043, 0.02, 0.02, 0.0), Vector4(-0.043, 0.08, 0.08, 0.0),
		Vector4(-0.043, 0.08, 0.08, 0.0), Vector4(-0.041, 0.08, 0.08, 0.0), Vector4(-0.041, 0.08, 0.08, 0.0),
		Vector4(-0.041, 0.02, 0.02, 0.0), Vector4(-0.041, 0.02, 0.02, 0.0)]
	metal.revolve(ax, disc, 36, func(_t: float, phi: float, r: float) -> Color:
		if r < 0.058:
			return CARBON if fposmod(phi * 6.0 / TAU, 1.0) < 0.5 else Color(0.3, 0.3, 0.32)
		return STEEL if fposmod(phi * 12.0 / TAU, 1.0) < 0.33 else SILVER)
	return _commit([rubber, rim, metal])


func _crank_mesh() -> ArrayMesh:
	var metal := Mb.new(_m_metal)
	var col := Color(0.16, 0.16, 0.18)
	_tube(metal, Vector3(-0.07, 0.0, 0.0), Vector3(0.07, 0.0, 0.0), 0.012, 0.012, SILVER, 10)
	for s: float in [-1.0, 1.0]:
		var end := Vector3(s * 0.08, -s * CRANK, 0.0)
		_tube(metal, Vector3(s * 0.066, 0.0, 0.0), end, 0.02, 0.012, col, 10, 0.55)
		_tube(metal, end, end + Vector3(s * 0.035, 0.0, 0.0), 0.0065, 0.0065, SILVER, 8)
	# Araignée à quatre branches : on voit le pédalier tourner.
	for k in 4:
		var th := TAU * k / 4.0 + PI / 4.0
		_tube(metal, Vector3(0.052, 0.0, 0.0), Vector3(0.047, 0.056 * cos(th), 0.056 * sin(th)), 0.011, 0.008, col, 8)
	return _commit([metal])


func _pedal_mesh() -> ArrayMesh:
	var gloss := Mb.new(_m_gloss)
	var metal := Mb.new(_m_metal)
	gloss.revolve(_axis(Vector3(0.0, 0.0, 0.045), Vector3.FORWARD, Vector3.UP), [Vector4(0.0, 0.0, 0.0, 0.0),
		Vector4(0.006, 0.026, 0.008, 0.0), Vector4(0.035, 0.036, 0.01, 0.0), Vector4(0.075, 0.03, 0.009, 0.0),
		Vector4(0.088, 0.016, 0.007, 0.0), Vector4(0.09, 0.0, 0.0, 0.0)], 12, CARBON)
	metal.box(Transform3D(Basis(), Vector3(0.0, 0.009, 0.005)), Vector3(0.05, 0.003, 0.05), SILVER)
	return _commit([gloss, metal])


static func _commit(parts: Array[Mb]) -> ArrayMesh:
	var mesh := ArrayMesh.new()
	for mb in parts:
		if mb.verts.is_empty():
			continue
		var arr := []
		arr.resize(Mesh.ARRAY_MAX)
		arr[Mesh.ARRAY_VERTEX] = mb.verts
		arr[Mesh.ARRAY_NORMAL] = mb.norms
		arr[Mesh.ARRAY_COLOR] = mb.cols
		mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arr)
		mesh.surface_set_material(mesh.get_surface_count() - 1, mb.mat)
	return mesh


## Accumulateur de triangles d'une surface, avec couleur par sommet.
class Mb:
	var mat: Material
	var verts := PackedVector3Array()
	var norms := PackedVector3Array()
	var cols := PackedColorArray()

	func _init(m: Material) -> void:
		mat = m

	## Grille de sommets (rangées × anneaux), une couleur par facette.
	## Faces avant dans le sens horaire vu de l'extérieur, comme le veut Godot.
	func grid(pts: Array[PackedVector3Array], nrm: Array[PackedVector3Array], quad_cols: PackedColorArray) -> void:
		var seg := pts[0].size() - 1
		for i in pts.size() - 1:
			var same := true
			for j in seg + 1:
				if not pts[i][j].is_equal_approx(pts[i + 1][j]):
					same = false
					break
			if same:
				continue
			for j in seg:
				var col := quad_cols[i * seg + j]
				for v: Vector2i in [Vector2i(i, j), Vector2i(i, j + 1), Vector2i(i + 1, j + 1),
						Vector2i(i, j), Vector2i(i + 1, j + 1), Vector2i(i + 1, j)]:
					verts.append(pts[v.x][v.y])
					norms.append(nrm[v.x][v.y])
					cols.append(col)

	## Surface de révolution autour de -Z local, placée par xf. Rangées :
	## (t le long de l'axe, demi-axe x, demi-axe y, décalage y) ; deux rangées
	## identiques font une arête vive. col : Color ou Callable(t, phi, r) -> Color.
	func revolve(xf: Transform3D, rows: Array[Vector4], seg: int, col: Variant, phi0 := 0.0, phi1 := TAU) -> void:
		var n := rows.size()
		var full := is_equal_approx(phi1 - phi0, TAU)
		var pts: Array[PackedVector3Array] = []
		for r: Vector4 in rows:
			var ring := PackedVector3Array()
			for j in seg + 1:
				var phi := lerpf(phi0, phi1, float(j) / seg)
				ring.append(Vector3(cos(phi) * r.y, sin(phi) * r.z + r.w, -r.x))
			pts.append(ring)
		var nrm: Array[PackedVector3Array] = []
		for i in n:
			var i0 := maxi(i - 1, 0)
			var i1 := mini(i + 1, n - 1)
			if i + 1 < n and rows[i + 1] == rows[i]:
				i1 = i
			if i > 0 and rows[i - 1] == rows[i]:
				i0 = i
			if i0 == i1:
				i0 = maxi(i - 1, 0)
			var ring_n := PackedVector3Array()
			for j in seg + 1:
				var jm := maxi(j - 1, 0)
				var jp := mini(j + 1, seg)
				if full:
					jm = (j - 1 + seg) % seg
					jp = (j + 1) % seg
				var v := (pts[i1][j] - pts[i0][j]).cross(pts[i][jp] - pts[i][jm])
				if v.length_squared() < 1e-14:
					var other := i1 if i1 != i else i0
					v = Vector3(0.0, 0.0, rows[other].x - rows[i].x)
				ring_n.append((xf.basis * v).normalized())
			nrm.append(ring_n)
		for ring in pts:
			for j in ring.size():
				ring[j] = xf * ring[j]
		var qc := PackedColorArray()
		qc.resize((n - 1) * seg)
		for i in n - 1:
			for j in seg:
				var c: Color
				if col is Color:
					c = col
				else:
					var t := (rows[i].x + rows[i + 1].x) * 0.5
					var phi := lerpf(phi0, phi1, (j + 0.5) / seg)
					c = (col as Callable).call(t, phi, (rows[i].z + rows[i + 1].z) * 0.5)
				qc[i * seg + j] = c
		grid(pts, nrm, qc)

	## Tube le long d'un chemin (repères transportés), rayon constant ou par point,
	## bouts fermés si `caps`.
	func sweep(path: PackedVector3Array, radii: PackedFloat32Array, seg: int, col: Color, caps := false) -> void:
		var n := path.size()
		var pts: Array[PackedVector3Array] = []
		var nrm: Array[PackedVector3Array] = []
		var t0 := (path[1] - path[0]).normalized()
		var normal := t0.cross(Vector3.UP if absf(t0.y) < 0.9 else Vector3.RIGHT).normalized()
		var first := PackedVector3Array()
		var last := PackedVector3Array()
		for i in n:
			var t := (path[mini(i + 1, n - 1)] - path[maxi(i - 1, 0)]).normalized()
			normal = (normal - t * normal.dot(t)).normalized()
			var bin := normal.cross(t)
			var r := radii[mini(i, radii.size() - 1)]
			var ring := PackedVector3Array()
			var rn := PackedVector3Array()
			for j in seg + 1:
				var phi := TAU * j / seg
				var d := normal * cos(phi) + bin * sin(phi)
				ring.append(path[i] + d * r)
				rn.append(d)
			pts.append(ring)
			nrm.append(rn)
			if i == 0:
				first = ring
			last = ring
		if caps:
			var t_a := (path[1] - path[0]).normalized()
			var t_b := (path[n - 1] - path[n - 2]).normalized()
			var c_a := PackedVector3Array()
			var c_b := PackedVector3Array()
			var n_a := PackedVector3Array()
			var n_b := PackedVector3Array()
			for j in seg + 1:
				c_a.append(path[0])
				c_b.append(path[n - 1])
				n_a.append(-t_a)
				n_b.append(t_b)
			pts.insert(0, first.duplicate())
			nrm.insert(0, n_a.duplicate())
			pts.insert(0, c_a)
			nrm.insert(0, n_a)
			pts.append(last.duplicate())
			nrm.append(n_b.duplicate())
			pts.append(c_b)
			nrm.append(n_b)
		var qc := PackedColorArray()
		qc.resize((pts.size() - 1) * seg)
		qc.fill(col)
		grid(pts, nrm, qc)

	## Pavé de taille `size` centré sur xf.
	func box(xf: Transform3D, size: Vector3, col: Color) -> void:
		var h := size * 0.5
		for nv: Vector3 in [Vector3.RIGHT, Vector3.LEFT, Vector3.UP, Vector3.DOWN, Vector3.BACK, Vector3.FORWARD]:
			var u := Vector3(nv.y, nv.z, nv.x)
			var v := nv.cross(u)
			var c := nv * h
			var hu := u * h
			var hv := v * h
			var n := (xf.basis * nv).normalized()
			var pts: Array[PackedVector3Array] = [
				PackedVector3Array([xf * (c - hu - hv), xf * (c - hu + hv)]),
				PackedVector3Array([xf * (c + hu - hv), xf * (c + hu + hv)])]
			var nrm: Array[PackedVector3Array] = [PackedVector3Array([n, n]), PackedVector3Array([n, n])]
			grid(pts, nrm, PackedColorArray([col]))
