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
var _power_samples: Array = []
var _elapsed := 0.0


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
	var left := _panel(root)
	left.position = Vector2(16, 14)
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

	var help := _label("↑ ↓ : vitesses   ← → : effort   A : Click auto   C : caméra   Espace : pause   Échap : menu", 14, Color(1, 1, 1, 0.7))
	help.set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
	help.offset_left = 20
	help.offset_top = -178
	help.offset_bottom = -156
	root.add_child(help)

	var tag := _label("MODE TEST", 13, Color(0.98, 0.62, 0.20))
	tag.set_anchors_preset(Control.PRESET_BOTTOM_RIGHT)
	tag.offset_left = -120
	tag.offset_right = -20
	tag.offset_top = -178
	tag.offset_bottom = -156
	root.add_child(tag)

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


func show_banner(text: String) -> void:
	_banner.text = text


func update(t: Dictionary, delta: float) -> void:
	if t.is_empty():
		return
	_elapsed += delta
	_power_samples.append([_elapsed, t.power_w])
	while _power_samples.size() > 1 and _elapsed - _power_samples[0][0] > 3.0:
		_power_samples.pop_front()
	var avg := 0.0
	for s in _power_samples:
		avg += s[1]
	avg /= maxf(_power_samples.size(), 1)

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
