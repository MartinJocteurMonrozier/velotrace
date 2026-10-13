extends Control
## Profil de l'ascension en cours, à la manière de MyWhoosh : le col seul,
## tronçon par tronçon coloré selon la pente, les kilomètres, le sommet, la
## position du cycliste et celle des coureurs virtuels, et en haut à gauche
## (au-dessus du pied, où le profil laisse de la place) ce qu'il reste à monter.

const ProfileView := preload("res://scripts/profile_view.gd")
const UI := preload("res://scripts/ui/theme.gd")
const Fmt := preload("res://scripts/ui/fmt.gd")

## Couleur du joueur, comme dans le classement.
const ME_COLOR := Color(0.98, 0.62, 0.20)
const OPPONENT_COLOR := Color(0.80, 0.88, 1.0)
const SHADE := Color(0.02, 0.03, 0.06)
## Hauteur réservée sous le profil aux kilomètres.
const AXIS_H := 18.0
## Le profil garde au moins cette hauteur, pour y écrire les pentes.
const BASE_H := 22.0
## Place au-dessus du sommet pour le drapeau.
const TOP_ROOM := 24.0
const SIDE := 8.0
## Lignes de l'en-tête : titre, puis valeurs.
const TITLE_Y := 22.0
const STATS_Y := 60.0
const HEADER_BOTTOM := 68.0
const STAT_GAP := 28.0
## Le marqueur du cycliste apparaît au pied quand il en est à moins de ça (m).
const APPROACH_M := 150.0

var elevation_offset := 0.0
var position_m := 0.0
## Distances des coureurs virtuels sur le parcours.
var opponents := PackedFloat32Array()
## Temps mis pour monter le col (s), une fois le sommet passé ; négatif sinon.
var climb_time_s := -1.0

var _dist := PackedFloat32Array()
var _elev := PackedFloat32Array()
var _climb := {}
var _number := 0
var _count := 0
## Tronçons : [début, fin, pente en %].
var _segments: Array = []
## Altitudes tous les 1/200 du col, pour l'échelle.
var _samples := PackedFloat32Array()
var _e_min := 0.0
var _e_max := 1.0
## Échelle verticale (px par m) et ordonnée du pied du profil.
var _scale := 1.0
var _base_y := 0.0
var _badge := StyleBoxFlat.new()


## Couleur de la catégorie : du vert (4e) au bordeaux (hors catégorie).
static func category_color(category: String) -> Color:
	if category.begins_with("hors"):
		return ProfileView.grade_color(12.0)
	if category.begins_with("1"):
		return ProfileView.grade_color(9.0)
	if category.begins_with("2"):
		return ProfileView.grade_color(6.0)
	if category.begins_with("3"):
		return ProfileView.grade_color(3.0)
	return ProfileView.grade_color(0.0)


## « HC », « 1re », « 2e »…
static func category_short(category: String) -> String:
	return "HC" if category.begins_with("hors") else category.get_slice(" ", 0)


static func capitalized(text: String) -> String:
	return text.left(1).to_upper() + text.substr(1)


func _init() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	_badge.set_corner_radius_all(6)


func _notification(what: int) -> void:
	if what == NOTIFICATION_RESIZED:
		queue_redraw()


func setup(distances: PackedFloat32Array, points: PackedVector3Array, base_elev: float) -> void:
	_dist = distances
	_elev.resize(points.size())
	for i in points.size():
		_elev[i] = points[i].y
	elevation_offset = base_elev


## Ascension à afficher (dictionnaire de `route_climbs()`), numéro sur nombre.
func set_climb(climb: Dictionary, number: int, count: int) -> void:
	_climb = climb
	_number = number
	_count = count
	var a: float = climb.start_m
	var length := maxf(float(climb.end_m) - a, 1.0)
	# Tronçons de 250 m sur les courtes montées, de 500 m sur les longues.
	var step := 250.0 if length <= 4000.0 else 500.0
	var n := maxi(1, roundi(length / step))
	_segments.clear()
	for i in n:
		var s0 := a + length * i / n
		var s1 := a + length * (i + 1) / n
		_segments.append([s0, s1, (elevation_at(s1) - elevation_at(s0)) / (s1 - s0) * 100.0])
	_samples.resize(201)
	for i in 201:
		_samples[i] = elevation_at(a + length * i / 200.0)
	_e_min = _samples[0]
	_e_max = _samples[0]
	for e in _samples:
		_e_min = minf(_e_min, e)
		_e_max = maxf(_e_max, e)
	_e_max = maxf(_e_max, _e_min + 30.0)
	queue_redraw()


