// Sol texturé par l'image aérienne (ou sa couleur de repli), avec un grain
// de détail près de la caméra qui masque le flou de l'image.
// Les grands carrés lointains se retirent là où un carré détaillé est chargé :
// le masque contient un texel par carré de 512 m.
shader_type spatial;

uniform sampler2D albedo_tex : source_color, filter_linear_mipmap_anisotropic, repeat_disable;
uniform sampler2D detail_tex : filter_linear_mipmap, repeat_enable;
uniform bool use_mask = false;
uniform sampler2D near_mask : filter_nearest, repeat_disable;
// x0, z0, largeur, profondeur du masque en mètres (repère Godot).
uniform vec4 mask_rect = vec4(0.0, 0.0, 1.0, 1.0);
uniform float detail_strength = 0.55;

varying vec2 world_xz;

void vertex() {
	world_xz = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xz;
}

void fragment() {
	if (use_mask) {
		vec2 m = (world_xz - mask_rect.xy) / mask_rect.zw;
		if (m.x >= 0.0 && m.y >= 0.0 && m.x < 1.0 && m.y < 1.0 && texture(near_mask, m).r > 0.5) {
			discard;
		}
	}
	vec3 col = texture(albedo_tex, UV).rgb;
	float dist = length(VERTEX);
	float fade = (1.0 - smoothstep(25.0, 260.0, dist)) * detail_strength;
	if (fade > 0.0) {
		float a = texture(detail_tex, world_xz / 7.0).r;
		float b = texture(detail_tex, world_xz / 1.3).r;
		float c = texture(detail_tex, world_xz / 0.35).r;
		float d = a * 0.45 + b * 0.35 + c * 0.2;
		// La végétation (verte) reçoit un grain d'herbe, le reste un grain minéral.
		float green = clamp((col.g - max(col.r, col.b)) * 8.0, 0.0, 1.0);
		vec3 tint = mix(vec3(1.0), vec3(0.92, 1.06, 0.88), green * (b - 0.5) * 2.0);
		col *= mix(vec3(1.0), (0.62 + 0.76 * d) * tint, fade);
	}
	ALBEDO = col;
	ROUGHNESS = 1.0;
	SPECULAR = 0.15;
}
