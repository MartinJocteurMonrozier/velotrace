extends RefCounted
## Thème sombre de l'application (gris anthracite neutres, cartes arrondies, accent jaune d'or)
## et petits constructeurs pour les éléments qui reviennent partout.
##
## Utilisation : `const UI := preload("res://scripts/ui/theme.gd")`, puis
## `root.theme = UI.build()` et `UI.card()`, `UI.metric_tile(...)`, etc.

const BG := Color("1e1e1e")
const BG_DEEP := Color("161616")
const CARD := Color("2a2a2a")
const CARD_HI := Color("343434")
const FIELD := Color("1a1a1a")
const BORDER := Color("3d3d3d")
const BORDER_HI := Color("575757")
const ACCENT := Color("e3a200")
const ACCENT_HI := Color("f5b81c")
const ACCENT_LO := Color("c48b00")
const ON_ACCENT := Color("161616")
## Couleur secondaire neutre (profils, barres) : un gris clair.
const SECOND := Color("cfcfcf")
const TEXT := Color("ffffff")
const TEXT_DIM := Color("b4b4b4")
const TEXT_MUTED := Color("858585")
const OK := Color("4cc46a")
const WARN := Color("f0b429")
const ERROR := Color("e5534b")
const INFO := Color("4f9cf0")

const RADIUS := 12
const FONT_SIZE := 16

## Polices système, de la plus agréable à la plus courante.
const FONT_NAMES := ["Inter", "Segoe UI", "Noto Sans", "Cantarell", "Ubuntu", "Roboto", "DejaVu Sans", "Sans-Serif"]

static var _theme: Theme
static var _fonts := {}


## Police système à la graisse demandée (400 normal, 600 demi-gras, 700 gras).
static func font(weight := 400) -> Font:
	if not _fonts.has(weight):
		var f := SystemFont.new()
		f.font_names = PackedStringArray(FONT_NAMES)
		f.font_weight = weight
		f.antialiasing = TextServer.FONT_ANTIALIASING_GRAY
		f.hinting = TextServer.HINTING_LIGHT
		f.subpixel_positioning = TextServer.SUBPIXEL_POSITIONING_AUTO
		_fonts[weight] = f
	return _fonts[weight]


## Police légèrement espacée pour les petits titres en capitales.
static func spaced_font() -> Font:
	if not _fonts.has("spaced"):
		var v := FontVariation.new()
		v.base_font = font(600)
		v.spacing_glyph = 1
		_fonts["spaced"] = v
	return _fonts["spaced"]