## Altitude (relative au départ du parcours) à une distance donnée.
func elevation_at(d: float) -> float:
	if _dist.size() < 2:
		return 0.0
	var k := clampi(_dist.bsearch(d), 1, _dist.size() - 1)
	var t := clampf((d - _dist[k - 1]) / maxf(_dist[k] - _dist[k - 1], 0.01), 0.0, 1.0)
	return lerpf(_elev[k - 1], _elev[k], t)


func _plot() -> Rect2:
	return Rect2(SIDE, 0.0, size.x - 2.0 * SIDE, size.y - AXIS_H)


func _x(d: float) -> float:
	var r := _plot()
	var a: float = _climb.start_m
	return r.position.x + (d - a) / maxf(float(_climb.end_m) - a, 1.0) * r.size.x


func _y(e: float) -> float:
	return _base_y - (e - _e_min) * _scale


## Échelle verticale : le col prend toute la hauteur, sans passer sous
## l'en-tête qui occupe le coin haut gauche jusqu'à `header_right`.
func _fit(header_right: float) -> void:
	var r := _plot()
	_base_y = r.end.y - BASE_H
	var full := (_base_y - TOP_ROOM) / (_e_max - _e_min)
	var upto := clampi(ceili((header_right + 14.0 - r.position.x) / r.size.x * 200.0), 0, 200)
	var e_hdr := _e_min
	for i in upto + 1:
		e_hdr = maxf(e_hdr, _samples[i])
	_scale = full
	if e_hdr > _e_min + 1.0:
		_scale = clampf((_base_y - HEADER_BOTTOM - 6.0) / (e_hdr - _e_min), full * 0.45, full)


func _draw() -> void:
	if _climb.is_empty() or _dist.size() < 2 or size.x < 200.0:
		return
	var header := _header()
	_fit(header.right)
	var r := _plot()
	var bottom := r.end.y
	var a: float = _climb.start_m
	var b: float = _climb.end_m
	var here := clampf(position_m, a, b)
	var here_x := _x(here)
	var bold := UI.font(700)

	# Profil rempli, tronçon par tronçon, plus sombre vers le bas.
	var outline := PackedVector2Array()
	for seg: Array in _segments:
		var col := ProfileView.grade_color(seg[2])
		var top := col.lightened(0.1)
		var low := col.darkened(0.5)
		var x0 := _x(seg[0])
		var x1 := _x(seg[1])
		var n := maxi(1, ceili((x1 - x0) / 3.0))
		for i in n:
			var d0 := lerpf(seg[0], seg[1], float(i) / n)
			var d1 := lerpf(seg[0], seg[1], float(i + 1) / n)
			var p0 := Vector2(_x(d0), _y(elevation_at(d0)))
			var p1 := Vector2(_x(d1), _y(elevation_at(d1)))
			draw_polygon(PackedVector2Array([p0, p1, Vector2(p1.x, bottom), Vector2(p0.x, bottom)]),
				PackedColorArray([top, top, low, low]))
			# Partie déjà montée, assombrie.
			if p0.x < here_x:
				var f := clampf((here_x - p0.x) / maxf(p1.x - p0.x, 0.01), 0.0, 1.0)
				var q1 := Vector2(minf(p1.x, here_x), lerpf(p0.y, p1.y, f))
				if q1.x - p0.x > 0.01:
					draw_colored_polygon(PackedVector2Array([p0, q1, Vector2(q1.x, bottom), Vector2(p0.x, bottom)]), Color(SHADE, 0.45))
			outline.append(p0)
			if i == n - 1:
				outline.append(p1)
	draw_polyline(outline, Color(1, 1, 1, 0.9), 2.0, true)

	# Séparations et pente de chaque tronçon.
	for i in _segments.size():
		var seg: Array = _segments[i]
		var x0 := _x(seg[0])
		var x1 := _x(seg[1])
		if i > 0:
			draw_line(Vector2(x0, _y(elevation_at(seg[0])) + 2.0), Vector2(x0, bottom), Color(SHADE, 0.5), 1.0)
		var g: float = seg[2]
		var text := Fmt.num(g, 1) + " %"
		var fs := 14
		var w := bold.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, fs).x
		if w > x1 - x0 - 6.0:
			text = Fmt.num(g, 1)
			w = bold.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, fs).x
		if w > x1 - x0 - 4.0:
			text = Fmt.num(g)
			fs = 12
			w = bold.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, fs).x
		if w > x1 - x0 - 2.0:
			continue
		var pos := Vector2((x0 + x1 - w) * 0.5, bottom - 6.0)
		draw_string_outline(bold, pos, text, HORIZONTAL_ALIGNMENT_LEFT, -1, fs, 4, Color(0, 0, 0, 0.5))
		draw_string(bold, pos, text, HORIZONTAL_ALIGNMENT_LEFT, -1, fs, Color.WHITE)

	_draw_axis(r)
	_draw_summit()

	# Coureurs virtuels, puis le joueur par-dessus.
	for od in opponents:
		if od >= a and od <= b:
			var p := Vector2(_x(od), _y(elevation_at(od)))
			draw_circle(p, 5.5, Color(SHADE, 0.85))
			draw_circle(p, 4.0, OPPONENT_COLOR)
	if position_m >= a - APPROACH_M:
		var p := Vector2(here_x, _y(elevation_at(here)))
		draw_line(p, p - Vector2(0.0, 22.0), Color.WHITE, 2.0)
		draw_circle(p - Vector2(0.0, 22.0), 3.0, Color.WHITE)
		draw_circle(p, 8.5, Color.WHITE)
		draw_circle(p, 6.5, ME_COLOR)

	_draw_header(header)


