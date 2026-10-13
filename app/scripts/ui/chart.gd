extends Control
## Graphique dessiné à la main : courbes, barres et aires, axes gradués à la
## française, échelle logarithmique en X pour les courbes de puissance et
## infobulle au survol.
##
## Chaque série : {name, color, points: PackedVector2Array (x croissants),
## kind: "line" | "bar" | "area", axis: "y" | "y2" (facultatif), width,
## format (facultatif, remplace celui de l'axe dans l'infobulle),
## colors: PackedColorArray (facultatif, une couleur par barre)}.
##
## Formats : "nombre", "duree" (s), "temps" (s écoulées, m:ss puis h:mm:ss), "watts", "wkg",
## "km" (valeur en km), "m", "heures", "date" (unix), "pourcent", ou un
## Callable(valeur: float, long: bool) -> String.

const UI := preload("res://scripts/ui/theme.gd")
const Fmt := preload("res://scripts/ui/fmt.gd")

## Graduations des durées pour l'échelle logarithmique.
const LOG_TICKS := [1, 5, 15, 60, 300, 1200, 3600, 10800, 18000]
const TIME_STEPS := [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1200, 1800, 3600, 7200, 10800, 21600]
const DAY := 86400.0
const DATE_STEPS := [1, 2, 7, 14, 28, 56, 91, 182, 364]
const TICK_SIZE := 12

var x_log := false
var x_format: Variant = "nombre"
var y_format: Variant = "nombre"
var y2_format: Variant = "m"
## Le bas de l'axe Y part de zéro (sinon il suit les données).
var y_from_zero := true
var y2_from_zero := false
## Affiche les graduations de l'axe Y2 à droite.
var show_y2_axis := true
var show_legend := true
## Bornes X imposées (sinon celles des données).
var x_range := Vector2.ZERO
var empty_text := "Pas encore de données"

var _series: Array = []
var _refs: Array = []
var _empty := true
var _x := Vector2(0, 1)
var _y := Vector2(0, 1)
var _y2 := Vector2(0, 1)
var _yticks: Array = []
var _y2ticks: Array = []
var _has_y2 := false
var _bar_w := 0.0
var _bar_count := 0
var _plot := Rect2()
var _hover := -1.0
var _bar_styles := {}


func _init() -> void:
	mouse_filter = Control.MOUSE_FILTER_PASS
	clip_contents = true
	custom_minimum_size = Vector2(200, 160)
	resized.connect(queue_redraw)
	mouse_exited.connect(func():
		_hover = -1.0
		queue_redraw())


## Remplace les séries affichées.
func set_series(series: Array) -> void:
	_series = []
	for s in series:
		if s is Dictionary and s.has("points"):
			_series.append(s)
	_compute_bounds()
	queue_redraw()


## Lignes horizontales de repère : [{y, color, label, axis}].
func set_reference_lines(lines: Array) -> void:
	_refs = lines
	_compute_bounds()
	queue_redraw()


