extends MarginContainer
## Écran d'accueil : parcours choisi avec son profil, parcours d'exemple,
## état des appareils, résumé de la semaine et accès à l'entraînement.

const UI := preload("res://scripts/ui/theme.gd")
const Fmt := preload("res://scripts/ui/fmt.gd")
const Chart := preload("res://scripts/ui/chart.gd")

const ROLE_NAMES := {"trainer": "Home trainer", "cadence": "Capteur de cadence", "heart_rate": "Fréquence cardiaque", "click": "Zwift Click"}
const STATE_NAMES := {"connected": "Connecté", "connecting": "Connexion…", "failed": "Échec de connexion", "disconnected": "Non connecté"}

var menu: Node

var _hello: Label
var _route_name: Label
var _route_file: Label
var _tiles := {}
var _chart: Control
var _go: Button
var _error: Label
var _examples: VBoxContainer
var _devices: VBoxContainer
var _week_tiles := {}
var _last_ride: Button
var _weeks_chart: Control
var _last_ride_id := -1
var _tried_last := false


func _ready() -> void:
	for side in ["left", "right"]:
		add_theme_constant_override("margin_" + side, 32)
	add_theme_constant_override("margin_top", 26)
	add_theme_constant_override("margin_bottom", 28)
	var row := UI.hbox(24)
	add_child(row)

	# Colonne principale : le parcours.
	var main := UI.vbox(18)
	main.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	main.size_flags_stretch_ratio = 2.2
	row.add_child(main)
	var head := UI.vbox(2)
	_hello = UI.label("", "H1")
	head.add_child(_hello)
	head.add_child(UI.label("Choisis ton parcours, puis ton entraînement.", "DimLabel"))
	main.add_child(head)

	var card := UI.card()
	card.size_flags_vertical = Control.SIZE_EXPAND_FILL
	main.add_child(card)
	var cv := UI.vbox(16)
	card.add_child(cv)
	var title_row := UI.hbox(12)
	cv.add_child(title_row)
	var tv := UI.vbox(2)
	tv.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	tv.add_child(UI.section_title("Parcours"))
	_route_name = UI.label("", "H1")
	_route_name.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	_route_name.clip_text = true
	tv.add_child(_route_name)
	_route_file = UI.label("", "MutedLabel")
	tv.add_child(_route_file)
	title_row.add_child(tv)
	var open := UI.button("Ouvrir un fichier GPX…")
	open.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	open.pressed.connect(_open_dialog)
	title_row.add_child(open)

	var tiles := UI.hbox(12)
	cv.add_child(tiles)
	for spec in [["distance", "Distance", "km"], ["gain", "Dénivelé +", "m"], ["top", "Altitude max", "m"], ["bottom", "Altitude min", "m"]]:
		var t := UI.metric_tile(spec[1], "--", spec[2])
		t.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		tiles.add_child(t)
		_tiles[spec[0]] = t

	var chart_box := UI.card("Field")
	chart_box.size_flags_vertical = Control.SIZE_EXPAND_FILL
	cv.add_child(chart_box)
	_chart = Chart.new()
	_chart.x_format = "km"
	_chart.y_format = "m"
	_chart.y_from_zero = false
	_chart.show_legend = false
	_chart.empty_text = "Choisis un parcours d'exemple, ouvre un fichier GPX ou glisse-le sur la fenêtre."
	_chart.custom_minimum_size.y = 150
	chart_box.add_child(_chart)

	_error = UI.label("", "ErrorLabel", true)
	_error.visible = false
	cv.add_child(_error)

	var bottom := UI.hbox(16)
	cv.add_child(bottom)
	var drop := UI.label("Astuce : glisse un fichier .gpx sur la fenêtre pour l'ouvrir.", "MutedLabel", true)
	drop.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	drop.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	bottom.add_child(drop)
	_go = UI.primary_button("Choisir l'entraînement  ›", true)
	_go.custom_minimum_size.x = 340
	_go.pressed.connect(func(): menu.go("training"))
	bottom.add_child(_go)

	# Colonne de droite : exemples, appareils, semaine.
	var side := UI.vbox(18)
	side.custom_minimum_size.x = 440
	side.size_flags_horizontal = Control.SIZE_FILL
	row.add_child(side)

	var ex := UI.card()
	side.add_child(ex)
	_examples = UI.vbox(8)
	ex.add_child(_examples)

	var dev := UI.card()
	side.add_child(dev)
	var dv := UI.vbox(12)
	dev.add_child(dv)
	var dh := UI.hbox(8)
	dh.add_child(UI.section_title("Appareils"))
	dh.get_child(0).size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var manage := UI.button("Gérer  ›", "LinkButtonish")
	manage.pressed.connect(func(): menu.go("connections"))
	dh.add_child(manage)
	dv.add_child(dh)
	_devices = UI.vbox(10)
	dv.add_child(_devices)

	var week := UI.card()
	week.size_flags_vertical = Control.SIZE_EXPAND_FILL
	side.add_child(week)
	var wv := UI.vbox(12)
	week.add_child(wv)
	var wh := UI.hbox(8)
	var wt := UI.section_title("Cette semaine")
	wt.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	wh.add_child(wt)
	var hist := UI.button("Historique  ›", "LinkButtonish")
	hist.pressed.connect(func(): menu.go("history"))
	wh.add_child(hist)
	wv.add_child(wh)
	var wrow := UI.hbox(10)
	wv.add_child(wrow)
	for spec in [["rides", "Sorties", ""], ["time", "Temps", ""], ["tss", "TSS", ""]]:
		var t := UI.metric_tile(spec[1], "--", spec[2])
		t.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		(t.get_meta("value") as Label).theme_type_variation = "H2"
		wrow.add_child(t)
		_week_tiles[spec[0]] = t
	_weeks_chart = Chart.new()
	_weeks_chart.x_format = "date"
	_weeks_chart.y_format = "heures"
	_weeks_chart.show_legend = false
	_weeks_chart.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_weeks_chart.custom_minimum_size.y = 90
	_weeks_chart.empty_text = "Aucune sortie ces dernières semaines."
	wv.add_child(_weeks_chart)
	_last_ride = UI.button("", "SegmentButton")
	_last_ride.alignment = HORIZONTAL_ALIGNMENT_LEFT
	_last_ride.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	_last_ride.custom_minimum_size.y = 42
	_last_ride.pressed.connect(func():
		if _last_ride_id >= 0:
			menu.show_ride(_last_ride_id))
	wv.add_child(_last_ride)


