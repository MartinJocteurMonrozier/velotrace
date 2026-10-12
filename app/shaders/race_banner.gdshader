// Banderoles imprimées : une case de l'atlas dessiné par race_atlas.gd sur
// la face avant, toile unie au dos. Le motif est `design`, ou, s'il est
// négatif, celui porté par l'alpha de la couleur d'instance (MultiMesh des
// barrières : (motif + 0,5) / 16).
shader_type spatial;
render_mode cull_disabled;

uniform sampler2D atlas : source_color, filter_linear_mipmap_anisotropic, repeat_disable;
uniform int design = -1;
// Texte vertical (panneaux des piles), lu de bas en haut.
uniform bool vertical = false;
uniform vec3 back_color : source_color = vec3(0.86, 0.86, 0.84);
// Case de l'atlas (fraction de la texture) et nombre de colonnes.
uniform vec2 cell = vec2(0.5, 0.166015625);
uniform float columns = 2.0;

varying flat float pick;

void vertex() {
	pick = design >= 0 ? float(design) : floor(COLOR.a * 16.0);
}

void fragment() {
	vec2 uv = vertical ? vec2(1.0 - UV.y, UV.x) : UV;
	vec2 at = vec2(mod(pick, columns), floor(pick / columns));
	vec3 c = texture(atlas, (at + clamp(uv, vec2(0.004), vec2(0.996))) * cell).rgb;
	ALBEDO = FRONT_FACING ? c : back_color;
	ROUGHNESS = 0.55;
	SPECULAR = 0.4;
}
