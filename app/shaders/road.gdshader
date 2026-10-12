// Chaussée, bordures, trottoirs et accotements ; amorces de routes et îlots
// de ronds-points (voir road.rs, push_strip).
// UV.x : écart à l'axe (m, + à droite) ; UV.y : abscisse le long du ruban (m).
// UV2.x : demi-largeur de la chaussée (m) ; UV2.y : voies (< 0 : sens unique).
// COLOR.r, COLOR.g : largeur des trottoirs gauche et droit (÷ 4 m) ;
// COLOR.a : marquages (bits ÷ 255).
shader_type spatial;

uniform vec3 asphalt : source_color = vec3(0.19, 0.19, 0.20);
uniform vec3 asphalt_worn : source_color = vec3(0.30, 0.30, 0.30);
uniform vec3 paint : source_color = vec3(0.90, 0.90, 0.86);
uniform vec3 gravel : source_color = vec3(0.47, 0.43, 0.37);
uniform vec3 grass : source_color = vec3(0.30, 0.38, 0.17);
uniform vec3 pavement : source_color = vec3(0.44, 0.43, 0.41);
uniform vec3 kerb_stone : source_color = vec3(0.66, 0.65, 0.62);

const int CENTRE_DASH = 1;
const int CENTRE_SOLID = 2;
const int EDGE_LEFT = 4;
const int EDGE_RIGHT = 8;
const int GAP_LEFT = 16;
const int GAP_RIGHT = 32;
const int GIVE_WAY = 64;
const int GRASS = 128;

varying flat int v_marks;
varying float v_half;
varying float v_lanes;
varying vec2 v_walk;

