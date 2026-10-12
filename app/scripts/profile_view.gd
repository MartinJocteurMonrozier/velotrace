extends Control
## Profil d'altitude du parcours, coloré par la pente, avec la position actuelle.

var _dist := PackedFloat32Array()
var _elev := PackedFloat32Array()
var _min_e := 0.0
var _max_e := 1.0
var position_m := 0.0
## Altitude réelle du point de départ, pour afficher les altitudes vraies.
var elevation_offset := 0.0


func setup(distances: PackedFloat32Array, points: PackedVector3Array) -> void:
	_dist = distances
	_elev.resize(points.size())
	for i in points.size():
		_elev[i] = points[i].y
	_min_e = Array(_elev).min()
	_max_e = maxf(Array(_elev).max(), _min_e + 20.0)
	queue_redraw()


static func grade_color(grade_pct: float) -> Color:
	if grade_pct < 0.0:
		return Color(0.35, 0.62, 0.95)
	if grade_pct < 3.0:
		return Color(0.40, 0.78, 0.40)
	if grade_pct < 6.0:
		return Color(0.95, 0.80, 0.25)
	if grade_pct < 9.0:
		return Color(0.95, 0.52, 0.20)
	return Color(0.90, 0.22, 0.20)


func _to_screen(d: float, e: float) -> Vector2:
	var total := maxf(_dist[_dist.size() - 1], 1.0)
	var pad := 8.0
	var x := pad + d / total * (size.x - 2.0 * pad)
	var y := size.y - pad - (e - _min_e) / (_max_e - _min_e) * (size.y - 2.0 * pad - 14.0)
	return Vector2(x, y)


func _draw() -> void:
	if _dist.size() < 2:
		return
	var bottom := size.y - 8.0
	var stride := maxi(1, _dist.size() / 400)
	var i := 0
	while i < _dist.size() - 1:
		var j := mini(i + stride, _dist.size() - 1)
		var a := _to_screen(_dist[i], _elev[i])
		var b := _to_screen(_dist[j], _elev[j])
		var grade := (_elev[j] - _elev[i]) / maxf(_dist[j] - _dist[i], 0.1) * 100.0
		var col := grade_color(grade)
		col.a = 0.85
		draw_colored_polygon(PackedVector2Array([a, b, Vector2(b.x, bottom), Vector2(a.x, bottom)]), col)
		i = j
	var here := position_m
	var k := clampi(_dist.bsearch(here), 1, _dist.size() - 1)
	var t := clampf((here - _dist[k - 1]) / maxf(_dist[k] - _dist[k - 1], 0.01), 0.0, 1.0)
	var p := _to_screen(here, lerpf(_elev[k - 1], _elev[k], t))
	draw_line(Vector2(p.x, 6.0), Vector2(p.x, bottom), Color(1, 1, 1, 0.9), 2.0)
	draw_circle(p, 6.0, Color.WHITE)
	draw_circle(p, 4.0, Color(0.10, 0.35, 0.80))
	var font := get_theme_default_font()
	draw_string(font, Vector2(10, size.y - 12), "%d m" % roundi(_min_e + elevation_offset), HORIZONTAL_ALIGNMENT_LEFT, -1, 13, Color(1, 1, 1, 0.7))
	draw_string(font, Vector2(10, 16), "%d m" % roundi(_max_e + elevation_offset), HORIZONTAL_ALIGNMENT_LEFT, -1, 13, Color(1, 1, 1, 0.7))
