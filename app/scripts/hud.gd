extends CanvasLayer
## Overlay des données de la sortie, dans l'esprit de Zwift et MyWhoosh.

const ProfileView := preload("res://scripts/profile_view.gd")
const MiniMap := preload("res://scripts/minimap.gd")

var _values := {}
var _profile: Control
var _minimap: Control
var _grade: Label
var _grade_ahead: Label
var _gear: Label
var _gear_bar: GearBar
var _route: Label
var _progress: Label
var _clock: Label
var _banner: Label
var _credits: Label
var _fps: Label
var _fps_timer := 0.0
var _power_samples: Array = []
var _elapsed := 0.0
var _left: VBoxContainer
var _tag: Label
var _workout_panel: PanelContainer
var _workout_bar: WorkoutBar
var _workout := {}
var _board_panel: PanelContainer
var _board: VBoxContainer
var _rider_kg := 70.0
var _avg := 0.0

## Séance : une barre par étape (hauteur = part de FTP), avancement en blanc.
class WorkoutBar extends Control:
	## Couleurs des zones de puissance (fraction de FTP) : Z1 à Z6.
	const ZONE_LIMITS := [0.55, 0.75, 0.90, 1.05, 1.20]
	const ZONE_COLORS := [Color(0.62, 0.64, 0.68), Color(0.25, 0.55, 0.95), Color(0.30, 0.78, 0.40), Color(0.98, 0.82, 0.20), Color(0.98, 0.55, 0.18), Color(0.92, 0.22, 0.20)]

	static func zone_color(fraction: float) -> Color:
		for i in ZONE_LIMITS.size():
			if fraction < ZONE_LIMITS[i]:
				return ZONE_COLORS[i]
		return ZONE_COLORS[-1]

	var steps: Array = []
	var total := 1.0
	var elapsed := 0.0

	func _draw() -> void:
		var x := 0.0
		var top := 1.6
		for st: Dictionary in steps:
			var w: float = size.x * float(st.duration_s) / total
			var a: float = 0.5 if st.free else float(st.from)
			var b: float = 0.5 if st.free else float(st.to)
			var n := maxi(1, int(w / 3.0))
			for i in n:
				var f := lerpf(a, b, (i + 0.5) / n)
				var h := size.y * clampf(f / top, 0.05, 1.0)
				var col: Color = Color(1, 1, 1, 0.3) if st.free else zone_color(f)
				draw_rect(Rect2(x + w * i / n, size.y - h, w / n + 0.5, h), col)
			x += w
		var px := size.x * clampf(elapsed / total, 0.0, 1.0)
		draw_rect(Rect2(0, 0, px, size.y), Color(0, 0, 0, 0.35))
		draw_line(Vector2(px, 0), Vector2(px, size.y), Color.WHITE, 2.0)


class GearBar extends Control:
	var count := 1
	var index := 0

	func _draw() -> void:
		var w := size.x / count
		for i in count:
			var col := Color(1, 1, 1, 0.18)
			if i == index:
				col = Color(0.98, 0.62, 0.20)
			elif i < index:
				col = Color(1, 1, 1, 0.45)
			var h := size.y * (0.35 + 0.65 * float(i + 1) / count)
			draw_rect(Rect2(i * w + 1.0, size.y - h, w - 2.0, h), col)


