extends CanvasLayer
## Application avant la sortie : accueil, connexions, paramètres, historique,
## choix de l'entraînement et bilan des sorties.
##
## Échap revient à l'écran précédent. Un fichier .gpx glissé sur la fenêtre
## devient le parcours choisi.

## L'utilisateur lance la sortie ; voir training.gd pour le contenu de `cfg`.
signal start_ride(cfg: Dictionary)

const UI := preload("res://scripts/ui/theme.gd")
const SCREENS := {
	"home": preload("res://scripts/ui/home.gd"),
	"connections": preload("res://scripts/ui/connections.gd"),
	"settings": preload("res://scripts/ui/settings.gd"),
	"history": preload("res://scripts/ui/history.gd"),
	"training": preload("res://scripts/ui/training.gd"),
}
const RideDetail := preload("res://scripts/ui/ride_detail.gd")
## Onglets de la barre du haut.
const NAV := [["home", "Accueil"], ["connections", "Connexions"], ["settings", "Paramètres"], ["history", "Historique"]]
const ROLES := ["trainer", "cadence", "heart_rate", "click"]
const ROLE_SHORT := {"trainer": "Home trainer", "cadence": "Cadence", "heart_rate": "FC", "click": "Click"}

var _root: Control
var _content: Control
var _nav := {}
var _screens := {}
var _stack: Array[String] = []
var _current: Control
var _device_chip: Button
var _device_dots := {}
var _device_label: Label
var _poll := 0.0


func _ready() -> void:
	layer = 5
	_root = Control.new()
	_root.set_anchors_preset(Control.PRESET_FULL_RECT)
	_root.theme = UI.build()
	add_child(_root)
	_build_background()

	var col := VBoxContainer.new()
	col.set_anchors_preset(Control.PRESET_FULL_RECT)
	col.add_theme_constant_override("separation", 0)
	_root.add_child(col)
	col.add_child(_build_top_bar())
	_content = Control.new()
	_content.size_flags_vertical = Control.SIZE_EXPAND_FILL
	col.add_child(_content)

	get_window().files_dropped.connect(_on_files_dropped)
	go("home")


func _exit_tree() -> void:
	if get_window().files_dropped.is_connected(_on_files_dropped):
		get_window().files_dropped.disconnect(_on_files_dropped)


func _build_background() -> void:
	var bg := ColorRect.new()
	bg.color = UI.BG
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	bg.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_root.add_child(bg)
	# Deux halos discrets, orange en haut à droite et bleu-vert en bas à gauche.
	for spec in [[Vector2(0.92, 0.0), UI.ACCENT, 0.07], [Vector2(0.0, 1.0), UI.TEAL, 0.05]]:
		var g := Gradient.new()
		g.set_color(0, Color(spec[1], spec[2]))
		g.set_color(1, Color(spec[1], 0.0))
		var tex := GradientTexture2D.new()
		tex.gradient = g
		tex.fill = GradientTexture2D.FILL_RADIAL
		tex.fill_from = spec[0]
		tex.fill_to = spec[0] + Vector2(0.55, 0.0)
		tex.width = 256
		tex.height = 256
		var r := TextureRect.new()
		r.texture = tex
		r.set_anchors_preset(Control.PRESET_FULL_RECT)
		r.stretch_mode = TextureRect.STRETCH_SCALE
		r.mouse_filter = Control.MOUSE_FILTER_IGNORE
		_root.add_child(r)


func _build_top_bar() -> Control:
	var bar := UI.card("TopBar")
	bar.custom_minimum_size.y = 64
	var row := UI.hbox(6)
	bar.add_child(row)

	var logo := HBoxContainer.new()
	logo.add_theme_constant_override("separation", 0)
	logo.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	var a := UI.label("velo", "H2")
	a.add_theme_font_size_override("font_size", 24)
	var b := UI.label("trace", "H2")
	b.add_theme_font_size_override("font_size", 24)
	b.add_theme_color_override("font_color", UI.ACCENT)
	logo.add_child(a)
	logo.add_child(b)
	row.add_child(logo)
	var gap := Control.new()
	gap.custom_minimum_size.x = 36
	row.add_child(gap)

	var group := ButtonGroup.new()
	for item in NAV:
		var nb := UI.button(item[1], "NavButton")
		nb.toggle_mode = true
		nb.button_group = group
		nb.pressed.connect(go.bind(item[0]))
		row.add_child(nb)
		_nav[item[0]] = nb
	row.add_child(UI.spacer())

	# Résumé des appareils : un point par rôle, ouvre l'écran Connexions.
	_device_chip = UI.button("", "GhostButton", "État des appareils")
	_device_chip.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	var chip_row := UI.hbox(8)
	chip_row.set_anchors_preset(Control.PRESET_FULL_RECT)
	chip_row.offset_left = 12
	chip_row.offset_right = -12
	chip_row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_device_chip.add_child(chip_row)
	for role in ROLES:
		var dot := UI.status_dot(UI.TEXT_MUTED, 9)
		dot.tooltip_text = ROLE_SHORT[role]
		chip_row.add_child(dot)
		_device_dots[role] = dot
	_device_label = UI.label("", "MutedLabel")
	_device_label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	chip_row.add_child(_device_label)
	_device_chip.custom_minimum_size = Vector2(250, 40)
	_device_chip.pressed.connect(go.bind("connections"))
	row.add_child(_device_chip)

	var quit := UI.button("Quitter", "GhostButton")
	quit.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	quit.pressed.connect(func(): get_tree().quit())
	row.add_child(quit)
	return bar


