extends MarginContainer
## Écran Paramètres : cycliste, vélo, home trainer, affichage et Strava.
## Chaque champ écrit directement dans le profil ; une saisie refusée est
## signalée en rouge sous le champ. Le profil est enregistré avec le bouton
## « Enregistrer » et en quittant l'écran.

const UI := preload("res://scripts/ui/theme.gd")
const Fmt := preload("res://scripts/ui/fmt.gd")
const NumberField := preload("res://scripts/ui/number_field.gd")
const Chart := preload("res://scripts/ui/chart.gd")
const Training := preload("res://scripts/ui/training.gd")

const TABS := ["Cycliste", "Vélo", "Home trainer", "Affichage et décor", "Strava"]
const WHEELS := [["700×25c", 2.105], ["700×28c", 2.136], ["700×32c", 2.155], ["700×40c", 2.200]]
const STRAVA_API := "https://www.strava.com/settings/api"
const LABEL_W := 250

var menu: Node

var _tabs: TabContainer
var _status: Label
var _dirty := false
var _strava := {}
var _poll := 0.0
var _zones_box: VBoxContainer
var _rider_summary: Label
var _gear_chart: Control
var _grade_chart: Control


func _ready() -> void:
	for side in ["left", "right"]:
		add_theme_constant_override("margin_" + side, 32)
	add_theme_constant_override("margin_top", 26)
	add_theme_constant_override("margin_bottom", 28)
	var col := UI.vbox(14)
	add_child(col)
	var head := UI.hbox(16)
	col.add_child(head)
	var hv := UI.vbox(2)
	hv.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	hv.add_child(UI.label("Paramètres", "H1"))
	hv.add_child(UI.label("Tout est enregistré dans ton profil et repris à la prochaine ouverture.", "DimLabel"))
	head.add_child(hv)
	_status = UI.label("", "MutedLabel")
	_status.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	head.add_child(_status)
	var save := UI.primary_button("Enregistrer")
	save.custom_minimum_size.x = 170
	save.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	save.pressed.connect(_save)
	head.add_child(save)

	_tabs = TabContainer.new()
	_tabs.size_flags_vertical = Control.SIZE_EXPAND_FILL
	col.add_child(_tabs)


func enter() -> void:
	var current := _tabs.current_tab
	for c in _tabs.get_children():
		_tabs.remove_child(c)
		c.queue_free()
	_strava = {}
	_build_rider(_page(0))
	_build_bike(_page(1))
	_build_trainer(_page(2))
	_build_display(_page(3))
	_build_strava(_page(4))
	_tabs.current_tab = maxi(current, 0)
	_dirty = false
	_status.text = ""
	_refresh_extras()
	_refresh_strava()


func leave() -> void:
	if _dirty:
		_save()


func select_tab(i: int) -> void:
	_tabs.current_tab = clampi(i, 0, TABS.size() - 1)


func _save() -> void:
	if App.ride.profile_save():
		_dirty = false
		_status.text = "Enregistré ✓"
		_status.theme_type_variation = "OkLabel"
	else:
		_status.text = App.ride.error_message()
		_status.theme_type_variation = "ErrorLabel"


## Page d'onglet : formulaire à gauche, explications à droite.
func _page(index: int) -> VBoxContainer:
	var scroll := ScrollContainer.new()
	scroll.name = TABS[index]
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_tabs.add_child(scroll)
	var page := UI.hbox(24)
	page.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(page)
	var left := UI.vbox(20)
	left.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	left.size_flags_stretch_ratio = 1.7
	page.add_child(left)
	var card := UI.card()
	left.add_child(card)
	var form := UI.vbox(14)
	card.add_child(form)
	var right := UI.vbox(20)
	right.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	page.add_child(right)
	var help := UI.card("CardHi")
	right.add_child(help)
	var hv := UI.vbox(10)
	help.add_child(hv)
	form.set_meta("help", hv)
	form.set_meta("left", left)
	form.set_meta("right", right)
	return form


## Carte supplémentaire sous le formulaire (« left ») ou sous l'aide (« right »).
func _extra_card(form: Control, side: String, title: String) -> VBoxContainer:
	var card := UI.card()
	(form.get_meta(side) as Control).add_child(card)
	var v := UI.vbox(12)
	card.add_child(v)
	v.add_child(UI.section_title(title))
	return v


