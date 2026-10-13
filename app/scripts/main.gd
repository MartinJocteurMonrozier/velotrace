extends Node3D
## Écran d'accueil, puis sortie en 3D : monde, cycliste, caméra et overlay.
##
## Arguments facultatifs (après « -- » sur la ligne de commande) :
##   --gpx <fichier>        démarre directement sur ce parcours
##   --puissance <W>        cycliste virtuel à puissance fixe (sinon cadence et Click auto)
##   --accelere <x>         accélère le temps (essais)
##   --camera <0|1|2|3>     caméra proche (par défaut), arrière, de côté ou haute
##   --capture <png>        enregistre une capture puis quitte
##   --capture-apres <s>    temps de sortie avant la capture (8 s par défaut)
##   --decor <reel|cache|simple>  décor réel téléchargé, réel depuis le cache, ou simplifié
##   --qualite <0|1|2>      qualité graphique basse, moyenne ou haute
##   --osm-fichier <json>   données OSM (JSON Overpass) à la place d'Overpass
##   --images <url>         source d'images aériennes ({z}/{x}/{y}, file:// accepté)
##   --aller-a <m>          démarre la sortie à cette distance (essais)
##   --mode <libre|seance|adversaires|test>  mode d'entraînement (test par défaut avec --gpx)
##   --seance <n>           séance prédéfinie n (mode seance)
##   --adversaires <n>      nombre de coureurs virtuels (mode adversaires, 5 par défaut)
##   --wkg <x>              leur rapport poids puissance (3,0 par défaut)
##   --ecran <nom>          sans --gpx : écran du menu à ouvrir (connections,
##                          settings, history, training ; ride:<id> pour un bilan)

const World := preload("res://scripts/world.gd")
const Cyclist := preload("res://scripts/cyclist.gd")
const Hud := preload("res://scripts/hud.gd")
const Menu := preload("res://scripts/ui/menu.gd")
const PauseMenu := preload("res://scripts/ui/pause_menu.gd")

const DECORS := ["reel", "cache", "simple"]
## Échelle de rendu 3D (FSR) selon la qualité : l'interface reste nette.
const RENDER_SCALE := [0.67, 0.85, 1.0]
## Délai maximal d'attente des carrés détaillés autour du départ.
const NEAR_WAIT_S := 45.0
## Nombre de caméras (touche C).
const CAMERA_MODES := 4
## Caméra proche : distance derrière le cycliste et hauteur.
const CHASE_DIST := 2.9
const CHASE_HEIGHT := 1.45
## Dans les virages, la caméra pivote vers l'extérieur de la vitesse de virage
## (lissée) multipliée par CHASE_TURN_S, puis suit avec un retard de CHASE_LAG_S ;
## l'écart reste sous CHASE_MAX_LAG (rad).
const CHASE_TURN_S := 1.6
const CHASE_RATE_S := 0.6
const CHASE_LAG_S := 0.6
const CHASE_MAX_LAG := 0.65
## Les coureurs virtuels sont dessinés jusqu'à cette distance du joueur.
const OPPONENT_SHOW_M := 400.0
## Temps laissé pour savourer l'arrivée avant le bilan.
const FINISH_HOLD_S := 4.0

var ride: Node
var _cfg := {}
var _base_elev := 0.0
var _cyclist: Node3D
var _camera: Camera3D
var _hud: CanvasLayer
var _camera_mode := 0
var _auto_shift := true
var _riding := false
var _finished := false
var _gpx_path := ""
var _capture_path := ""
var _capture_after := 8.0
var _ride_time := 0.0
var _menu: CanvasLayer
var _screen := ""
var _pause: CanvasLayer
var _opponents: Array[Node3D] = []
var _finish_time := -1.0
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
var _chase_yaw := 0.0
var _prev_yaw := 0.0
var _yaw_rate := 0.0


func _ready() -> void:
	if not App.ok:
		_show_fatal(App.error)
		return
	ride = App.ride
	_camera_mode = int(ride.profile_get("app.camera"))
	_decor = String(ride.profile_get("app.decor"))
	_quality = int(ride.profile_get("scenery.quality"))
	_auto_shift = bool(ride.profile_get("app.auto_shift"))
	_cfg = {"mode": "test", "test_rider": ride.profile_get("app.test_rider"), "test_value": ride.profile_get("app.test_value")}
	_parse_args()
	if _gpx_path != "":
		if not ride.load_gpx(_gpx_path):
			_show_fatal(ride.error_message())
			return
		_start()
	else:
		_menu = Menu.new()
		add_child(_menu)
		_menu.start_ride.connect(_on_start_ride)
		if App.finished_ride_id >= 0:
			_menu.show_ride(App.finished_ride_id, true)
			App.finished_ride_id = -1
		elif _screen.begins_with("ride:"):
			_menu.show_ride(_screen.trim_prefix("ride:").to_int())
		elif _screen != "":
			_menu.go(_screen)
		if _capture_path != "":
			get_tree().create_timer(_capture_after).timeout.connect(_capture)


