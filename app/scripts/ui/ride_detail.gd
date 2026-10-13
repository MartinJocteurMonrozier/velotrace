extends MarginContainer
## Bilan d'une sortie, depuis l'historique ou à l'arrivée : mesures, puissance
## et altitude au fil du temps, courbe de puissance comparée aux 90 derniers
## jours, envoi sur Strava, export .FIT et suppression.

## L'utilisateur quitte le bilan (« Retour », ou sortie supprimée).
signal closed()
## L'utilisateur demande l'accueil.
signal home_requested()

const UI := preload("res://scripts/ui/theme.gd")
const Fmt := preload("res://scripts/ui/fmt.gd")
const Chart := preload("res://scripts/ui/chart.gd")

const STRAVA := Color("fc4c02")
const ELEVATION := Color("9a9a9a")
const CURVE_90 := Color("4f9cf0")

var menu: Node

var _id := -1
var _just := false
var _ride := {}
var _built := false
var _kicker: Label
var _title: Label
var _subtitle: Label
var _strava_b: Button
var _tiles := {}
var _time_chart: Control
var _curve_chart: Control
var _notice: PanelContainer
var _notice_dot: Panel
var _notice_label: Label
var _notice_bar: ProgressBar
var _notice_link: Button
var _notice_url := ""
var _uploading := false
var _poll := 0.0


## Affiche la sortie `ride_id` ; `just_finished` à l'arrivée d'une sortie.
func setup(ride_id: int, just_finished: bool) -> void:
	_id = ride_id
	_just = just_finished
	if _built:
		_fill()


func _ready() -> void:
	for side in ["left", "right"]:
		add_theme_constant_override("margin_" + side, 32)
	add_theme_constant_override("margin_top", 22)
	add_theme_constant_override("margin_bottom", 26)
	var col := UI.vbox(16)
	add_child(col)

	var head := UI.hbox(14)
	col.add_child(head)
	var back := UI.button("‹  Retour", "GhostButton")
	back.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	back.pressed.connect(func(): closed.emit())
	head.add_child(back)
	var hv := UI.vbox(0)
	hv.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_kicker = UI.label("", "AccentLabel")
	hv.add_child(_kicker)
	_title = UI.label("", "H1")
	_title.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	_title.clip_text = true
	hv.add_child(_title)
	_subtitle = UI.label("", "DimLabel")
	hv.add_child(_subtitle)
	head.add_child(hv)
	_strava_b = UI.button("Envoyer sur Strava")
	_strava_b.add_theme_color_override("font_color", Color("ff8a4c"))
	_strava_b.pressed.connect(_upload)
	var fit := UI.button("Exporter en .FIT…")
	fit.pressed.connect(_export_fit)
	var del := UI.button("Supprimer", "DangerButton", "Supprimer la sortie de l'historique")
	del.pressed.connect(_confirm_delete)
	for b in [_strava_b, fit, del]:
		b.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		head.add_child(b)
	var home := UI.primary_button("Accueil")
	home.custom_minimum_size.x = 150
	home.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	home.pressed.connect(func(): home_requested.emit())
	head.add_child(home)
	home.set_meta("only_when_finished", true)

	_notice = UI.card("Field")
	_notice.visible = false
	col.add_child(_notice)
	var nr := UI.hbox(12)
	_notice.add_child(nr)
	_notice_dot = UI.status_dot(UI.TEXT_MUTED)
	nr.add_child(_notice_dot)
	_notice_label = UI.label("", "DimLabel")
	_notice_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_notice_label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	nr.add_child(_notice_label)
	_notice_bar = ProgressBar.new()
	_notice_bar.max_value = 1.0
	_notice_bar.show_percentage = false
	_notice_bar.custom_minimum_size = Vector2(240, 8)
	_notice_bar.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	nr.add_child(_notice_bar)
	_notice_link = UI.button("", "LinkButtonish")
	_notice_link.pressed.connect(_on_notice_link)
	nr.add_child(_notice_link)

	var grid := GridContainer.new()
	grid.columns = 7
	grid.add_theme_constant_override("h_separation", 12)
	grid.add_theme_constant_override("v_separation", 12)
	col.add_child(grid)
	for spec in [
		["duration", "Durée", ""], ["distance", "Distance", "km"], ["gain", "Dénivelé +", "m"],
		["speed", "Vitesse moy.", "km/h"], ["speed_max", "Vitesse max", "km/h"], ["power", "Puissance moy.", "W"],
		["power_max", "Puissance max", "W"], ["np", "Puissance normalisée", "W"], ["if", "Intensité (IF)", ""],
		["tss", "Charge (TSS)", ""], ["work", "Travail", "kJ"], ["cadence", "Cadence moy.", "rpm"],
		["hr", "FC moy.", "bpm"], ["hr_max", "FC max", "bpm"],
	]:
		var t := UI.metric_tile(spec[1], "--", spec[2])
		t.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		grid.add_child(t)
		_tiles[spec[0]] = t

	var charts := UI.hbox(18)
	charts.size_flags_vertical = Control.SIZE_EXPAND_FILL
	col.add_child(charts)
	_time_chart = Chart.new()
	_time_chart.x_format = "temps"
	_time_chart.y_format = "watts"
	_time_chart.y2_format = "m"
	_time_chart.empty_text = "Pas de données enregistrées pour cette sortie."
	var c1 := _chart_card(charts, "Puissance et altitude", _time_chart)
	c1.size_flags_stretch_ratio = 1.8
	_curve_chart = Chart.new()
	_curve_chart.x_log = true
	_curve_chart.x_format = "duree"
	_curve_chart.y_format = "watts"
	_curve_chart.y_from_zero = false
	_curve_chart.empty_text = "Pas de données de puissance."
	_chart_card(charts, "Courbe de puissance", _curve_chart)
	_built = true
	if _id >= 0:
		_fill()


