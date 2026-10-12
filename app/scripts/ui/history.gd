extends MarginContainer
## Historique : liste des sorties, courbe de puissance, volume par semaine,
## forme (CTL, ATL, TSB) et records.

const UI := preload("res://scripts/ui/theme.gd")
const Fmt := preload("res://scripts/ui/fmt.gd")
const Chart := preload("res://scripts/ui/chart.gd")

const PAGE := 12
## Durées clés de la courbe de puissance.
const KEY_DURATIONS := [[5, "5 s"], [60, "1 min"], [300, "5 min"], [1200, "20 min"], [3600, "1 h"]]
const CURVES := [[0, "Depuis toujours"], [365, "365 jours"], [90, "90 jours"], [42, "42 jours"]]
const CURVE_COLORS := [Color(0.98, 0.62, 0.20), Color("2ec4b6"), Color("5aa9ff"), Color("c08cf0")]
const CTL_COLOR := Color("5aa9ff")
const ATL_COLOR := Color("f0605a")
const TSB_COLOR := Color(0.98, 0.62, 0.20)

var menu: Node

var _tabs: TabContainer
var _tree: Tree
var _page := 0
var _page_label: Label
var _prev: Button
var _next: Button
var _empty: Label
var _curve_chart: Control
var _curve_grid: GridContainer
var _ftp_value: Label
var _ftp_sub: Label
var _ftp_use: Button
var _zones: VBoxContainer
var _ftp_estimate := 0.0
var _vol_weeks := 12
var _vol_tiles := {}
var _hours_chart: Control
var _tss_chart: Control
var _fit_days := 90
var _fit_tiles := {}
var _fit_chart: Control
var _records: VBoxContainer


func _ready() -> void:
	for side in ["left", "right"]:
		add_theme_constant_override("margin_" + side, 32)
	add_theme_constant_override("margin_top", 26)
	add_theme_constant_override("margin_bottom", 28)
	var col := UI.vbox(14)
	add_child(col)
	var hv := UI.vbox(2)
	hv.add_child(UI.label("Historique", "H1"))
	hv.add_child(UI.label("Tes sorties sur home trainer et ton suivi à long terme.", "DimLabel"))
	col.add_child(hv)
	_tabs = TabContainer.new()
	_tabs.size_flags_vertical = Control.SIZE_EXPAND_FILL
	col.add_child(_tabs)
	_build_rides()
	_build_curve()
	_build_volume()
	_build_fitness()
	_build_records()
	_tabs.tab_changed.connect(func(_i: int): _refresh_tab())


func enter() -> void:
	_refresh_tab()


func select_tab(i: int) -> void:
	_tabs.current_tab = clampi(i, 0, _tabs.get_tab_count() - 1)
	_refresh_tab()


func _refresh_tab() -> void:
	match _tabs.current_tab:
		0:
			_refresh_rides()
		1:
			_refresh_curve()
		2:
			_refresh_volume()
		3:
			_refresh_fitness()
		4:
			_refresh_records()


func _chart_card(parent: Control, title: String, chart: Control) -> PanelContainer:
	var card := UI.card()
	card.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	card.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var v := UI.vbox(10)
	card.add_child(v)
	if title != "":
		v.add_child(UI.section_title(title))
	chart.size_flags_vertical = Control.SIZE_EXPAND_FILL
	v.add_child(chart)
	parent.add_child(card)
	return card


# --- Sorties -----------------------------------------------------------------------------

func _build_rides() -> void:
	var page := UI.vbox(12)
	page.name = "Sorties"
	_tabs.add_child(page)
	_tree = Tree.new()
	_tree.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_tree.hide_root = true
	_tree.columns = 9
	_tree.column_titles_visible = true
	_tree.select_mode = Tree.SELECT_ROW
	_tree.add_theme_font_size_override("font_size", 15)
	var cols := [["Date", 170, false], ["Parcours", 0, true], ["Mode", 160, false], ["Durée", 100, false],
		["Distance", 110, false], ["D+", 90, false], ["Moy. / NP", 140, false], ["TSS", 70, false], ["Strava", 100, false]]
	for i in cols.size():
		_tree.set_column_title(i, String(cols[i][0]).to_upper())
		_tree.set_column_expand(i, cols[i][2])
		if cols[i][1] > 0:
			_tree.set_column_custom_minimum_width(i, cols[i][1])
		_tree.set_column_title_alignment(i, HORIZONTAL_ALIGNMENT_LEFT if i < 3 else HORIZONTAL_ALIGNMENT_RIGHT)
		_tree.set_column_clip_content(i, true)
	_tree.item_mouse_selected.connect(func(_p: Vector2, _b: int): _open_selected())
	_tree.item_activated.connect(_open_selected)
	page.add_child(_tree)
	_empty = UI.label("Aucune sortie enregistrée pour l'instant. Termine une sortie : elle apparaîtra ici avec ses courbes.", "DimLabel", true)
	_empty.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_empty.visible = false
	page.add_child(_empty)
	var nav := UI.hbox(10)
	page.add_child(nav)
	_page_label = UI.label("", "MutedLabel")
	_page_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_page_label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	nav.add_child(_page_label)
	_prev = UI.button("‹  Plus récentes")
	_prev.pressed.connect(func():
		_page -= 1
		_refresh_rides())
	nav.add_child(_prev)
	_next = UI.button("Plus anciennes  ›")
	_next.pressed.connect(func():
		_page += 1
		_refresh_rides())
	nav.add_child(_next)