func _compute_bounds() -> void:
	var xs := Vector2(INF, -INF)
	var ys := Vector2(INF, -INF)
	var y2s := Vector2(INF, -INF)
	var min_dx := INF
	_bar_count = 0
	_has_y2 = false
	for s in _series:
		var pts: PackedVector2Array = s.points
		var second: bool = s.get("axis", "y") == "y2"
		_has_y2 = _has_y2 or second
		for p in pts:
			if x_log and p.x <= 0.0:
				continue
			xs = Vector2(minf(xs.x, p.x), maxf(xs.y, p.x))
			if second:
				y2s = Vector2(minf(y2s.x, p.y), maxf(y2s.y, p.y))
			else:
				ys = Vector2(minf(ys.x, p.y), maxf(ys.y, p.y))
		if s.get("kind", "line") == "bar":
			_bar_count += 1
			for i in range(1, pts.size()):
				min_dx = minf(min_dx, absf(pts[i].x - pts[i - 1].x))
	_empty = xs.x == INF
	if _empty:
		return
	for r in _refs:
		if r.get("axis", "y") == "y":
			ys = Vector2(minf(ys.x, r.y), maxf(ys.y, r.y))
	_x = xs
	_bar_w = 0.0
	if _bar_count > 0:
		_bar_w = min_dx if min_dx < INF and min_dx > 0.0 else 1.0
		_x = Vector2(xs.x - _bar_w * 0.5, xs.y + _bar_w * 0.5)
	if x_range != Vector2.ZERO:
		_x = x_range
	if is_equal_approx(_x.x, _x.y):
		_x = Vector2(_x.x - 1.0, _x.y + 1.0) if not x_log else Vector2(_x.x, _x.x * 10.0)
	if ys.x == INF:
		ys = Vector2(0, 1)
	var r1 := _nice_range(ys, y_from_zero, y_format)
	_y = r1[0]
	_yticks = r1[1]
	if _has_y2:
		var r2 := _nice_range(y2s, y2_from_zero, y2_format)
		_y2 = r2[0]
		_y2ticks = r2[1]


## Plage arrondie à des graduations « rondes » englobant les données.
func _nice_range(r: Vector2, from_zero: bool, fmt: Variant) -> Array:
	var lo := minf(r.x, 0.0) if from_zero else r.x
	var hi := r.y
	if hi - lo < 1e-6:
		hi = lo + (absf(lo) * 0.1 if lo != 0.0 else 1.0)
	if not from_zero:
		var pad := (hi - lo) * 0.06
		lo -= pad
		hi += pad
	var step := _nice_step((hi - lo) / 4.0, fmt)
	lo = floorf(lo / step + 1e-6) * step
	hi = ceilf(hi / step - 1e-6) * step
	var ticks := []
	var v := lo
	while v <= hi + step * 0.5:
		ticks.append(v)
		v += step
	return [Vector2(lo, hi), ticks]


func _nice_step(raw: float, fmt: Variant) -> float:
	if fmt is String and (fmt == "duree" or fmt == "temps"):
		for s in TIME_STEPS:
			if s >= raw:
				return float(s)
	var mag := pow(10.0, floorf(log(maxf(raw, 1e-9)) / log(10.0)))
	var f := raw / mag
	var nice := 10.0
	if f <= 1.0:
		nice = 1.0
	elif f <= 2.0:
		nice = 2.0
	elif f <= 2.5:
		nice = 2.5
	elif f <= 5.0:
		nice = 5.0
	return nice * mag


func _x_ticks() -> Array:
	var ticks := []
	if x_log:
		for t in LOG_TICKS:
			if t >= _x.x - 1e-6 and t <= _x.y + 1e-6:
				ticks.append(float(t))
		return ticks
	if _bar_count > 0 and not (x_format is String and x_format == "date"):
		# Une graduation par barre ; les étiquettes qui se chevauchent sont sautées.
		for s in _series:
			if s.get("kind", "line") == "bar":
				for p in s.points:
					ticks.append(p.x)
				return ticks
	var span := _x.y - _x.x
	var max_count := maxi(2, int(_plot.size.x / 90.0))
	if x_format is String and x_format == "date":
		var step := DATE_STEPS[-1] * DAY
		for d in DATE_STEPS:
			if span / (d * DAY) <= max_count:
				step = d * DAY
				break
		var origin := _first_x()
		var v := origin + ceilf((_x.x - origin) / step - 1e-6) * step
		while v <= _x.y + 1e-6:
			ticks.append(v)
			v += step
		return ticks
	var st := _nice_step(span / max_count, x_format)
	var v2 := ceilf(_x.x / st - 1e-6) * st
	while v2 <= _x.y + 1e-6:
		ticks.append(v2)
		v2 += st
	return ticks


func _first_x() -> float:
	for s in _series:
		var pts: PackedVector2Array = s.points
		if pts.size() > 0:
			return pts[0].x
	return 0.0


# --- Conversions -----------------------------------------------------------------