void vertex() {
	v_marks = int(round(COLOR.a * 255.0));
	v_half = UV2.x;
	v_lanes = UV2.y;
	v_walk = COLOR.rg * 4.0;
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

// Trait longitudinal de demi-largeur `w` en `x`, plein ou discontinu
// (`on` mètres peints tous les `period` mètres).
float stripe(vec2 m, float x, float w, float on, float period) {
	float line = step(abs(m.x - x), w);
	return period > 0.0 ? line * step(fract(m.y / period), on / period) : line;
}

void fragment() {
	vec2 m = UV;
	float px = length(fwidth(m));
	float fine = 1.0 - smoothstep(0.02, 0.12, px);
	float grain = mix(0.5, hash(floor(m * 40.0)), fine);
	vec3 col;
	float rough = 1.0;
	float hw = v_half;
	float out_m = abs(m.x) - hw;
	float walk = m.x < 0.0 ? v_walk.x : v_walk.y;

	if ((v_marks & GRASS) != 0) {
		// Îlot : pelouse tondue et quelques fleurs.
		float n = fbm(m * 0.5);
		col = grass * (0.8 + 0.45 * n) * vec3(1.0, 1.08, 1.0);
		float f = hash(floor(m * 3.0));
		if (f > 0.965 && fine > 0.5) {
			col = mix(vec3(0.85, 0.25, 0.30), vec3(0.95, 0.85, 0.30), step(0.985, f));
		}
	} else if (out_m <= 0.0 && abs(v_lanes) < 0.5) {
		// Piste non revêtue : terre et gravier, deux ornières, herbe au milieu.
		float n = fbm(m * vec2(0.8, 0.4));
		col = gravel * (0.8 + 0.35 * n) * (0.9 + 0.2 * grain);
		float centre = 1.0 - smoothstep(0.15, 0.45, abs(m.x));
		col = mix(col, grass * (0.8 + 0.4 * n), centre * 0.7);
		float rut = exp(-pow((abs(m.x) - hw * 0.5) / 0.25, 2.0));
		col *= 1.0 - 0.18 * rut;
	} else if (out_m <= 0.0) {
		// Bitume : grain, taches, bandes de roulement plus claires par voie.
		float blotch = fbm(m * vec2(0.35, 0.12));
		float lanes = max(1.0, round(abs(v_lanes)));
		bool oneway = v_lanes < 0.0;
		float lane_w = 2.0 * hw / lanes;
		float lane = fract((m.x + hw) / lane_w);
		float tracks = exp(-pow((lane - 0.28) / 0.07, 2.0)) + exp(-pow((lane - 0.72) / 0.07, 2.0));
		col = mix(asphalt, asphalt_worn, clamp(blotch * 0.7 + tracks * 0.25, 0.0, 1.0));
		col *= 0.88 + 0.24 * grain;
		// Rapiéçages sombres ici et là.
		float patch_mask = smoothstep(0.72, 0.75, vnoise(m * vec2(0.25, 0.05) + 3.1));
		col = mix(col, asphalt * 0.75, patch_mask * 0.8);
		rough = 0.88 - tracks * 0.12;

		// Marquages : rives (discontinues au débouché d'une route), axe,
		// séparations de voies, ligne « cédez le passage ».
		float p = 0.0;
		if ((v_marks & EDGE_LEFT) != 0) {
			p += stripe(m, -hw + 0.25, 0.06, 3.0, (v_marks & GAP_LEFT) != 0 ? 6.5 : 0.0);
		}
		if ((v_marks & EDGE_RIGHT) != 0) {
			p += stripe(m, hw - 0.25, 0.06, 3.0, (v_marks & GAP_RIGHT) != 0 ? 6.5 : 0.0);
		}
		if ((v_marks & CENTRE_SOLID) != 0) {
			p += stripe(m, 0.0, 0.06, 1.0, 0.0);
		} else if ((v_marks & CENTRE_DASH) != 0) {
			p += stripe(m, 0.0, 0.06, 3.0, 13.0);
		}
		int n_lanes = int(lanes);
		for (int k = 1; k < 8; k++) {
			if (k >= n_lanes) {
				break;
			}
			float x = -hw + float(k) * lane_w;
			// Sur une route à double sens, l'axe est tracé à part.
			if (!oneway && abs(x) < 0.3) {
				continue;
			}
			p += stripe(m, x, 0.06, 3.0, 13.0);
		}
		if ((v_marks & GIVE_WAY) != 0 && m.y > 0.4 && m.y < 0.9 && abs(m.x) < hw - 0.3) {
			p += step(fract(m.x * 1.0 + 0.25), 0.5);
		}
		float paint_mask = clamp(p, 0.0, 1.0);
		// Peinture usée par endroits.
		paint_mask *= 0.75 + 0.25 * vnoise(m * vec2(2.0, 0.7));
		col = mix(col, paint, paint_mask);
		rough = mix(rough, 0.6, paint_mask);
	} else if (walk > 0.05 && out_m <= 0.15) {
		// Bordure en granit, face verticale éclairée comme telle.
		col = kerb_stone * (0.85 + 0.25 * grain) * (0.9 + 0.2 * vnoise(m * vec2(1.0, 0.5)));
		vec3 nv = normalize(cross(dFdx(VERTEX), dFdy(VERTEX)));
		NORMAL = dot(nv, VERTEX) > 0.0 ? -nv : nv;
		rough = 0.8;
	} else if (walk > 0.05 && out_m <= walk) {
		// Trottoir : enrobé clair, joints, taches.
		float n = fbm(m * vec2(0.6, 0.3) + 11.0);
		col = pavement * (0.82 + 0.3 * n) * (0.92 + 0.16 * grain);
		float joint = step(fract(m.y / 1.5), 0.012) * fine;
		col *= 1.0 - 0.25 * joint;
		rough = 0.92;
	} else {
		// Accotement : gravier tassé près du bitume, puis herbe.
		float v = walk > 0.05 ? out_m - walk : out_m;
		float n = fbm(m * 1.3);
		float g = smoothstep(0.25, 0.9, v + (n - 0.5) * 0.8);
		col = mix(gravel * (0.85 + 0.3 * n), grass * (0.8 + 0.4 * fbm(m * 0.6 + 7.0)), g);
		col *= 0.9 + 0.2 * mix(0.5, hash(floor(m * 25.0)), fine);
	}
	ALBEDO = col;
	ROUGHNESS = rough;
	SPECULAR = 0.35;
}