## Axe des kilomètres sous le profil.
func _draw_axis(r: Rect2) -> void:
	var font := UI.font(500)
	var dim := Color(1, 1, 1, 0.65)
	var y := r.end.y
	draw_line(Vector2(r.position.x, y), Vector2(r.end.x, y), Color(1, 1, 1, 0.35), 1.0)
	var length: float = float(_climb.end_m) - float(_climb.start_m)
	var per_km := r.size.x / maxf(length / 1000.0, 0.001)
	var step := 1000.0
	for s in [500.0, 1000.0, 2000.0, 5000.0]:
		step = s
		if per_km * s / 1000.0 >= 56.0:
			break
	var end_text := Fmt.km(length)
	var end_w := font.get_string_size(end_text, HORIZONTAL_ALIGNMENT_LEFT, -1, 12).x
	draw_string(font, Vector2(r.end.x - end_w, y + 15.0), end_text, HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.85))
	var k := 0.0
	while k < length:
		var x := _x(float(_climb.start_m) + k)
		draw_line(Vector2(x, y), Vector2(x, y + 4.0), dim, 1.0)
		var text := "0" if k == 0.0 else Fmt.km(k, 0 if fmod(k, 1000.0) == 0.0 else 1)
		var w := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, 12).x
		var tx := maxf(r.position.x, x - w * 0.5)
		if tx + w < r.end.x - end_w - 10.0:
			draw_string(font, Vector2(tx, y + 15.0), text, HORIZONTAL_ALIGNMENT_LEFT, -1, 12, dim)
		k += step


## Drapeau du grand prix de la montagne au sommet, avec son altitude.
func _draw_summit() -> void:
	var b: float = _climb.end_m
	var p := Vector2(_x(b), _y(elevation_at(b)))
	var top := p.y - TOP_ROOM + 3.0
	draw_line(Vector2(p.x - 1.0, p.y), Vector2(p.x - 1.0, top), Color.WHITE, 2.0)
	# Maillot à pois : blanc à pois rouges.
	var flag := Rect2(p.x - 22.0, top, 21.0, 14.0)
	draw_rect(flag, Color.WHITE)
	for dot in [Vector2(4, 4), Vector2(10.5, 4), Vector2(17, 4), Vector2(7.2, 10), Vector2(13.8, 10)]:
		draw_circle(flag.position + dot, 2.1, Color(0.85, 0.12, 0.15))
	var bold := UI.font(700)
	var text := Fmt.num(elevation_at(b) + elevation_offset) + " m"
	var w := bold.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, 15).x
	var pos := Vector2(flag.position.x - w - 7.0, top + 12.0)
	draw_string_outline(bold, pos, text, HORIZONTAL_ALIGNMENT_LEFT, -1, 15, 4, Color(0, 0, 0, 0.55))
	draw_string(bold, pos, text, HORIZONTAL_ALIGNMENT_LEFT, -1, 15, Color.WHITE)
	# Altitude du pied, au-dessus du départ du profil.
	var a: float = _climb.start_m
	var foot := Fmt.num(elevation_at(a) + elevation_offset) + " m"
	var font := UI.font(500)
	var fp := Vector2(SIDE + 14.0, _y(elevation_at(a)) - 9.0)
	draw_string_outline(font, fp, foot, HORIZONTAL_ALIGNMENT_LEFT, -1, 12, 4, Color(0, 0, 0, 0.5))
	draw_string(font, fp, foot, HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.8))