func _ready() -> void:
	var root := Control.new()
	root.set_anchors_preset(Control.PRESET_FULL_RECT)
	root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(root)

	# Bandeau principal en haut au centre.
	var top := _panel(root)
	top.set_anchors_preset(Control.PRESET_CENTER_TOP)
	top.offset_left = -330
	top.offset_right = 330
	top.offset_top = 14
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 34)
	row.alignment = BoxContainer.ALIGNMENT_CENTER
	top.add_child(row)
	for spec in [["power", "PUISSANCE", "W"], ["cadence", "CADENCE", "rpm"], ["speed", "VITESSE", "km/h"], ["hr", "FC", "bpm"]]:
		row.add_child(_metric(spec[0], spec[1], spec[2]))

	# Parcours, distance et temps en haut à gauche.
	_left = VBoxContainer.new()
	_left.position = Vector2(16, 14)
	_left.add_theme_constant_override("separation", 10)
	_left.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(_left)
	var left := _panel(_left)
	var lv := VBoxContainer.new()
	left.add_child(lv)
	_route = _label("", 15, Color(1, 1, 1, 0.75))
	_progress = _label("", 24)
	_clock = _label("", 20)
	_values["wkg"] = _label("", 15, Color(1, 1, 1, 0.75))
	for l in [_route, _progress, _clock, _values["wkg"]]:
		lv.add_child(l)

	# Minicarte, pente et rapport à droite.
	var right := VBoxContainer.new()
	right.set_anchors_preset(Control.PRESET_TOP_RIGHT)
	right.offset_left = -266
	right.offset_right = -16
	right.offset_top = 14
	right.add_theme_constant_override("separation", 10)
	root.add_child(right)
	_minimap = MiniMap.new()
	_minimap.custom_minimum_size = Vector2(250, 200)
	right.add_child(_minimap)
	var gp := _panel(right)
	var gv := VBoxContainer.new()
	gp.add_child(gv)
	gv.add_child(_label("PENTE", 13, Color(1, 1, 1, 0.6)))
	_grade = _label("0,0 %", 44)
	gv.add_child(_grade)
	_grade_ahead = _label("", 14, Color(1, 1, 1, 0.75))
	gv.add_child(_grade_ahead)
	gv.add_child(HSeparator.new())
	gv.add_child(_label("RAPPORT", 13, Color(1, 1, 1, 0.6)))
	_gear = _label("", 34)
	gv.add_child(_gear)
	_gear_bar = GearBar.new()
	_gear_bar.custom_minimum_size = Vector2(230, 34)
	gv.add_child(_gear_bar)

	# Profil d'altitude en bas.
	var bottom := _panel(root)
	bottom.set_anchors_preset(Control.PRESET_BOTTOM_WIDE)
	bottom.offset_left = 16
	bottom.offset_right = -16
	bottom.offset_top = -150
	bottom.offset_bottom = -14
	_profile = ProfileView.new()
	bottom.add_child(_profile)

	var help := _label("↑ ↓ : vitesses   ← → : effort (mode test)   A : Click auto   C : caméra   Espace ou Échap : pause", 14, Color(1, 1, 1, 0.7))
	help.set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
	help.offset_left = 20
	help.offset_top = -178
	help.offset_bottom = -156
	root.add_child(help)

	_tag = _label("MODE TEST", 13, Color(0.98, 0.62, 0.20))
	_tag.set_anchors_preset(Control.PRESET_BOTTOM_RIGHT)
	_tag.offset_left = -220
	_tag.offset_right = -20
	_tag.offset_top = -178
	_tag.offset_bottom = -156
	_tag.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	root.add_child(_tag)

	_fps = _label("", 13, Color(1, 1, 1, 0.6))
	_fps.set_anchors_preset(Control.PRESET_BOTTOM_RIGHT)
	_fps.offset_left = -360
	_fps.offset_right = -230
	_fps.offset_top = -178
	_fps.offset_bottom = -156
	_fps.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	root.add_child(_fps)

	# Sources du décor, obligatoires pour les données IGN et OSM.
	_credits = _label("", 11, Color(1, 1, 1, 0.55))
	_credits.set_anchors_preset(Control.PRESET_BOTTOM_WIDE)
	_credits.offset_left = 20
	_credits.offset_right = -20
	_credits.offset_top = -14
	_credits.offset_bottom = 0
	_credits.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	root.add_child(_credits)

	_banner = _label("", 54)
	_banner.set_anchors_preset(Control.PRESET_CENTER)
	_banner.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_banner.offset_left = -500
	_banner.offset_right = 500
	_banner.offset_top = -80
	_banner.add_theme_constant_override("outline_size", 10)
	_banner.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.6))
	root.add_child(_banner)


func setup(route_name: String, distances: PackedFloat32Array, points: PackedVector3Array, base_elev: float) -> void:
	_route.text = route_name
	_profile.elevation_offset = base_elev
	_profile.setup(distances, points)
	_minimap.setup(points)