# --- Navigation ---------------------------------------------------------------------

## Affiche un écran : « home », « connections », « settings », « history » ou
## « training ». Les onglets du haut repartent de l'accueil.
func go(screen: String) -> void:
	if screen in _nav:
		_stack.assign(["home"] if screen == "home" else ["home", screen])
	else:
		_stack.append(screen)
	_show(_screen(screen))


## Revient à l'écran précédent (Échap).
func back() -> void:
	if _current and _current.has_method("back") and _current.back():
		return
	if _stack.size() <= 1:
		return
	_stack.pop_back()
	_show(_screen(_stack[-1]))


## Bilan d'une sortie, depuis l'historique ou à l'arrivée (`just_finished`).
func show_ride(ride_id: int, just_finished := false) -> void:
	var d: Control = RideDetail.new()
	d.menu = self
	d.closed.connect(func():
		if _current == d:
			back())
	d.home_requested.connect(go.bind("home"))
	if just_finished:
		_stack.assign(["home", "history", "ride_detail"])
	else:
		_stack.append("ride_detail")
	d.setup(ride_id, just_finished)
	_show(d)


func launch(cfg: Dictionary) -> void:
	start_ride.emit(cfg)


func _screen(screen: String) -> Control:
	if not _screens.has(screen):
		var s: Control = SCREENS[screen].new()
		s.set("menu", self)
		_screens[screen] = s
	return _screens[screen]


func _show(s: Control) -> void:
	if _current == s:
		if s.has_method("enter"):
			s.enter()
		return
	if _current:
		if _current.has_method("leave"):
			_current.leave()
		_content.remove_child(_current)
		# Le bilan d'une sortie est recréé à chaque ouverture.
		if _current.get_script() == RideDetail:
			_current.queue_free()
	_current = s
	s.set_anchors_preset(Control.PRESET_FULL_RECT)
	_content.add_child(s)
	s.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	var top: String = _stack[1] if _stack.size() > 1 else "home"
	for key in _nav:
		_nav[key].set_pressed_no_signal(key == top)
	if s.has_method("enter"):
		s.enter()
	s.modulate.a = 0.0
	create_tween().tween_property(s, "modulate:a", 1.0, 0.14)
	_refresh_devices()


func current_screen() -> String:
	return _stack[-1] if not _stack.is_empty() else ""


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel"):
		back()
		get_viewport().set_input_as_handled()


func _on_files_dropped(files: PackedStringArray) -> void:
	for f in files:
		if f.get_extension().to_lower() == "gpx":
			go("home")
			_screens["home"].load_route(f)
			return


# --- Appareils (barre du haut) --------------------------------------------------------

func _process(delta: float) -> void:
	_poll -= delta
	if _poll <= 0.0:
		_poll = 1.0
		_refresh_devices()


func _refresh_devices() -> void:
	if not _device_label:
		return
	if App.ride.profile_get("app.test_mode") == true:
		for role in ROLES:
			UI.set_dot(_device_dots[role], UI.TEAL)
		_device_label.text = "Mode test"
		return
	var states := device_states()
	var n := 0
	for role in ROLES:
		var st: String = states.get(role, "")
		UI.set_dot(_device_dots[role], UI.link_color(st) if st != "" else Color(UI.TEXT_MUTED, 0.5))
		if st == "connected":
			n += 1
	_device_label.text = "Aucun appareil" if n == 0 else ("%d appareil connecté" % n if n == 1 else "%d appareils connectés" % n)


## État de liaison par rôle : « connected », « connecting », « failed »,
## « disconnected », ou absent si aucun appareil n'est associé.
static func device_states() -> Dictionary:
	var out := {}
	for r in App.ride.remembered_devices():
		out[r.kind] = "disconnected"
	for d in App.devices.found():
		if d.role != "":
			out[d.role] = d.state
	return out