func _on_start_ride(cfg: Dictionary) -> void:
	_cfg = cfg
	# Les réglages ont pu changer dans le menu.
	_camera_mode = int(ride.profile_get("app.camera"))
	_decor = String(ride.profile_get("app.decor"))
	_quality = int(ride.profile_get("scenery.quality"))
	_auto_shift = bool(ride.profile_get("app.auto_shift"))
	_start()


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
				_cfg["test_rider"] = "puissance"
				_cfg["test_value"] = val.to_float()
			"--mode":
				_cfg["mode"] = val
			"--seance":
				var presets: Array = ride.workout_presets()
				_cfg["workout"] = presets[clampi(val.to_int(), 0, presets.size() - 1)]
				_cfg["erg"] = true
				_cfg["intensity"] = 1.0
			"--adversaires":
				_cfg["opponents"] = {"count": val.to_int(), "w_per_kg": _cfg.get("opponents", {}).get("w_per_kg", 3.0), "spread": 0.3}
			"--ecran":
				_screen = val
			"--wkg":
				_cfg["opponents"] = {"count": _cfg.get("opponents", {}).get("count", 5), "w_per_kg": val.to_float(), "spread": 0.3}
			"--accelere":
				Engine.time_scale = val.to_float()
			"--camera":
				_camera_mode = posmod(val.to_int(), CAMERA_MODES)
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


func _start() -> void:
	if _cfg.get("mode") == "seance" and not _cfg.has("workout"):
		_cfg["workout"] = ride.workout_presets()[0]
	if _cfg.get("mode") == "adversaires" and not _cfg.has("opponents"):
		_cfg["opponents"] = {"count": 5, "w_per_kg": 3.0, "spread": 0.3}
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
	# Après le décor réel, les ascensions suivent l'altitude du relief.
	_hud.setup_climbs(ride.route_climbs())
	_hud.set_credits(ride.world_credits() if real else "Décor simplifié")
	_hud.setup_mode(_cfg)


func _begin_ride() -> void:
	# Mode test, ou pas de home trainer connecté : cycliste virtuel.
	var simulated: bool = _cfg.get("mode") == "test" or bool(ride.profile_get("app.test_mode")) or not App.devices.has_trainer()
	_cfg["simulated"] = simulated
	if not ride.start_ride(_cfg):
		_show_fatal(ride.error_message())
		return
	ride.set_auto_shift(_auto_shift)
	if _skip_to > 0.0:
		ride.skip_to(_skip_to)
	for o in ride.opponents_state():
		var c := Cyclist.new()
		c.kit = _opponents.size() + 1
		c.visible = false
		add_child(c)
		_opponents.append(c)
	_riding = true
	_place_cyclist(0.0, true)
	var note := _decor_note
	if simulated and _cfg.get("mode") != "test":
		note = "Aucun home trainer connecté : cycliste virtuel"
	if note != "":
		_hud.show_banner(note)
		get_tree().create_timer(3.0).timeout.connect(func(): _hud.show_banner(""))


func _process(delta: float) -> void:
	if _loading_phase != "":
		_poll_loading(delta)
		return
	if not _riding:
		return
	_ride_time += delta
	_place_cyclist(delta, false)
	if _finish_time >= 0.0 and _pause == null:
		_finish_time += delta
		if _finish_time > FINISH_HOLD_S and _capture_path == "":
			_end_ride(true)
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
	_cyclist.set_state(t.speed_kmh / 3.6, t.power_w, t.grade_pct / 100.0, t.rider_kg, delta)
	_cyclist.animate(t.speed_kmh / 3.6, t.cadence_rpm, delta)
	_update_camera(pos, dir, delta, snap)
	if _real_world:
		# Les carrés détaillés se chargent un peu en avant du cycliste.
		_real_world.focus = pos + Vector3(dir.x, 0.0, dir.z).normalized() * 150.0
	_hud.update(t, delta)
	_place_opponents(pos, delta, snap)
	if t.finished and not _finished:
		_finished = true
		_finish_time = 0.0
		_hud.show_banner("Arrivée !\n%s" % _hud._clock.text)


## Coureurs virtuels : position, posture et classement.
func _place_opponents(player: Vector3, delta: float, snap: bool) -> void:
	if _opponents.is_empty():
		return
	var list: Array = ride.opponents_state()
	_hud.update_opponents(list)
	for i in mini(list.size(), _opponents.size()):
		var o: Dictionary = list[i]
		var c := _opponents[i]
		var p: Vector3 = o.position - Vector3(0.0, _base_elev, 0.0)
		c.visible = p.distance_to(player) < OPPONENT_SHOW_M
		if not c.visible:
			continue
		var dir: Vector3 = o.direction
		if dir.length() < 0.01:
			dir = Vector3.FORWARD
		var basis := Basis.looking_at(dir.normalized(), Vector3.UP) * Basis(Vector3(0.0, 0.0, 1.0), float(o.lean))
		c.transform = Transform3D(basis, p + Vector3(0.0, 0.05, 0.0))
		c.set_state(o.speed_kmh / 3.6, o.power_w, o.grade_pct / 100.0, o.rider_kg, 0.0 if snap else delta)
		c.animate(o.speed_kmh / 3.6, o.cadence_rpm, delta)