static func build() -> Theme:
	if _theme:
		return _theme
	var t := Theme.new()
	t.default_font = font(400)
	t.default_font_size = FONT_SIZE

	# Textes.
	t.set_color("font_color", "Label", TEXT)
	_label_variation(t, "HeroTitle", 44, font(700), TEXT)
	_label_variation(t, "H1", 28, font(700), TEXT)
	_label_variation(t, "H2", 20, font(600), TEXT)
	_label_variation(t, "H3", 17, font(600), TEXT)
	_label_variation(t, "SectionLabel", 12, spaced_font(), TEXT_MUTED)
	_label_variation(t, "DimLabel", 15, font(400), TEXT_DIM)
	_label_variation(t, "MutedLabel", 13, font(400), TEXT_MUTED)
	_label_variation(t, "ValueLabel", 30, font(700), TEXT)
	_label_variation(t, "BigValueLabel", 40, font(700), TEXT)
	_label_variation(t, "UnitLabel", 14, font(400), TEXT_DIM)
	_label_variation(t, "ErrorLabel", 14, font(400), ERROR)
	_label_variation(t, "WarnLabel", 15, font(400), WARN)
	_label_variation(t, "OkLabel", 15, font(400), OK)
	_label_variation(t, "AccentLabel", 15, font(600), ACCENT)
	t.set_color("default_color", "RichTextLabel", TEXT_DIM)
	t.set_font("bold_font", "RichTextLabel", font(700))
	t.set_font("normal_font", "RichTextLabel", font(400))
	t.set_font_size("normal_font_size", "RichTextLabel", 15)
	t.set_font_size("bold_font_size", "RichTextLabel", 15)

	# Panneaux.
	t.set_stylebox("panel", "Panel", StyleBoxEmpty.new())
	t.set_stylebox("panel", "PanelContainer", StyleBoxEmpty.new())
	_panel_variation(t, "Card", _box(CARD, RADIUS, 20, 18, BORDER.darkened(0.15)))
	_panel_variation(t, "CardHi", _box(CARD_HI, RADIUS, 20, 18, BORDER))
	_panel_variation(t, "Tile", _box(CARD_HI, 10, 14, 10))
	_panel_variation(t, "Field", _box(FIELD, 10, 12, 10, BORDER.darkened(0.2)))
	var top := _box(BG_DEEP, 0, 24, 0)
	top.border_width_bottom = 1
	top.border_color = BORDER.darkened(0.25)
	_panel_variation(t, "TopBar", top)
	var chip := _box(CARD_HI, 99, 12, 5)
	_panel_variation(t, "Chip", chip)
	var warn := _box(Color(WARN, 0.10), 10, 14, 10, Color(WARN, 0.35))
	_panel_variation(t, "WarnBox", warn)
	_panel_variation(t, "InfoBox", _box(Color(INFO, 0.10), 10, 14, 10, Color(INFO, 0.30)))
	var sel := _box(Color(ACCENT, 0.10), RADIUS, 20, 18, ACCENT)
	sel.set_border_width_all(2)
	_panel_variation(t, "CardSelected", sel)

	# Boutons.
	_button_styles(t, "Button", CARD_HI, BORDER, 16, 9, 8)
	_button_colors(t, "Button", TEXT, TEXT, TEXT, TEXT_MUTED)
	t.set_font("font", "Button", font(600))
	t.set_constant("h_separation", "Button", 8)
	t.set_color("icon_normal_color", "Button", TEXT_DIM)

	t.set_type_variation("PrimaryButton", "Button")
	t.set_stylebox("normal", "PrimaryButton", _box(ACCENT, 10, 28, 13))
	t.set_stylebox("hover", "PrimaryButton", _box(ACCENT_HI, 10, 28, 13))
	t.set_stylebox("pressed", "PrimaryButton", _box(ACCENT_LO, 10, 28, 13))
	t.set_stylebox("hover_pressed", "PrimaryButton", _box(ACCENT_LO, 10, 28, 13))
	t.set_stylebox("disabled", "PrimaryButton", _box(Color(ACCENT, 0.18), 10, 28, 13))
	t.set_stylebox("focus", "PrimaryButton", _focus(10))
	_button_colors(t, "PrimaryButton", ON_ACCENT, ON_ACCENT, ON_ACCENT, Color(TEXT, 0.35))
	t.set_font("font", "PrimaryButton", font(700))
	t.set_font_size("font_size", "PrimaryButton", 18)

	t.set_type_variation("GhostButton", "Button")
	var ghost := _box(Color(1, 1, 1, 0.0), 8, 12, 8)
	t.set_stylebox("normal", "GhostButton", ghost)
	t.set_stylebox("hover", "GhostButton", _box(Color(1, 1, 1, 0.06), 8, 12, 8))
	t.set_stylebox("pressed", "GhostButton", _box(Color(1, 1, 1, 0.10), 8, 12, 8))
	t.set_stylebox("hover_pressed", "GhostButton", _box(Color(1, 1, 1, 0.10), 8, 12, 8))
	t.set_stylebox("disabled", "GhostButton", ghost)
	_button_colors(t, "GhostButton", TEXT_DIM, TEXT, TEXT, TEXT_MUTED)

	t.set_type_variation("LinkButtonish", "GhostButton")
	_button_colors(t, "LinkButtonish", ACCENT, ACCENT_HI, ACCENT_LO, TEXT_MUTED)

	t.set_type_variation("DangerButton", "Button")
	_button_styles(t, "DangerButton", Color(ERROR, 0.08), Color(ERROR, 0.45), 16, 9, 8)
	t.set_stylebox("hover", "DangerButton", _box(Color(ERROR, 0.18), 8, 16, 9, ERROR))
	_button_colors(t, "DangerButton", Color("ff8b85"), Color("ffb0ab"), Color("ffb0ab"), TEXT_MUTED)

	# Onglets de la barre du haut : soulignés en orange quand ils sont actifs.
	t.set_type_variation("NavButton", "Button")
	var nav := _box(Color(0, 0, 0, 0), 0, 18, 18)
	var nav_hover := _box(Color(1, 1, 1, 0.04), 0, 18, 18)
	var nav_on := _box(Color(1, 1, 1, 0.0), 0, 18, 18)
	nav_on.border_width_bottom = 3
	nav_on.border_color = ACCENT
	nav_on.content_margin_bottom = 15
	t.set_stylebox("normal", "NavButton", nav)
	t.set_stylebox("hover", "NavButton", nav_hover)
	t.set_stylebox("pressed", "NavButton", nav_on)
	t.set_stylebox("hover_pressed", "NavButton", nav_on)
	t.set_stylebox("disabled", "NavButton", nav)
	t.set_stylebox("focus", "NavButton", _focus(4))
	_button_colors(t, "NavButton", TEXT_DIM, TEXT, TEXT, TEXT_MUTED)
	t.set_font_size("font_size", "NavButton", 16)

	# Grandes cartes sélectionnables (choix du mode d'entraînement).
	t.set_type_variation("ChoiceCard", "Button")
	var cc := _box(CARD, RADIUS, 18, 16, BORDER)
	var cc_hover := _box(CARD_HI, RADIUS, 18, 16, BORDER_HI)
	var cc_on := _box(CARD.blend(Color(ACCENT, 0.12)), RADIUS, 18, 16, ACCENT)
	cc_on.set_border_width_all(2)
	t.set_stylebox("normal", "ChoiceCard", cc)
	t.set_stylebox("hover", "ChoiceCard", cc_hover)
	t.set_stylebox("pressed", "ChoiceCard", cc_on)
	t.set_stylebox("hover_pressed", "ChoiceCard", cc_on)
	t.set_stylebox("focus", "ChoiceCard", _focus(RADIUS))

	# Boutons segmentés (choix d'une période, d'un type de cycliste…).
	t.set_type_variation("SegmentButton", "Button")
	var seg := _box(FIELD, 8, 14, 7, BORDER)
	var seg_on := _box(Color(ACCENT, 0.16), 8, 14, 7, ACCENT)
	t.set_stylebox("normal", "SegmentButton", seg)
	t.set_stylebox("hover", "SegmentButton", _box(FIELD.lightened(0.05), 8, 14, 7, BORDER_HI))
	t.set_stylebox("pressed", "SegmentButton", seg_on)
	t.set_stylebox("hover_pressed", "SegmentButton", seg_on)
	t.set_stylebox("disabled", "SegmentButton", _box(Color(FIELD, 0.5), 8, 14, 7, Color(BORDER, 0.4)))
	_button_colors(t, "SegmentButton", TEXT_DIM, TEXT, ACCENT_HI, TEXT_MUTED)
	t.set_font_size("font_size", "SegmentButton", 14)

	# Interrupteurs.
	t.set_icon("checked", "CheckButton", _switch_icon(true, 1.0))
	t.set_icon("unchecked", "CheckButton", _switch_icon(false, 1.0))
	t.set_icon("checked_disabled", "CheckButton", _switch_icon(true, 0.4))
	t.set_icon("unchecked_disabled", "CheckButton", _switch_icon(false, 0.4))
	t.set_icon("checked_mirrored", "CheckButton", _switch_icon(true, 1.0))
	t.set_icon("unchecked_mirrored", "CheckButton", _switch_icon(false, 1.0))
	for s in ["normal", "pressed", "hover", "hover_pressed", "disabled"]:
		t.set_stylebox(s, "CheckButton", _box(Color(0, 0, 0, 0), 8, 4, 6))
	t.set_stylebox("hover", "CheckButton", _box(Color(1, 1, 1, 0.03), 8, 4, 6))
	t.set_stylebox("hover_pressed", "CheckButton", _box(Color(1, 1, 1, 0.03), 8, 4, 6))
	t.set_stylebox("focus", "CheckButton", _focus(8))
	_button_colors(t, "CheckButton", TEXT, TEXT, TEXT, TEXT_MUTED)
	t.set_color("font_pressed_color", "CheckButton", TEXT)
	t.set_color("font_hover_pressed_color", "CheckButton", TEXT)
	t.set_font("font", "CheckButton", font(400))
	t.set_constant("h_separation", "CheckButton", 12)
	t.set_color("font_color", "CheckBox", TEXT)

	# Listes déroulantes.
	_button_styles(t, "OptionButton", FIELD, BORDER, 14, 9, 8)
	_button_colors(t, "OptionButton", TEXT, TEXT, TEXT, TEXT_MUTED)
	t.set_font("font", "OptionButton", font(400))
	t.set_icon("arrow", "OptionButton", _chevron_icon(TEXT_DIM))
	t.set_constant("arrow_margin", "OptionButton", 12)
	t.set_constant("h_separation", "OptionButton", 10)

	# Champs de saisie.
	var field := _box(FIELD, 8, 12, 8, BORDER)
	var field_focus := _box(FIELD, 8, 12, 8, ACCENT)
	field_focus.set_border_width_all(2)
	field_focus.content_margin_left = 11
	field_focus.content_margin_right = 11
	t.set_stylebox("normal", "LineEdit", field)
	t.set_stylebox("focus", "LineEdit", field_focus)
	t.set_stylebox("read_only", "LineEdit", _box(Color(FIELD, 0.5), 8, 12, 8, Color(BORDER, 0.5)))
	t.set_color("font_color", "LineEdit", TEXT)
	t.set_color("font_placeholder_color", "LineEdit", TEXT_MUTED)
	t.set_color("font_uneditable_color", "LineEdit", TEXT_MUTED)
	t.set_color("caret_color", "LineEdit", ACCENT)
	t.set_color("selection_color", "LineEdit", Color(ACCENT, 0.35))
	t.set_color("font_selected_color", "LineEdit", TEXT)
	t.set_stylebox("normal", "TextEdit", field)
	t.set_stylebox("focus", "TextEdit", field_focus)
	t.set_color("font_color", "TextEdit", TEXT)
	t.set_color("caret_color", "TextEdit", ACCENT)
	t.set_color("selection_color", "TextEdit", Color(ACCENT, 0.35))
	t.set_type_variation("ErrorField", "LineEdit")
	var bad := _box(FIELD, 8, 12, 8, ERROR)
	t.set_stylebox("normal", "ErrorField", bad)
	t.set_stylebox("focus", "ErrorField", bad)

	# Onglets.
	var tab_off := _box(Color(0, 0, 0, 0), 0, 18, 11)
	var tab_hover := _box(Color(1, 1, 1, 0.04), 0, 18, 11)
	var tab_on := _box(Color(0, 0, 0, 0), 0, 18, 11)
	tab_on.border_width_bottom = 3
	tab_on.border_color = ACCENT
	tab_on.content_margin_bottom = 8
	var tab_bar := StyleBoxFlat.new()
	tab_bar.bg_color = Color(0, 0, 0, 0)
	tab_bar.border_width_bottom = 1
	tab_bar.border_color = BORDER
	var tab_panel := StyleBoxEmpty.new()
	tab_panel.content_margin_top = 20
	for type in ["TabContainer", "TabBar"]:
		t.set_stylebox("tab_unselected", type, tab_off)
		t.set_stylebox("tab_hovered", type, tab_hover)
		t.set_stylebox("tab_selected", type, tab_on)
		t.set_stylebox("tab_disabled", type, tab_off)
		t.set_stylebox("tab_focus", type, _focus(4))
		t.set_color("font_selected_color", type, TEXT)
		t.set_color("font_hovered_color", type, TEXT)
		t.set_color("font_unselected_color", type, TEXT_DIM)
		t.set_color("font_disabled_color", type, TEXT_MUTED)
		t.set_font("font", type, font(600))
		t.set_font_size("font_size", type, 16)
		t.set_constant("h_separation", type, 4)
	t.set_stylebox("tabbar_background", "TabContainer", tab_bar)
	t.set_stylebox("panel", "TabContainer", tab_panel)
	t.set_constant("side_margin", "TabContainer", 0)

	# Listes et tableaux.
	var list_panel := _box(CARD, 10, 6, 6, BORDER.darkened(0.15))
	var list_sel := _box(Color(ACCENT, 0.16), 6, 10, 6)
	var list_hover := _box(Color(1, 1, 1, 0.05), 6, 10, 6)
	var list_cursor := _box(Color(0, 0, 0, 0), 6, 10, 6, Color(ACCENT, 0.7))
	list_cursor.draw_center = false
	t.set_stylebox("panel", "ItemList", list_panel)
	t.set_stylebox("focus", "ItemList", StyleBoxEmpty.new())
	t.set_stylebox("selected", "ItemList", list_sel)
	t.set_stylebox("selected_focus", "ItemList", list_sel)
	t.set_stylebox("hovered", "ItemList", list_hover)
	t.set_stylebox("hovered_selected", "ItemList", list_sel)
	t.set_stylebox("hovered_selected_focus", "ItemList", list_sel)
	t.set_stylebox("cursor", "ItemList", list_cursor)
	t.set_stylebox("cursor_unfocused", "ItemList", StyleBoxEmpty.new())
	t.set_color("font_color", "ItemList", TEXT_DIM)
	t.set_color("font_hovered_color", "ItemList", TEXT)
	t.set_color("font_selected_color", "ItemList", TEXT)
	t.set_color("font_hovered_selected_color", "ItemList", TEXT)
	t.set_color("guide_color", "ItemList", Color(0, 0, 0, 0))
	t.set_constant("v_separation", "ItemList", 10)
	t.set_constant("h_separation", "ItemList", 10)
	t.set_constant("line_separation", "ItemList", 4)

	t.set_stylebox("panel", "Tree", list_panel)
	t.set_stylebox("focus", "Tree", StyleBoxEmpty.new())
	t.set_stylebox("selected", "Tree", list_sel)
	t.set_stylebox("selected_focus", "Tree", list_sel)
	t.set_stylebox("hovered", "Tree", list_hover)
	t.set_stylebox("hovered_dimmed", "Tree", list_hover)
	t.set_stylebox("hovered_selected", "Tree", list_sel)
	t.set_stylebox("hovered_selected_focus", "Tree", list_sel)
	t.set_stylebox("cursor", "Tree", list_cursor)
	t.set_stylebox("cursor_unfocused", "Tree", StyleBoxEmpty.new())
	var head := _box(Color(0, 0, 0, 0), 0, 10, 10)
	head.border_width_bottom = 1
	head.border_color = BORDER
	t.set_stylebox("title_button_normal", "Tree", head)
	t.set_stylebox("title_button_hover", "Tree", head)
	t.set_stylebox("title_button_pressed", "Tree", head)
	t.set_color("title_button_color", "Tree", TEXT_MUTED)
	t.set_font("title_button_font", "Tree", spaced_font())
	t.set_font_size("title_button_font_size", "Tree", 12)
	t.set_color("font_color", "Tree", TEXT)
	t.set_color("font_selected_color", "Tree", TEXT)
	t.set_color("font_hovered_color", "Tree", TEXT)
	t.set_color("guide_color", "Tree", Color(1, 1, 1, 0.05))
	t.set_color("relationship_line_color", "Tree", Color(0, 0, 0, 0))
	t.set_constant("draw_guides", "Tree", 1)
	t.set_constant("draw_relationship_lines", "Tree", 0)
	t.set_constant("v_separation", "Tree", 9)
	t.set_constant("h_separation", "Tree", 0)
	t.set_constant("item_margin", "Tree", 0)
	t.set_constant("inner_item_margin_left", "Tree", 12)
	t.set_constant("inner_item_margin_right", "Tree", 12)
	t.set_constant("inner_item_margin_top", "Tree", 11)
	t.set_constant("inner_item_margin_bottom", "Tree", 11)

	# Barres de progression et curseurs.
	var bar_bg := _box(FIELD, 6, 0, 0, BORDER.darkened(0.2))
	bar_bg.content_margin_top = 4
	bar_bg.content_margin_bottom = 4
	t.set_stylebox("background", "ProgressBar", bar_bg)
	t.set_stylebox("fill", "ProgressBar", _box(ACCENT, 6, 0, 0))
	t.set_color("font_color", "ProgressBar", TEXT)
	t.set_color("font_outline_color", "ProgressBar", Color(0, 0, 0, 0.6))
	t.set_type_variation("TealProgress", "ProgressBar")
	t.set_stylebox("fill", "TealProgress", _box(SECOND, 6, 0, 0))

	var track := _box(Color("4a4a4a"), 3, 0, 3)
	var track_fill := _box(ACCENT, 3, 0, 3)
	t.set_stylebox("slider", "HSlider", track)
	t.set_stylebox("grabber_area", "HSlider", track_fill)
	t.set_stylebox("grabber_area_highlight", "HSlider", _box(ACCENT_HI, 3, 0, 3))
	t.set_icon("grabber", "HSlider", _knob_icon(22, TEXT, ACCENT, 1.0))
	t.set_icon("grabber_highlight", "HSlider", _knob_icon(22, Color.WHITE, ACCENT_HI, 1.0))
	t.set_icon("grabber_disabled", "HSlider", _knob_icon(22, TEXT_MUTED, BORDER, 1.0))
	t.set_icon("tick", "HSlider", _knob_icon(4, TEXT_MUTED, TEXT_MUTED, 1.0))
	t.set_constant("center_grabber", "HSlider", 0)
	t.set_constant("grabber_offset", "HSlider", 0)

	# Barres de défilement fines.
	for type in ["VScrollBar", "HScrollBar"]:
		var vertical: bool = type == "VScrollBar"
		var scroll := StyleBoxFlat.new()
		scroll.bg_color = Color(0, 0, 0, 0)
		var grab := _box(Color(1, 1, 1, 0.16), 4, 0, 0)
		if vertical:
			scroll.content_margin_left = 5
			scroll.content_margin_right = 5
			grab.content_margin_left = 4
			grab.content_margin_right = 4
		else:
			scroll.content_margin_top = 5
			scroll.content_margin_bottom = 5
			grab.content_margin_top = 4
			grab.content_margin_bottom = 4
		t.set_stylebox("scroll", type, scroll)
		t.set_stylebox("scroll_focus", type, scroll)
		t.set_stylebox("grabber", type, grab)
		var gh := grab.duplicate() as StyleBoxFlat
		gh.bg_color = Color(1, 1, 1, 0.30)
		t.set_stylebox("grabber_highlight", type, gh)
		var gp := grab.duplicate() as StyleBoxFlat
		gp.bg_color = Color(ACCENT, 0.8)
		t.set_stylebox("grabber_pressed", type, gp)
	t.set_stylebox("panel", "ScrollContainer", StyleBoxEmpty.new())
	t.set_stylebox("focus", "ScrollContainer", StyleBoxEmpty.new())

	# Séparateurs.
	var sep := StyleBoxLine.new()
	sep.color = BORDER.darkened(0.1)
	sep.thickness = 1
	t.set_stylebox("separator", "HSeparator", sep)
	t.set_constant("separation", "HSeparator", 18)
	var vsep := StyleBoxLine.new()
	vsep.color = BORDER.darkened(0.1)
	vsep.thickness = 1
	vsep.vertical = true
	t.set_stylebox("separator", "VSeparator", vsep)

	# Menus contextuels, infobulles et fenêtres.
	var popup := _box(CARD_HI, 10, 6, 6, BORDER_HI)
	popup.shadow_color = Color(0, 0, 0, 0.45)
	popup.shadow_size = 12
	t.set_stylebox("panel", "PopupMenu", popup)
	t.set_stylebox("hover", "PopupMenu", _box(Color(ACCENT, 0.18), 6, 10, 6))
	var psep := StyleBoxLine.new()
	psep.color = BORDER
	t.set_stylebox("separator", "PopupMenu", psep)
	t.set_color("font_color", "PopupMenu", TEXT_DIM)
	t.set_color("font_hover_color", "PopupMenu", TEXT)
	t.set_color("font_disabled_color", "PopupMenu", TEXT_MUTED)
	t.set_constant("v_separation", "PopupMenu", 10)
	t.set_constant("item_start_padding", "PopupMenu", 12)
	t.set_constant("item_end_padding", "PopupMenu", 16)
	t.set_icon("radio_checked", "PopupMenu", _knob_icon(10, ACCENT, ACCENT, 1.0))
	t.set_icon("radio_unchecked", "PopupMenu", _knob_icon(10, Color(0, 0, 0, 0), Color(0, 0, 0, 0), 1.0))

	var tip := _box(Color("1f2f44"), 8, 12, 8, BORDER_HI)
	tip.shadow_color = Color(0, 0, 0, 0.4)
	tip.shadow_size = 8
	t.set_stylebox("panel", "TooltipPanel", tip)
	t.set_color("font_color", "TooltipLabel", TEXT)
	t.set_font_size("font_size", "TooltipLabel", 14)

	var win := _box(CARD_HI, RADIUS, 8, 8, BORDER_HI)
	win.expand_margin_top = 44
	win.expand_margin_left = 0
	win.shadow_color = Color(0, 0, 0, 0.5)
	win.shadow_size = 24
	t.set_stylebox("embedded_border", "Window", win)
	t.set_stylebox("embedded_unfocused_border", "Window", win)
	t.set_constant("title_height", "Window", 40)
	t.set_color("title_color", "Window", TEXT)
	t.set_font("title_font", "Window", font(700))
	t.set_font_size("title_font_size", "Window", 17)
	t.set_stylebox("panel", "AcceptDialog", _box(CARD_HI, RADIUS, 22, 16))
	t.set_constant("buttons_separation", "AcceptDialog", 12)
	t.set_color("font_color", "LinkButton", ACCENT)
	t.set_color("font_hover_color", "LinkButton", ACCENT_HI)
	t.set_color("font_pressed_color", "LinkButton", ACCENT_LO)

	_theme = t
	return t