func enter() -> void:
	if not _tried_last:
		_tried_last = true
		# Au premier affichage, on reprend le dernier parcours utilisé.
		var last: Variant = App.ride.profile_get("app.last_gpx")
		if not App.ride.has_route() and last is String and last != "" and FileAccess.file_exists(last):
			if not App.ride.load_gpx(last):
				_show_error(App.ride.error_message())
	var rider: Variant = App.ride.profile_get("rider.name")
	_hello.text = "Prêt à rouler, %s ?" % rider if rider is String and rider != "" else "Prêt à rouler ?"
	_refresh_route()
	_build_examples()
	_refresh_devices()
	_refresh_week()


## Charge un fichier GPX et en fait le parcours courant.
func load_route(path: String) -> bool:
	if App.ride.load_gpx(path):
		App.ride.profile_set("app.last_gpx", path)
		App.ride.profile_save()
		_error.visible = false
		_refresh_route()
		_build_examples()
		return true
	_show_error(App.ride.error_message())
	return false


func _show_error(text: String) -> void:
	_error.text = text if text != "" else "Ce fichier GPX n'a pas pu être lu."
	_error.visible = true


func _open_dialog() -> void:
	var dialog := FileDialog.new()
	dialog.file_mode = FileDialog.FILE_MODE_OPEN_FILE
	dialog.access = FileDialog.ACCESS_FILESYSTEM
	dialog.filters = PackedStringArray(["*.gpx ; Traces GPX"])
	dialog.use_native_dialog = true
	dialog.title = "Ouvrir un parcours GPX"
	dialog.file_selected.connect(func(path: String): load_route(path))
	dialog.visibility_changed.connect(func():
		if not dialog.visible:
			dialog.queue_free())
	add_child(dialog)
	dialog.popup_centered_ratio(0.7)


func _refresh_route() -> void:
	var has: bool = App.ride.has_route()
	_go.disabled = not has
	_go.tooltip_text = "" if has else "Choisis d'abord un parcours."
	if not has:
		_route_name.text = "Aucun parcours choisi"
		_route_file.text = "Choisis un parcours d'exemple à droite ou ouvre ta trace GPX."
		for k in _tiles:
			UI.set_tile(_tiles[k], "--")
		_chart.set_series([])
		return
	var pts: PackedVector3Array = App.ride.route_points()
	var dist: PackedFloat32Array = App.ride.route_distances()
	_route_name.text = App.ride.route_name()
	var last: Variant = App.ride.profile_get("app.last_gpx")
	_route_file.text = String(last).get_file() if last is String else ""
	UI.set_tile(_tiles.distance, Fmt.num(App.ride.route_length_m() / 1000.0, 1))
	UI.set_tile(_tiles.gain, Fmt.num(App.ride.route_elevation_gain_m()))
	var top := -INF
	var bottom := INF
	for p in pts:
		top = maxf(top, p.y)
		bottom = minf(bottom, p.y)
	UI.set_tile(_tiles.top, Fmt.num(top))
	UI.set_tile(_tiles.bottom, Fmt.num(bottom))
	_chart.set_series([{"name": "Altitude", "color": UI.SECOND, "kind": "area", "points": profile_points(pts, dist), "format": "m"}])


