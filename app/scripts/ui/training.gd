extends MarginContainer
## Choix de l'entraînement : sortie libre, séance paramétrée, coureurs virtuels
## ou séance test, puis lancement de la sortie.
##
## La configuration transmise par `menu.launch(cfg)` :
##   mode : « libre », « seance », « adversaires » ou « test » ;
##   workout : la séance (même forme que `workout_presets()`), {} hors séance ;
##   erg : bool ; intensity : 1.0 = 100 % ;
##   opponents : {count, w_per_kg, spread} — spread est l'écart entre deux
##     coureurs consécutifs, répartis autour de w_per_kg ;
##   test_rider : « cadence » ou « puissance » ; test_value : rpm ou W.

const UI := preload("res://scripts/ui/theme.gd")
const Fmt := preload("res://scripts/ui/fmt.gd")
const Chart := preload("res://scripts/ui/chart.gd")
const NumberField := preload("res://scripts/ui/number_field.gd")
const WorkoutView := preload("res://scripts/ui/workout_view.gd")
const Home := preload("res://scripts/ui/home.gd")

const MODES := [
	["libre", "Sortie libre", "Roule à ton rythme : le home trainer reproduit les pentes du parcours."],
	["seance", "Séance paramétrée", "Des intervalles en % de ta FTP, avec le mode ERG si tu le souhaites."],
	["adversaires", "Contre des coureurs virtuels", "Des coureurs roulent au rapport poids/puissance que tu choisis."],
	["test", "Séance test", "Appareils simulés : un cycliste virtuel pédale pour toi."],
]
const CUSTOM_NAME := "Ma séance"

var menu: Node

var _mode := "libre"
var _cards := {}
var _panels := {}
var _route_label: Label
var _status_box: PanelContainer
var _status_label: Label
var _launch: Button
var _route_charts: Array = []

# Séance.
var _presets: Array = []
var _imported: Array = []
var _custom: Dictionary = {}
var _entries: Array = []
var _list: ItemList
var _workout: Dictionary = {}
var _wv: Control
var _w_name: Label
var _w_name_edit: LineEdit
var _legend: Control
var _w_desc: Label
var _w_dur: Label
var _w_copy: Button
var _editor: Control
var _steps_box: VBoxContainer
var _erg: CheckButton
var _intensity: HSlider
var _intensity_label: Label

# Coureurs virtuels.
var _opp_count := 4
var _opp_wkg: HSlider
var _opp_spread: HSlider
var _opp_wkg_label: Label
var _opp_spread_label: Label
var _opp_summary: Label
var _opp_chart: Control

# Test.
var _test_rider := "cadence"
var _test_value: LineEdit
var _test_unit: Label
var _test_help: Label
var _test_seg: HBoxContainer


func _ready() -> void:
	for side in ["left", "right"]:
		add_theme_constant_override("margin_" + side, 32)
	add_theme_constant_override("margin_top", 22)
	add_theme_constant_override("margin_bottom", 26)
	var col := UI.vbox(18)
	add_child(col)

	var head := UI.hbox(14)
	col.add_child(head)
	var back := UI.button("‹  Retour", "GhostButton")
	back.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	back.pressed.connect(func(): menu.back())
	head.add_child(back)
	var hv := UI.vbox(0)
	hv.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	hv.add_child(UI.label("Choisis ton entraînement", "H1"))
	_route_label = UI.label("", "DimLabel")
	hv.add_child(_route_label)
	head.add_child(hv)

	var cards := UI.hbox(16)
	col.add_child(cards)
	var group := ButtonGroup.new()
	for m in MODES:
		cards.add_child(_mode_card(m[0], m[1], m[2], group))

	var options := UI.card()
	options.size_flags_vertical = Control.SIZE_EXPAND_FILL
	col.add_child(options)
	var stack := Control.new()
	stack.size_flags_vertical = Control.SIZE_EXPAND_FILL
	options.add_child(stack)
	_panels["libre"] = _build_free()
	_panels["seance"] = _build_workout()
	_panels["adversaires"] = _build_opponents()
	_panels["test"] = _build_test()
	for k in _panels:
		var p: Control = _panels[k]
		p.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		stack.add_child(p)

	var bottom := UI.hbox(18)
	col.add_child(bottom)
	_status_box = UI.card("WarnBox")
	_status_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_status_box.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_status_label = UI.label("", "WarnLabel", true)
	_status_box.add_child(_status_label)
	bottom.add_child(_status_box)
	_launch = UI.primary_button("Lancer la sortie  ▸", true)
	_launch.custom_minimum_size.x = 320
	_launch.pressed.connect(_on_launch)
	bottom.add_child(_launch)