func _chart_card(parent: Control, title: String, chart: Control) -> PanelContainer:
	var card := UI.card()
	card.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	card.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var v := UI.vbox(8)
	card.add_child(v)
	v.add_child(UI.section_title(title))
	chart.size_flags_vertical = Control.SIZE_EXPAND_FILL
	v.add_child(chart)
	parent.add_child(card)
	return card


func _fill() -> void:
	_ride = App.history.ride(_id)
	for c in find_children("*", "Button", true, false):
		if c.has_meta("only_when_finished"):
			c.visible = _just
	if _ride.is_empty():
		_kicker.text = ""
		_title.text = "Sortie introuvable"
		_subtitle.text = "Elle a peut-être été supprimée."
		_time_chart.set_series([])
		_curve_chart.set_series([])
		return
	var r := _ride
	_kicker.text = "SORTIE TERMINÉE  ·  BRAVO !" if _just else ""
	_kicker.visible = _just
	var route: String = r.get("route_name", "")
	var name_: String = r.get("name", "")
	_title.text = route if route != "" else "Sortie"
	var sub := "%s à %s  ·  %s" % [Fmt.date_long(r.started_unix), Fmt.clock(r.started_unix), Fmt.mode(r.get("mode", ""))]
	if name_ != "":
		sub += " : " + name_
	_subtitle.text = sub

	UI.set_tile(_tiles.duration, Fmt.hms(r.duration_s))
	UI.set_tile(_tiles.distance, Fmt.num(r.distance_m / 1000.0, 1))
	UI.set_tile(_tiles.gain, Fmt.num(r.elevation_gain_m))
	UI.set_tile(_tiles.speed, Fmt.num(r.avg_speed_kmh, 1))
	UI.set_tile(_tiles.speed_max, Fmt.num(r.max_speed_kmh, 1))
	UI.set_tile(_tiles.power, Fmt.opt(r.avg_power_w))
	UI.set_tile(_tiles.power_max, Fmt.opt(r.max_power_w))
	UI.set_tile(_tiles.np, Fmt.opt(r.np_w))
	UI.set_tile(_tiles["if"], Fmt.num(r["if"], 2) if float(r["if"]) > 0.0 else "--")
	UI.set_tile(_tiles.tss, Fmt.opt(r.tss))
	UI.set_tile(_tiles.work, Fmt.opt(r.work_kj))
	UI.set_tile(_tiles.cadence, Fmt.opt(r.avg_cadence))
	UI.set_tile(_tiles.hr, Fmt.opt(r.avg_hr))
	UI.set_tile(_tiles.hr_max, Fmt.opt(r.max_hr))

	var step := maxf(1.0, ceilf(float(r.duration_s) / 900.0))
	var s: Dictionary = App.history.ride_series(_id, step)
	var t: PackedFloat32Array = s.get("t", PackedFloat32Array())
	var pw: PackedFloat32Array = s.get("power", PackedFloat32Array())
	var el: PackedFloat32Array = s.get("elevation", PackedFloat32Array())
	var power := PackedVector2Array()
	var elev := PackedVector2Array()
	for i in t.size():
		if i < pw.size() and pw[i] >= 0.0:
			power.append(Vector2(t[i], pw[i]))
		if i < el.size():
			elev.append(Vector2(t[i], el[i]))
	var series := []
	if not elev.is_empty():
		series.append({"name": "Altitude", "color": ELEVATION, "kind": "area", "axis": "y2", "points": elev, "fill_alpha": 0.22})
	if not power.is_empty():
		series.append({"name": "Puissance", "color": UI.ACCENT, "kind": "line", "points": power, "width": 1.5})
	_time_chart.set_series(series)
	var ftp := float(r.get("ftp_w", 0.0))
	_time_chart.set_reference_lines([{"y": ftp, "color": Color(1, 1, 1, 0.45), "label": "FTP %s W" % Fmt.num(ftp)}] if ftp > 0.0 else [])

	var mine: PackedVector2Array = App.history.ride_curve(_id)
	var best: PackedVector2Array = App.history.best_curve(90)
	var cs := []
	if not best.is_empty():
		cs.append({"name": "Meilleure sur 90 jours", "color": CURVE_90, "kind": "line", "points": best, "width": 1.5})
	if not mine.is_empty():
		cs.append({"name": "Cette sortie", "color": UI.ACCENT, "kind": "line", "points": mine, "width": 2.5})
	_curve_chart.set_series(cs)

	_refresh_strava()
	var st: Dictionary = App.strava.status()
	if _just and st.get("connected", false) and App.ride.profile_get("strava.auto_upload") == true and int(r.get("strava_activity_id", 0)) == 0 and not st.get("busy", false):
		_upload()