## Panneaux propres au mode : séance (consigne, étape, barres) ou classement.
func setup_mode(cfg: Dictionary) -> void:
	var mode: String = cfg.get("mode", "test")
	_tag.text = {"libre": "SORTIE LIBRE", "seance": "SÉANCE", "adversaires": "COURSE", "test": "MODE TEST"}.get(mode, "")
	if mode == "seance" and cfg.has("workout"):
		_workout = cfg.workout
		_workout_panel = _panel(_left)
		_workout_panel.custom_minimum_size = Vector2(330, 0)
		var v := VBoxContainer.new()
		v.add_theme_constant_override("separation", 2)
		_workout_panel.add_child(v)
		_values["w_name"] = _label(String(_workout.name), 15, Color(1, 1, 1, 0.75))
		v.add_child(_values["w_name"])
		var line := HBoxContainer.new()
		line.add_theme_constant_override("separation", 14)
		v.add_child(line)
		var tv := VBoxContainer.new()
		tv.add_child(_label("CONSIGNE", 13, Color(1, 1, 1, 0.6)))
		_values["w_target"] = _label("--", 40)
		tv.add_child(_values["w_target"])
		line.add_child(tv)
		var rv := VBoxContainer.new()
		rv.add_child(_label("ÉTAPE", 13, Color(1, 1, 1, 0.6)))
		_values["w_left"] = _label("--", 40)
		rv.add_child(_values["w_left"])
		line.add_child(rv)
		_values["w_info"] = _label("", 14, Color(1, 1, 1, 0.8))
		v.add_child(_values["w_info"])
		_values["w_next"] = _label("", 14, Color(1, 1, 1, 0.65))
		v.add_child(_values["w_next"])
		_workout_bar = WorkoutBar.new()
		_workout_bar.custom_minimum_size = Vector2(300, 46)
		_workout_bar.steps = _workout.steps
		_workout_bar.total = maxf(1.0, float(_workout.duration_s))
		v.add_child(_workout_bar)
	elif mode == "adversaires":
		_board_panel = _panel(_left)
		_board_panel.custom_minimum_size = Vector2(300, 0)
		_board = VBoxContainer.new()
		_board.add_theme_constant_override("separation", 1)
		_board_panel.add_child(_board)


## Classement : le joueur parmi les coureurs virtuels, écart en mètres.
func update_opponents(list: Array) -> void:
	if _board == null:
		return
	var rows: Array = [{"name": "Toi", "gap_m": 0.0, "w_per_kg": _avg / _rider_kg, "me": true}]
	for o: Dictionary in list:
		rows.append({"name": o.name, "gap_m": o.gap_m, "w_per_kg": o.w_per_kg, "me": false, "finished": o.get("finished", false)})
	rows.sort_custom(func(a, b): return a.gap_m > b.gap_m)
	var labels := _board.get_children()
	while labels.size() < rows.size() + 1:
		var l := _label("", 15)
		_board.add_child(l)
		labels.append(l)
	labels[0].text = "CLASSEMENT"
	labels[0].add_theme_font_size_override("font_size", 13)
	labels[0].add_theme_color_override("font_color", Color(1, 1, 1, 0.6))
	for i in rows.size():
		var r: Dictionary = rows[i]
		var gap := "" if r.me else ("%+d m" % roundi(r.gap_m)).replace("+", "+ ").replace("-", "− ")
		var l: Label = labels[i + 1]
		l.text = "%d. %-10s %7s   %s W/kg" % [i + 1, r.name, gap, _fr("%.1f" % r.w_per_kg)]
		l.add_theme_color_override("font_color", Color(0.98, 0.62, 0.20) if r.me else Color.WHITE)


func _update_workout(w: Dictionary, power: float) -> void:
	if _workout_bar == null or w.is_empty():
		return
	var target: float = w.get("target_w", -1.0)
	if target > 0.0:
		_values["w_target"].text = "%d W" % roundi(target)
		var ratio := power / target
		var col := Color(0.40, 0.85, 0.45) if absf(ratio - 1.0) < 0.06 else (Color(0.98, 0.62, 0.20) if ratio < 1.0 else Color(0.95, 0.30, 0.25))
		_values["power"].add_theme_color_override("font_color", col)
	else:
		_values["w_target"].text = "Libre"
		_values["power"].remove_theme_color_override("font_color")
	var left := int(ceil(float(w.step_remaining_s)))
	_values["w_left"].text = "%d:%02d" % [left / 60, left % 60]
	var info := "Étape %d / %d" % [int(w.step_index) + 1, int(w.step_count)]
	if w.get("cadence_rpm", 0.0) > 0.0:
		info += "   ·   %d rpm" % roundi(w.cadence_rpm)
	info += "   ·   ERG" if w.get("erg", false) else "   ·   ERG coupé"
	_values["w_info"].text = info
	var nxt: float = w.get("next_target_w", -1.0)
	_values["w_next"].text = ("Ensuite : %d W" % roundi(nxt)) if nxt > 0.0 else ("Dernière étape" if int(w.step_index) + 1 >= int(w.step_count) else "Ensuite : libre")
	_workout_bar.elapsed = w.total_elapsed_s
	_workout_bar.queue_redraw()


