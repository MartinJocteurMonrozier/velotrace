extends SubViewport
## Atlas des banderoles imprimées (barrières, panneaux du portique) : chaque
## motif, numéroté comme dans velo-world (race::banner_designs), est dessiné
## une fois par des contrôles 2D, puis recopié dans une texture avec mipmaps
## pour que le texte reste net de loin.

const CELL := Vector2i(512, 170)
const COLUMNS := 2
## Fond, texte, inscription, ornement (0 rien, 1 liserés, 2 tricolore,
## 3 pois, 4 damier).
const DESIGNS := [
	[Color(0.99, 0.83, 0.06), Color(0.05, 0.08, 0.2), "VELOTRACE", 1],
	[Color(0.06, 0.25, 0.6), Color(1.0, 1.0, 1.0), "VELOTRACE", 1],
	[Color(0.84, 0.07, 0.1), Color(1.0, 1.0, 1.0), "ALLEZ ALLEZ !", 0],
	[Color(0.06, 0.5, 0.26), Color(1.0, 1.0, 1.0), "ÉTAPE DU JOUR", 1],
	[Color(0.98, 0.98, 0.97), Color(0.06, 0.25, 0.6), "VELOTRACE", 2],
	[Color(0.05, 0.09, 0.24), Color(0.99, 0.83, 0.06), "ROUE LIBRE", 0],
	[Color(0.98, 0.98, 0.97), Color(0.84, 0.07, 0.1), "GRIMPEURS", 3],
	[Color(0.96, 0.45, 0.07), Color(1.0, 1.0, 1.0), "PELOTON", 1],
	[Color(0.2, 0.58, 0.9), Color(1.0, 1.0, 1.0), "BONNE ROUTE", 0],
	[Color(0.98, 0.98, 0.97), Color(0.05, 0.05, 0.06), "SPRINT FINAL", 4],
]

var _materials: Array[ShaderMaterial] = []


func _init() -> void:
	name = "AtlasBanderoles"
	size = Vector2i(CELL.x * COLUMNS, 1024)
	disable_3d = true
	transparent_bg = false
	render_target_update_mode = SubViewport.UPDATE_ONCE
	var font := FontVariation.new()
	font.base_font = ThemeDB.fallback_font
	font.variation_embolden = 1.1
	for i: int in DESIGNS.size():
		_cell(i, DESIGNS[i], font)


## Le matériau reçoit l'atlas (texture du viewport, puis copie avec mipmaps).
func use(m: ShaderMaterial) -> void:
	_materials.append(m)
	m.set_shader_parameter("atlas", get_texture())


func _ready() -> void:
	# Une image rendue, puis copiée : le viewport ne sert plus.
	await RenderingServer.frame_post_draw
	await RenderingServer.frame_post_draw
	var img := get_texture().get_image()
	if img == null or img.is_empty():
		return
	img.generate_mipmaps()
	var tex := ImageTexture.create_from_image(img)
	for m: ShaderMaterial in _materials:
		m.set_shader_parameter("atlas", tex)
	render_target_update_mode = SubViewport.UPDATE_DISABLED


func _cell(i: int, d: Array, font: Font) -> void:
	var bg: Color = d[0]
	var fg: Color = d[1]
	var text: String = d[2]
	var deco: int = d[3]
	var box := Control.new()
	box.position = Vector2((i % COLUMNS) * CELL.x, floori(float(i) / COLUMNS) * CELL.y)
	box.size = Vector2(CELL)
	box.clip_contents = true
	add_child(box)
	var w := float(CELL.x)
	var h := float(CELL.y)
	_rect(box, Rect2(0.0, 0.0, w, h), bg)
	match deco:
		1:
			_rect(box, Rect2(0.0, 8.0, w, 10.0), fg)
			_rect(box, Rect2(0.0, h - 18.0, w, 10.0), fg)
		2:
			_rect(box, Rect2(0.0, 0.0, w, 16.0), Color(0.06, 0.25, 0.6))
			_rect(box, Rect2(0.0, h - 16.0, w, 16.0), Color(0.84, 0.07, 0.1))
		3:
			var red := Color(0.84, 0.07, 0.1)
			for row: int in 4:
				for col: int in 12:
					var x := col * 46.0 + (23.0 if row % 2 == 1 else 0.0) + 4.0
					_dot(box, Vector2(x, row * 46.0 + 8.0), 15.0, red)
		4:
			for col: int in 32:
				for row: int in 2:
					var dark := (col + row) % 2 == 0
					var c := Color(0.05, 0.05, 0.06) if dark else bg
					_rect(box, Rect2(col * 16.0, row * 16.0, 16.0, 16.0), c)
					_rect(box, Rect2(col * 16.0, h - 32.0 + row * 16.0, 16.0, 16.0), c)
	var label := Label.new()
	label.text = text
	label.position = Vector2.ZERO
	label.size = box.size
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	var ls := LabelSettings.new()
	ls.font = font
	var width := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, 100).x
	ls.font_size = int(clampf(100.0 * (w - 60.0) / maxf(1.0, width), 30.0, 112.0))
	ls.font_color = fg
	if deco == 3:
		# Lisible par-dessus les pois.
		ls.outline_size = 22
		ls.outline_color = bg
	label.label_settings = ls
	box.add_child(label)


func _rect(parent: Control, r: Rect2, c: Color) -> void:
	var cr := ColorRect.new()
	cr.position = r.position
	cr.size = r.size
	cr.color = c
	parent.add_child(cr)


func _dot(parent: Control, center: Vector2, radius: float, c: Color) -> void:
	var p := Panel.new()
	var sb := StyleBoxFlat.new()
	sb.bg_color = c
	sb.set_corner_radius_all(int(radius))
	sb.anti_aliasing = true
	p.add_theme_stylebox_override("panel", sb)
	p.position = center - Vector2(radius, radius)
	p.size = Vector2(radius, radius) * 2.0
	parent.add_child(p)
