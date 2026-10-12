extends CanvasLayer
## Menu de pause, par-dessus la sortie : reprendre, ajuster les réglages,
## terminer (avec enregistrement) ou abandonner.
##
## L'appelant met la sortie en pause, appelle `setup(App.ride.ride_state())`,
## puis libère ce menu après `resumed` ou `finished`. Le menu fonctionne que
## l'arbre soit en pause ou non. Espace ou Échap : reprendre.

## Reprendre la sortie.
signal resumed()
## Un réglage a changé. Clés : grade_difficulty, rider_kg, auto_shift, camera,
## erg, intensity, skip_step, w_per_kg, test_value.
signal adjusted(key: String, value: Variant)
## Terminer la sortie ; `save` est faux pour l'abandonner sans l'enregistrer.
signal finished(save: bool)

const UI := preload("res://scripts/ui/theme.gd")
const Fmt := preload("res://scripts/ui/fmt.gd")
const NumberField := preload("res://scripts/ui/number_field.gd")

const BLUR_SHADER := """
shader_type canvas_item;
uniform sampler2D screen_tex : hint_screen_texture, filter_linear_mipmap;
uniform float dim = 0.55;
void fragment() {
	vec3 c = textureLod(screen_tex, SCREEN_UV, 3.0).rgb;
	COLOR = vec4(mix(c, vec3(0.03, 0.05, 0.09), dim), 1.0);
}
"""
const MODE_NAMES := {"libre": "Sortie libre", "seance": "Séance", "adversaires": "Contre des coureurs virtuels", "test": "Séance test"}
const CAMERAS := ["Proche", "Arrière", "De côté", "Haute"]

var _state := {}
var _root: Control
var _form: VBoxContainer
var _subtitle: Label
var _resume: Button
var _step_label: Label
var _dialog: ConfirmationDialog


func _init() -> void:
	layer = 20
	process_mode = Node.PROCESS_MODE_ALWAYS


func _ready() -> void:
	_root = Control.new()
	_root.set_anchors_preset(Control.PRESET_FULL_RECT)
	_root.theme = UI.build()
	add_child(_root)
	var back := ColorRect.new()
	back.set_anchors_preset(Control.PRESET_FULL_RECT)
	var mat := ShaderMaterial.new()
	var sh := Shader.new()
	sh.code = BLUR_SHADER
	mat.shader = sh
	back.material = mat
	_root.add_child(back)

	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	_root.add_child(center)
	var card := PanelContainer.new()
	card.custom_minimum_size.x = 640
	var sb := UI._box(Color(UI.CARD, 0.97), 16, 32, 26, UI.BORDER_HI)
	sb.shadow_color = Color(0, 0, 0, 0.5)
	sb.shadow_size = 30
	card.add_theme_stylebox_override("panel", sb)
	center.add_child(card)
	var col := UI.vbox(16)
	card.add_child(col)

	var head := UI.hbox(12)
	col.add_child(head)
	var hv := UI.vbox(2)
	hv.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	hv.add_child(UI.label("Pause", "H1"))
	_subtitle = UI.label("", "DimLabel", true)
	hv.add_child(_subtitle)
	head.add_child(hv)
	var keys := UI.label("Espace ou Échap\npour reprendre", "MutedLabel")
	keys.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	keys.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	head.add_child(keys)

	_resume = UI.primary_button("Reprendre  ▸", true)
	_resume.pressed.connect(func(): resumed.emit())
	col.add_child(_resume)

	col.add_child(UI.section_title("Réglages"))
	_form = UI.vbox(12)
	col.add_child(_form)

	col.add_child(HSeparator.new())
	var end := UI.hbox(12)
	col.add_child(end)
	var finish := UI.button("Terminer et enregistrer")
	finish.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	finish.custom_minimum_size.y = 46
	finish.pressed.connect(func(): finished.emit(true))
	end.add_child(finish)
	var quit := UI.button("Abandonner sans enregistrer", "DangerButton")
	quit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	quit.custom_minimum_size.y = 46
	quit.pressed.connect(_confirm_abandon)
	end.add_child(quit)
	if not _state.is_empty():
		_build_form()


## Remplit le menu avec l'état courant de la sortie (`App.ride.ride_state()`).
func setup(state: Dictionary) -> void:
	_state = state.duplicate()
	if _form:
		_build_form()


