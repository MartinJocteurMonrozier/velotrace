// Murs de soutènement et parapets : moellons de calcaire jointoyés,
// couronnement en pierres de taille, terrasse herbeuse au-dessus.
// UV.x : abscisse le long du mur (m) ; UV.y : hauteur au-dessus du pied, ou
// recul depuis la face sur le dessus (m).
// COLOR.r : 0 parement, 0,5 couronnement, 1 terrasse.
shader_type spatial;

uniform vec3 stone : source_color = vec3(0.58, 0.55, 0.50);
uniform vec3 stone_dark : source_color = vec3(0.40, 0.38, 0.35);
uniform vec3 mortar : source_color = vec3(0.70, 0.68, 0.63);
uniform vec3 grass : source_color = vec3(0.30, 0.37, 0.18);
uniform vec3 soil : source_color = vec3(0.36, 0.30, 0.22);

varying float v_zone;

void vertex() {
	v_zone = COLOR.r;
}

float hash(vec2 p) {
	p = fract(p * vec2(123.34, 456.21));
	p += dot(p, p + 45.32);
	return fract(p.x * p.y);
}

float vnoise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	float a = hash(i);
	float b = hash(i + vec2(1.0, 0.0));
	float c = hash(i + vec2(0.0, 1.0));
	float d = hash(i + vec2(1.0, 1.0));
	return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// Appareil de moellons : rangées de hauteur variable, pierres décalées de
// longueurs et de teintes inégales.
vec3 masonry(vec2 m, float fine, out float rough) {
	float row_h = 0.24;
	float row = floor(m.y / row_h);
	float y = fract(m.y / row_h);
	float len = 0.3 + 0.4 * hash(vec2(row, 1.3));
	float x = m.x / len + hash(vec2(row, 7.1)) * 3.0;
	float cell = floor(x);
	float fx = fract(x);
	float id = hash(vec2(cell, row));
	vec3 tint = mix(vec3(1.0, 0.96, 0.9), vec3(0.9, 0.94, 1.0), hash(vec2(cell, row) + 0.5));
	vec3 col = mix(stone_dark, stone, 0.2 + 0.8 * id) * tint * (0.85 + 0.3 * vnoise(m * 5.0));
	float jx = min(fx, 1.0 - fx) * len;
	float jy = min(y, 1.0 - y) * row_h;
	float edge = min(jx, jy);
	// Pierres bombées : plus sombres près des joints.
	col *= 0.82 + 0.18 * smoothstep(0.0, 0.07, edge);
	float joint = 1.0 - smoothstep(0.01, 0.025, edge);
	col = mix(col, mortar * 0.75, joint * fine);
	// Coulures et lichens vers le bas du mur.
	col *= 1.0 - 0.18 * vnoise(vec2(m.x * 0.7, m.y * 0.15)) * (1.0 - smoothstep(0.0, 2.5, m.y));
	rough = mix(0.9, 1.0, joint);
	return col;
}

void fragment() {
	vec2 m = UV;
	float px = length(fwidth(m));
	float fine = 1.0 - smoothstep(0.02, 0.15, px);
	float rough = 0.95;
	vec3 col;
	if (v_zone > 0.75) {
		float n = vnoise(m * 0.7) * 0.6 + vnoise(m * 3.0) * 0.4;
		col = mix(soil, grass, smoothstep(0.3, 0.6, n)) * (0.85 + 0.3 * vnoise(m * 9.0));
		rough = 1.0;
	} else if (v_zone > 0.25) {
		// Couronnement : grandes dalles posées en travers.
		float x = fract(m.x / 0.9);
		float id = hash(vec2(floor(m.x / 0.9), 5.0));
		col = mix(stone_dark, stone, 0.6 + 0.4 * id) * 1.05;
		col = mix(col, mortar * 0.75, (1.0 - smoothstep(0.0, 0.03, min(x, 1.0 - x))) * fine);
	} else {
		col = masonry(m, fine, rough);
	}
	ALBEDO = col;
	ROUGHNESS = rough;
	SPECULAR = 0.25;
}