func _mode_card(mode: String, title: String, desc: String, group: ButtonGroup) -> Button:
	var b := UI.button("", "ChoiceCard")
	b.toggle_mode = true
	b.button_group = group
	b.custom_minimum_size = Vector2(0, 104)
	b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	b.pressed.connect(select_mode.bind(mode))
	var m := UI.margin(18, 14)
	m.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	m.mouse_filter = Control.MOUSE_FILTER_IGNORE
	b.add_child(m)
	var v := UI.vbox(6)
	v.mouse_filter = Control.MOUSE_FILTER_IGNORE
	m.add_child(v)
	var t := UI.label(title, "H3")
	t.mouse_filter = Control.MOUSE_FILTER_IGNORE
	v.add_child(t)
	var d := UI.label(desc, "MutedLabel", true)
	d.add_theme_font_size_override("font_size", 14)
	d.mouse_filter = Control.MOUSE_FILTER_IGNORE
	v.add_child(d)
	_cards[mode] = b
	return b


func enter() -> void:
	if App.ride.has_route():
		_route_label.text = "%s · %s · D+ %s m" % [App.ride.route_name(), Fmt.km(App.ride.route_length_m()), Fmt.num(App.ride.route_elevation_gain_m())]
		var pts := Home.profile_points(App.ride.route_points(), App.ride.route_distances())
		for c in _route_charts:
			c.set_series([{"name": "Altitude", "color": UI.SECOND, "kind": "area", "points": pts}])
	else:
		_route_label.text = "Aucun parcours choisi"
	_presets = App.ride.workout_presets()
	_fill_list()
	var r: Variant = App.ride.profile_get("app.test_rider")
	_test_rider = r if r is String and r in ["cadence", "puissance"] else "cadence"
	var tv: Variant = App.ride.profile_get("app.test_value")
	_set_test_rider(_test_rider, float(tv) if tv != null and float(tv) > 0.0 else -1.0)
	_update_opponents()
	_refresh_free_info()
	select_mode(_mode)


## Affiche les options d'un mode : « libre », « seance », « adversaires », « test ».
func select_mode(mode: String) -> void:
	_mode = mode
	for k in _panels:
		_panels[k].visible = k == mode
		_cards[k].set_pressed_no_signal(k == mode)
	_refresh_status()


func _ftp() -> float:
	var f: Variant = App.ride.profile_get("rider.ftp_w")
	return float(f) if f != null and float(f) > 0.0 else 200.0


func _weight() -> float:
	var w: Variant = App.ride.profile_get("rider.weight_kg")
	return float(w) if w != null and float(w) > 0.0 else 70.0


# --- Sortie libre --------------------------------------------------------------------

var _free_info: VBoxContainer


func _build_free() -> Control:
	var row := UI.hbox(28)
	var left := UI.vbox(12)
	left.custom_minimum_size.x = 430
	row.add_child(left)
	left.add_child(UI.label("Sortie libre", "H2"))
	left.add_child(UI.label("Tu roules à ton rythme sur tout le parcours. Le home trainer fait ressentir chaque pente ; change de vitesse avec les Click ou les flèches ↑ ↓.", "DimLabel", true))
	_free_info = UI.vbox(8)
	left.add_child(_free_info)
	left.add_child(UI.spacer())
	left.add_child(UI.label("Ces réglages se modifient dans Paramètres, ou en pause pendant la sortie.", "MutedLabel", true))
	row.add_child(_route_chart())
	return row