func _refresh_rides() -> void:
	var count: int = App.history.ride_count()
	var pages := maxi(1, ceili(float(count) / PAGE))
	_page = clampi(_page, 0, pages - 1)
	_tree.clear()
	var root := _tree.create_item()
	var rides: Array = App.history.rides(PAGE, _page * PAGE)
	for r in rides:
		var it := _tree.create_item(root)
		it.set_metadata(0, int(r.id))
		var title: String = r.get("name", "")
		var route: String = r.get("route_name", "")
		var texts := [
			Fmt.date_long(r.started_unix),
			route if title == "" else "%s · %s" % [route, title],
			Fmt.mode(r.get("mode", "")),
			Fmt.hms(r.duration_s),
			Fmt.km(r.distance_m),
			Fmt.num(r.elevation_gain_m) + " m",
			"%s / %s W" % [Fmt.opt(r.avg_power_w), Fmt.opt(r.np_w)],
			Fmt.opt(r.tss),
			"Envoyée" if int(r.get("strava_activity_id", 0)) != 0 else "—",
		]
		for i in texts.size():
			it.set_text(i, texts[i])
			it.set_text_alignment(i, HORIZONTAL_ALIGNMENT_LEFT if i < 3 else HORIZONTAL_ALIGNMENT_RIGHT)
			it.set_tooltip_text(i, "Ouvrir le bilan de la sortie")
		it.set_custom_color(0, UI.TEXT_DIM)
		it.set_custom_color(2, UI.TEXT_DIM)
		it.set_custom_color(8, Color("fc4c02") if int(r.get("strava_activity_id", 0)) != 0 else UI.TEXT_MUTED)
	_tree.visible = count > 0
	_empty.visible = count == 0
	_page_label.text = "" if count == 0 else "Sorties %d à %d sur %d" % [_page * PAGE + 1, _page * PAGE + rides.size(), count]
	_prev.disabled = _page == 0
	_next.disabled = _page >= pages - 1
	_prev.visible = pages > 1
	_next.visible = pages > 1


func _open_selected() -> void:
	var it := _tree.get_selected()
	if it:
		menu.show_ride(int(it.get_metadata(0)))


# --- Courbe de puissance ------------------------------------------------------------------------

func _build_curve() -> void:
	var page := UI.hbox(20)
	page.name = "Courbe de puissance"
	_tabs.add_child(page)
	_curve_chart = Chart.new()
	_curve_chart.x_log = true
	_curve_chart.x_format = "duree"
	_curve_chart.y_format = "watts"
	_curve_chart.y_from_zero = false
	_curve_chart.empty_text = "Pas encore de données de puissance."
	_chart_card(page, "Meilleure puissance moyenne selon la durée", _curve_chart)

	var side := UI.vbox(18)
	side.custom_minimum_size.x = 440
	page.add_child(side)
	var tcard := UI.card()
	side.add_child(tcard)
	var tv := UI.vbox(10)
	tcard.add_child(tv)
	tv.add_child(UI.section_title("Durées clés"))
	_curve_grid = GridContainer.new()
	_curve_grid.columns = 4
	_curve_grid.add_theme_constant_override("h_separation", 18)
	_curve_grid.add_theme_constant_override("v_separation", 10)
	tv.add_child(_curve_grid)

	var fcard := UI.card()
	fcard.size_flags_vertical = Control.SIZE_EXPAND_FILL
	side.add_child(fcard)
	var fv := UI.vbox(6)
	fcard.add_child(fv)
	fv.add_child(UI.section_title("FTP estimée"))
	_ftp_value = UI.label("--", "BigValueLabel")
	fv.add_child(_ftp_value)
	_ftp_sub = UI.label("", "DimLabel", true)
	fv.add_child(_ftp_sub)
	var gap := Control.new()
	gap.custom_minimum_size.y = 6
	fv.add_child(gap)
	fv.add_child(UI.section_title("Zones selon cette FTP"))
	_zones = UI.vbox(0)
	fv.add_child(_zones)
	fv.add_child(UI.spacer())
	_ftp_use = UI.button("Utiliser comme FTP")
	_ftp_use.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	_ftp_use.pressed.connect(func():
		if _ftp_estimate > 0.0 and App.ride.profile_set("rider.ftp_w", roundi(_ftp_estimate)):
			App.ride.profile_save()
			_refresh_curve())
	fv.add_child(_ftp_use)