# --- Constructeurs ------------------------------------------------------------

## Carte arrondie (variantes : « Card », « CardHi », « Tile », « Field »…).
static func card(variation := "Card") -> PanelContainer:
	var p := PanelContainer.new()
	p.theme_type_variation = variation
	return p


## Petit titre de section en capitales, discret.
static func section_title(text: String) -> Label:
	var l := label(text.to_upper(), "SectionLabel")
	return l


static func label(text: String, variation := "", wrap := false) -> Label:
	var l := Label.new()
	l.text = text
	if variation != "":
		l.theme_type_variation = variation
	if wrap:
		l.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		l.custom_minimum_size.x = 40
	return l


static func button(text: String, variation := "", tooltip := "") -> Button:
	var b := Button.new()
	b.text = text
	if variation != "":
		b.theme_type_variation = variation
	b.tooltip_text = tooltip
	b.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	return b


## Gros bouton orange pour l'action principale de l'écran.
static func primary_button(text: String, big := false) -> Button:
	var b := button(text, "PrimaryButton")
	if big:
		b.add_theme_font_size_override("font_size", 22)
		b.custom_minimum_size.y = 62
	else:
		b.custom_minimum_size.y = 48
	return b


static func hbox(separation := 12) -> HBoxContainer:
	var b := HBoxContainer.new()
	b.add_theme_constant_override("separation", separation)
	return b