func _help(form: Control, paragraphs: Array) -> void:
	var hv: VBoxContainer = form.get_meta("help")
	hv.add_child(UI.section_title("À savoir"))
	for p in paragraphs:
		hv.add_child(UI.label(p, "DimLabel", true))


# --- Onglets ---------------------------------------------------------------------

func _build_rider(f: VBoxContainer) -> void:
	f.add_child(UI.section_title("Cycliste"))
	_text(f, "rider.name", "Nom", false, "Ton prénom")
	_num(f, "rider.weight_kg", "Poids", 30.0, 200.0, 0.5, 1, "kg")
	_num(f, "rider.ftp_w", "FTP (puissance au seuil)", 0.0, 600.0, 5.0, 0, "W", 1.0, "0 = inconnue : 200 W sont utilisés.", true)
	var z := _extra_card(f, "left", "Tes zones de puissance")
	_rider_summary = UI.label("", "DimLabel", true)
	z.add_child(_rider_summary)
	_zones_box = UI.vbox(0)
	z.add_child(_zones_box)
	_help(f, [
		"Le poids sert au calcul de la vitesse en montée et des W/kg.",
		"La FTP est la puissance que tu peux tenir environ une heure. Elle règle l'intensité des séances et la charge d'entraînement (TSS).",
		"Tu ne la connais pas ? Laisse 0 : 200 W sont utilisés. L'historique propose une estimation à partir de tes meilleures 20 minutes.",
	])


func _build_bike(f: VBoxContainer) -> void:
	f.add_child(UI.section_title("Vélo"))
	_text(f, "bike.name", "Nom du vélo", false, "Route, gravel…")
	_num(f, "bike.weight_kg", "Poids du vélo", 3.0, 40.0, 0.1, 1, "kg")
	_list(f, "bike.chainrings", "Plateaux", "36, 52")
	_list(f, "bike.cassette", "Cassette", "11, 13, 15…")
	_wheel(f)
	f.add_child(HSeparator.new())
	f.add_child(UI.section_title("Résistance à l'avancement"))
	_num(f, "bike.cda_m2", "CdA (traînée aérodynamique)", 0.15, 0.6, 0.01, 2, "m²")
	_num(f, "bike.crr", "Crr (résistance au roulement)", 0.001, 0.02, 0.0005, 4, "")
	_num(f, "bike.drivetrain_efficiency", "Rendement de la transmission", 80.0, 100.0, 0.5, 1, "%", 100.0)
	var g := _extra_card(f, "right", "Développement de chaque vitesse")
	_gear_chart = Chart.new()
	_gear_chart.x_format = func(v: float, long: bool) -> String: return ("pignon de %d" if long else "%d") % roundi(v)
	_gear_chart.y_format = func(v: float, long: bool) -> String: return Fmt.num(v, 2 if long else 1) + " m"
	_gear_chart.y_from_zero = false
	_gear_chart.custom_minimum_size.y = 230
	g.add_child(_gear_chart)
	g.add_child(UI.label("Distance parcourue à chaque tour de pédale.", "MutedLabel"))
	_help(f, [
		"Les plateaux et la cassette, du plus petit au plus grand, donnent les vitesses virtuelles que tu changes avec les Click ou les flèches ↑ ↓.",
		"La circonférence de roue est indiquée sur le flanc du pneu (700×28c ≈ 2,136 m).",
		"CdA : environ 0,25 m² en position aéro, 0,32 m² mains en haut du cintre. Crr : environ 0,004 pour un bon pneu route, 0,006 pour un pneu gravel.",
	])


