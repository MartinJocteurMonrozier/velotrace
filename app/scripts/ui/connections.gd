extends MarginContainer
## Écran Connexions : état du Bluetooth, recherche des appareils, un rôle par
## carte (home trainer, cadence, fréquence cardiaque, Zwift Click) avec la
## valeur mesurée en direct, et mode test à appareils simulés.

const UI := preload("res://scripts/ui/theme.gd")
const Fmt := preload("res://scripts/ui/fmt.gd")

const ROLES := ["trainer", "cadence", "heart_rate", "click"]
const ROLE_NAMES := {"trainer": "Home trainer", "cadence": "Capteur de cadence", "heart_rate": "Fréquence cardiaque", "click": "Zwift Click"}
const ROLE_SHORT := {"trainer": "Home trainer", "cadence": "Cadence", "heart_rate": "Cardio", "click": "Click"}
const STATE_NAMES := {"connected": "Connecté", "connecting": "Connexion en cours…", "failed": "Échec de la connexion", "disconnected": "Non connecté"}
const SCAN_S := 10.0
## Durée de l'éclair après un appui sur un Click.
const FLASH_S := 0.6

var menu: Node

var _test: CheckButton
var _adapter_dot: Panel
var _adapter_label: Label
var _retry: Button
var _scan: Button
var _scan_bar: ProgressBar
var _scan_label: Label
var _scan_left := 0.0
var _found_box: VBoxContainer
var _found_card: Control
var _test_note: Control
var _cards := {}
var _found_sig := "-"
var _poll := 0.0


func _ready() -> void:
	for side in ["left", "right"]:
		add_theme_constant_override("margin_" + side, 32)
	add_theme_constant_override("margin_top", 26)
	add_theme_constant_override("margin_bottom", 28)
	var col := UI.vbox(20)
	add_child(col)

	var head := UI.hbox(16)
	col.add_child(head)
	var hv := UI.vbox(2)
	hv.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	hv.add_child(UI.label("Connexions", "H1"))
	hv.add_child(UI.label("Vérifie tes appareils avant de rouler : chaque carte affiche la mesure en direct.", "DimLabel"))
	head.add_child(hv)
	var chip := UI.card("Chip")
	chip.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	var cr := UI.hbox(10)
	chip.add_child(cr)
	_adapter_dot = UI.status_dot(UI.TEXT_MUTED)
	cr.add_child(_adapter_dot)
	_adapter_label = UI.label("", "DimLabel")
	cr.add_child(_adapter_label)
	head.add_child(chip)
	_retry = UI.button("Réessayer")
	_retry.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_retry.pressed.connect(func(): App.devices.retry())
	head.add_child(_retry)

	var body := UI.hbox(24)
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	col.add_child(body)

	# À gauche : une carte par rôle, puis l'aide.
	var left := UI.vbox(18)
	left.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_child(left)
	var grid := GridContainer.new()
	grid.columns = 2
	grid.add_theme_constant_override("h_separation", 18)
	grid.add_theme_constant_override("v_separation", 18)
	grid.size_flags_vertical = Control.SIZE_EXPAND_FILL
	left.add_child(grid)
	for role in ROLES:
		grid.add_child(_role_card(role))
	var help := UI.card("InfoBox")
	var hl := UI.vbox(6)
	help.add_child(hl)
	for line in [
		"Ferme d'abord Zwift, MyWhoosh ou toute autre application : un home trainer n'accepte qu'une seule connexion Bluetooth à la fois.",
		"Les Zwift Click V2 doivent parfois être déverrouillés chaque jour : ouvre une fois l'application Zwift, attends qu'elle les reconnaisse, puis ferme-la.",
		"Chaque appareil connecté est retenu pour son rôle ; « Oublier » le dissocie.",
	]:
		hl.add_child(UI.label("•  " + line, "DimLabel", true))
	left.add_child(help)

	# À droite : mode test et appareils à proximité.
	var right := UI.vbox(18)
	right.custom_minimum_size.x = 500
	body.add_child(right)
	var tcard := UI.card()
	right.add_child(tcard)
	var tv := UI.vbox(6)
	tcard.add_child(tv)
	_test = CheckButton.new()
	_test.text = "Mode test : appareils simulés"
	_test.add_theme_font_override("font", UI.font(600))
	_test.toggled.connect(_on_test_toggled)
	tv.add_child(_test)
	tv.add_child(UI.label("Pour essayer un parcours sans home trainer : la puissance, la cadence, la fréquence cardiaque et les Click sont imités.", "MutedLabel", true))

	_found_card = UI.card()
	_found_card.size_flags_vertical = Control.SIZE_EXPAND_FILL
	right.add_child(_found_card)
	var fv := UI.vbox(12)
	_found_card.add_child(fv)
	var fh := UI.hbox(12)
	fv.add_child(fh)
	var ft := UI.section_title("Appareils à proximité")
	ft.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	ft.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	fh.add_child(ft)
	_scan = UI.button("Rechercher")
	_scan.pressed.connect(_start_scan)
	fh.add_child(_scan)
	var sr := UI.hbox(10)
	fv.add_child(sr)
	_scan_bar = ProgressBar.new()
	_scan_bar.theme_type_variation = "TealProgress"
	_scan_bar.show_percentage = false
	_scan_bar.max_value = 1.0
	_scan_bar.custom_minimum_size.y = 6
	_scan_bar.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scan_bar.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	sr.add_child(_scan_bar)
	_scan_label = UI.label("", "MutedLabel")
	sr.add_child(_scan_label)
	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	fv.add_child(scroll)
	_found_box = UI.vbox(10)
	_found_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(_found_box)

	_test_note = UI.card("InfoBox")
	_test_note.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	_test_note.add_child(UI.label("Le mode test est actif : le Bluetooth n'est pas utilisé. Désactive-le pour rechercher et connecter tes appareils.", "DimLabel", true))
	right.add_child(_test_note)