static func vbox(separation := 12) -> VBoxContainer:
	var b := VBoxContainer.new()
	b.add_theme_constant_override("separation", separation)
	return b


## Espace extensible dans une boîte horizontale ou verticale.
static func spacer() -> Control:
	var c := Control.new()
	c.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	c.size_flags_vertical = Control.SIZE_EXPAND_FILL
	c.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return c


static func margin(all := 24, vertical := -1) -> MarginContainer:
	var m := MarginContainer.new()
	var v := all if vertical < 0 else vertical
	m.add_theme_constant_override("margin_left", all)
	m.add_theme_constant_override("margin_right", all)
	m.add_theme_constant_override("margin_top", v)
	m.add_theme_constant_override("margin_bottom", v)
	return m


## Tuile de mesure : petit titre, grande valeur et unité. `set_tile` la met à jour.
static func metric_tile(title: String, value := "--", unit := "", big := false) -> PanelContainer:
	var p := card("Tile")
	var v := vbox(2)
	p.add_child(v)
	v.add_child(section_title(title))
	var row := hbox(6)
	v.add_child(row)
	var val := label(value, "BigValueLabel" if big else "ValueLabel")
	row.add_child(val)
	var u := label(unit, "UnitLabel")
	u.size_flags_vertical = Control.SIZE_SHRINK_END
	u.add_theme_constant_override("line_spacing", 0)
	row.add_child(u)
	p.set_meta("value", val)
	p.set_meta("unit", u)
	return p