func _sx(x: float) -> float:
	var t := 0.0
	if x_log:
		t = (log(maxf(x, 1e-6)) - log(_x.x)) / (log(_x.y) - log(_x.x))
	else:
		t = (x - _x.x) / (_x.y - _x.x)
	return _plot.position.x + t * _plot.size.x


func _inv_x(px: float) -> float:
	var t := (px - _plot.position.x) / maxf(_plot.size.x, 1.0)
	if x_log:
		return exp(log(_x.x) + t * (log(_x.y) - log(_x.x)))
	return _x.x + t * (_x.y - _x.x)


func _sy(y: float, second := false) -> float:
	var r := _y2 if second else _y
	var t := (y - r.x) / maxf(r.y - r.x, 1e-9)
	return _plot.end.y - clampf(t, -0.02, 1.02) * _plot.size.y


# --- Mise en forme ---------------------------------------------------------------

## Texte d'une valeur ; `long` pour l'infobulle, sinon court pour les graduations.
static func format_value(fmt: Variant, v: float, long := false) -> String:
	if fmt is Callable:
		return fmt.call(v, long)
	match fmt:
		"duree":
			return Fmt.duration(v)
		"temps":
			return Fmt.hms(v) if long else Fmt.mss(v)
		"watts":
			return Fmt.num(v) + " W"
		"wkg":
			return Fmt.num(v, 1 if long else _auto_decimals(v)) + " W/kg"
		"km":
			return Fmt.num(v, 1 if long else _auto_decimals(v)) + " km"
		"m":
			return Fmt.num(v) + " m"
		"heures":
			return Fmt.duration(v * 3600.0) if long else Fmt.num(v, _auto_decimals(v)) + " h"
		"date":
			return Fmt.date_long(v) if long else Fmt.date_short(v)
		"pourcent":
			return Fmt.num(v) + " %"
	return Fmt.num(v, 1 if long and absf(v) < 100.0 and not is_equal_approx(v, roundf(v)) else _auto_decimals(v))


static func _auto_decimals(v: float) -> int:
	if is_equal_approx(v, roundf(v)):
		return 0
	if is_equal_approx(v * 10.0, roundf(v * 10.0)):
		return 1
	return 2


# --- Dessin -----------------------------------------------------------------------