func set_credits(text: String) -> void:
	_credits.text = text


func show_banner(text: String) -> void:
	_banner.text = text


func update(t: Dictionary, delta: float) -> void:
	if t.is_empty():
		return
	_elapsed += delta
	_fps_timer -= delta
	if _fps_timer <= 0.0:
		_fps_timer = 0.5
		_fps.text = "%d i/s" % Engine.get_frames_per_second()
	_power_samples.append([_elapsed, t.power_w])
	while _power_samples.size() > 1 and _elapsed - _power_samples[0][0] > 3.0:
		_power_samples.pop_front()
	var avg := 0.0
	for s in _power_samples:
		avg += s[1]
	avg /= maxf(_power_samples.size(), 1)
	_avg = avg
	_rider_kg = maxf(1.0, t.rider_kg)
	_update_workout(t.get("workout", {}), avg)

	_values["power"].text = "%d" % roundi(avg)
	_values["cadence"].text = "%d" % roundi(t.cadence_rpm)
	_values["speed"].text = _fr("%.1f" % t.speed_kmh)
	_values["hr"].text = "%d" % t.heart_rate if t.heart_rate > 0 else "--"
	_values["wkg"].text = _fr("%.1f W/kg" % (avg / t.rider_kg))
	_progress.text = _fr("%.2f / %.2f km" % [t.distance_m / 1000.0, (t.distance_m + t.remaining_m) / 1000.0])
	var s := int(t.elapsed_s)
	_clock.text = "%02d:%02d:%02d" % [s / 3600, (s / 60) % 60, s % 60]
	_grade.text = _fr("%+.1f %%" % t.grade_pct)
	_grade.add_theme_color_override("font_color", ProfileView.grade_color(t.grade_pct))
	_grade_ahead.text = _fr("dans 200 m : %+.1f %%" % t.grade_ahead_pct)
	_gear.text = t.gear
	_gear_bar.count = t.gear_count
	_gear_bar.index = t.gear_index
	_gear_bar.queue_redraw()
	_profile.position_m = t.distance_m
	_profile.queue_redraw()
	_minimap.position_xz = Vector2(t.position.x, t.position.z)
	_minimap.queue_redraw()


static func _fr(text: String) -> String:
	return text.replace(".", ",")


func _metric(key: String, title: String, unit: String) -> Control:
	var box := VBoxContainer.new()
	box.alignment = BoxContainer.ALIGNMENT_CENTER
	box.add_child(_label(title, 13, Color(1, 1, 1, 0.6)))
	var line := HBoxContainer.new()
	var value := _label("--", 44)
	line.add_child(value)
	var u := _label(unit, 16, Color(1, 1, 1, 0.7))
	u.size_flags_vertical = Control.SIZE_SHRINK_END
	line.add_child(u)
	box.add_child(line)
	_values[key] = value
	return box


func _panel(parent: Control) -> PanelContainer:
	var p := PanelContainer.new()
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.04, 0.06, 0.10, 0.62)
	sb.set_corner_radius_all(10)
	sb.content_margin_left = 16
	sb.content_margin_right = 16
	sb.content_margin_top = 8
	sb.content_margin_bottom = 8
	p.add_theme_stylebox_override("panel", sb)
	p.mouse_filter = Control.MOUSE_FILTER_IGNORE
	parent.add_child(p)
	return p


func _label(text: String, font_size: int, color := Color.WHITE) -> Label:
	var l := Label.new()
	l.text = text
	l.add_theme_font_size_override("font_size", font_size)
	l.add_theme_color_override("font_color", color)
	l.add_theme_constant_override("outline_size", 4)
	l.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.55))
	return l