## Puissance d'une courbe à une durée donnée (interpolée) ; -1 si hors courbe.
static func curve_at(curve: PackedVector2Array, d: float) -> float:
	if curve.is_empty() or d > curve[-1].x + 0.5:
		return -1.0
	for i in curve.size():
		if curve[i].x >= d - 0.01:
			if i == 0 or is_equal_approx(curve[i].x, d):
				return curve[i].y
			var a := curve[i - 1]
			var b := curve[i]
			return lerpf(a.y, b.y, (d - a.x) / (b.x - a.x))
	return -1.0


func _refresh_curve() -> void:
	var series := []
	var curves := []
	for i in CURVES.size():
		var c: PackedVector2Array = App.history.best_curve(CURVES[i][0])
		curves.append(c)
		if not c.is_empty():
			series.append({"name": CURVES[i][1], "color": CURVE_COLORS[i], "kind": "line", "points": c, "width": 2.5 if i == 0 else 2.0})
	_curve_chart.set_series(series)

	for c in _curve_grid.get_children():
		c.queue_free()
	var kg := _weight()
	for h in ["Durée", "Record", "W/kg", "90 jours"]:
		var l := UI.label(h.to_upper(), "SectionLabel")
		l.horizontal_alignment = HORIZONTAL_ALIGNMENT_LEFT if h == "Durée" else HORIZONTAL_ALIGNMENT_RIGHT
		l.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		_curve_grid.add_child(l)
	for kd in KEY_DURATIONS:
		var best := curve_at(curves[0], kd[0])
		var recent := curve_at(curves[2], kd[0])
		var cells := [kd[1], Fmt.watts(best) if best > 0.0 else "--", Fmt.num(best / kg, 1) if best > 0.0 else "--", Fmt.watts(recent) if recent > 0.0 else "--"]
		for i in cells.size():
			var l := UI.label(cells[i], "H3" if i == 1 else ("DimLabel" if i != 0 else ""))
			l.horizontal_alignment = HORIZONTAL_ALIGNMENT_LEFT if i == 0 else HORIZONTAL_ALIGNMENT_RIGHT
			if i == 3:
				l.add_theme_color_override("font_color", CURVE_COLORS[2])
			_curve_grid.add_child(l)

	var rec: Dictionary = App.history.records()
	_ftp_estimate = float(rec.get("estimated_ftp_w", 0.0))
	var current: Variant = App.ride.profile_get("rider.ftp_w")
	var cur := float(current) if current != null else 0.0
	if _ftp_estimate > 0.0:
		_ftp_value.text = "%s W  ·  %s W/kg" % [Fmt.num(_ftp_estimate), Fmt.num(_ftp_estimate / kg, 1)]
		_ftp_sub.text = "95 %% de ta meilleure puissance sur 20 minutes. FTP du profil : %s." % (Fmt.watts(cur) if cur > 0.0 else "inconnue (200 W utilisés)")
	else:
		_ftp_value.text = "--"
		_ftp_sub.text = "Roule au moins 20 minutes avec un capteur de puissance pour obtenir une estimation."
	_ftp_use.visible = _ftp_estimate > 0.0 and roundi(_ftp_estimate) != roundi(cur)
	for c in _zones.get_children():
		c.queue_free()
	_zones.add_child(UI.zones_grid(_ftp_estimate if _ftp_estimate > 0.0 else (cur if cur > 0.0 else 200.0)))


func _weight() -> float:
	var w: Variant = App.ride.profile_get("rider.weight_kg")
	return float(w) if w != null and float(w) > 0.0 else 70.0


# --- Volume ------------------------------------------------------------------------------