func _role_card(role: String) -> Control:
	var card := UI.card()
	card.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	card.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var v := UI.vbox(8)
	card.add_child(v)
	var top := UI.hbox(10)
	v.add_child(top)
	var dot := UI.status_dot(UI.TEXT_MUTED, 12)
	top.add_child(dot)
	var title := UI.label(ROLE_NAMES[role], "H3")
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	top.add_child(title)
	var battery := UI.label("", "MutedLabel")
	top.add_child(battery)
	var dev := UI.label("", "DimLabel")
	dev.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	dev.clip_text = true
	v.add_child(dev)
	var state := UI.label("", "MutedLabel")
	v.add_child(state)
	v.add_child(UI.spacer())

	var live := UI.hbox(28)
	live.alignment = BoxContainer.ALIGNMENT_CENTER
	v.add_child(live)
	var values := []
	var units: Array = {"trainer": ["W", "rpm"], "cadence": ["rpm"], "heart_rate": ["bpm"], "click": [""]}[role]
	for u in units:
		var pair := UI.hbox(6)
		var val := UI.label("--", "BigValueLabel")
		val.add_theme_font_size_override("font_size", 54)
		pair.add_child(val)
		var ul := UI.label(u, "UnitLabel")
		ul.add_theme_font_size_override("font_size", 18)
		ul.size_flags_vertical = Control.SIZE_SHRINK_END
		pair.add_child(ul)
		live.add_child(pair)
		values.append(val)
	v.add_child(UI.spacer())
	var bottom := UI.hbox(12)
	v.add_child(bottom)
	var hint := UI.label({"trainer": "Puissance et cadence mesurées", "cadence": "Cadence de pédalage", "heart_rate": "Battements par minute", "click": "Appuie sur un bouton pour l'essayer."}[role], "MutedLabel")
	hint.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	hint.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	bottom.add_child(hint)
	if role == "click":
		values.append(hint)
	var forget := UI.button("Oublier", "DangerButton", "Déconnecte et dissocie cet appareil")
	forget.pressed.connect(_forget.bind(role))
	bottom.add_child(forget)
	_cards[role] = {"dot": dot, "battery": battery, "device": dev, "state": state, "values": values, "forget": forget}
	return card


func enter() -> void:
	_test.set_pressed_no_signal(App.ride.profile_get("app.test_mode") == true)
	_found_sig = "-"
	_refresh(true)


func leave() -> void:
	App.ride.profile_save()


func _on_test_toggled(on: bool) -> void:
	if App.ride.profile_set("app.test_mode", on):
		App.ride.profile_save()
	_found_sig = "-"
	_refresh(true)


func _start_scan() -> void:
	App.devices.scan(SCAN_S)
	_scan_left = SCAN_S
	_refresh(true)


