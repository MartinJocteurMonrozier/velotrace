extends Control
## Petite carte vue du dessus : la trace et la position du cycliste (nord en haut).

var _points := PackedVector2Array()
var _min := Vector2.ZERO
var _extent := Vector2.ONE
var position_xz := Vector2.ZERO


func setup(points: PackedVector3Array) -> void:
	_points.resize(points.size())
	for i in points.size():
		_points[i] = Vector2(points[i].x, points[i].z)
	var lo := Vector2(INF, INF)
	var hi := Vector2(-INF, -INF)
	for p in _points:
		lo = Vector2(minf(lo.x, p.x), minf(lo.y, p.y))
		hi = Vector2(maxf(hi.x, p.x), maxf(hi.y, p.y))
	_min = lo
	_extent = hi - lo
	queue_redraw()


func _map(p: Vector2) -> Vector2:
	var pad := 18.0
	var s := minf((size.x - 2.0 * pad) / maxf(_extent.x, 1.0), (size.y - 2.0 * pad) / maxf(_extent.y, 1.0))
	return (size - _extent * s) * 0.5 + (p - _min) * s


func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, size), Color(0.05, 0.08, 0.12, 0.65))
	if _points.size() < 2:
		return
	var line := PackedVector2Array()
	for i in range(0, _points.size(), maxi(1, _points.size() / 600)):
		line.append(_map(_points[i]))
	draw_polyline(line, Color(1, 1, 1, 0.25), 6.0, true)
	draw_polyline(line, Color(0.98, 0.62, 0.20), 3.0, true)
	draw_circle(_map(_points[0]), 4.0, Color(0.4, 0.8, 0.4))
	draw_circle(_map(_points[_points.size() - 1]), 4.0, Color(0.9, 0.25, 0.2))
	var me := _map(position_xz)
	draw_circle(me, 7.0, Color.WHITE)
	draw_circle(me, 5.0, Color(0.10, 0.35, 0.80))
	draw_string(get_theme_default_font(), Vector2(size.x - 18, 16), "N", HORIZONTAL_ALIGNMENT_LEFT, -1, 13, Color(1, 1, 1, 0.7))
