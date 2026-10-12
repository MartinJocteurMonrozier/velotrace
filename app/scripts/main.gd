extends Node3D
## Écran d'accueil, puis sortie en 3D : monde, cycliste, caméra et overlay.
##
## Arguments facultatifs (après « -- » sur la ligne de commande) :
##   --gpx <fichier>        démarre directement sur ce parcours
##   --puissance <W>        cycliste virtuel à puissance fixe (sinon cadence et Click auto)
##   --accelere <x>         accélère le temps (essais)
##   --camera <0|1|2>       caméra arrière, de côté ou haute
##   --capture <png>        enregistre une capture puis quitte
##   --capture-apres <s>    temps de sortie avant la capture (8 s par défaut)

const World := preload("res://scripts/world.gd")
const Cyclist := preload("res://scripts/cyclist.gd")
const Hud := preload("res://scripts/hud.gd")

const EXAMPLES := {
	"Vallée puis lacets (7,5 km)": "res://../data/exemple-lacets.gpx",
	"Plat puis col à 7 % (8 km)": "res://../data/exemple-col.gpx",
}

var ride: Node
var _base_elev := 0.0
var _cyclist: Node3D
var _camera: Camera3D
var _hud: CanvasLayer
var _camera_mode := 0
var _auto_shift := true
var _riding := false
var _finished := false
var _gpx_path := ""
var _mode := "cadence"
var _mode_value := 88.0
var _capture_path := ""
var _capture_after := 8.0
var _ride_time := 0.0
var _menu: CanvasLayer
var _menu_status: Label


func _ready() -> void:
	# Sans import préalable par l'éditeur, Godot ne connaît pas encore l'extension.
	if not ClassDB.class_exists("VeloRide"):
		GDExtensionManager.load_extension("res://velotrace.gdextension")
	if not ClassDB.class_exists("VeloRide"):
		_show_fatal("Extension Rust absente : lance « cargo build -p velo-godot » à la racine du dépôt, puis relance.")
		return
	ride = ClassDB.instantiate("VeloRide")
	add_child(ride)
	_parse_args()
	if _gpx_path != "":
		_start()
	else:
		_build_menu()


func _parse_args() -> void:
	var args := OS.get_cmdline_user_args()
	var i := 0
	while i < args.size():
		var key := args[i]
		var val := args[i + 1] if i + 1 < args.size() else ""
		match key:
			"--gpx":
				_gpx_path = val
			"--puissance":
				_mode = "puissance"
				_mode_value = val.to_float()
			"--accelere":
				Engine.time_scale = val.to_float()
			"--camera":
				_camera_mode = val.to_int() % 3
			"--capture":
				_capture_path = val
			"--capture-apres":
				_capture_after = val.to_float()
		i += 2


func _build_menu() -> void:
	_menu = CanvasLayer.new()
	add_child(_menu)
	var bg := ColorRect.new()
	bg.color = Color(0.05, 0.08, 0.12)
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	_menu.add_child(bg)
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	_menu.add_child(center)
	var box := VBoxContainer.new()
	box.custom_minimum_size = Vector2(520, 0)
	box.add_theme_constant_override("separation", 14)
	center.add_child(box)
	var title := Label.new()
	title.text = "velotrace"
	title.add_theme_font_size_override("font_size", 64)
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	box.add_child(title)
	var sub := Label.new()
	sub.text = "Roule sur ta trace GPX. Mode test : home trainer et accessoires simulés."
	sub.autowrap_mode = TextServer.AUTOWRAP_WORD
	sub.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	sub.add_theme_color_override("font_color", Color(1, 1, 1, 0.7))
	box.add_child(sub)

	box.add_child(_section("Parcours"))
	for name in EXAMPLES:
		var b := Button.new()
		b.text = name
		b.pressed.connect(func(): _choose(EXAMPLES[name], name))
		box.add_child(b)
	var open := Button.new()
	open.text = "Ouvrir un fichier GPX…"
	open.pressed.connect(_open_dialog)
	box.add_child(open)
	_menu_status = Label.new()
	_menu_status.text = "Aucun parcours choisi"
	_menu_status.autowrap_mode = TextServer.AUTOWRAP_WORD
	_menu_status.add_theme_color_override("font_color", Color(0.98, 0.62, 0.20))
	box.add_child(_menu_status)

	box.add_child(_section("Cycliste virtuel"))
	var mode := OptionButton.new()
	mode.add_item("Cadence 88 rpm, Click automatiques")
	mode.add_item("Puissance fixe 200 W")
	mode.item_selected.connect(func(i: int):
		_mode = "cadence" if i == 0 else "puissance"
		_mode_value = 88.0 if i == 0 else 200.0)
	box.add_child(mode)

	var go := Button.new()
	go.text = "Rouler"
	go.add_theme_font_size_override("font_size", 26)
	go.pressed.connect(func():
		if _gpx_path == "":
			_menu_status.text = "Choisis d'abord un parcours."
		else:
			_start())
	box.add_child(go)


func _section(text: String) -> Label:
	var l := Label.new()
	l.text = text.to_upper()
	l.add_theme_font_size_override("font_size", 13)
	l.add_theme_color_override("font_color", Color(1, 1, 1, 0.55))
	return l