static func set_tile(tile: Control, value: String, unit = null) -> void:
	(tile.get_meta("value") as Label).text = value
	if unit != null:
		(tile.get_meta("unit") as Label).text = unit


## Boutons exclusifs côte à côte ; `on_select` reçoit l'index choisi.
static func segmented(options: Array, selected: int, on_select: Callable) -> HBoxContainer:
	var row := hbox(6)
	var group := ButtonGroup.new()
	for i in options.size():
		var b := button(str(options[i]), "SegmentButton")
		b.toggle_mode = true
		b.button_group = group
		b.button_pressed = i == selected
		b.pressed.connect(on_select.bind(i))
		row.add_child(b)
	return row


## Pastille d'état ronde ; `set_dot` change sa couleur.
static func status_dot(color: Color, diameter := 10) -> Panel:
	var p := Panel.new()
	p.custom_minimum_size = Vector2(diameter, diameter)
	p.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	p.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	p.mouse_filter = Control.MOUSE_FILTER_IGNORE
	set_dot(p, color)
	return p


static func set_dot(dot: Panel, color: Color) -> void:
	if dot.has_meta("color") and dot.get_meta("color") == color:
		return
	dot.set_meta("color", color)
	var sb := StyleBoxFlat.new()
	sb.bg_color = color
	sb.set_corner_radius_all(99)
	sb.anti_aliasing_size = 0.8
	if color.a > 0.5:
		sb.shadow_color = Color(color, 0.35)
		sb.shadow_size = 4
	dot.add_theme_stylebox_override("panel", sb)