func _build_volume() -> void:
	var page := UI.vbox(16)
	page.name = "Volume"
	_tabs.add_child(page)
	var top := UI.hbox(14)
	page.add_child(top)
	var seg := UI.segmented(["12 semaines", "52 semaines"], 0, func(i: int):
		_vol_weeks = 12 if i == 0 else 52
		_refresh_volume())
	seg.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	top.add_child(seg)
	for spec in [["rides", "Sorties", ""], ["time", "Durée", ""], ["dist", "Distance", "km"], ["elev", "Dénivelé", "m"], ["tss", "TSS", ""]]:
		var t := UI.metric_tile(spec[1], "--", spec[2])
		t.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		top.add_child(t)
		_vol_tiles[spec[0]] = t
	var charts := UI.hbox(20)
	charts.size_flags_vertical = Control.SIZE_EXPAND_FILL
	page.add_child(charts)
	_hours_chart = Chart.new()
	_hours_chart.x_format = "date"
	_hours_chart.y_format = "heures"
	_hours_chart.show_legend = false
	_chart_card(charts, "Heures par semaine", _hours_chart)
	_tss_chart = Chart.new()
	_tss_chart.x_format = "date"
	_tss_chart.y_format = "nombre"
	_tss_chart.show_legend = false
	_chart_card(charts, "Charge d'entraînement (TSS) par semaine", _tss_chart)


func _refresh_volume() -> void:
	var weeks: Array = App.history.weekly(_vol_weeks)
	var hours := PackedVector2Array()
	var tss := PackedVector2Array()
	var tot := {"rides": 0, "time": 0.0, "dist": 0.0, "elev": 0.0, "tss": 0.0}
	for w in weeks:
		hours.append(Vector2(w.week_start_unix, w.duration_s / 3600.0))
		tss.append(Vector2(w.week_start_unix, w.tss))
		tot.rides += int(w.rides)
		tot.time += w.duration_s
		tot.dist += w.distance_m
		tot.elev += w.elevation_m
		tot.tss += w.tss
	_hours_chart.set_series([{"name": "Heures", "color": UI.ACCENT, "kind": "bar", "points": hours}])
	_tss_chart.set_series([{"name": "TSS", "color": UI.TEAL, "kind": "bar", "points": tss}])
	UI.set_tile(_vol_tiles.rides, str(tot.rides))
	UI.set_tile(_vol_tiles.time, Fmt.duration(tot.time))
	UI.set_tile(_vol_tiles.dist, Fmt.num(tot.dist / 1000.0))
	UI.set_tile(_vol_tiles.elev, Fmt.num(tot.elev))
	UI.set_tile(_vol_tiles.tss, Fmt.num(tot.tss))


# --- Forme ------------------------------------------------------------------------------

func _build_fitness() -> void:
	var page := UI.vbox(16)
	page.name = "Forme"
	_tabs.add_child(page)
	var top := UI.hbox(14)
	page.add_child(top)
	var seg := UI.segmented(["90 jours", "180 jours", "365 jours"], 0, func(i: int):
		_fit_days = [90, 180, 365][i]
		_refresh_fitness())
	seg.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	top.add_child(seg)
	for spec in [["ctl", "Condition (CTL)", CTL_COLOR], ["atl", "Fatigue (ATL)", ATL_COLOR], ["tsb", "Forme (TSB)", TSB_COLOR]]:
		var t := UI.metric_tile(spec[1], "--", "")
		t.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		(t.get_meta("value") as Label).add_theme_color_override("font_color", spec[2])
		top.add_child(t)
		_fit_tiles[spec[0]] = t
	_fit_chart = Chart.new()
	_fit_chart.x_format = "date"
	_fit_chart.y_format = "nombre"
	_fit_chart.y_from_zero = false
	var card := _chart_card(page, "", _fit_chart)
	var v: VBoxContainer = card.get_child(0)
	v.add_child(UI.label("La condition (CTL) est ta charge moyenne sur 42 jours, la fatigue (ATL) sur 7 jours ; la forme (TSB) est leur différence : positive, tu es reposé ; très négative, tu accumules de la fatigue.", "MutedLabel", true))