func _draw() -> void:
	var font := UI.font(400)
	var bold := UI.font(600)
	if _empty:
		draw_string(font, Vector2(0, size.y * 0.5), empty_text, HORIZONTAL_ALIGNMENT_CENTER, size.x, 15, UI.TEXT_MUTED)
		return
	var legend_h := 26.0 if show_legend and _series.size() > 0 else 6.0
	var left := 0.0
	for t in _yticks:
		left = maxf(left, font.get_string_size(format_value(y_format, t), HORIZONTAL_ALIGNMENT_LEFT, -1, TICK_SIZE).x)
	var right := 10.0
	if _has_y2 and show_y2_axis:
		for t in _y2ticks:
			right = maxf(right, font.get_string_size(format_value(y2_format, t), HORIZONTAL_ALIGNMENT_LEFT, -1, TICK_SIZE).x + 10.0)
	_plot = Rect2(left + 10.0, legend_h + 6.0, size.x - left - 10.0 - right, size.y - legend_h - 6.0 - 24.0)
	if _plot.size.x < 20.0 or _plot.size.y < 20.0:
		return

	# Grille et graduations Y.
	var grid := Color(1, 1, 1, 0.06)
	for t in _yticks:
		var y := _sy(t)
		draw_line(Vector2(_plot.position.x, y), Vector2(_plot.end.x, y), grid, 1.0)
		var txt := format_value(y_format, t)
		draw_string(font, Vector2(0, y + 4.0), txt, HORIZONTAL_ALIGNMENT_RIGHT, left, TICK_SIZE, UI.TEXT_MUTED)
	if _has_y2 and show_y2_axis:
		for t in _y2ticks:
			var y2 := _sy(t, true)
			draw_string(font, Vector2(_plot.end.x + 8.0, y2 + 4.0), format_value(y2_format, t), HORIZONTAL_ALIGNMENT_LEFT, -1, TICK_SIZE, UI.TEXT_MUTED)

	# Graduations X (espacées pour éviter les chevauchements).
	var xt := _x_ticks()
	var last_end := -INF
	for t in xt:
		var x := _sx(t)
		var txt := format_value(x_format, t)
		var w := font.get_string_size(txt, HORIZONTAL_ALIGNMENT_LEFT, -1, TICK_SIZE).x
		if x_log:
			draw_line(Vector2(x, _plot.position.y), Vector2(x, _plot.end.y), grid, 1.0)
		var lx := clampf(x - w * 0.5, 0.0, size.x - w)
		if lx < last_end + 10.0:
			continue
		draw_line(Vector2(x, _plot.end.y), Vector2(x, _plot.end.y + 4.0), Color(1, 1, 1, 0.2), 1.0)
		draw_string(font, Vector2(lx, _plot.end.y + 18.0), txt, HORIZONTAL_ALIGNMENT_LEFT, -1, TICK_SIZE, UI.TEXT_MUTED)
		last_end = lx + w
	draw_line(Vector2(_plot.position.x, _plot.end.y), Vector2(_plot.end.x, _plot.end.y), Color(1, 1, 1, 0.16), 1.0)

	# Séries : aires d'abord, puis barres, puis courbes.
	var hover_x := _inv_x(_hover) if _hover >= 0.0 else NAN
	for kind in ["area", "bar", "line"]:
		var bar_i := 0
		for s in _series:
			if s.get("kind", "line") != kind:
				continue
			match kind:
				"area":
					_draw_area(s)
				"bar":
					_draw_bars(s, bar_i, hover_x)
					bar_i += 1
				"line":
					_draw_line(s)

	for r in _refs:
		var second: bool = r.get("axis", "y") == "y2"
		var ry := _sy(r.y, second)
		var col: Color = r.get("color", UI.TEXT_DIM)
		draw_dashed_line(Vector2(_plot.position.x, ry), Vector2(_plot.end.x, ry), col, 1.0, 6.0)
		if r.has("label"):
			var lw := bold.get_string_size(r.label, HORIZONTAL_ALIGNMENT_LEFT, -1, 12).x
			var lp := Vector2(_plot.end.x - lw - 10.0, ry - 6.0)
			draw_rect(Rect2(lp.x - 5.0, lp.y - 12.0, lw + 10.0, 16.0), Color(UI.CARD, 0.85))
			draw_string(bold, lp, r.label, HORIZONTAL_ALIGNMENT_LEFT, -1, 12, col)

	if show_legend:
		_draw_legend(font)
	if not is_nan(hover_x):
		_draw_hover(font, bold, hover_x)


## Réduit une série à un point par pixel environ (minimum et maximum conservés).
func _screen_points(pts: PackedVector2Array, second: bool) -> PackedVector2Array:
	var out := PackedVector2Array()
	var last_px := -INF
	var bucket_min := Vector2(0, INF)
	var bucket_max := Vector2(0, -INF)
	for p in pts:
		if x_log and p.x <= 0.0:
			continue
		var sp := Vector2(_sx(p.x), _sy(p.y, second))
		if pts.size() < _plot.size.x * 2.0:
			out.append(sp)
			continue
		if floorf(sp.x) != last_px:
			if last_px > -INF:
				if bucket_min.x <= bucket_max.x:
					out.append(bucket_max)
					out.append(bucket_min)
				else:
					out.append(bucket_min)
					out.append(bucket_max)
			last_px = floorf(sp.x)
			bucket_min = sp
			bucket_max = sp
		else:
			if sp.y > bucket_min.y:
				bucket_min = sp
			if sp.y < bucket_max.y:
				bucket_max = sp
	if pts.size() >= _plot.size.x * 2.0 and last_px > -INF:
		out.append(bucket_min)
	return out


