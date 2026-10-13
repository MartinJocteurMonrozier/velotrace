extends Node3D
## Cycliste et vélo de route, pédalage synchronisé sur la cadence et posture
## choisie selon la télémétrie : pied à terre, assis sur les cocottes, aéro
## dans le bas du cintre, en danseuse. L'avant du vélo pointe vers -Z, la
## droite du cycliste vers +X. Le vélo lui-même (maillages, roues, pédalier)
## et ses points de contact viennent de bike.gd ; le corps (rider_body.gd)
## suit les cibles articulaires calculées ici.

const Bike := preload("res://scripts/bike.gd")
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

## Géométrie du vélo (voir bike.gd).
const WHEEL_RADIUS := Bike.WHEEL_RADIUS
const BB_Y := Bike.BB_Y
const BB_Z := Bike.BB_Z
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

var _posture := Posture.ASSIS
var _posture_time := 0.0
var _weights := PackedFloat32Array([0.0, 1.0, 0.0, 0.0])
var _first := true
var _avg_power := 0.0
var _power := 0.0

var _crank_angle := 0.0
var _wheel_angle := 0.0

var _bike: Bike
var _body: RiderBody


func _ready() -> void:
	var k: Array = KITS[posmod(kit, KITS.size())]
	_jersey = k[0]
	_jersey_dark = k[1]
	_accent = k[2]
	_frame_col = k[3]
	# Cadre à la couleur de l'équipe, bidons à celle du maillot.
	_bike = Bike.new()
	_bike.frame_color = _frame_col
	_bike.accent_color = _accent
	_bike.bottle_color = _jersey
	add_child(_bike)
	_body = RiderBody.new()
	_body.setup(_jersey, _jersey_dark, _accent)
	add_child(_body)
	_update_pose()


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
	_bike.set_wheel(_wheel_angle)
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
	_bike.set_crank(a)
	var body := Basis(Vector3.BACK, stop * STOP_BODY_TILT + stand * ROCK * BODY_FOLLOW * push_r)
	var right := body.x
	var hip_mid := body * Bike.P(hip_p.z, hip_p.y, hip_p.x + stand * SWAY * push_r)

	# Pieds : sur les pédales, le gauche posé au sol à l'arrêt.
	var ankles: Array[Vector3] = []
	var foot_bases: Array[Basis] = []
	for i in 2:
		var ang := a + (0.0 if i == 0 else PI)
		var spindle := Bike.pedal_position(i, a)
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
		_bike.set_pedal(i, pedal_basis)
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
		var hood := bike_xf * Bike.hood_point(i)
		var low := bike_xf * Bike.drop_point(i)
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