func _build_trainer(f: VBoxContainer) -> void:
	f.add_child(UI.section_title("Rapport physique"))
	var row := UI.hbox(10)
	var ring := _num_field("trainer.physical_chainring", 20.0, 60.0, 1.0, 0, 1.0, true, 80.0)
	var cog := _num_field("trainer.physical_cog", 9.0, 40.0, 1.0, 0, 1.0, true, 80.0)
	row.add_child(ring[0])
	var x := UI.label("×", "DimLabel")
	row.add_child(x)
	row.add_child(cog[0])
	row.add_child(UI.label("plateau × pignon", "UnitLabel"))
	var err := _row(f, "Rapport où reste la chaîne", row)
	ring[1].call(err)
	cog[1].call(err)
	f.add_child(HSeparator.new())
	f.add_child(UI.section_title("Pentes"))
	_slider(f, "trainer.grade_difficulty", "Difficulté des pentes", 0.0, 1.0, 0.05, func(v: float) -> String: return Fmt.percent(v))
	_num(f, "trainer.min_grade", "Pente minimale envoyée", -30.0, 0.0, 0.5, 1, "%", 100.0, "Descentes : au-delà, la pente est plafonnée.")
	_num(f, "trainer.max_grade", "Pente maximale envoyée", 0.0, 30.0, 0.5, 1, "%", 100.0, "Montées : au-delà, la pente est plafonnée.")
	var gr := _extra_card(f, "right", "Pente ressentie sur le home trainer")
	_grade_chart = Chart.new()
	_grade_chart.x_format = func(v: float, long: bool) -> String: return ("pente réelle %s %%" if long else "%s %%") % Fmt.num(v)
	_grade_chart.y_format = func(v: float, _long: bool) -> String: return Fmt.num(v, 1 if absf(v - roundf(v)) > 0.01 else 0) + " %"
	_grade_chart.y_from_zero = false
	_grade_chart.custom_minimum_size.y = 230
	gr.add_child(_grade_chart)
	_help(f, [
		"En changement de vitesse virtuel, la chaîne reste sur un seul rapport du vélo : indique celui sur lequel tu roules, par exemple 52 × 19.",
		"La difficulté des pentes adoucit ce que le home trainer fait ressentir : à 50 %, une pente de 8 % se roule comme 4 %. La vitesse à l'écran reste celle de la vraie pente.",
		"La plage de pente protège le home trainer et tes genoux des passages extrêmes du GPX.",
	])


func _build_display(f: VBoxContainer) -> void:
	f.add_child(UI.section_title("Au départ"))
	_segment(f, "app.camera", "Caméra", ["Proche", "Arrière", "De côté", "Haute"], [0, 1, 2, 3])
	_check(f, "app.auto_shift", "Click automatiques", "Les vitesses changent seules pour garder ta cadence.")
	f.add_child(HSeparator.new())
	f.add_child(UI.section_title("Décor"))
	_option(f, "app.decor", "Décor", [
		["Réel : relief, images aériennes et OpenStreetMap", "reel"],
		["Réel sans connexion (données déjà en cache)", "cache"],
		["Simplifié (aucun téléchargement)", "simple"],
	])
	_segment(f, "scenery.quality", "Qualité graphique", ["Basse", "Moyenne", "Haute"], [0, 1, 2])
	var keys_card := UI.card()
	keys_card.add_child(Training.keys_help())
	(f.get_meta("right") as Control).add_child(keys_card)
	_help(f, [
		"La caméra se change aussi pendant la sortie avec la touche C, et les Click automatiques avec la touche A.",
		"Le décor réel télécharge le relief, les images aériennes et les données OpenStreetMap autour du parcours, puis les garde en cache pour les sorties suivantes.",
		"La qualité règle la distance d'affichage, les ombres et la résolution 3D : baisse-la si l'image saccade.",
	])