func _refresh_free_info() -> void:
	for c in _free_info.get_children():
		c.queue_free()
	var diff: Variant = App.ride.profile_get("trainer.grade_difficulty")
	var cam: Variant = App.ride.profile_get("app.camera")
	var items := [
		["Difficulté des pentes", Fmt.percent(float(diff) if diff != null else 1.0)],
		["Click automatiques", "Oui" if App.ride.profile_get("app.auto_shift") == true else "Non"],
		["Caméra au départ", ["Proche", "Arrière", "De côté", "Haute"][clampi(int(cam) if cam != null else 0, 0, 3)]],
		["FTP", Fmt.watts(_ftp()) + ("" if App.ride.profile_get("rider.ftp_w") else " (par défaut)")],
	]
	for it in items:
		var r := UI.hbox(10)
		var a := UI.label(it[0], "DimLabel")
		a.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		r.add_child(a)
		r.add_child(UI.label(it[1], "H3"))
		_free_info.add_child(r)


func _route_chart() -> Control:
	var box := UI.vbox(8)
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	box.add_child(UI.section_title("Profil du parcours"))
	var c := Chart.new()
	c.x_format = "km"
	c.y_format = "m"
	c.y_from_zero = false
	c.show_legend = false
	c.empty_text = "Aucun parcours choisi"
	c.size_flags_vertical = Control.SIZE_EXPAND_FILL
	box.add_child(c)
	_route_charts.append(c)
	return box


# --- Séance ---------------------------------------------------------------------------

func _build_workout() -> Control:
	var row := UI.hbox(24)
	var left := UI.vbox(10)
	left.custom_minimum_size.x = 330
	row.add_child(left)
	left.add_child(UI.section_title("Séances"))
	_list = ItemList.new()
	_list.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_list.item_selected.connect(_on_list_selected)
	left.add_child(_list)
	var br := UI.hbox(8)
	left.add_child(br)
	var imp := UI.button("Importer un .zwo…")
	imp.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	imp.tooltip_text = "Fichier de séance Zwift (.zwo)"
	imp.pressed.connect(_open_zwo)
	br.add_child(imp)
	var new_b := UI.button("Nouvelle séance")
	new_b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	new_b.pressed.connect(new_custom)
	br.add_child(new_b)

	var right := UI.vbox(12)
	right.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(right)
	var head := UI.hbox(16)
	right.add_child(head)
	var tv := UI.vbox(2)
	tv.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_w_name = UI.label("", "H2")
	tv.add_child(_w_name)
	_w_name_edit = LineEdit.new()
	_w_name_edit.add_theme_font_override("font", UI.font(600))
	_w_name_edit.add_theme_font_size_override("font_size", 18)
	_w_name_edit.custom_minimum_size.x = 360
	_w_name_edit.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	_w_name_edit.text_changed.connect(func(t: String):
		_custom["name"] = t if t.strip_edges() != "" else CUSTOM_NAME
		var i := _entries.find(_custom)
		if i >= 0:
			_list.set_item_text(i, "%s   ·   %s" % [_custom.name, Fmt.duration(WorkoutView.total_duration(_custom))]))
	tv.add_child(_w_name_edit)
	_w_desc = UI.label("", "DimLabel", true)
	tv.add_child(_w_desc)
	head.add_child(tv)
	_w_copy = UI.button("Modifier une copie", "GhostButton")
	_w_copy.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	_w_copy.pressed.connect(func(): _edit_copy(_workout))
	head.add_child(_w_copy)
	var dur := UI.card("Tile")
	dur.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	var dv := UI.vbox(0)
	dur.add_child(dv)
	dv.add_child(UI.section_title("Durée"))
	_w_dur = UI.label("", "H2")
	dv.add_child(_w_dur)
	head.add_child(dur)

	_wv = WorkoutView.new()
	_wv.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_wv.custom_minimum_size.y = 120
	right.add_child(_wv)
	var legend := UI.hbox(14)
	for z in [["Z1 récup", 0.5], ["Z2 endurance", 0.65], ["Z3 tempo", 0.8], ["Z4 seuil", 0.95], ["Z5 VO2max", 1.1], ["Z6 anaérobie", 1.3]]:
		var it := UI.hbox(6)
		var sw := ColorRect.new()
		sw.color = UI.zone_color(z[1])
		sw.custom_minimum_size = Vector2(10, 10)
		sw.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		it.add_child(sw)
		it.add_child(UI.label(z[0], "MutedLabel"))
		legend.add_child(it)
	right.add_child(legend)
	_legend = legend

	_editor = UI.vbox(8)
	_editor.size_flags_vertical = Control.SIZE_EXPAND_FILL
	right.add_child(_editor)
	var eh := UI.hbox(8)
	_editor.add_child(eh)
	for spec in [["", 30], ["Durée (m:s)", 96], ["Début", 84], ["", 16], ["Fin", 84], ["Cadence", 84]]:
		var l := UI.label(spec[0], "MutedLabel")
		l.custom_minimum_size.x = spec[1]
		l.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		eh.add_child(l)
	eh.add_child(UI.label("en % de FTP ; cadence 0 = libre", "MutedLabel"))
	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_editor.add_child(scroll)
	_steps_box = UI.vbox(6)
	_steps_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(_steps_box)
	var add := UI.button("+  Ajouter une étape", "LinkButtonish")
	add.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	add.pressed.connect(func():
		var last: Dictionary = _custom.steps[-1] if not _custom.steps.is_empty() else {}
		_custom.steps.append({"duration_s": 300.0, "from": last.get("to", 0.75), "to": last.get("to", 0.75), "free": false, "cadence_rpm": 0.0, "label": ""})
		_rebuild_steps())
	_editor.add_child(add)

	var bottom := UI.hbox(18)
	right.add_child(bottom)
	_erg = CheckButton.new()
	_erg.text = "Mode ERG : le home trainer impose la puissance"
	_erg.button_pressed = true
	bottom.add_child(_erg)
	bottom.add_child(UI.spacer())
	var il := UI.label("Intensité", "DimLabel")
	il.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	bottom.add_child(il)
	_intensity = HSlider.new()
	_intensity.min_value = 0.5
	_intensity.max_value = 1.5
	_intensity.step = 0.01
	_intensity.value = 1.0
	_intensity.custom_minimum_size = Vector2(220, 28)
	_intensity.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_intensity.value_changed.connect(func(v: float):
		_intensity_label.text = Fmt.percent(v)
		_wv.intensity = v
		_wv.queue_redraw())
	bottom.add_child(_intensity)
	_intensity_label = UI.label("100 %", "H3")
	_intensity_label.custom_minimum_size.x = 56
	_intensity_label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	bottom.add_child(_intensity_label)
	return row