func _draw_line(s: Dictionary) -> void:
	var sp := _screen_points(s.points, s.get("axis", "y") == "y2")
	if sp.size() >= 2:
		draw_polyline(sp, s.get("color", UI.ACCENT), s.get("width", 2.0), true)
	elif sp.size() == 1:
		draw_circle(sp[0], 3.0, s.get("color", UI.ACCENT))


func _draw_area(s: Dictionary) -> void:
	var second: bool = s.get("axis", "y") == "y2"
	var sp := _screen_points(s.points, second)
	if sp.size() < 2:
		return
	var col: Color = s.get("color", UI.SECOND)
	var top_a: float = s.get("fill_alpha", 0.35)
	var base := _plot.end.y
	var c_top := Color(col, top_a)
	var c_bot := Color(col, top_a * 0.12)
	for i in range(1, sp.size()):
		var a := sp[i - 1]
		var b := sp[i]
		if b.x - a.x < 0.01 and i < sp.size() - 1:
			continue
		draw_polygon(PackedVector2Array([a, b, Vector2(b.x, base), Vector2(a.x, base)]),
			PackedColorArray([c_top, c_top, c_bot, c_bot]))
	draw_polyline(sp, Color(col, minf(1.0, top_a * 2.4)), s.get("width", 1.5), true)


func _draw_bars(s: Dictionary, index: int, hover_x: float) -> void:
	var pts: PackedVector2Array = s.points
	var second: bool = s.get("axis", "y") == "y2"
	var col: Color = s.get("color", UI.ACCENT)
	var colors: PackedColorArray = s.get("colors", PackedColorArray())
	var slot := _bar_w * 0.72 / maxi(_bar_count, 1)
	var hovered := _nearest(pts, hover_x) if not is_nan(hover_x) else -1
	for i in pts.size():
		var p := pts[i]
		var x0 := _sx(p.x - _bar_w * 0.36 + slot * index)
		var x1 := _sx(p.x - _bar_w * 0.36 + slot * (index + 1))
		var y0 := _sy(0.0, second)
		var y1 := _sy(p.y, second)
		var c := colors[i] if i < colors.size() else col
		if hovered >= 0 and i != hovered:
			c = Color(c, c.a * 0.55)
		var w := maxf(x1 - x0 - 2.0, 1.0)
		if w > 72.0:
			x0 += (w - 72.0) * 0.5
			w = 72.0
		if absf(y0 - y1) < 0.5:
			continue
		draw_style_box(_bar_style(c, minf(5.0, w * 0.3)), Rect2(x0 + 1.0, minf(y0, y1), w, absf(y0 - y1)))


func _bar_style(c: Color, radius: float) -> StyleBoxFlat:
	var key := "%s/%d" % [c.to_html(), int(radius)]
	if not _bar_styles.has(key):
		var sb := StyleBoxFlat.new()
		sb.bg_color = c
		sb.corner_radius_top_left = int(radius)
		sb.corner_radius_top_right = int(radius)
		sb.anti_aliasing_size = 0.6
		_bar_styles[key] = sb
	return _bar_styles[key]


func _draw_legend(font: Font) -> void:
	var x := _plot.position.x
	for s in _series:
		var title: String = s.get("name", "")
		if title == "":
			continue
		var col: Color = s.get("color", UI.ACCENT)
		var k: String = s.get("kind", "line")
		if k == "line":
			draw_line(Vector2(x, 11), Vector2(x + 16, 11), col, 3.0, true)
		else:
			draw_rect(Rect2(x + 2, 5, 12, 12), Color(col, 0.85 if k == "bar" else 0.6))
		draw_string(font, Vector2(x + 22, 16), title, HORIZONTAL_ALIGNMENT_LEFT, -1, 13, UI.TEXT_DIM)
		x += 22 + font.get_string_size(title, HORIZONTAL_ALIGNMENT_LEFT, -1, 13).x + 20