func _open_dialog() -> void:
	var dialog := FileDialog.new()
	dialog.file_mode = FileDialog.FILE_MODE_OPEN_FILE
	dialog.access = FileDialog.ACCESS_FILESYSTEM
	dialog.filters = PackedStringArray(["*.gpx ; Traces GPX"])
	dialog.use_native_dialog = true
	dialog.file_selected.connect(func(path: String): _choose(path, path.get_file()))
	_menu.add_child(dialog)
	dialog.popup_centered_ratio(0.7)


func _choose(path: String, label: String) -> void:
	if ride.load_gpx(path):
		_gpx_path = path
		_menu_status.text = "%s : %s km, D+ %d m" % [label, ("%.1f" % (ride.route_length_m() / 1000.0)).replace(".", ","), roundi(ride.route_elevation_gain_m())]
	else:
		_gpx_path = ""
		_menu_status.text = ride.error_message()


func _start() -> void:
	if not ride.load_gpx(_gpx_path):
		_show_fatal(ride.error_message())
		return
	if _menu:
		_menu.queue_free()
	var points: PackedVector3Array = ride.route_points()
	_base_elev = points[0].y
	var world := Node3D.new()
	world.name = "Monde"
	add_child(world)
	World.build(ride, world, _base_elev)

	_cyclist = Cyclist.new()
	add_child(_cyclist)
	_camera = Camera3D.new()
	_camera.fov = 70.0
	_camera.far = 3000.0
	add_child(_camera)
	_camera.make_current()

	var local := PackedVector3Array()
	for p in points:
		local.append(p - Vector3(0.0, _base_elev, 0.0))
	_hud = Hud.new()
	add_child(_hud)
	_hud.setup(ride.route_name(), ride.route_distances(), local, _base_elev)

	ride.start_test(_mode, _mode_value)
	_riding = true
	_place_cyclist(0.0, true)


func _process(delta: float) -> void:
	if not _riding:
		return
	_ride_time += delta
	_place_cyclist(delta, false)
	if _capture_path != "" and _ride_time >= _capture_after:
		_capture()


func _place_cyclist(delta: float, snap: bool) -> void:
	var t: Dictionary = ride.telemetry()
	if t.is_empty():
		return
	var pos: Vector3 = t.position - Vector3(0.0, _base_elev, 0.0)
	var dir: Vector3 = t.direction
	if dir.length() < 0.01:
		dir = Vector3.FORWARD
	_cyclist.transform = Transform3D(Basis.looking_at(dir.normalized(), Vector3.UP), pos + Vector3(0.0, 0.05, 0.0))
	_cyclist.animate(t.speed_kmh / 3.6, t.cadence_rpm, delta)
	_update_camera(pos, dir, delta, snap)
	_hud.update(t, delta)
	if t.finished and not _finished:
		_finished = true
		_hud.show_banner("Arrivée !\n%s" % _hud._clock.text)


func _update_camera(pos: Vector3, dir: Vector3, delta: float, snap: bool) -> void:
	var flat := Vector3(dir.x, 0.0, dir.z).normalized()
	var side := flat.cross(Vector3.UP)
	var eye: Vector3
	var look := pos + Vector3(0.0, 1.1, 0.0)
	match _camera_mode:
		0:
			eye = pos - flat * 6.5 + Vector3(0.0, 2.6, 0.0)
		1:
			eye = pos + side * 5.0 + flat * 1.5 + Vector3(0.0, 1.4, 0.0)
		_:
			eye = pos - flat * 25.0 + Vector3(0.0, 14.0, 0.0)
	var target := Transform3D(Basis.looking_at(look - eye, Vector3.UP), eye)
	if snap:
		_camera.transform = target
	else:
		_camera.transform = _camera.transform.interpolate_with(target, clampf(delta * 4.0, 0.0, 1.0))


func _unhandled_input(event: InputEvent) -> void:
	if not _riding or not (event is InputEventKey) or not event.pressed or event.echo:
		return
	match event.keycode:
		KEY_UP, KEY_KP_ADD, KEY_EQUAL:
			ride.shift_up()
		KEY_DOWN, KEY_KP_SUBTRACT, KEY_MINUS:
			ride.shift_down()
		KEY_RIGHT:
			ride.adjust_rider(10.0)
		KEY_LEFT:
			ride.adjust_rider(-10.0)
		KEY_A:
			_auto_shift = not _auto_shift
			ride.set_auto_shift(_auto_shift)
		KEY_C:
			_camera_mode = (_camera_mode + 1) % 3
		KEY_SPACE:
			var paused: bool = ride.is_running()
			ride.set_paused(paused)
			_hud.show_banner("Pause" if paused else "")
		KEY_ESCAPE:
			get_tree().reload_current_scene()


func _capture() -> void:
	_capture_path = ""
	await RenderingServer.frame_post_draw
	var path := _capture_path_arg()
	get_viewport().get_texture().get_image().save_png(path)
	print("Capture enregistrée : ", path)
	get_tree().quit()


func _capture_path_arg() -> String:
	var args := OS.get_cmdline_user_args()
	var i := args.find("--capture")
	return args[i + 1] if i >= 0 and i + 1 < args.size() else "capture.png"


func _show_fatal(text: String) -> void:
	var layer := CanvasLayer.new()
	add_child(layer)
	var l := Label.new()
	l.text = text
	l.set_anchors_preset(Control.PRESET_CENTER)
	l.add_theme_font_size_override("font_size", 22)
	layer.add_child(l)
	push_error(text)