func _build_strava(f: VBoxContainer) -> void:
	f.add_child(UI.section_title("Compte"))
	var box := UI.card("Tile")
	f.add_child(box)
	var row := UI.hbox(12)
	box.add_child(row)
	var dot := UI.status_dot(UI.TEXT_MUTED, 12)
	row.add_child(dot)
	var tv := UI.vbox(2)
	tv.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(tv)
	var title := UI.label("", "H3")
	tv.add_child(title)
	var msg := UI.label("", "MutedLabel", true)
	tv.add_child(msg)
	var connect_b := UI.button("Se connecter à Strava", "PrimaryButton")
	connect_b.add_theme_font_size_override("font_size", 16)
	connect_b.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	connect_b.pressed.connect(func():
		App.ride.profile_save()
		_dirty = false
		App.strava.connect_account())
	row.add_child(connect_b)
	var disconnect_b := UI.button("Se déconnecter", "DangerButton")
	disconnect_b.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	disconnect_b.pressed.connect(func(): App.strava.disconnect_account())
	row.add_child(disconnect_b)
	_strava = {"dot": dot, "title": title, "msg": msg, "connect": connect_b, "disconnect": disconnect_b}
	_check(f, "strava.auto_upload", "Envoyer automatiquement", "Chaque sortie enregistrée part sur Strava sans le demander.")
	f.add_child(HSeparator.new())
	f.add_child(UI.section_title("Application API Strava"))
	_text(f, "strava.client_id", "Identifiant client (Client ID)", false, "123456")
	_text(f, "strava.client_secret", "Secret client (Client Secret)", true, "")
	var link := UI.button("Ouvrir strava.com/settings/api  ↗", "LinkButtonish")
	link.pressed.connect(func(): OS.shell_open(STRAVA_API))
	_row(f, "", link)
	_help(f, [
		"Strava demande à chaque utilisateur de créer sa propre « application API », gratuite :",
		"1. Ouvre strava.com/settings/api et crée l'application (nom et site web au choix).",
		"2. Dans « Domaine du rappel d'autorisation », indique localhost.",
		"3. Copie l'identifiant client et le secret client ici, puis clique sur « Se connecter à Strava » : ton navigateur s'ouvre pour autoriser l'envoi des sorties.",
	])


## Met à jour les aperçus calculés (zones, développements, pentes).
func _refresh_extras() -> void:
	var ftp_v := _get_float("rider.ftp_w")
	var ftp := ftp_v if ftp_v > 0.0 else 200.0
	var kg := maxf(_get_float("rider.weight_kg"), 1.0)
	if _zones_box:
		for c in _zones_box.get_children():
			c.queue_free()
		_zones_box.add_child(UI.zones_grid(ftp))
		_rider_summary.text = "FTP %s%s, soit %s W/kg ; masse totale avec le vélo : %s kg." % [
			Fmt.watts(ftp), "" if ftp_v > 0.0 else " (par défaut)", Fmt.num(ftp / kg, 1), Fmt.num(kg + _get_float("bike.weight_kg"), 1)]
	if _gear_chart:
		var wheel := _get_float("bike.wheel_circumference_m")
		var rings: Variant = App.ride.profile_get("bike.chainrings")
		var cogs: Variant = App.ride.profile_get("bike.cassette")
		var series := []
		var colors := [UI.TEAL, UI.ACCENT, UI.INFO]
		if rings is Array and cogs is Array:
			for i in rings.size():
				var pts := PackedVector2Array()
				for c in cogs:
					if int(c) > 0:
						pts.append(Vector2(int(c), float(rings[i]) / int(c) * wheel))
				series.append({"name": "Plateau de %d" % int(rings[i]), "color": colors[i % colors.size()], "kind": "line", "points": pts})
		_gear_chart.set_series(series)
	if _grade_chart:
		var diff := _get_float("trainer.grade_difficulty")
		var lo := _get_float("trainer.min_grade") * 100.0
		var hi := _get_float("trainer.max_grade") * 100.0
		var real := PackedVector2Array()
		var felt := PackedVector2Array()
		for g in range(-15, 26):
			real.append(Vector2(g, g))
			felt.append(Vector2(g, clampf(g * diff, lo, hi)))
		_grade_chart.set_series([
			{"name": "Pente réelle", "color": UI.TEXT_MUTED, "kind": "line", "points": real, "width": 1.5},
			{"name": "Pente ressentie", "color": UI.ACCENT, "kind": "line", "points": felt, "width": 2.5},
		])


# --- Champs ------------------------------------------------------------------------