# --- Strava -----------------------------------------------------------------------------

func _upload() -> void:
	App.strava.upload_ride(_id)
	_uploading = true
	_poll = 0.0
	_refresh_strava()


func _process(delta: float) -> void:
	if not _uploading:
		return
	_poll -= delta
	if _poll <= 0.0:
		_poll = 0.5
		_refresh_strava()


func _refresh_strava() -> void:
	var st: Dictionary = App.strava.status()
	var connected: bool = st.get("connected", false)
	var busy: bool = st.get("busy", false)
	var sent := int(_ride.get("strava_activity_id", 0)) != 0
	_strava_b.disabled = not connected or busy or sent
	_strava_b.text = "Déjà sur Strava" if sent else ("Envoi…" if busy and _uploading else "Envoyer sur Strava")
	_strava_b.tooltip_text = "" if connected else "Connecte ton compte Strava dans Paramètres › Strava."

	if _uploading and busy:
		var p: float = st.get("progress", -1.0)
		var m: String = st.get("message", "")
		_show_notice(m if m != "" else "Envoi sur Strava…", UI.WARN, p if p >= 0.0 else -2.0, "", "")
		return
	if _uploading:
		_uploading = false
		var url: String = st.get("activity_url", "")
		var m: String = st.get("message", "")
		if url != "":
			_ride = App.history.ride(_id)
			_strava_b.disabled = true
			_strava_b.text = "Déjà sur Strava"
			_show_notice(m if m != "" else "Sortie envoyée sur Strava.", STRAVA, -1.0, "Voir sur Strava  ↗", url)
		else:
			_show_notice(m if m != "" else "L'envoi sur Strava a échoué.", UI.ERROR, -1.0, "", "")
		return
	if sent:
		_show_notice("Sortie déjà envoyée sur Strava.", STRAVA, -1.0, "Voir sur Strava  ↗", "https://www.strava.com/activities/%d" % int(_ride.strava_activity_id))
	elif not connected:
		_show_notice("Connecte ton compte Strava pour y envoyer tes sorties.", UI.TEXT_MUTED, -1.0, "Ouvrir les paramètres", "settings")
	else:
		_notice.visible = false


