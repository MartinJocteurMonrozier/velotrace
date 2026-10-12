// Sol texturé par l'image aérienne (ou sa couleur de repli).
// Les grands carrés lointains se retirent là où un carré détaillé est chargé :
// le masque contient un texel par carré de 512 m.
shader_type spatial;

uniform sampler2D albedo_tex : source_color, filter_linear_mipmap_anisotropic, repeat_disable;
uniform bool use_mask = false;
uniform sampler2D near_mask : filter_nearest, repeat_disable;
// x0, z0, largeur, profondeur du masque en mètres (repère Godot).
uniform vec4 mask_rect = vec4(0.0, 0.0, 1.0, 1.0);

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
	ALBEDO = texture(albedo_tex, UV).rgb;
	ROUGHNESS = 1.0;
	SPECULAR = 0.15;
}