## Ligne de formulaire ; renvoie l'étiquette d'erreur placée dessous.
func _row(parent: Control, title: String, control: Control, unit := "", hint := "") -> Label:
	var box := UI.vbox(4)
	var row := UI.hbox(14)
	box.add_child(row)
	var l := UI.label(title)
	l.custom_minimum_size.x = LABEL_W
	l.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	l.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	row.add_child(l)
	control.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	row.add_child(control)
	if unit != "":
		var u := UI.label(unit, "UnitLabel")
		u.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		row.add_child(u)
	if hint != "":
		var h := UI.label(hint, "MutedLabel", true)
		h.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		h.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		row.add_child(h)
	var err := UI.label("", "ErrorLabel", true)
	err.visible = false
	var pad := MarginContainer.new()
	pad.add_theme_constant_override("margin_left", LABEL_W + 14)
	pad.add_child(err)
	box.add_child(pad)
	parent.add_child(box)
	return err


## Écrit une valeur du profil ; affiche l'erreur éventuelle sous le champ.
func _write(path: String, value: Variant, err: Label) -> bool:
	if App.ride.profile_set(path, value):
		err.visible = false
		_dirty = true
		_refresh_extras()
		_status.text = "Modifications non enregistrées"
		_status.theme_type_variation = "MutedLabel"
		return true
	var m: String = App.ride.error_message()
	err.text = m if m != "" else "Valeur refusée."
	err.visible = true
	return false


func _get_float(path: String) -> float:
	var v: Variant = App.ride.profile_get(path)
	return float(v) if v != null else 0.0


## Champ numérique seul, et une fonction qui le relie à son étiquette d'erreur.
func _num_field(path: String, lo: float, hi: float, step: float, decimals: int, scale := 1.0, as_int := false, width := 110.0) -> Array:
	var f: LineEdit = NumberField.new()
	f.setup(lo, hi, step, decimals, _get_float(path) * scale, width)
	var bind := func(err: Label) -> void:
		f.value_changed.connect(func(x: float):
			_write(path, roundi(x) if as_int else x / scale, err))
	return [f, bind]


func _num(parent: Control, path: String, title: String, lo: float, hi: float, step: float, decimals: int,
		unit := "", scale := 1.0, hint := "", as_int := false) -> LineEdit:
	var pair := _num_field(path, lo, hi, step, decimals, scale, as_int)
	var err := _row(parent, title, pair[0], unit, hint)
	pair[1].call(err)
	return pair[0]


func _text(parent: Control, path: String, title: String, secret := false, placeholder := "") -> LineEdit:
	var e := LineEdit.new()
	var v: Variant = App.ride.profile_get(path)
	e.text = str(v) if v != null else ""
	e.placeholder_text = placeholder
	e.secret = secret
	e.custom_minimum_size.x = 340
	var holder: Control = e
	if secret:
		var row := UI.hbox(8)
		row.add_child(e)
		var reveal := UI.button("Afficher", "GhostButton")
		reveal.toggle_mode = true
		reveal.toggled.connect(func(on: bool):
			e.secret = not on
			reveal.text = "Masquer" if on else "Afficher")
		row.add_child(reveal)
		holder = row
	var err := _row(parent, title, holder)
	var commit := func() -> void:
		var cur: Variant = App.ride.profile_get(path)
		if e.text != (str(cur) if cur != null else ""):
			_write(path, e.text.strip_edges(), err)
	e.text_submitted.connect(func(_t: String): commit.call())
	e.focus_exited.connect(commit)
	return e


## Liste d'entiers saisie « 36, 52 ».
func _list(parent: Control, path: String, title: String, placeholder: String) -> LineEdit:
	var e := LineEdit.new()
	e.text = Fmt.int_list(App.ride.profile_get(path))
	e.placeholder_text = placeholder
	e.custom_minimum_size.x = 340
	var err := _row(parent, title, e, "dents")
	var commit := func() -> void:
		var values := Fmt.parse_int_list(e.text)
		if values.is_empty():
			err.text = "Saisis des nombres entiers séparés par des virgules, par exemple « %s »." % placeholder
			err.visible = true
			return
		if _write(path, values, err):
			e.text = Fmt.int_list(values)
	e.text_submitted.connect(func(_t: String): commit.call())
	e.focus_exited.connect(commit)
	return e


func _check(parent: Control, path: String, title: String, hint := "") -> CheckButton:
	var c := CheckButton.new()
	c.button_pressed = App.ride.profile_get(path) == true
	var err := _row(parent, title, c, "", hint)
	c.toggled.connect(func(on: bool): _write(path, on, err))
	return c