func _fill_list() -> void:
	var keep := _workout
	_list.clear()
	_entries = []
	for w in _presets:
		_entries.append(w)
	for w in _imported:
		_entries.append(w)
	if not _custom.is_empty():
		_entries.append(_custom)
	for w in _entries:
		var label := "%s   ·   %s" % [w.get("name", "Séance"), Fmt.duration(WorkoutView.total_duration(w))]
		var i := _list.add_item(label)
		_list.set_item_tooltip(i, w.get("description", ""))
	var idx := _entries.find(keep)
	if idx < 0 and not _entries.is_empty():
		idx = 0
	if idx >= 0:
		_list.select(idx)
		_select_workout(_entries[idx])


func _on_list_selected(i: int) -> void:
	_select_workout(_entries[i])


func _select_workout(w: Dictionary) -> void:
	_workout = w
	var custom := w == _custom and not _custom.is_empty()
	_w_name.text = w.get("name", "Séance")
	_w_name.visible = not custom
	_w_name_edit.visible = custom
	if custom:
		_w_name_edit.text = w.get("name", CUSTOM_NAME)
	_legend.visible = not custom
	_w_desc.text = w.get("description", "")
	_w_desc.visible = not custom and _w_desc.text != ""
	_w_copy.visible = not custom
	_editor.visible = custom
	_wv.size_flags_vertical = Control.SIZE_FILL if custom else Control.SIZE_EXPAND_FILL
	_wv.custom_minimum_size.y = 104 if custom else 120
	_wv.ftp_w = _ftp()
	_wv.set_workout(w)
	_update_duration()
	if custom:
		_rebuild_steps()
	_refresh_status()