func _build_form() -> void:
	for c in _form.get_children():
		c.queue_free()
	var mode: String = _state.get("mode", "libre")
	var sub: String = MODE_NAMES.get(mode, mode)
	if mode == "seance" and str(_state.get("workout_name", "")) != "":
		sub += " : " + str(_state.workout_name)
	_subtitle.text = sub

	_slider_row("Difficulté des pentes", "grade_difficulty", 0.0, 1.0, 0.05, float(_state.get("grade_difficulty", 1.0)),
		func(v: float) -> String: return Fmt.percent(v))
	var kg: LineEdit = NumberField.new()
	kg.setup(30.0, 200.0, 0.5, 1, float(_state.get("rider_kg", 70.0)), 100.0)
	kg.value_changed.connect(func(v: float): adjusted.emit("rider_kg", v))
	_row("Poids du cycliste", kg, "kg")
	var auto := CheckButton.new()
	auto.button_pressed = bool(_state.get("auto_shift", true))
	auto.toggled.connect(func(on: bool): adjusted.emit("auto_shift", on))
	_row("Click automatiques", auto)
	var cam := UI.segmented(CAMERAS, clampi(int(_state.get("camera", 0)), 0, 3), func(i: int): adjusted.emit("camera", i))
	_row("Caméra", cam)

	match mode:
		"seance":
			_form.add_child(HSeparator.new())
			var erg := CheckButton.new()
			erg.text = "le home trainer impose la puissance"
			erg.button_pressed = bool(_state.get("erg", true))
			erg.toggled.connect(func(on: bool): adjusted.emit("erg", on))
			_row("Mode ERG", erg)
			_slider_row("Intensité", "intensity", 0.5, 1.5, 0.01, float(_state.get("intensity", 1.0)),
				func(v: float) -> String: return Fmt.percent(v))
			var sr := UI.hbox(12)
			_step_label = UI.label("", "DimLabel")
			_step_label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
			sr.add_child(_step_label)
			var skip := UI.button("Passer à l'étape suivante  ›")
			skip.pressed.connect(func():
				adjusted.emit("skip_step", true)
				_state["step_index"] = mini(int(_state.get("step_index", 0)) + 1, int(_state.get("step_count", 1)) - 1)
				_update_step())
			sr.add_child(skip)
			_row("Étape", sr)
			_update_step()
		"adversaires":
			_form.add_child(HSeparator.new())
			var kg_v := float(_state.get("rider_kg", 70.0))
			_slider_row("Rythme des coureurs", "w_per_kg", 1.5, 6.0, 0.1, float(_state.get("w_per_kg", 3.0)),
				func(v: float) -> String: return "%s W/kg · %s W" % [Fmt.num(v, 1), Fmt.num(v * kg_v)], 170.0)
		"test":
			_form.add_child(HSeparator.new())
			var cadence: bool = _state.get("test_rider", "cadence") == "cadence"
			var tv: LineEdit = NumberField.new()
			if cadence:
				tv.setup(40.0, 130.0, 1.0, 0, float(_state.get("test_value", 88.0)), 100.0)
			else:
				tv.setup(50.0, 600.0, 5.0, 0, float(_state.get("test_value", 200.0)), 100.0)
			tv.value_changed.connect(func(v: float): adjusted.emit("test_value", v))
			_row("Cadence du cycliste virtuel" if cadence else "Puissance du cycliste virtuel", tv, "rpm" if cadence else "W")
	_resume.grab_focus.call_deferred()


func _update_step() -> void:
	if _step_label:
		_step_label.text = "%d / %d" % [int(_state.get("step_index", 0)) + 1, int(_state.get("step_count", 1))]


func _row(title: String, control: Control, unit := "") -> void:
	var row := UI.hbox(14)
	var l := UI.label(title)
	l.custom_minimum_size.x = 210
	l.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	row.add_child(l)
	control.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	row.add_child(control)
	if unit != "":
		var u := UI.label(unit, "UnitLabel")
		u.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		row.add_child(u)
	_form.add_child(row)


func _slider_row(title: String, key: String, lo: float, hi: float, step: float, value: float, fmt: Callable, label_w := 64.0) -> void:
	var box := UI.hbox(12)
	var s := HSlider.new()
	s.min_value = lo
	s.max_value = hi
	s.step = step
	s.value = value
	s.custom_minimum_size = Vector2(220, 28)
	s.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	box.add_child(s)
	var val := UI.label(fmt.call(value), "H3")
	val.custom_minimum_size.x = label_w
	val.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	box.add_child(val)
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	s.value_changed.connect(func(v: float):
		val.text = fmt.call(v)
		adjusted.emit(key, v))
	_row(title, box)


func _confirm_abandon() -> void:
	_dialog = ConfirmationDialog.new()
	_dialog.title = "Abandonner la sortie"
	_dialog.dialog_text = "La sortie ne sera pas enregistrée : elle n'apparaîtra ni dans l'historique ni sur Strava."
	_dialog.ok_button_text = "Abandonner"
	_dialog.cancel_button_text = "Continuer la sortie"
	_dialog.process_mode = Node.PROCESS_MODE_ALWAYS
	_dialog.confirmed.connect(func(): finished.emit(false))
	_dialog.visibility_changed.connect(func():
		if _dialog and not _dialog.visible:
			_dialog.queue_free()
			_dialog = null
			_resume.grab_focus())
	_root.add_child(_dialog)
	_dialog.popup_centered(Vector2i(520, 0))
	_dialog.get_cancel_button().grab_focus()


func _input(event: InputEvent) -> void:
	if not (event is InputEventKey) or not event.pressed or event.echo:
		return
	if _dialog and _dialog.visible:
		return
	var editing := get_viewport().gui_get_focus_owner() is LineEdit
	if event.keycode == KEY_ESCAPE or (event.keycode == KEY_SPACE and not editing):
		get_viewport().set_input_as_handled()
		resumed.emit()