## Couleur d'un état de connexion.
static func link_color(state: String) -> Color:
	match state:
		"connected", "ready":
			return OK
		"connecting", "scanning", "starting":
			return WARN
		"failed", "unavailable":
			return ERROR
	return TEXT_MUTED


## Couleur de zone d'entraînement selon la fraction de FTP.
static func zone_color(ftp_fraction: float) -> Color:
	if ftp_fraction < 0.55:
		return Color("8c8c8c")
	if ftp_fraction < 0.75:
		return Color("3d8bfd")
	if ftp_fraction < 0.90:
		return Color("3fbf6f")
	if ftp_fraction < 1.05:
		return Color("f2c94c")
	if ftp_fraction < 1.20:
		return Color("f2923a")
	return Color("eb5545")


## Tableau des zones de puissance (Z1 à Z6) pour une FTP donnée.
static func zones_grid(ftp: float) -> GridContainer:
	var grid := GridContainer.new()
	grid.columns = 3
	grid.add_theme_constant_override("h_separation", 12)
	grid.add_theme_constant_override("v_separation", 6)
	var bounds := [0.0, 0.55, 0.75, 0.90, 1.05, 1.20, -1.0]
	var names := ["Z1  Récupération", "Z2  Endurance", "Z3  Tempo", "Z4  Seuil", "Z5  VO2max", "Z6  Anaérobie"]
	for i in names.size():
		var sw := ColorRect.new()
		sw.color = zone_color(bounds[i] + 0.01)
		sw.custom_minimum_size = Vector2(10, 10)
		sw.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		grid.add_child(sw)
		var n := label(names[i], "DimLabel")
		n.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		grid.add_child(n)
		var txt := ""
		if i == 0:
			txt = "moins de %d W" % roundi(bounds[1] * ftp)
		elif bounds[i + 1] < 0.0:
			txt = "plus de %d W" % roundi(bounds[i] * ftp)
		else:
			txt = "%d à %d W" % [roundi(bounds[i] * ftp), roundi(bounds[i + 1] * ftp)]
		var v := label(txt, "DimLabel")
		v.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
		grid.add_child(v)
	return grid