func _update_duration() -> void:
	var total := WorkoutView.total_duration(_workout)
	_workout["duration_s"] = total
	_w_dur.text = Fmt.hms(total)


## Crée une séance personnalisée à partir de rien.
func new_custom() -> void:
	_edit_copy({"name": CUSTOM_NAME, "description": "", "steps": [
		{"duration_s": 600.0, "from": 0.45, "to": 0.70, "free": false, "cadence_rpm": 0.0, "label": "Échauffement"},
		{"duration_s": 480.0, "from": 0.90, "to": 0.90, "free": false, "cadence_rpm": 90.0, "label": ""},
		{"duration_s": 240.0, "from": 0.55, "to": 0.55, "free": false, "cadence_rpm": 0.0, "label": ""},
		{"duration_s": 480.0, "from": 0.90, "to": 0.90, "free": false, "cadence_rpm": 90.0, "label": ""},
		{"duration_s": 300.0, "from": 0.55, "to": 0.40, "free": false, "cadence_rpm": 0.0, "label": "Retour au calme"},
	]})


func _edit_copy(w: Dictionary) -> void:
	_custom = w.duplicate(true)
	_custom["name"] = CUSTOM_NAME if w.get("name", "") in ["", CUSTOM_NAME] else "%s (modifiée)" % w.name
	_custom["description"] = "Séance personnalisée."
	_workout = _custom
	_fill_list()


func _rebuild_steps() -> void:
	for c in _steps_box.get_children():
		c.queue_free()
	var steps: Array = _custom.steps
	for i in steps.size():
		_steps_box.add_child(_step_row(i, steps[i]))
	_wv.set_workout(_custom)
	_update_duration()
	_refresh_status()


func _step_row(i: int, s: Dictionary) -> Control:
	var row := UI.hbox(8)
	var n := UI.label("%d" % (i + 1), "MutedLabel")
	n.custom_minimum_size.x = 30
	n.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	row.add_child(n)
	var dur: LineEdit = NumberField.new()
	dur.as_duration = true
	dur.setup(10.0, 4 * 3600.0, 30.0, 0, s.duration_s, 96.0)
	dur.value_changed.connect(func(v: float):
		s.duration_s = v
		_after_step_edit())
	row.add_child(dur)
	var from: LineEdit = NumberField.new()
	from.setup(10.0, 300.0, 5.0, 0, s.from * 100.0, 84.0)
	row.add_child(from)
	var arrow := UI.label("→", "MutedLabel")
	arrow.custom_minimum_size.x = 16
	row.add_child(arrow)
	var to: LineEdit = NumberField.new()
	to.setup(10.0, 300.0, 5.0, 0, s.to * 100.0, 84.0)
	row.add_child(to)
	from.value_changed.connect(func(v: float):
		# Une étape constante suit son début.
		if is_equal_approx(s.from, s.to):
			s.to = v / 100.0
			to.set_value(v)
		s.from = v / 100.0
		_after_step_edit())
	to.value_changed.connect(func(v: float):
		s.to = v / 100.0
		_after_step_edit())
	var cad: LineEdit = NumberField.new()
	cad.setup(0.0, 150.0, 5.0, 0, s.get("cadence_rpm", 0.0), 84.0)
	cad.value_changed.connect(func(v: float):
		s.cadence_rpm = v
		_after_step_edit())
	row.add_child(cad)
	var zone := ColorRect.new()
	zone.color = UI.zone_color((s.from + s.to) * 0.5)
	zone.custom_minimum_size = Vector2(6, 26)
	zone.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	row.add_child(zone)
	row.add_child(UI.spacer())
	for spec in [["↑", "Monter", -1], ["↓", "Descendre", 1]]:
		var b := UI.button(spec[0], "GhostButton", spec[1])
		var j: int = i + spec[2]
		b.disabled = j < 0 or j >= _custom.steps.size()
		b.pressed.connect(func():
			var tmp = _custom.steps[i]
			_custom.steps[i] = _custom.steps[j]
			_custom.steps[j] = tmp
			_rebuild_steps())
		row.add_child(b)
	var del := UI.button("×", "GhostButton", "Supprimer l'étape")
	del.add_theme_color_override("font_hover_color", UI.ERROR)
	del.disabled = _custom.steps.size() <= 1
	del.pressed.connect(func():
		_custom.steps.remove_at(i)
		_rebuild_steps())
	row.add_child(del)
	return row