func _forget(role: String) -> void:
	App.devices.disconnect_kind(role)
	App.ride.forget_device(role)
	App.ride.profile_save()
	_found_sig = "-"
	_refresh(true)


func _connect_device(d: Dictionary, kind: String) -> void:
	App.devices.connect_device(d.id, kind)
	App.ride.remember_device(d.id, d.name, kind)
	App.ride.profile_save()
	_found_sig = "-"
	_refresh(true)


func _process(delta: float) -> void:
	if not is_visible_in_tree():
		return
	_scan_left = maxf(0.0, _scan_left - delta)
	_poll -= delta
	if _poll <= 0.0:
		_poll = 0.25
		_refresh(false)


func _refresh(full: bool) -> void:
	var test: bool = App.ride.profile_get("app.test_mode") == true
	var ad: Dictionary = App.devices.adapter_state()
	var st: String = ad.get("state", "starting")
	var msg: String = ad.get("message", "")
	if test:
		UI.set_dot(_adapter_dot, UI.TEAL)
		_adapter_label.text = "Mode test : appareils simulés"
	else:
		UI.set_dot(_adapter_dot, UI.link_color(st))
		var default_msg: String = {"starting": "Démarrage du Bluetooth…", "ready": "Bluetooth prêt", "scanning": "Recherche en cours…", "unavailable": "Bluetooth indisponible"}.get(st, st)
		_adapter_label.text = msg if msg != "" else default_msg
	_retry.visible = not test and st == "unavailable"
	var scanning := st == "scanning" or _scan_left > 0.0
	_scan.disabled = test or scanning or st == "unavailable"
	_scan.text = "Recherche…" if scanning else "Rechercher"
	_scan_bar.visible = scanning and not test
	_scan_bar.value = 1.0 - _scan_left / SCAN_S if _scan_left > 0.0 else 0.5 + 0.5 * sin(Time.get_ticks_msec() / 300.0)
	_scan_label.text = "%d s" % ceili(_scan_left) if _scan_left > 0.0 else ""
	_scan_label.visible = _scan_bar.visible
	_found_card.visible = not test
	_test_note.visible = test

	var found: Array = App.devices.found()
	var by_role := {}
	for d in found:
		if d.role != "":
			by_role[d.role] = d
	var remembered := {}
	for r in App.ride.remembered_devices():
		remembered[r.kind] = r
	var live: Dictionary = App.devices.live()
	for role in ROLES:
		_update_card(role, by_role.get(role, {}), remembered.get(role, {}), live, test)
	if not test:
		_update_found(found)


func _update_card(role: String, d: Dictionary, rem: Dictionary, live: Dictionary, test: bool) -> void:
	var c: Dictionary = _cards[role]
	var state: String = d.get("state", "disconnected" if not rem.is_empty() else "")
	var connected := test or state == "connected"
	if test:
		UI.set_dot(c.dot, UI.TEAL)
		c.device.text = "Appareil simulé"
		c.state.text = "Simulé par l'application"
		c.state.remove_theme_color_override("font_color")
	elif d.is_empty() and rem.is_empty():
		UI.set_dot(c.dot, Color(UI.TEXT_MUTED, 0.5))
		c.device.text = "Aucun appareil associé"
		c.state.text = "Lance une recherche, puis « Connecter »."
		c.state.remove_theme_color_override("font_color")
	else:
		UI.set_dot(c.dot, UI.link_color(state))
		c.device.text = d.get("name", rem.get("name", ""))
		var text: String = STATE_NAMES.get(state, state)
		if d.get("message", "") != "":
			text += " : " + d.message
		c.state.text = text
		c.state.add_theme_color_override("font_color", UI.link_color(state) if state != "disconnected" else UI.TEXT_MUTED)
	var bat: int = d.get("battery", -1)
	c.battery.text = "Batterie %d %%" % bat if bat >= 0 and not test else ""
	c.battery.add_theme_color_override("font_color", UI.ERROR if bat >= 0 and bat < 20 else UI.TEXT_MUTED)
	c.forget.visible = not test and not rem.is_empty()

	var vals: Array = c.values
	match role:
		"trainer":
			vals[0].text = _live(live.get("power_w", -1.0), connected)
			vals[1].text = _live(live.get("cadence_rpm", -1.0), connected)
		"cadence":
			vals[0].text = _live(live.get("cadence_rpm", -1.0), connected)
		"heart_rate":
			vals[0].text = _live(live.get("heart_rate", -1.0), connected)
		"click":
			var click: String = live.get("click", "")
			var age: float = live.get("click_age_s", 99.0)
			vals[0].text = {"up": "+", "down": "−"}.get(click, "--") if connected else "--"
			var flash := clampf(1.0 - age / FLASH_S, 0.0, 1.0) if connected and click != "" else 0.0
			vals[0].add_theme_color_override("font_color", UI.TEXT.lerp(UI.ACCENT, flash))
			vals[1].text = ("Dernier appui : vitesse " + ("supérieure" if click == "up" else "inférieure")) if connected and click != "" else "Appuie sur un bouton pour l'essayer."


