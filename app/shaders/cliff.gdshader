// Falaises sculptées au bord des routes (voir roadnet.rs, push_cliff) :
// strates inclinées de couleurs variées, coulures sombres, fissures,
// lichens, mousse sur les replats et rebord herbeux et caillouteux.
// UV.x : abscisse le long du parcours (m) ; UV.y : hauteur au-dessus du
// pied (m). COLOR.r : creux de la paroi ; COLOR.g : 1 sur le rebord.
shader_type spatial;

#include "res://shaders/ground.gdshaderinc"

uniform vec3 rock_light : source_color = vec3(0.62, 0.59, 0.54);
uniform vec3 rock_dark : source_color = vec3(0.38, 0.37, 0.36);
uniform vec3 rock_warm : source_color = vec3(0.58, 0.49, 0.40);
uniform vec3 moss : source_color = vec3(0.26, 0.33, 0.13);

varying vec3 world_pos;
varying vec3 world_n;
varying float v_recess;
varying float v_cap;

void vertex() {
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	world_n = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
	v_recess = COLOR.r;
	v_cap = COLOR.g;
}

void fragment() {
	float s = UV.x;
	float y = world_pos.y + 0.07 * s;
	float fine = 1.0 - smoothstep(0.02, 0.15, length(fwidth(vec2(s, y))));

	// Bancs de 1,7 m environ, chacun avec sa teinte.
	float layer_y = y / 1.7;
	ivec2 layer = ivec2(0, int(floor(layer_y)));
	float tone = g_hash(layer, 70u);
	vec3 col = mix(rock_light, rock_dark, 0.15 + 0.4 * tone + 0.25 * g_fbm(vec2(s * 0.05, y * 0.08), 79u));
	col = mix(col, rock_warm, step(0.85, g_hash(layer, 71u)) * 0.35);
	// Litage fin et blocs.
	float bedding = g_noise(vec2(s * 0.35, y * 7.0), 72u);
	float blocks = g_fbm(vec2(s, y) * 1.3, 73u);
	col *= 0.78 + 0.3 * bedding + 0.2 * (blocks - 0.5);
	// Coulures sombres sous les corniches.
	float streak = smoothstep(0.55, 0.8, g_noise(vec2(s * 2.2, y * 0.12), 74u));
	col *= 1.0 - 0.35 * streak * (1.0 - fract(layer_y));
	// Fissures verticales et joints de banc.
	float crack = smoothstep(0.86, 0.93, g_noise(vec2(s * 1.6, y * 0.35), 75u));
	float joint = 1.0 - smoothstep(0.0, 0.05, min(fract(layer_y), 1.0 - fract(layer_y)));
	col *= 1.0 - 0.45 * max(crack, joint * 0.8) * fine;
	// Lichens.
	vec3 c = g_cells(vec2(s, y) * 3.0, 76u);
	float lichen = (1.0 - smoothstep(0.08, 0.16, c.x)) * step(0.96, c.z) * fine * smoothstep(0.4, 0.7, g_noise(vec2(s, y) * 0.2, 78u));
	col = mix(col, pow(mix(vec3(0.66, 0.50, 0.24), vec3(0.52, 0.56, 0.38), step(0.98, c.z)), vec3(2.2)), lichen * 0.5);
	// Creux à l'ombre.
	col *= mix(1.0, 0.62, v_recess);

	// Mousse et herbe sur ce qui regarde le ciel, rebord herbeux.
	float up = smoothstep(0.5, 0.8, world_n.y);
	float green = max(up * smoothstep(0.35, 0.65, g_fbm(world_pos.xz * 0.8, 77u) + 0.2), v_cap);
	float rough = 0.9;
	float h = (bedding * 0.04 + blocks * 0.12 - joint * 0.05 - crack * 0.06);
	if (green > 0.0) {
		GroundSample g = ground_sample(world_pos.xz, vec3(0.65, 0.0, 0.35), fine);
		vec3 top = v_cap > 0.5 ? g.col : mix(moss, g.col, 0.4);
		col = mix(col, top, green);
		h = mix(h, g.h, green);
		rough = mix(rough, g.rough, green);
	}
	NORMAL = g_bump(NORMAL, VERTEX, h * fine);
	ALBEDO = col;
	ROUGHNESS = rough;
	SPECULAR = 0.2;
}
