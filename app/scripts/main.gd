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
##   --decor <reel|cache|simple>  décor réel téléchargé, réel depuis le cache, ou simplifié
##   --qualite <0|1|2>      qualité graphique basse, moyenne ou haute
##   --osm-fichier <json>   données OSM (JSON Overpass) à la place d'Overpass
##   --images <url>         source d'images aériennes ({z}/{x}/{y}, file:// accepté)
##   --aller-a <m>          démarre la sortie à cette distance (essais)

const World := preload("res://scripts/world.gd")
const Cyclist := preload("res://scripts/cyclist.gd")
const Hud := preload("res://scripts/hud.gd")

const EXAMPLES := {
	"Alpe d'Huez depuis Le Bourg-d'Oisans (14 km)": "res://../data/alpe-d-huez.gpx",
	"Vallée puis lacets (7,5 km)": "res://../data/exemple-lacets.gpx",
	"Plat puis col à 7 % (8 km)": "res://../data/exemple-col.gpx",
	"Rond-point du Bourg-d'Oisans (0,7 km)": "res://../data/exemple-rond-point.gpx",
}
const DECORS := ["reel", "cache", "simple"]
const QUALITY_NAMES := ["Basse", "Moyenne", "Haute"]
## Échelle de rendu 3D (FSR) selon la qualité : l'interface reste nette.
const RENDER_SCALE := [0.67, 0.85, 1.0]
## Délai maximal d'attente des carrés détaillés autour du départ.
const NEAR_WAIT_S := 45.0

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
var _decor := "reel"
var _quality := 1
var _osm_file := ""
var _skip_to := 0.0
var _imagery_url := ""
var _real_world: Node3D
var _loading: CanvasLayer
var _loading_bar: ProgressBar
var _loading_label: Label
var _loading_phase := ""
var _loading_time := 0.0
var _decor_note := ""
var _lean := 0.0


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
			"--decor":
				_decor = val if val in DECORS else "reel"
			"--qualite":
				_quality = clampi(val.to_int(), 0, 2)
			"--osm-fichier":
				_osm_file = val
			"--images":
				_imagery_url = val
			"--aller-a":
				_skip_to = val.to_float()
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

	box.add_child(_section("Décor et graphismes"))
	var decor := OptionButton.new()
	decor.add_item("Réel : relief, images aériennes et OpenStreetMap")
	decor.add_item("Réel sans connexion (données déjà en cache)")
	decor.add_item("Simplifié (aucun téléchargement)")
	decor.selected = DECORS.find(_decor)
	decor.item_selected.connect(func(i: int): _decor = DECORS[i])
	box.add_child(decor)
	var quality := OptionButton.new()
	for q in QUALITY_NAMES.size():
		quality.add_item("Qualité %s" % QUALITY_NAMES[q].to_lower())
	quality.selected = _quality
	quality.item_selected.connect(func(i: int): _quality = i)
	box.add_child(quality)

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
		_menu = null
	_apply_quality()
	if _decor == "simple":
		_build_scene(false)
		_begin_ride()
		return
	if _imagery_url != "":
		ride.set_imagery_url(_imagery_url)
	ride.start_world(_quality, _decor == "reel", _osm_file)
	_show_loading()


## Réglages de rendu selon la qualité : moins de pixels 3D, d'ombres et
## d'anticrénelage sur les petites machines, 60 images/s au plus.
func _apply_quality() -> void:
	Engine.max_fps = 60
	var vp := get_viewport()
	vp.scaling_3d_scale = RENDER_SCALE[_quality]
	vp.scaling_3d_mode = Viewport.SCALING_3D_MODE_FSR if _quality < 2 else Viewport.SCALING_3D_MODE_BILINEAR
	vp.msaa_3d = Viewport.MSAA_2X if _quality == 2 else Viewport.MSAA_DISABLED
	vp.screen_space_aa = Viewport.SCREEN_SPACE_AA_FXAA if _quality == 1 else Viewport.SCREEN_SPACE_AA_DISABLED
	RenderingServer.directional_shadow_atlas_set_size(2048 if _quality == 0 else 4096, true)
	RenderingServer.directional_soft_shadow_filter_set_quality(
		[RenderingServer.SHADOW_QUALITY_HARD, RenderingServer.SHADOW_QUALITY_SOFT_LOW, RenderingServer.SHADOW_QUALITY_SOFT_MEDIUM][_quality])