## Ligne de formulaire : libellé à gauche, contrôle et aide à droite.
static func form_row(title: String, control: Control, hint := "", label_width := 250) -> HBoxContainer:
	var row := hbox(16)
	var l := label(title, "")
	l.custom_minimum_size.x = label_width
	l.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	row.add_child(l)
	row.add_child(control)
	if hint != "":
		var h := label(hint, "MutedLabel", true)
		h.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		h.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		row.add_child(h)
	return row


# --- Fabrication des styles ----------------------------------------------------

static func _box(bg: Color, radius := RADIUS, mx := 12, my := 8, border := Color(0, 0, 0, 0)) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = bg
	sb.set_corner_radius_all(radius)
	sb.content_margin_left = mx
	sb.content_margin_right = mx
	sb.content_margin_top = my
	sb.content_margin_bottom = my
	sb.anti_aliasing_size = 0.7
	if border.a > 0.0:
		sb.border_color = border
		sb.set_border_width_all(1)
	return sb


static func _focus(radius: int) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.draw_center = false
	sb.set_corner_radius_all(radius + 2)
	sb.set_border_width_all(2)
	sb.border_color = Color(ACCENT_HI, 0.9)
	sb.set_expand_margin_all(3)
	return sb


static func _label_variation(t: Theme, name: String, size: int, f: Font, color: Color) -> void:
	t.set_type_variation(name, "Label")
	t.set_font_size("font_size", name, size)
	t.set_font("font", name, f)
	t.set_color("font_color", name, color)


