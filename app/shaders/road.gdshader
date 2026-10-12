// Chaussée et accotements.
// UV.x : 0 à 1 en travers de la chaussée (6 m), < 0 ou > 1 sur les accotements ;
// UV.y : abscisse le long de la route (m).
shader_type spatial;

uniform vec3 asphalt : source_color = vec3(0.19, 0.19, 0.20);
uniform vec3 asphalt_worn : source_color = vec3(0.30, 0.30, 0.30);
uniform vec3 paint : source_color = vec3(0.90, 0.90, 0.86);
uniform vec3 gravel : source_color = vec3(0.47, 0.43, 0.37);
uniform vec3 grass : source_color = vec3(0.30, 0.38, 0.17);
const float ROAD_WIDTH = 6.0;

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

float fbm(vec2 p) {
	float s = 0.0;
	float a = 0.5;
	for (int k = 0; k < 4; k++) {
		s += vnoise(p) * a;
		p *= 2.03;
		a *= 0.5;
	}
	return s;
}

void fragment() {
	// Position en mètres : en travers (0 au bord gauche) et le long de la route.
	vec2 m = vec2(UV.x * ROAD_WIDTH, UV.y);
	// Atténue les détails fins au loin pour éviter le scintillement.
	float px = length(fwidth(m));
	float fine = 1.0 - smoothstep(0.02, 0.12, px);
	vec3 col;
	float rough;
	if (UV.x < 0.0 || UV.x > 1.0) {
		// Accotement : gravier tassé près du bitume, puis herbe.
		float out_m = UV.x < 0.0 ? -UV.x * ROAD_WIDTH : (UV.x - 1.0) * ROAD_WIDTH;
		float n = fbm(m * 1.3);
		float g = smoothstep(0.25, 0.9, out_m + (n - 0.5) * 0.8);
		col = mix(gravel * (0.85 + 0.3 * n), grass * (0.8 + 0.4 * fbm(m * 0.6 + 7.0)), g);
		col *= 0.9 + 0.2 * mix(0.5, hash(floor(m * 25.0)), fine);
		rough = 1.0;
	} else {
		// Bitume : grain, taches, bandes de roulement plus claires et lisses.
		float grain = mix(0.5, hash(floor(m * 40.0)), fine);
		float blotch = fbm(m * vec2(0.35, 0.12));
		float lane = fract(UV.x * 2.0);
		float tracks = exp(-pow((lane - 0.28) / 0.07, 2.0)) + exp(-pow((lane - 0.72) / 0.07, 2.0));
		col = mix(asphalt, asphalt_worn, clamp(blotch * 0.7 + tracks * 0.25, 0.0, 1.0));
		col *= 0.88 + 0.24 * grain;
		// Rapiéçages sombres ici et là.
		float patch_mask = smoothstep(0.72, 0.75, vnoise(m * vec2(0.25, 0.05) + 3.1));
		col = mix(col, asphalt * 0.75, patch_mask * 0.8);
		rough = 0.88 - tracks * 0.12;
		// Lignes de rive continues et axe discontinu (3 m peints, 10 m de vide).
		float edge = step(abs(m.x - 0.25), 0.06) + step(abs(m.x - (ROAD_WIDTH - 0.25)), 0.06);
		float centre = step(abs(m.x - ROAD_WIDTH * 0.5), 0.06) * step(fract(m.y / 13.0), 3.0 / 13.0);
		float paint_mask = clamp(edge + centre, 0.0, 1.0);
		// Peinture usée par endroits.
		paint_mask *= 0.75 + 0.25 * vnoise(m * vec2(2.0, 0.7));
		col = mix(col, paint, paint_mask);
		rough = mix(rough, 0.6, paint_mask);
	}
	ALBEDO = col;
	ROUGHNESS = rough;
	SPECULAR = 0.35;
}