func _show_loading() -> void:
	_loading = CanvasLayer.new()
	_loading.layer = 10
	add_child(_loading)
	var bg := ColorRect.new()
	bg.color = Color(0.05, 0.08, 0.12)
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	_loading.add_child(bg)
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	_loading.add_child(center)
	var box := VBoxContainer.new()
	box.custom_minimum_size = Vector2(560, 0)
	box.add_theme_constant_override("separation", 12)
	center.add_child(box)
	var title := Label.new()
	title.text = ride.route_name()
	title.add_theme_font_size_override("font_size", 34)
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	box.add_child(title)
	_loading_bar = ProgressBar.new()
	_loading_bar.max_value = 1.0
	_loading_bar.custom_minimum_size = Vector2(0, 22)
	box.add_child(_loading_bar)
	_loading_label = Label.new()
	_loading_label.text = "Préparation du décor…"
	_loading_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_loading_label.autowrap_mode = TextServer.AUTOWRAP_WORD
	_loading_label.add_theme_color_override("font_color", Color(1, 1, 1, 0.7))
	box.add_child(_loading_label)
	_loading_phase = "monde"
	_loading_time = 0.0


## Avance du chargement : construction du décor, puis carrés détaillés du départ.
func _poll_loading(delta: float) -> void:
	_loading_time += delta
	if _loading_phase == "monde":
		var st: Dictionary = ride.world_status()
		match st.state:
			"building":
				_loading_bar.value = 0.8 * float(st.progress)
				_loading_label.text = st.message
			"ready":
				_build_scene(true)
				_loading_phase = "proches"
				_loading_time = 0.0
				print("Décor : ", st.message)
			_:
				_decor_note = "Décor réel indisponible (%s) : décor simplifié." % st.message
				push_warning(_decor_note)
				_build_scene(false)
				_finish_loading()
		return
	var start: Vector3 = ride.route_points()[0] - Vector3(0.0, _base_elev, 0.0)
	_real_world.focus = start
	var r: Vector2i = _real_world.readiness(start)
	_loading_bar.value = 0.8 + 0.2 * (float(r.x) / maxf(1.0, r.y))
	_loading_label.text = "Décor détaillé autour du départ : %d / %d" % [r.x, r.y]
	if r.x >= r.y or _loading_time > NEAR_WAIT_S:
		_finish_loading()


func _finish_loading() -> void:
	_loading.queue_free()
	_loading = null
	_loading_phase = ""
	_begin_ride()


func _build_scene(real: bool) -> void:
	var points: PackedVector3Array = ride.route_points()
	_base_elev = points[0].y
	var world := Node3D.new()
	world.name = "Monde"
	add_child(world)
	_real_world = World.build(ride, world, _base_elev, _quality, real)

	_cyclist = Cyclist.new()
	add_child(_cyclist)
	_camera = Camera3D.new()
	_camera.fov = 70.0
	_camera.near = 0.2
	_camera.far = World.FOG_REACH[_quality] + 1500.0
	add_child(_camera)
	_camera.make_current()

	var local := PackedVector3Array()
	for p in points:
		local.append(p - Vector3(0.0, _base_elev, 0.0))
	_hud = Hud.new()
	add_child(_hud)
	_hud.setup(ride.route_name(), ride.route_distances(), local, _base_elev)
	_hud.set_credits(ride.world_credits() if real else "Décor simplifié")


func _begin_ride() -> void:
	ride.start_test(_mode, _mode_value)
	if _skip_to > 0.0:
		ride.skip_to(_skip_to)
	_riding = true
	_place_cyclist(0.0, true)
	if _decor_note != "":
		_hud.show_banner("Décor simplifié")
		get_tree().create_timer(3.0).timeout.connect(func(): _hud.show_banner(""))


func _process(delta: float) -> void:
	if _loading_phase != "":
		_poll_loading(delta)
		return
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
	# Inclinaison en virage, lissée pour rester naturelle.
	var lean_target: float = t.get("lean", 0.0)
	_lean = lean_target if snap else lerpf(_lean, lean_target, clampf(delta * 3.0, 0.0, 1.0))
	var basis := Basis.looking_at(dir.normalized(), Vector3.UP) * Basis(Vector3(0.0, 0.0, 1.0), _lean)
	_cyclist.transform = Transform3D(basis, pos + Vector3(0.0, 0.05, 0.0))
	_cyclist.animate(t.speed_kmh / 3.6, t.cadence_rpm, delta)
	_update_camera(pos, dir, delta, snap)
	if _real_world:
		# Les carrés détaillés se chargent un peu en avant du cycliste.
		_real_world.focus = pos + Vector3(dir.x, 0.0, dir.z).normalized() * 150.0
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
	print("Rendu : %d appels de dessin, %d triangles, %d Mo de mémoire vidéo, %d objets" % [
		Performance.get_monitor(Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME),
		Performance.get_monitor(Performance.RENDER_TOTAL_PRIMITIVES_IN_FRAME),
		Performance.get_monitor(Performance.RENDER_VIDEO_MEM_USED) / 1048576.0,
		Performance.get_monitor(Performance.RENDER_TOTAL_OBJECTS_IN_FRAME)])
	if OS.is_debug_build() and OS.get_environment("VELOTRACE_NOEUDS") != "":
		var counts := {}
		var stack: Array[Node] = [self]
		while not stack.is_empty():
			var n: Node = stack.pop_back()
			counts[n.get_class()] = counts.get(n.get_class(), 0) + 1
			stack.append_array(n.get_children())
		print(counts)
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