func _after_step_edit() -> void:
	_wv.queue_redraw()
	_update_duration()
	_list.set_item_text(_entries.find(_custom), "%s   ·   %s" % [_custom.name, Fmt.duration(WorkoutView.total_duration(_custom))])
	for i in _steps_box.get_child_count():
		var row := _steps_box.get_child(i)
		var s: Dictionary = _custom.steps[i]
		for c in row.get_children():
			if c is ColorRect:
				c.color = UI.zone_color((s.from + s.to) * 0.5)


func _open_zwo() -> void:
	var dialog := FileDialog.new()
	dialog.file_mode = FileDialog.FILE_MODE_OPEN_FILE
	dialog.access = FileDialog.ACCESS_FILESYSTEM
	dialog.filters = PackedStringArray(["*.zwo ; Séances Zwift"])
	dialog.use_native_dialog = true
	dialog.title = "Importer une séance"
	dialog.file_selected.connect(func(path: String):
		var w: Dictionary = App.ride.workout_from_zwo(path)
		if w.is_empty():
			var m: String = App.ride.error_message()
			_show_status(m if m != "" else "Ce fichier .zwo n'a pas pu être lu.", "error")
			return
		_imported.append(w)
		_workout = w
		_fill_list())
	dialog.visibility_changed.connect(func():
		if not dialog.visible:
			dialog.queue_free())
	add_child(dialog)
	dialog.popup_centered_ratio(0.7)


# --- Coureurs virtuels ---------------------------------------------------------------

func _build_opponents() -> Control:
	var row := UI.hbox(28)
	var left := UI.vbox(16)
	left.custom_minimum_size.x = 560
	row.add_child(left)
	left.add_child(UI.label("Contre des coureurs virtuels", "H2"))
	left.add_child(UI.label("Les coureurs partent avec toi et gardent leur rapport poids/puissance : à toi de suivre le rythme ou de les lâcher dans les bosses.", "DimLabel", true))

	left.add_child(UI.section_title("Nombre de coureurs"))
	var counts := []
	for i in 8:
		counts.append(str(i + 1))
	left.add_child(UI.segmented(counts, _opp_count - 1, func(i: int):
		_opp_count = i + 1
		_update_opponents()))

	left.add_child(UI.section_title("Rapport poids/puissance"))
	var r1 := UI.hbox(14)
	_opp_wkg = _slider(1.5, 6.0, 0.1, 3.0)
	r1.add_child(_opp_wkg)
	_opp_wkg_label = UI.label("", "H3")
	_opp_wkg_label.custom_minimum_size.x = 200
	r1.add_child(_opp_wkg_label)
	left.add_child(r1)

	left.add_child(UI.section_title("Écart entre coureurs"))
	var r2 := UI.hbox(14)
	_opp_spread = _slider(0.0, 1.0, 0.05, 0.3)
	r2.add_child(_opp_spread)
	_opp_spread_label = UI.label("", "H3")
	_opp_spread_label.custom_minimum_size.x = 200
	r2.add_child(_opp_spread_label)
	left.add_child(r2)
	_opp_wkg.value_changed.connect(func(_v): _update_opponents())
	_opp_spread.value_changed.connect(func(_v): _update_opponents())
	_opp_summary = UI.label("", "DimLabel", true)
	left.add_child(_opp_summary)
	left.add_child(UI.spacer())
	left.add_child(UI.label("Repère : ta FTP rapportée à ton poids. Un coureur au-dessus de ce trait te fera souffrir dans les montées.", "MutedLabel", true))

	var right := UI.vbox(8)
	right.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	right.add_child(UI.section_title("Les coureurs"))
	_opp_chart = Chart.new()
	_opp_chart.x_format = func(v: float, _long: bool) -> String: return "Coureur %d" % roundi(v)
	_opp_chart.y_format = "wkg"
	_opp_chart.show_legend = false
	_opp_chart.size_flags_vertical = Control.SIZE_EXPAND_FILL
	right.add_child(_opp_chart)
	row.add_child(right)
	return row


