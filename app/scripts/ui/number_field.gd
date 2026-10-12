extends LineEdit
## Champ numérique à la française : virgule décimale, bornes, pas ;
## ↑ et ↓ (ou la molette quand le champ est actif) ajustent la valeur.
## Avec `as_duration`, la valeur est une durée en secondes saisie en m:ss.

const Fmt := preload("res://scripts/ui/fmt.gd")

signal value_changed(value: float)

var min_value := 0.0
var max_value := 100.0
var step := 1.0
var decimals := 0
var as_duration := false
var value := 0.0


## Configure le champ et renvoie lui-même pour enchaîner.
func setup(lo: float, hi: float, step_size: float, digits: int, initial: float, width := 110.0) -> LineEdit:
	min_value = lo
	max_value = hi
	step = step_size
	decimals = digits
	custom_minimum_size.x = width
	alignment = HORIZONTAL_ALIGNMENT_RIGHT
	select_all_on_focus = true
	set_value(initial)
	if not text_submitted.is_connected(_on_submitted):
		text_submitted.connect(_on_submitted)
		focus_exited.connect(_commit)
	return self


func set_value(v: float, emit := false) -> void:
	v = clampf(snappedf(v, step / 100.0) if step > 0.0 else v, min_value, max_value)
	var changed := not is_equal_approx(v, value)
	value = v
	text = Fmt.mss(v) if as_duration else Fmt.num(v, decimals)
	if emit and changed:
		value_changed.emit(value)


func _on_submitted(_t: String) -> void:
	_commit()
	release_focus()


func _commit() -> void:
	var v := Fmt.parse_duration(text) if as_duration else Fmt.parse(text)
	if is_nan(v):
		set_value(value)
		return
	set_value(v, true)


func _gui_input(event: InputEvent) -> void:
	if not editable:
		return
	var dir := 0
	if event is InputEventKey and event.pressed:
		if event.keycode == KEY_UP:
			dir = 1
		elif event.keycode == KEY_DOWN:
			dir = -1
	elif event is InputEventMouseButton and event.pressed and has_focus():
		if event.button_index == MOUSE_BUTTON_WHEEL_UP:
			dir = 1
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			dir = -1
	if dir != 0:
		var v := Fmt.parse_duration(text) if as_duration else Fmt.parse(text)
		set_value((value if is_nan(v) else v) + dir * step, true)
		caret_column = text.length()
		accept_event()