func _refresh_fitness() -> void:
	var days: Array = App.history.fitness(_fit_days)
	var ctl := PackedVector2Array()
	var atl := PackedVector2Array()
	var tsb := PackedVector2Array()
	for d in days:
		ctl.append(Vector2(d.day_unix, d.ctl))
		atl.append(Vector2(d.day_unix, d.atl))
		tsb.append(Vector2(d.day_unix, d.tsb))
	_fit_chart.set_series([
		{"name": "Condition (CTL)", "color": CTL_COLOR, "kind": "line", "points": ctl, "width": 2.5},
		{"name": "Fatigue (ATL)", "color": ATL_COLOR, "kind": "line", "points": atl, "width": 1.5},
		{"name": "Forme (TSB)", "color": TSB_COLOR, "kind": "line", "points": tsb, "width": 2.0},
	])
	_fit_chart.set_reference_lines([{"y": 0.0, "color": Color(1, 1, 1, 0.3)}])
	if days.is_empty():
		for k in _fit_tiles:
			UI.set_tile(_fit_tiles[k], "--", "")
		return
	var last: Dictionary = days[-1]
	UI.set_tile(_fit_tiles.ctl, Fmt.num(last.ctl), "")
	UI.set_tile(_fit_tiles.atl, Fmt.num(last.atl), "")
	var t: float = last.tsb
	var mood := "fatigue élevée, prévois du repos"
	if t > 5.0:
		mood = "reposé, idéal pour un test"
	elif t > -10.0:
		mood = "équilibré"
	elif t > -30.0:
		mood = "en charge, tu progresses"
	UI.set_tile(_fit_tiles.tsb, ("+" if t > 0.5 else "") + Fmt.num(t), mood)


# --- Records ----------------------------------------------------------------------------

func _build_records() -> void:
	var scroll := ScrollContainer.new()
	scroll.name = "Records"
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_tabs.add_child(scroll)
	_records = UI.vbox(14)
	_records.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(_records)


func _refresh_records() -> void:
	for c in _records.get_children():
		c.queue_free()
	var rec: Dictionary = App.history.records()
	var kg := _weight()
	var curve: PackedVector2Array = App.history.best_curve(0)
	var weeks: Array = App.history.weekly(520)
	var total_s := 0.0
	var best_week := {"duration_s": 0.0, "tss": 0.0, "distance_m": 0.0, "rides": 0}
	for w in weeks:
		total_s += w.duration_s
		for k in best_week:
			best_week[k] = maxf(best_week[k], w[k])
	var peak_ctl := 0.0
	for d in App.history.fitness(365):
		peak_ctl = maxf(peak_ctl, d.ctl)

	_record_row("Sorties", [
		["Plus longue sortie", Fmt.duration(rec.get("longest_s", 0.0)) if rec.get("longest_s", 0.0) > 0.0 else "--", "", ""],
		["Plus grande distance", Fmt.num(rec.get("farthest_m", 0.0) / 1000.0, 1), "km", ""],
		["Plus grand dénivelé", Fmt.num(rec.get("most_elevation_m", 0.0)), "m", ""],
		["Sorties enregistrées", str(App.history.ride_count()), "", ""],
		["Temps total", Fmt.num(total_s / 3600.0), "h", ""],
	])
	var power := []
	for kd in KEY_DURATIONS:
		var w := curve_at(curve, kd[0])
		if kd[0] == 1200 and float(rec.get("best_20min_w", 0.0)) > 0.0:
			w = rec.best_20min_w
		power.append(["Sur " + kd[1], Fmt.opt(w), "W", "%s W/kg" % Fmt.num(w / kg, 1) if w > 0.0 else ""])
	var ftp: float = rec.get("estimated_ftp_w", 0.0)
	power.append(["FTP estimée", Fmt.opt(ftp), "W", "%s W/kg" % Fmt.num(ftp / kg, 1) if ftp > 0.0 else ""])
	_record_row("Puissance", power)
	_record_row("Meilleure semaine", [
		["Temps", Fmt.duration(best_week.duration_s) if best_week.duration_s > 0.0 else "--", "", ""],
		["Distance", Fmt.num(best_week.distance_m / 1000.0), "km", ""],
		["Sorties", str(int(best_week.rides)), "", ""],
		["Charge", Fmt.num(best_week.tss), "TSS", ""],
		["Condition maximale", Fmt.num(peak_ctl), "CTL", "sur un an"],
	])


func _record_row(title: String, items: Array) -> void:
	_records.add_child(UI.section_title(title))
	var row := UI.hbox(16)
	_records.add_child(row)
	for it in items:
		var card := UI.card()
		card.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		var v := UI.vbox(2)
		card.add_child(v)
		v.add_child(UI.label(it[0], "DimLabel"))
		var r := UI.hbox(6)
		r.add_child(UI.label(it[1], "BigValueLabel"))
		var u := UI.label(it[2], "UnitLabel")
		u.size_flags_vertical = Control.SIZE_SHRINK_END
		r.add_child(u)
		v.add_child(r)
		var sub := UI.label(it[3] if it[3] != "" else " ", "MutedLabel")
		v.add_child(sub)
		row.add_child(card)