func _update_camera(pos: Vector3, dir: Vector3, delta: float, snap: bool) -> void:
	var flat := Vector3(dir.x, 0.0, dir.z).normalized()
	var side := flat.cross(Vector3.UP)
	var eye: Vector3
	var look := pos + Vector3(0.0, 1.1, 0.0)
	# Cap suivi avec retard : dans les virages, la caméra proche glisse vers
	# l'extérieur et montre le cycliste un peu de côté.
	var yaw := atan2(-flat.x, -flat.z)
	if snap:
		_chase_yaw = yaw
		_prev_yaw = yaw
		_yaw_rate = 0.0
	elif delta > 0.0:
		var rate := wrapf(yaw - _prev_yaw, -PI, PI) / delta
		_prev_yaw = yaw
		_yaw_rate = lerpf(_yaw_rate, rate, 1.0 - exp(-delta / CHASE_RATE_S))
		var aim := yaw - clampf(_yaw_rate * CHASE_TURN_S, -CHASE_MAX_LAG, CHASE_MAX_LAG)
		_chase_yaw += wrapf(aim - _chase_yaw, -PI, PI) * (1.0 - exp(-delta / CHASE_LAG_S))
	_chase_yaw = yaw - clampf(wrapf(yaw - _chase_yaw, -PI, PI), -CHASE_MAX_LAG, CHASE_MAX_LAG)
	match _camera_mode:
		0:
			var chase := Vector3(-sin(_chase_yaw), 0.0, -cos(_chase_yaw))
			eye = pos - chase * CHASE_DIST + Vector3(0.0, CHASE_HEIGHT, 0.0)
			look = pos + dir.normalized() * 3.0 + Vector3(0.0, 0.55, 0.0)
		1:
			eye = pos - flat * 6.5 + Vector3(0.0, 2.6, 0.0)
		2:
			eye = pos + side * 5.0 + flat * 1.5 + Vector3(0.0, 1.4, 0.0)
		_:
			eye = pos - flat * 25.0 + Vector3(0.0, 14.0, 0.0)
	var target := Transform3D(Basis.looking_at(look - eye, Vector3.UP), eye)
	if _camera_mode == 0 and not snap:
		# Déjà lissée par le cap : un suivi serré évite que la caméra décroche.
		_camera.transform = _camera.transform.interpolate_with(target, clampf(delta * 12.0, 0.0, 1.0))
	elif snap:
		_camera.transform = target
	else:
		_camera.transform = _camera.transform.interpolate_with(target, clampf(delta * 4.0, 0.0, 1.0))


func _unhandled_input(event: InputEvent) -> void:
	if not _riding or _pause != null or not (event is InputEventKey) or not event.pressed or event.echo:
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
			_camera_mode = (_camera_mode + 1) % CAMERA_MODES
		KEY_P:
			_hud.toggle_profile()
		KEY_SPACE, KEY_ESCAPE:
			_open_pause()
	get_viewport().set_input_as_handled()


## Pause : la sortie s'arrête et le menu permet d'ajuster les réglages.
func _open_pause() -> void:
	ride.set_paused(true)
	_pause = PauseMenu.new()
	add_child(_pause)
	var state: Dictionary = ride.ride_state()
	state["camera"] = _camera_mode
	state["auto_shift"] = _auto_shift
	_pause.setup(state)
	_pause.resumed.connect(_close_pause)
	_pause.adjusted.connect(_on_adjusted)
	_pause.finished.connect(func(save: bool): _end_ride(save))


func _close_pause() -> void:
	if _pause:
		_pause.queue_free()
		_pause = null
	if not _finished:
		ride.set_paused(false)


func _on_adjusted(key: String, value: Variant) -> void:
	match key:
		"camera":
			_camera_mode = posmod(int(value), CAMERA_MODES)
		"auto_shift":
			_auto_shift = bool(value)
			ride.set_auto_shift(_auto_shift)
		_:
			ride.adjust(key, value)


## Fin de la sortie : enregistrement, puis bilan (ou retour direct au menu).
func _end_ride(save: bool) -> void:
	if not _riding:
		return
	_riding = false
	if _pause:
		_pause.queue_free()
		_pause = null
	var id: int = ride.finish_ride(save)
	if not save or id < 0:
		if save:
			push_warning(ride.error_message())
		get_tree().reload_current_scene()
		return
	# Le bilan s'affiche dans le menu, une fois le décor libéré.
	App.finished_ride_id = id
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