## Bandeau d'information : progress = -1 sans barre, -2 barre indéterminée.
func _show_notice(text: String, color: Color, progress: float, link: String, url: String) -> void:
	_notice.visible = true
	UI.set_dot(_notice_dot, color)
	_notice_label.text = text
	_notice_bar.visible = progress > -1.5 and progress != -1.0
	_notice_bar.value = progress if progress >= 0.0 else 0.5
	_notice_link.visible = link != ""
	_notice_link.text = link
	_notice_url = url


func _on_notice_link() -> void:
	if _notice_url == "settings":
		menu.go("settings")
		var s: Node = menu._screen("settings")
		if s.has_method("select_tab"):
			s.select_tab(4)
	elif _notice_url != "":
		OS.shell_open(_notice_url)


# --- Export et suppression -------------------------------------------------------------------

func _export_fit() -> void:
	var dialog := FileDialog.new()
	dialog.file_mode = FileDialog.FILE_MODE_SAVE_FILE
	dialog.access = FileDialog.ACCESS_FILESYSTEM
	dialog.filters = PackedStringArray(["*.fit ; Fichier FIT"])
	dialog.use_native_dialog = true
	dialog.title = "Exporter la sortie"
	var d := Time.get_datetime_dict_from_unix_time(int(_ride.get("started_unix", 0)))
	dialog.current_file = "velotrace-%04d-%02d-%02d.fit" % [d.year, d.month, d.day]
	dialog.file_selected.connect(func(path: String):
		if not path.to_lower().ends_with(".fit"):
			path += ".fit"
		if App.history.export_fit(_id, path):
			_show_notice("Sortie exportée : %s" % path, UI.OK, -1.0, "", "")
		else:
			_show_notice("L'export a échoué.", UI.ERROR, -1.0, "", ""))
	dialog.visibility_changed.connect(func():
		if not dialog.visible:
			dialog.queue_free())
	add_child(dialog)
	dialog.popup_centered_ratio(0.7)


func _confirm_delete() -> void:
	var dialog := ConfirmationDialog.new()
	dialog.title = "Supprimer la sortie"
	dialog.dialog_text = "Cette sortie sera effacée de l'historique et de tes courbes.\nSi elle a été envoyée sur Strava, elle y restera."
	dialog.ok_button_text = "Supprimer"
	dialog.cancel_button_text = "Annuler"
	dialog.confirmed.connect(func():
		if App.history.delete_ride(_id):
			if _just:
				home_requested.emit()
			else:
				closed.emit()
		else:
			_show_notice("La sortie n'a pas pu être supprimée.", UI.ERROR, -1.0, "", ""))
	dialog.visibility_changed.connect(func():
		if not dialog.visible:
			dialog.queue_free())
	add_child(dialog)
	dialog.popup_centered(Vector2i(520, 0))
	dialog.get_cancel_button().grab_focus()