static func _panel_variation(t: Theme, name: String, sb: StyleBox) -> void:
	t.set_type_variation(name, "PanelContainer")
	t.set_stylebox("panel", name, sb)


static func _button_styles(t: Theme, type: String, bg: Color, border: Color, mx: int, my: int, radius: int) -> void:
	t.set_stylebox("normal", type, _box(bg, radius, mx, my, border))
	t.set_stylebox("hover", type, _box(bg.lightened(0.07), radius, mx, my, BORDER_HI))
	t.set_stylebox("pressed", type, _box(bg.darkened(0.15), radius, mx, my, ACCENT))
	t.set_stylebox("hover_pressed", type, _box(bg.darkened(0.1), radius, mx, my, ACCENT))
	t.set_stylebox("disabled", type, _box(Color(bg, 0.45), radius, mx, my, Color(border, 0.4)))
	t.set_stylebox("focus", type, _focus(radius))


static func _button_colors(t: Theme, type: String, normal: Color, hover: Color, pressed: Color, disabled: Color) -> void:
	t.set_color("font_color", type, normal)
	t.set_color("font_hover_color", type, hover)
	t.set_color("font_pressed_color", type, pressed)
	t.set_color("font_hover_pressed_color", type, pressed)
	t.set_color("font_focus_color", type, hover)
	t.set_color("font_disabled_color", type, disabled)


# --- Petites icônes dessinées à la volée (bords lissés) -------------------------

static func _cover(d: float) -> float:
	return clampf(0.5 - d, 0.0, 1.0)


static func _blend(dst: Color, src: Color, a: float) -> Color:
	var sa := src.a * a
	var out_a := sa + dst.a * (1.0 - sa)
	if out_a <= 0.0:
		return Color(0, 0, 0, 0)
	var rgb := (Color(src.r, src.g, src.b) * sa + Color(dst.r, dst.g, dst.b) * dst.a * (1.0 - sa)) / out_a
	return Color(rgb.r, rgb.g, rgb.b, out_a)


static func _seg_dist(p: Vector2, a: Vector2, b: Vector2) -> float:
	var ab := b - a
	var h := clampf((p - a).dot(ab) / ab.length_squared(), 0.0, 1.0)
	return (p - a - ab * h).length()


## Interrupteur en forme de pilule (allumé : orange, bouton à droite).
static func _switch_icon(on: bool, alpha: float) -> ImageTexture:
	var w := 44
	var h := 24
	var img := Image.create_empty(w, h, false, Image.FORMAT_RGBA8)
	var r := h * 0.5 - 1.0
	var track := ACCENT if on else Color("2c3d55")
	var knob_c := Vector2(w - 1.0 - r, h * 0.5) if on else Vector2(1.0 + r, h * 0.5)
	for y in h:
		for x in w:
			var p := Vector2(x + 0.5, y + 0.5)
			var c := Color(0, 0, 0, 0)
			c = _blend(c, track, _cover(_seg_dist(p, Vector2(1.0 + r, h * 0.5), Vector2(w - 1.0 - r, h * 0.5)) - r))
			c = _blend(c, Color.WHITE if on else Color("c3cedb"), _cover(p.distance_to(knob_c) - (r - 3.0)))
			c.a *= alpha
			img.set_pixel(x, y, c)
	return ImageTexture.create_from_image(img)


## Bouton rond (curseurs) : disque plein entouré d'un anneau coloré.
static func _knob_icon(d: int, fill: Color, ring: Color, alpha: float) -> ImageTexture:
	var img := Image.create_empty(d, d, false, Image.FORMAT_RGBA8)
	var c0 := Vector2(d, d) * 0.5
	var r := d * 0.5 - 0.5
	for y in d:
		for x in d:
			var dist := Vector2(x + 0.5, y + 0.5).distance_to(c0)
			var c := _blend(Color(0, 0, 0, 0), ring, _cover(dist - r))
			c = _blend(c, fill, _cover(dist - (r - maxf(2.0, d * 0.14))))
			c.a *= alpha
			img.set_pixel(x, y, c)
	return ImageTexture.create_from_image(img)


## Chevron vers le bas des listes déroulantes.
static func _chevron_icon(color: Color) -> ImageTexture:
	var w := 14
	var h := 10
	var img := Image.create_empty(w, h, false, Image.FORMAT_RGBA8)
	var a := Vector2(2.5, 3.0)
	var m := Vector2(7.0, 7.0)
	var b := Vector2(11.5, 3.0)
	for y in h:
		for x in w:
			var p := Vector2(x + 0.5, y + 0.5)
			var d := minf(_seg_dist(p, a, m), _seg_dist(p, m, b)) - 0.9
			img.set_pixel(x, y, Color(color, _cover(d)))
	return ImageTexture.create_from_image(img)
