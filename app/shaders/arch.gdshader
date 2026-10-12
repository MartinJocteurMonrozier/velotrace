// Arches gonflables, bandeaux et marquages : uni, à pois (grimpeur), en
// damier (arrivée) ou à bandes. Le motif suit les UV de la primitive, ou les
// coordonnées locales du maillage pour les boîtes.
shader_type spatial;

uniform vec3 base_color : source_color = vec3(1.0);
uniform vec3 second_color : source_color = vec3(0.85, 0.08, 0.1);
// 0 uni, 1 pois, 2 damier, 3 bandes (selon y).
uniform int pattern = 0;
uniform vec2 uv_scale = vec2(1.0);
uniform bool use_local = false;
// Toile satinée (arches) ou peinture mate (marquage au sol).
uniform float roughness = 0.5;

varying vec3 local;

void vertex() {
	local = VERTEX;
}

void fragment() {
	vec2 p = (use_local ? local.xy : UV) * uv_scale;
	vec3 c = base_color;
	if (pattern == 1) {
		vec2 q = p;
		q.x += 0.5 * mod(floor(q.y), 2.0);
		float d = length(fract(q) - 0.5);
		c = mix(base_color, second_color, 1.0 - smoothstep(0.24, 0.28, d));
	} else if (pattern == 2) {
		c = mix(base_color, second_color, mod(floor(p.x) + floor(p.y), 2.0));
	} else if (pattern == 3) {
		c = mix(base_color, second_color, step(0.5, fract(p.y)));
	}
	ALBEDO = c;
	ROUGHNESS = roughness;
	SPECULAR = 0.45;
	RIM = roughness < 0.8 ? 0.25 : 0.0;
	RIM_TINT = 0.6;
}