func _option(parent: Control, path: String, title: String, items: Array) -> OptionButton:
	var o := OptionButton.new()
	var cur: Variant = App.ride.profile_get(path)
	for i in items.size():
		o.add_item(items[i][0])
		if items[i][1] == cur:
			o.selected = i
	o.custom_minimum_size.x = 400
	var err := _row(parent, title, o)
	o.item_selected.connect(func(i: int): _write(path, items[i][1], err))
	return o


func _segment(parent: Control, path: String, title: String, labels: Array, values: Array) -> void:
	var cur: Variant = App.ride.profile_get(path)
	var sel := 0
	for i in values.size():
		if cur != null and int(cur) == values[i]:
			sel = i
	var err_ref: Array = []
	var seg := UI.segmented(labels, sel, func(i: int): _write(path, values[i], err_ref[0]))
	err_ref.append(_row(parent, title, seg))


func _slider(parent: Control, path: String, title: String, lo: float, hi: float, step: float, fmt: Callable) -> HSlider:
	var row := UI.hbox(14)
	var s := HSlider.new()
	s.min_value = lo
	s.max_value = hi
	s.step = step
	s.value = _get_float(path)
	s.custom_minimum_size = Vector2(300, 28)
	row.add_child(s)
	var val := UI.label(fmt.call(s.value), "H3")
	val.custom_minimum_size.x = 64
	row.add_child(val)
	var err := _row(parent, title, row)
	s.value_changed.connect(func(v: float):
		val.text = fmt.call(v)
		_write(path, v, err))
	return s


## Circonférence de roue : tailles courantes ou valeur libre.
func _wheel(parent: Control) -> void:
	var row := UI.hbox(10)
	var o := OptionButton.new()
	for w in WHEELS:
		o.add_item("%s · %s m" % [w[0], Fmt.num(w[1], 3)])
	o.add_item("Autre")
	o.custom_minimum_size.x = 210
	row.add_child(o)
	var pair := _num_field("bike.wheel_circumference_m", 1.5, 2.5, 0.001, 3, 1.0, false, 100.0)
	var f: LineEdit = pair[0]
	row.add_child(f)
	row.add_child(UI.label("m", "UnitLabel"))
	var err := _row(parent, "Circonférence de roue", row)
	pair[1].call(err)
	var sync := func(v: float) -> void:
		o.selected = WHEELS.size()
		for i in WHEELS.size():
			if absf(WHEELS[i][1] - v) < 0.0005:
				o.selected = i
	sync.call(f.value)
	f.value_changed.connect(sync)
	o.item_selected.connect(func(i: int):
		if i < WHEELS.size():
			f.set_value(WHEELS[i][1], true))


# --- Strava ---------------------------------------------------------------------------

func _process(delta: float) -> void:
	if not is_visible_in_tree() or _strava.is_empty():
		return
	_poll -= delta
	if _poll <= 0.0:
		_poll = 0.5
		_refresh_strava()


func _refresh_strava() -> void:
	if _strava.is_empty():
		return
	var st: Dictionary = App.strava.status()
	var connected: bool = st.get("connected", false)
	var configured: bool = st.get("configured", false)
	UI.set_dot(_strava.dot, Color("fc4c02") if connected else (UI.WARN if st.get("busy", false) else UI.TEXT_MUTED))
	if connected:
		var who: String = st.get("athlete", "")
		_strava.title.text = "Connecté à Strava" + (" : " + who if who != "" else "")
	else:
		_strava.title.text = "Non connecté à Strava"
	var m: String = st.get("message", "")
	if m == "":
		m = "Tes sorties peuvent être envoyées sur Strava." if connected else ("Clique sur « Se connecter à Strava » pour autoriser l'envoi." if configured else "Renseigne d'abord l'identifiant et le secret client ci-dessous.")
	_strava.msg.text = m
	_strava.connect.visible = not connected
	_strava.connect.disabled = st.get("busy", false) or not (configured or _has_keys())
	_strava.disconnect.visible = connected


func _has_keys() -> bool:
	return str(App.ride.profile_get("strava.client_id")) not in ["", "<null>"] and str(App.ride.profile_get("strava.client_secret")) not in ["", "<null>"]
