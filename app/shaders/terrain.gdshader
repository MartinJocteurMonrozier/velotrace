// Sol texturé par l'image aérienne (ou sa couleur de repli), avec un grain
// de détail près de la caméra qui masque le flou de l'image. Au bord des
// chaussées, l'image laisse place au sol détaillé des accotements (herbe,
// ville, cailloux : ground.gdshaderinc), qui s'y raccorde sans couture. Les
// pentes très raides (falaises, talus taillés) deviennent rocheuses.
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
uniform vec3 rock : source_color = vec3(0.50, 0.48, 0.45);
// Carte du sol du carré : nature (RGB) et proximité d'une chaussée (A).
uniform sampler2D ground_tex : filter_linear_mipmap, repeat_disable;
uniform bool has_ground = false;

#include "res://shaders/ground.gdshaderinc"

varying vec2 world_xz;
varying vec3 world_n;
varying float world_y;

void vertex() {
	vec4 w = MODEL_MATRIX * vec4(VERTEX, 1.0);
	world_xz = w.xz;
	world_y = w.y;
	world_n = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
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
	float rough = 1.0;
	if (has_ground) {
		vec4 g = texture(ground_tex, UV);
		// Lisière irrégulière entre le sol détaillé et l'image.
		float edge = g.a + (g_fbm(world_xz * 0.12, 50u) - 0.5) * 0.6;
		float near = smoothstep(0.35, 0.6, edge);
		if (near > 0.0) {
			float fine = 1.0 - smoothstep(0.02, 0.12, length(fwidth(world_xz)));
			GroundSample s = ground_sample(world_xz, g.rgb, fine);
			col = mix(col, s.col, near);
			rough = mix(rough, s.rough, near);
			NORMAL = normalize(mix(NORMAL, g_bump(NORMAL, VERTEX, s.h * fine), near));
		}
	}
	// Roche : strates horizontales et fissures, plus claire que l'image.
	float steep = 1.0 - smoothstep(0.58, 0.74, normalize(world_n).y);
	if (steep > 0.0) {
		vec2 q = vec2(world_xz.x + world_xz.y, world_y);
		float strata = g_noise(vec2(q.x * 0.08, q.y * 1.3), 60u) * 0.6 + g_noise(q * vec2(0.5, 2.5), 61u) * 0.4;
		float crack = smoothstep(0.82, 0.9, g_noise(q * vec2(0.9, 0.25), 62u));
		vec3 r = rock * (0.72 + 0.5 * strata) * (1.0 - 0.35 * crack);
		float lum = dot(col, vec3(0.3, 0.55, 0.15));
		r *= 0.75 + 0.5 * clamp(lum * 2.5, 0.0, 1.0);
		col = mix(col, r, steep * 0.9);
	}
	ALBEDO = col;
	ROUGHNESS = rough;
	SPECULAR = 0.15;
}