func _live(v: float, connected: bool) -> String:
	return Fmt.num(v) if connected and v >= 0.0 else "--"


func _update_found(found: Array) -> void:
	var sig := ""
	for d in found:
		sig += "%s|%s|%s|%s;" % [d.id, d.state, d.role, d.get("message", "")]
	if sig == _found_sig:
		return
	_found_sig = sig
	for ch in _found_box.get_children():
		ch.queue_free()
	if found.is_empty():
		var l := UI.label("Aucun appareil trouvé pour l'instant. Réveille tes appareils (un tour de pédale, un appui sur les Click), puis lance une recherche.", "MutedLabel", true)
		_found_box.add_child(l)
		return
	var sorted := found.duplicate()
	sorted.sort_custom(func(a, b):
		var ca: int = 0 if a.role != "" else 1
		var cb: int = 0 if b.role != "" else 1
		if ca != cb:
			return ca < cb
		return int(a.get("rssi", -999)) > int(b.get("rssi", -999)))
	for d in sorted:
		_found_box.add_child(_found_row(d))


func _found_row(d: Dictionary) -> Control:
	var card := UI.card("Tile")
	var v := UI.vbox(8)
	card.add_child(v)
	var top := UI.hbox(10)
	v.add_child(top)
	top.add_child(UI.status_dot(UI.link_color(d.state) if d.role != "" or d.state == "failed" else Color(UI.TEXT_MUTED, 0.5)))
	var info := UI.vbox(0)
	info.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	top.add_child(info)
	var title := UI.label(d.name if d.name != "" else "Appareil sans nom", "H3")
	title.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	title.clip_text = true
	info.add_child(title)
	var kinds := PackedStringArray()
	for k in d.kinds:
		kinds.append(ROLE_SHORT.get(k, k))
	var sub := " · ".join(kinds)
	var rssi: int = d.get("rssi", -999)
	if rssi > -999:
		sub += "  ·  signal %s" % ("fort" if rssi > -65 else ("moyen" if rssi > -80 else "faible"))
	info.add_child(UI.label(sub, "MutedLabel"))
	if d.get("message", "") != "":
		var m := UI.label(d.message, "ErrorLabel" if d.state == "failed" else "MutedLabel", true)
		info.add_child(m)

	var actions := UI.hbox(8)
	actions.alignment = BoxContainer.ALIGNMENT_END
	if d.role != "" and d.state in ["connected", "connecting"]:
		var chip := UI.label("%s · %s" % [ROLE_SHORT.get(d.role, d.role), STATE_NAMES.get(d.state, "")], "OkLabel" if d.state == "connected" else "WarnLabel")
		chip.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		actions.add_child(chip)
		var off := UI.button("Déconnecter", "GhostButton")
		off.pressed.connect(func():
			App.devices.disconnect_kind(d.role)
			_found_sig = "-")
		actions.add_child(off)
		top.add_child(actions)
		return card
	var kinds_list: Array = Array(d.kinds)
	if kinds_list.size() == 1:
		var b := UI.button("Connecter")
		b.tooltip_text = "Connecter comme %s" % ROLE_NAMES.get(kinds_list[0], kinds_list[0]).to_lower()
		b.pressed.connect(_connect_device.bind(d, kinds_list[0]))
		top.add_child(b)
	else:
		var l := UI.label("Connecter comme", "MutedLabel")
		l.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		actions.add_child(l)
		for k in kinds_list:
			var b := UI.button(ROLE_SHORT.get(k, k))
			b.pressed.connect(_connect_device.bind(d, k))
			actions.add_child(b)
		v.add_child(actions)
	return card