## Profil d'altitude (x en km, y en m) réduit à 600 points au plus.
static func profile_points(pts: PackedVector3Array, dist: PackedFloat32Array) -> PackedVector2Array:
	var out := PackedVector2Array()
	var n := mini(pts.size(), dist.size())
	var stride := maxi(1, n / 600)
	for i in range(0, n, stride):
		out.append(Vector2(dist[i] / 1000.0, pts[i].y))
	if n > 0 and (n - 1) % stride != 0:
		out.append(Vector2(dist[n - 1] / 1000.0, pts[n - 1].y))
	return out


func _build_examples() -> void:
	for c in _examples.get_children():
		c.queue_free()
	_examples.add_child(UI.section_title("Parcours d'exemple"))
	var last: Variant = App.ride.profile_get("app.last_gpx")
	var current := String(last) if last is String and App.ride.has_route() else ""
	var known := false
	for label in App.EXAMPLES:
		var path: String = App.EXAMPLES[label]
		known = known or path == current
		_examples.add_child(_route_button(label, path, path == current))
	if current == "" and last is String and last != "" and FileAccess.file_exists(last):
		current = last
	if current != "" and not known:
		var sep := HSeparator.new()
		sep.add_theme_constant_override("separation", 8)
		_examples.add_child(sep)
		_examples.add_child(UI.section_title("Dernier fichier ouvert"))
		_examples.add_child(_route_button(current.get_file(), current, App.ride.has_route()))


func _route_button(text: String, path: String, selected: bool) -> Button:
	var b := UI.button(text, "SegmentButton")
	b.alignment = HORIZONTAL_ALIGNMENT_LEFT
	b.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	b.add_theme_font_size_override("font_size", 15)
	b.custom_minimum_size.y = 42
	b.toggle_mode = true
	b.button_pressed = selected
	b.tooltip_text = path
	b.pressed.connect(func():
		load_route(path))
	return b


func _refresh_devices() -> void:
	for c in _devices.get_children():
		c.queue_free()
	if App.ride.profile_get("app.test_mode") == true:
		var box := UI.card("InfoBox")
		var l := UI.label("Mode test : appareils simulés. Home trainer, cadence, fréquence cardiaque et Click sont imités par l'application.", "DimLabel", true)
		box.add_child(l)
		_devices.add_child(box)
		return
	var names := {}
	for r in App.ride.remembered_devices():
		names[r.kind] = r.name
	var states := {}
	for d in App.devices.found():
		if d.role != "":
			states[d.role] = d.state
			names[d.role] = d.name
	for role in ROLE_NAMES:
		var row := UI.hbox(12)
		var st: String = states.get(role, "disconnected" if names.has(role) else "")
		row.add_child(UI.status_dot(UI.link_color(st) if st != "" else Color(UI.TEXT_MUTED, 0.5)))
		var l := UI.label(ROLE_NAMES[role])
		l.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		row.add_child(l)
		var info := UI.label(STATE_NAMES.get(st, "") if names.has(role) else "Aucun appareil", "MutedLabel")
		if names.has(role) and st == "connected":
			info.text = names[role]
			info.theme_type_variation = "DimLabel"
		info.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
		info.custom_minimum_size.x = 150
		info.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
		row.add_child(info)
		_devices.add_child(row)


func _refresh_week() -> void:
	var weeks: Array = App.history.weekly(8)
	var w: Dictionary = weeks[-1] if not weeks.is_empty() else {}
	var bars := PackedVector2Array()
	for e in weeks:
		bars.append(Vector2(e.week_start_unix, e.duration_s / 3600.0))
	_weeks_chart.set_series([{"name": "Heures", "color": UI.ACCENT, "kind": "bar", "points": bars}])
	UI.set_tile(_week_tiles.rides, str(int(w.get("rides", 0))))
	UI.set_tile(_week_tiles.time, Fmt.duration(w.get("duration_s", 0.0)) if w.get("duration_s", 0.0) > 0.0 else "0 h")
	UI.set_tile(_week_tiles.tss, Fmt.num(w.get("tss", 0.0)))
	var last: Array = App.history.rides(1, 0)
	if last.is_empty():
		_last_ride_id = -1
		_last_ride.text = "Aucune sortie enregistrée pour l'instant."
		_last_ride.disabled = true
		return
	var r: Dictionary = last[0]
	_last_ride_id = int(r.id)
	_last_ride.disabled = false
	_last_ride.text = "Dernière sortie : %s, %s" % [Fmt.date_short(r.started_unix), r.route_name]
	_last_ride.tooltip_text = "%s · %s · %s" % [Fmt.date_long(r.started_unix), Fmt.duration(r.duration_s), Fmt.km(r.distance_m)]