## Index du point le plus proche de x (points triés par x croissant).
func _nearest(pts: PackedVector2Array, x: float) -> int:
	if pts.is_empty():
		return -1
	var lo := 0
	var hi := pts.size() - 1
	while hi - lo > 1:
		var mid := (lo + hi) / 2
		if pts[mid].x < x:
			lo = mid
		else:
			hi = mid
	if x_log:
		return lo if absf(log(maxf(pts[lo].x, 1e-6)) - log(x)) <= absf(log(maxf(pts[hi].x, 1e-6)) - log(x)) else hi
	return lo if absf(pts[lo].x - x) <= absf(pts[hi].x - x) else hi


func _draw_hover(font: Font, bold: Font, hx: float) -> void:
	var lines: Array = []
	var anchor_x := NAN
	for s in _series:
		var pts: PackedVector2Array = s.points
		var i := _nearest(pts, hx)
		if i < 0:
			continue
		var p := pts[i]
		if is_nan(anchor_x):
			anchor_x = p.x
		var second: bool = s.get("axis", "y") == "y2"
		var col: Color = s.get("color", UI.ACCENT)
		if s.get("kind", "line") != "bar":
			var sp := Vector2(_sx(p.x), _sy(p.y, second))
			draw_circle(sp, 5.0, Color(UI.BG, 0.9))
			draw_circle(sp, 3.5, col)
		var fmt: Variant = s.get("format", y2_format if second else y_format)
		lines.append([col, s.get("name", ""), format_value(fmt, p.y, true)])
	if is_nan(anchor_x) or lines.is_empty():
		return
	var ax := _sx(anchor_x)
	draw_line(Vector2(ax, _plot.position.y), Vector2(ax, _plot.end.y), Color(1, 1, 1, 0.25), 1.0)

	var header := format_value(x_format, anchor_x, true)
	var w := font.get_string_size(header, HORIZONTAL_ALIGNMENT_LEFT, -1, 13).x
	for l in lines:
		var lw: float = bold.get_string_size(l[2], HORIZONTAL_ALIGNMENT_LEFT, -1, 14).x
		lw += font.get_string_size(l[1], HORIZONTAL_ALIGNMENT_LEFT, -1, 13).x + 34.0
		w = maxf(w, lw)
	var h := 30.0 + lines.size() * 21.0
	var box := Rect2(ax + 14.0, _plot.position.y + 4.0, w + 24.0, h)
	if box.end.x > size.x - 2.0:
		box.position.x = ax - 14.0 - box.size.x
	box.position.x = maxf(box.position.x, 2.0)
	var sb := StyleBoxFlat.new()
	sb.bg_color = UI.CARD_HI
	sb.set_corner_radius_all(8)
	sb.border_color = UI.BORDER_HI
	sb.set_border_width_all(1)
	sb.shadow_color = Color(0, 0, 0, 0.35)
	sb.shadow_size = 8
	draw_style_box(sb, box)
	var y := box.position.y + 21.0
	draw_string(font, Vector2(box.position.x + 12.0, y), header, HORIZONTAL_ALIGNMENT_LEFT, -1, 13, UI.TEXT_DIM)
	for l in lines:
		y += 21.0
		draw_circle(Vector2(box.position.x + 17.0, y - 5.0), 4.0, l[0])
		var nx := box.position.x + 28.0
		draw_string(font, Vector2(nx, y), l[1], HORIZONTAL_ALIGNMENT_LEFT, -1, 13, UI.TEXT_DIM)
		draw_string(bold, Vector2(box.end.x - 12.0 - bold.get_string_size(l[2], HORIZONTAL_ALIGNMENT_LEFT, -1, 14).x, y), l[2], HORIZONTAL_ALIGNMENT_LEFT, -1, 14, UI.TEXT)


func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion:
		var inside: bool = event.position.x >= _plot.position.x and event.position.x <= _plot.end.x
		_hover = event.position.x if inside and not _empty else -1.0
		queue_redraw()


## Simule le survol à une abscisse en pixels (captures d'écran).
func hover_at(px: float) -> void:
	_hover = px
	queue_redraw()