## Contenu de l'en-tête et sa largeur (`right`) : titre, caractéristiques du
## col, puis ce qu'il reste à monter (ou le col entier avant le pied).
func _header() -> Dictionary:
	var bold := UI.font(700)
	var semi := UI.font(600)
	var font := UI.font(400)
	var a: float = _climb.start_m
	var b: float = _climb.end_m
	var cat := String(_climb.category)
	var d := position_m

	var badge_text := category_short(cat)
	var badge := Rect2(SIDE, 2.0, maxf(bold.get_string_size(badge_text, HORIZONTAL_ALIGNMENT_LEFT, -1, 17).x + 18.0, 40.0), 28.0)
	var title := "Ascension"
	if _count > 1:
		title += " %d / %d" % [_number, _count]
	title += " · " + capitalized(cat)
	var sub := "%s à %s %% · max %s %%" % [Fmt.km(b - a), Fmt.num(float(_climb.avg_grade_pct), 1), Fmt.num(float(_climb.max_grade_pct), 1)]
	if d < a:
		var to_foot := a - d
		sub = ("pied dans %d m" % (roundi(to_foot / 10.0) * 10) if to_foot < 1000.0 else "pied dans " + Fmt.km(to_foot)) + " · " + sub
	var title_x := badge.end.x + 12.0
	var sub_x := title_x + semi.get_string_size(title, HORIZONTAL_ALIGNMENT_LEFT, -1, 18).x + 14.0

	var stats: Array = []
	if d > b:
		if climb_time_s >= 0.0:
			stats.append([Fmt.mss(climb_time_s), "pour monter"])
		stats.append(["Sommet", "franchi"])
	elif d < a:
		stats.append([Fmt.km(b - a), "de montée"])
		stats.append([Fmt.num(float(_climb.gain_m)) + " m", "D+"])
		stats.append([Fmt.num(float(_climb.avg_grade_pct), 1) + " %", "de moyenne"])
	else:
		var left_m := b - d
		var gain := maxf(0.0, float(_climb.top_elevation_m) - (elevation_at(d) + elevation_offset))
		stats.append([Fmt.km(left_m), "au sommet"])
		stats.append([Fmt.num(gain) + " m", "D+ restants"])
		stats.append([Fmt.num(gain / maxf(left_m, 1.0) * 100.0, 1) + " %", "de moyenne restante"])
	# Valeurs mises en page à la suite ; celles qui débordent sautent.
	var x := SIDE
	var placed: Array = []
	for s: Array in stats:
		var vw := bold.get_string_size(s[0], HORIZONTAL_ALIGNMENT_LEFT, -1, 28).x
		var cw := font.get_string_size(" " + s[1], HORIZONTAL_ALIGNMENT_LEFT, -1, 14).x
		if x + vw + cw > size.x * 0.62 and not placed.is_empty():
			break
		placed.append([x, s[0], x + vw, " " + s[1]])
		x += vw + cw + STAT_GAP
	var right := maxf(x - STAT_GAP, sub_x + font.get_string_size(sub, HORIZONTAL_ALIGNMENT_LEFT, -1, 14).x)
	return {"badge": badge, "badge_text": badge_text, "title": title, "title_x": title_x,
		"sub": sub, "sub_x": sub_x, "stats": placed, "right": right}


func _draw_header(h: Dictionary) -> void:
	var bold := UI.font(700)
	var semi := UI.font(600)
	var font := UI.font(400)
	var outline := Color(0, 0, 0, 0.45)
	var badge: Rect2 = h.badge
	_badge.bg_color = category_color(String(_climb.category))
	draw_style_box(_badge, badge)
	var bt: String = h.badge_text
	var tw := bold.get_string_size(bt, HORIZONTAL_ALIGNMENT_LEFT, -1, 17).x
	var tp := Vector2(badge.get_center().x - tw * 0.5, badge.position.y + 20.5)
	draw_string_outline(bold, tp, bt, HORIZONTAL_ALIGNMENT_LEFT, -1, 17, 4, Color(0, 0, 0, 0.3))
	draw_string(bold, tp, bt, HORIZONTAL_ALIGNMENT_LEFT, -1, 17, Color.WHITE)
	_text(semi, Vector2(h.title_x, TITLE_Y), h.title, 18, Color.WHITE, outline)
	_text(font, Vector2(h.sub_x, TITLE_Y), h.sub, 14, Color(1, 1, 1, 0.75), outline)
	for s: Array in h.stats:
		_text(bold, Vector2(s[0], STATS_Y), s[1], 28, Color.WHITE, outline)
		_text(font, Vector2(s[2], STATS_Y), s[3], 14, Color(1, 1, 1, 0.75), outline)


func _text(font: Font, pos: Vector2, text: String, font_size: int, color: Color, outline: Color) -> void:
	draw_string_outline(font, pos, text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, 4, outline)
	draw_string(font, pos, text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, color)