func _slider(lo: float, hi: float, step: float, value: float) -> HSlider:
	var s := HSlider.new()
	s.min_value = lo
	s.max_value = hi
	s.step = step
	s.value = value
	s.custom_minimum_size = Vector2(320, 28)
	s.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	return s


## W/kg de chaque coureur, répartis autour de la valeur choisie.
func opponent_values() -> Array:
	var out := []
	for i in _opp_count:
		out.append(_opp_wkg.value + _opp_spread.value * (i - (_opp_count - 1) * 0.5))
	return out


func _update_opponents() -> void:
	if not _opp_wkg:
		return
	var kg := _weight()
	_opp_wkg_label.text = "%s W/kg  ·  %s W" % [Fmt.num(_opp_wkg.value, 1), Fmt.num(_opp_wkg.value * kg)]
	_opp_spread_label.text = "%s W/kg" % Fmt.num(_opp_spread.value, 2)
	_opp_spread.editable = _opp_count > 1
	var vals := opponent_values()
	if _opp_count == 1:
		_opp_summary.text = "Un coureur à %s W/kg, soit %s W pour tes %s kg." % [Fmt.num(vals[0], 1), Fmt.num(vals[0] * kg), Fmt.num(kg, 1)]
	else:
		_opp_summary.text = "De %s à %s W/kg, soit %s à %s W pour tes %s kg." % [Fmt.num(vals[0], 2), Fmt.num(vals[-1], 2), Fmt.num(vals[0] * kg), Fmt.num(vals[-1] * kg), Fmt.num(kg, 1)]
	var pts := PackedVector2Array()
	var cols := PackedColorArray()
	for i in vals.size():
		pts.append(Vector2(i + 1, vals[i]))
		cols.append(UI.SECOND.darkened(0.35 * (1.0 - float(i) / maxf(1.0, vals.size() - 1))))
	_opp_chart.set_series([{"name": "Coureurs", "color": UI.SECOND, "kind": "bar", "points": pts, "colors": cols}])
	var me := _ftp() / kg
	_opp_chart.set_reference_lines([{"y": me, "color": UI.TEXT_DIM, "label": "Ta FTP : %s W/kg" % Fmt.num(me, 1)}])


# --- Séance test -----------------------------------------------------------------------

func _build_test() -> Control:
	var row := UI.hbox(28)
	var left := UI.vbox(16)
	left.custom_minimum_size.x = 560
	row.add_child(left)
	left.add_child(UI.label("Séance test (appareils simulés)", "H2"))
	left.add_child(UI.label("Aucun appareil n'est utilisé : un cycliste virtuel pédale pour toi. Pratique pour découvrir un parcours, le décor ou l'application.", "DimLabel", true))
	left.add_child(UI.section_title("Cycliste virtuel"))
	_test_seg = UI.segmented(["Cadence imposée, Click automatiques", "Puissance imposée"], 0, func(i: int):
		_set_test_rider("cadence" if i == 0 else "puissance", -1.0))
	left.add_child(_test_seg)
	var vr := UI.hbox(12)
	_test_value = NumberField.new()
	_test_value.setup(40.0, 130.0, 1.0, 0, 88.0, 110.0)
	vr.add_child(_test_value)
	_test_unit = UI.label("rpm", "UnitLabel")
	_test_unit.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	vr.add_child(_test_unit)
	left.add_child(vr)
	_test_help = UI.label("", "MutedLabel", true)
	left.add_child(_test_help)
	left.add_child(UI.spacer())
	left.add_child(keys_help())
	row.add_child(_route_chart())
	return row


