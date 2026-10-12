extends Control
## Aperçu d'une séance : une barre par étape, largeur selon la durée, hauteur
## selon le % de FTP, couleur selon la zone. Les rampes sont en biais.

const UI := preload("res://scripts/ui/theme.gd")
const Fmt := preload("res://scripts/ui/fmt.gd")

var workout: Dictionary = {}
## FTP utilisée pour afficher les watts dans l'infobulle.
var ftp_w := 200.0
## Coefficient appliqué à toutes les étapes (1.0 = 100 %).
var intensity := 1.0
## Étape mise en évidence (-1 : aucune).
var highlight := -1
var show_axes := true

var _hover := -1
var _rects: Array = []
var _plot := Rect2()


func _init() -> void:
	custom_minimum_size = Vector2(200, 110)
	mouse_filter = Control.MOUSE_FILTER_PASS
	resized.connect(queue_redraw)
	mouse_exited.connect(func():
		_hover = -1
		queue_redraw())


func set_workout(w: Dictionary) -> void:
	workout = w
	queue_redraw()


static func total_duration(w: Dictionary) -> float:
	var t := 0.0
	for s in w.get("steps", []):
		t += float(s.get("duration_s", 0.0))
	return t


func _draw() -> void:
	var font := UI.font(400)
	var steps: Array = workout.get("steps", [])
	var total := total_duration(workout)
	if steps.is_empty() or total <= 0.0:
		draw_string(font, Vector2(0, size.y * 0.5), "Aucune étape", HORIZONTAL_ALIGNMENT_CENTER, size.x, 15, UI.TEXT_MUTED)
		return
	var top := 0.0
	for s in steps:
		top = maxf(top, maxf(float(s.get("from", 0.5)), float(s.get("to", 0.5))) * intensity)
	var ymax := maxf(1.25, top + 0.1)
	var left := 44.0 if show_axes else 0.0
	var bottom := 20.0 if show_axes else 0.0
	_plot = Rect2(left, 6.0, size.x - left, size.y - 6.0 - bottom)

	if show_axes:
		for g in [0.5, 1.0]:
			if g > ymax:
				continue
			var gy := _plot.end.y - float(g) / ymax * _plot.size.y
			var col := Color(1, 1, 1, 0.16) if g == 1.0 else Color(1, 1, 1, 0.07)
			draw_dashed_line(Vector2(_plot.position.x, gy), Vector2(_plot.end.x, gy), col, 1.0, 5.0)
			draw_string(font, Vector2(0, gy + 4.0), "FTP" if g == 1.0 else "50 %", HORIZONTAL_ALIGNMENT_RIGHT, left - 8.0, 12, UI.TEXT_MUTED)

	_rects.clear()
	var x := 0.0
	for i in steps.size():
		var s: Dictionary = steps[i]
		var d := float(s.get("duration_s", 0.0))
		var f := float(s.get("from", 0.5)) * intensity
		var t := float(s.get("to", s.get("from", 0.5))) * intensity
		var free: bool = s.get("free", false)
		if free:
			f = 0.45
			t = 0.45
		var x0 := _plot.position.x + x / total * _plot.size.x
		var x1 := _plot.position.x + (x + d) / total * _plot.size.x
		x += d
		var y0 := _plot.end.y - f / ymax * _plot.size.y
		var y1 := _plot.end.y - t / ymax * _plot.size.y
		var c0 := Color("5c6878") if free else UI.zone_color(f)
		var c1 := Color("5c6878") if free else UI.zone_color(t)
		var dim := 1.0
		if (_hover >= 0 and i != _hover) or (highlight >= 0 and i != highlight and _hover < 0):
			dim = 0.55
		var gap := 1.0 if x1 - x0 > 3.0 else 0.0
		var pts := PackedVector2Array([Vector2(x0 + gap * 0.5, _plot.end.y), Vector2(x0 + gap * 0.5, y0), Vector2(x1 - gap * 0.5, y1), Vector2(x1 - gap * 0.5, _plot.end.y)])
		var cols := PackedColorArray([Color(c0, 0.55 * dim), Color(c0, 0.95 * dim), Color(c1, 0.95 * dim), Color(c1, 0.55 * dim)])
		draw_polygon(pts, cols)
		if i == highlight:
			draw_polyline(PackedVector2Array([pts[0], pts[1], pts[2], pts[3]]), Color.WHITE, 2.0, true)
		_rects.append(Rect2(x0, _plot.position.y, x1 - x0, _plot.size.y))

	if show_axes:
		var step_min := 5
		for m in [5, 10, 15, 30, 60]:
			step_min = m
			if total / (m * 60.0) <= maxf(2.0, _plot.size.x / 80.0):
				break
		var tm := 0.0
		while tm <= total + 1.0:
			var tx := _plot.position.x + tm / total * _plot.size.x
			var txt := Fmt.mss(tm) if tm < 3600.0 else Fmt.hms(tm)
			var w := font.get_string_size(txt, HORIZONTAL_ALIGNMENT_LEFT, -1, 12).x
			draw_string(font, Vector2(clampf(tx - w * 0.5, _plot.position.x, size.x - w), size.y - 3.0), txt, HORIZONTAL_ALIGNMENT_LEFT, -1, 12, UI.TEXT_MUTED)
			tm += step_min * 60.0

	if _hover >= 0 and _hover < steps.size():
		_draw_tip(font, steps[_hover], _hover)


func _draw_tip(font: Font, s: Dictionary, i: int) -> void:
	var f := float(s.get("from", 0.5)) * intensity
	var t := float(s.get("to", 0.5)) * intensity
	var power := ""
	if s.get("free", false):
		power = "libre"
	elif is_equal_approx(f, t):
		power = "%s (%s)" % [Fmt.percent(f), Fmt.watts(f * ftp_w)]
	else:
		power = "%s → %s" % [Fmt.percent(f), Fmt.percent(t)]
	var text := "Étape %d · %s · %s" % [i + 1, Fmt.duration(s.get("duration_s", 0.0)), power]
	if float(s.get("cadence_rpm", 0.0)) > 0.0:
		text += " · %d rpm" % roundi(s.cadence_rpm)
	var label: String = s.get("label", "")
	if label != "":
		text = label + " · " + text
	var w := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, 13).x + 20.0
	var r: Rect2 = _rects[i]
	var bx := clampf(r.get_center().x - w * 0.5, 0.0, size.x - w)
	var box := Rect2(bx, 2.0, w, 26.0)
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color("1d2c40")
	sb.set_corner_radius_all(6)
	sb.border_color = UI.BORDER_HI
	sb.set_border_width_all(1)
	draw_style_box(sb, box)
	draw_string(font, Vector2(bx + 10.0, 20.0), text, HORIZONTAL_ALIGNMENT_LEFT, -1, 13, UI.TEXT)


func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion:
		var h := -1
		for i in _rects.size():
			var r: Rect2 = _rects[i]
			if event.position.x >= r.position.x and event.position.x < r.end.x:
				h = i
		if h != _hover:
			_hover = h
			queue_redraw()


## Simule le survol d'une étape (captures d'écran).
func hover_at(px: float) -> void:
	_gui_input_motion(px)


func _gui_input_motion(px: float) -> void:
	var ev := InputEventMouseMotion.new()
	ev.position = Vector2(px, size.y * 0.5)
	_gui_input(ev)