## Rappel des touches utiles pendant la sortie.
static func keys_help() -> Control:
	var box := UI.vbox(6)
	box.add_child(UI.section_title("Pendant la sortie"))
	var grid := GridContainer.new()
	grid.columns = 2
	grid.add_theme_constant_override("h_separation", 16)
	grid.add_theme_constant_override("v_separation", 4)
	for k in [["← →", "effort du cycliste virtuel"], ["↑ ↓", "vitesses"], ["A", "Click automatiques"], ["C", "caméra"], ["Espace", "pause et réglages"]]:
		var key := UI.label(k[0], "AccentLabel")
		key.custom_minimum_size.x = 60
		grid.add_child(key)
		grid.add_child(UI.label(k[1], "DimLabel"))
	box.add_child(grid)
	return box


func _set_test_rider(kind: String, value: float) -> void:
	_test_rider = kind
	var cadence := kind == "cadence"
	for i in _test_seg.get_child_count():
		(_test_seg.get_child(i) as Button).set_pressed_no_signal(i == (0 if cadence else 1))
	if cadence:
		_test_value.setup(40.0, 130.0, 1.0, 0, value if value > 0.0 and value <= 130.0 else 88.0, 110.0)
		_test_unit.text = "rpm"
		_test_help.text = "Le cycliste pédale à cadence constante et les Click automatiques choisissent le braquet : la puissance suit la pente."
	else:
		_test_value.setup(50.0, 600.0, 5.0, 0, value if value > 130.0 else 200.0, 110.0)
		_test_unit.text = "W"
		_test_help.text = "Le cycliste développe une puissance constante : la vitesse varie avec la pente."


# --- État et lancement ----------------------------------------------------------------

func _refresh_status() -> void:
	if not _launch:
		return
	var ok: bool = App.ride.has_route()
	var tip := ""
	if not ok:
		tip = "Choisis d'abord un parcours sur l'accueil."
	elif _mode == "seance" and WorkoutView.total_duration(_workout) <= 0.0:
		ok = false
		tip = "Choisis une séance ou ajoute des étapes."
	_launch.disabled = not ok
	_launch.tooltip_text = tip
	if tip != "":
		_show_status(tip, "warn")
	elif _mode == "test":
		_show_status("Séance test : home trainer, cadence, fréquence cardiaque et Click sont simulés.", "info")
	elif App.ride.profile_get("app.test_mode") == true:
		_show_status("Mode test actif (Connexions) : les appareils sont simulés.", "info")
	elif not App.devices.has_trainer():
		_show_status("Aucun home trainer connecté : la sortie utilisera le cycliste virtuel.", "warn")
	else:
		var trainer_name := ""
		for d in App.devices.found():
			if d.role == "trainer":
				trainer_name = d.name
		_show_status("Home trainer prêt%s." % (" : " + trainer_name if trainer_name != "" else ""), "ok")


func _show_status(text: String, kind: String) -> void:
	_status_label.text = text
	_status_box.theme_type_variation = {"warn": "WarnBox", "error": "WarnBox", "info": "InfoBox", "ok": "Tile"}[kind]
	_status_label.theme_type_variation = {"warn": "WarnLabel", "error": "ErrorLabel", "info": "DimLabel", "ok": "OkLabel"}[kind]


## Configuration de la sortie selon le mode choisi.
func ride_config() -> Dictionary:
	var w := {}
	if _mode == "seance":
		w = _workout.duplicate(true)
		w["duration_s"] = WorkoutView.total_duration(w)
	return {
		"mode": _mode,
		"workout": w,
		"erg": _erg.button_pressed,
		"intensity": snappedf(_intensity.value, 0.01),
		"opponents": {"count": _opp_count, "w_per_kg": snappedf(_opp_wkg.value, 0.01), "spread": snappedf(_opp_spread.value, 0.01)},
		"test_rider": _test_rider,
		"test_value": float(_test_value.value),
	}


func _on_launch() -> void:
	if _mode == "test":
		App.ride.profile_set("app.test_rider", _test_rider)
		App.ride.profile_set("app.test_value", float(_test_value.value))
		App.ride.profile_save()
	menu.launch(ride_config())
