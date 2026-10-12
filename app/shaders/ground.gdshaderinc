// Sol détaillé au bord des routes, partagé par le relief (terrain.gdshader)
// et les accotements (road.gdshader) pour qu'ils se raccordent sans couture :
// herbe, revêtements de ville, cailloux et terre, dessinés en coordonnées du
// monde (m). Hachage entier : reste précis loin de l'origine. Les teintes
// sont écrites en sRGB, comme les couleurs choisies dans l'éditeur, et
// converties en linéaire par ground_sample.

struct GroundSample {
	vec3 col;
	// Relief fin (m), pour g_bump.
	float h;
	float rough;
};

uint g_hash_u(ivec2 c, uint k) {
	uint h = uint(c.x) * 0x8da6b343u ^ uint(c.y) * 0xd8163841u ^ k * 0xcb1ab31fu;
	h ^= h >> 16u;
	h *= 0x7feb352du;
	h ^= h >> 15u;
	h *= 0x846ca68bu;
	h ^= h >> 16u;
	return h;
}

float g_hash(ivec2 c, uint k) {
	return float(g_hash_u(c, k) >> 8u) / 16777216.0;
}

float g_noise(vec2 p, uint k) {
	vec2 fl = floor(p);
	ivec2 i = ivec2(fl);
	vec2 f = p - fl;
	f = f * f * (3.0 - 2.0 * f);
	float a = g_hash(i, k);
	float b = g_hash(i + ivec2(1, 0), k);
	float c = g_hash(i + ivec2(0, 1), k);
	float d = g_hash(i + ivec2(1, 1), k);
	return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

float g_fbm(vec2 p, uint k) {
	float s = 0.0;
	float a = 0.5;
	for (int o = 0; o < 4; o++) {
		s += g_noise(p, k + uint(o) * 17u) * a;
		p = mat2(vec2(1.6, 1.2), vec2(-1.2, 1.6)) * p;
		a *= 0.5;
	}
	return s / 0.9375;
}

// Cellules irrégulières (galets, pierres) : distance au centre le plus
// proche, à la frontière avec la voisine, et identifiant de la cellule.
vec3 g_cells(vec2 p, uint k) {
	vec2 fl = floor(p);
	ivec2 i = ivec2(fl);
	vec2 f = p - fl;
	float d1 = 8.0;
	float d2 = 8.0;
	float id = 0.0;
	vec2 best = vec2(0.0);
	for (int y = -1; y <= 1; y++) {
		for (int x = -1; x <= 1; x++) {
			ivec2 c = i + ivec2(x, y);
			vec2 r = vec2(float(x), float(y)) + vec2(g_hash(c, k), g_hash(c, k + 7u)) * 0.8 + 0.1 - f;
			float d = dot(r, r);
			if (d < d1) {
				d2 = d1;
				d1 = d;
				id = g_hash(c, k + 13u);
				best = r;
			} else if (d < d2) {
				d2 = d;
			}
		}
	}
	return vec3(sqrt(d1), sqrt(d2) - sqrt(d1), id);
}

// Prairie : plaques plus ou moins grasses, touffes, brins orientés,
// quelques fleurs.
GroundSample g_grass(vec2 p, float fine) {
	float patches = g_fbm(p * 0.25, 1u);
	float clumps = g_noise(p * 1.7, 2u);
	vec3 col = mix(vec3(0.21, 0.32, 0.10), vec3(0.35, 0.41, 0.17), smoothstep(0.3, 0.75, patches));
	col = mix(col, vec3(0.48, 0.45, 0.26), smoothstep(0.6, 0.85, g_fbm(p * 0.09 + 31.0, 3u)) * 0.6);
	col *= 0.8 + 0.32 * clumps;
	float a = g_noise(p * 0.8, 4u) * 6.283;
	vec2 dir = vec2(cos(a), sin(a));
	vec2 q = vec2(dot(p, dir), dot(p, vec2(-dir.y, dir.x)));
	float blades = g_noise(q * vec2(30.0, 4.0), 5u) * 0.6 + g_noise(p * 21.0, 6u) * 0.4;
	col *= mix(1.0, 0.7 + 0.6 * blades, fine);
	ivec2 fc = ivec2(floor(p * 3.0));
	float fh = g_hash(fc, 8u);
	if (fh > 0.982 && fine > 0.2) {
		vec2 fp = fract(p * 3.0) - 0.5 - (vec2(g_hash(fc, 9u), g_hash(fc, 10u)) - 0.5) * 0.6;
		float petal = 1.0 - smoothstep(0.04, 0.08, length(fp));
		vec3 flower = fh > 0.994 ? vec3(0.95, 0.83, 0.22) : (fh > 0.988 ? vec3(0.93, 0.93, 0.95) : vec3(0.66, 0.45, 0.78));
		col = mix(col, flower, petal * fine);
	}
	return GroundSample(col, (blades * 0.6 + clumps * 0.4) * 0.03, 0.95);
}

// Sous-bois : terre, humus, aiguilles et brindilles, quelques cailloux.
GroundSample g_dirt(vec2 p, float fine) {
	vec3 col = mix(vec3(0.33, 0.25, 0.16), vec3(0.20, 0.15, 0.10), g_fbm(p * 0.6, 11u));
	float a = g_noise(p * 2.0, 12u) * 6.283;
	vec2 dir = vec2(cos(a), sin(a));
	vec2 q = vec2(dot(p, dir), dot(p, vec2(-dir.y, dir.x)));
	float needles = g_noise(q * vec2(45.0, 3.0), 13u);
	col = mix(col, vec3(0.45, 0.32, 0.18), smoothstep(0.62, 0.8, needles) * fine * 0.7);
	vec3 c = g_cells(p * 4.0, 14u);
	float pebble = (1.0 - smoothstep(0.18, 0.28, c.x)) * step(0.75, c.z);
	col = mix(col, vec3(0.50, 0.48, 0.44) * (0.8 + 0.3 * c.z), pebble * fine);
	return GroundSample(col * (0.85 + 0.25 * g_noise(p * 9.0, 15u)), needles * 0.01 + pebble * 0.03, 0.97);
}

// Teinte d'une pierre d'après l'identifiant de sa cellule : calcaire gris
// clair, schiste sombre, grès roux.
vec3 g_stone_tint(float id) {
	vec3 t = mix(vec3(0.50, 0.48, 0.45), vec3(0.62, 0.59, 0.54), fract(id * 3.7));
	t = mix(t, vec3(0.38, 0.38, 0.39), step(0.7, fract(id * 7.3)));
	return mix(t, vec3(0.56, 0.47, 0.37), step(0.86, fract(id * 13.1)));
}

// Cailloux : gravillons sur un fond de terre, galets, quelques grosses
// pierres par endroits.
GroundSample g_stones(vec2 p, float fine) {
	vec3 c1 = g_cells(p * 14.0, 21u);
	vec3 c2 = g_cells(p * 4.5, 22u);
	vec3 c3 = g_cells(p * 1.2, 25u);
	vec3 soil = vec3(0.40, 0.36, 0.30);
	float grit = smoothstep(0.0, 0.12, c1.y);
	vec3 col = mix(soil, g_stone_tint(c1.z), grit * 0.85) * (0.8 + 0.3 * (1.0 - c1.x));
	float peb = (1.0 - smoothstep(0.3, 0.4, c2.x)) * step(c2.z, 0.7);
	col = mix(col, g_stone_tint(c2.z) * (0.72 + 0.45 * (1.0 - c2.x * 2.2)), peb);
	float big = (1.0 - smoothstep(0.34, 0.42, c3.x)) * step(0.72, c3.z) * smoothstep(0.45, 0.65, g_noise(p * 0.4, 23u));
	col = mix(col, g_stone_tint(c3.z * 5.0) * (0.7 + 0.5 * (1.0 - c3.x * 2.0)), big);
	col *= mix(1.0, 0.88 + 0.24 * g_noise(p * 60.0, 24u), fine);
	// Au loin, une teinte moyenne plutôt qu'un scintillement.
	col = mix(vec3(0.48, 0.46, 0.42), col, fine);
	float h = grit * 0.006 + peb * (0.4 - c2.x) * 0.12 + big * (0.42 - c3.x) * 0.35;
	return GroundSample(col, h, 0.85);
}

// Ville : enrobé, dalles de béton ou pavés selon le quartier, joints
// envahis d'herbe, taches.
GroundSample g_paved(vec2 p, float fine) {
	float zone = g_noise(p * 0.035, 31u);
	float stain = g_fbm(p * 0.3, 32u);
	float grain = mix(0.5, g_noise(p * 35.0, 33u), fine);
	vec3 col;
	float h = 0.0;
	float joint = 0.0;
	if (zone < 0.45) {
		// Enrobé usé, rapiéçages.
		col = mix(vec3(0.22, 0.22, 0.23), vec3(0.32, 0.32, 0.32), stain);
		col *= 0.86 + 0.28 * grain;
		col = mix(col, vec3(0.16, 0.16, 0.17), smoothstep(0.7, 0.73, g_noise(p * 0.2 + 5.0, 34u)) * 0.8);
		h = grain * 0.004;
	} else if (zone < 0.78) {
		// Dalles de béton de 1,5 m.
		vec2 cell = floor(p / 1.5);
		vec2 f = fract(p / 1.5);
		float tint = g_hash(ivec2(cell), 35u);
		col = mix(vec3(0.52, 0.51, 0.48), vec3(0.62, 0.60, 0.56), tint) * (0.85 + 0.25 * stain);
		col *= 0.9 + 0.2 * grain;
		vec2 e = min(f, 1.0 - f) * 1.5;
		joint = 1.0 - smoothstep(0.008, 0.02, min(e.x, e.y));
		h = -joint * 0.01 + grain * 0.003;
	} else {
		// Pavés de 20 × 12 cm posés en quinconce.
		vec2 q = p / vec2(0.2, 0.12);
		q.x += step(1.0, mod(floor(q.y), 2.0)) * 0.5;
		vec2 cell = floor(q);
		vec2 f = fract(q);
		float tint = g_hash(ivec2(cell), 36u);
		col = mix(vec3(0.42, 0.40, 0.38), vec3(0.58, 0.52, 0.46), tint) * (0.85 + 0.2 * stain);
		vec2 e = min(f, 1.0 - f) * vec2(0.2, 0.12);
		float d = min(e.x, e.y);
		joint = 1.0 - smoothstep(0.004, 0.012, d);
		h = smoothstep(0.0, 0.03, d) * 0.012;
	}
	joint *= fine;
	vec3 weed = vec3(0.22, 0.30, 0.10) * (0.8 + 0.4 * grain);
	float weedy = smoothstep(0.55, 0.75, g_noise(p * 0.7, 37u));
	col = mix(col, mix(col * 0.55, weed, weedy), joint);
	return GroundSample(col, h, 0.9);
}

// Mélange des natures du sol `w` (herbe, revêtement, cailloux ; le reste en
// terre) par plaques irrégulières plutôt qu'en fondu. `fine` : 1 quand les
// détails fins sont visibles, 0 au loin.
GroundSample ground_sample(vec2 p, vec3 w, float fine) {
	vec4 k = vec4(w, max(0.0, 1.0 - w.x - w.y - w.z));
	vec4 n = vec4(g_fbm(p * 0.3, 41u), g_fbm(p * 0.15, 42u), g_fbm(p * 0.35, 43u), g_fbm(p * 0.3, 44u));
	k *= 0.3 + 1.4 * n;
	k = k * k * k;
	k /= max(k.x + k.y + k.z + k.w, 1e-5);
	GroundSample s = GroundSample(vec3(0.0), 0.0, 0.0);
	if (k.x > 0.01) {
		GroundSample g = g_grass(p, fine);
		s.col += g.col * k.x;
		s.h += g.h * k.x;
		s.rough += g.rough * k.x;
	}
	if (k.y > 0.01) {
		GroundSample g = g_paved(p, fine);
		s.col += g.col * k.y;
		s.h += g.h * k.y;
		s.rough += g.rough * k.y;
	}
	if (k.z > 0.01) {
		GroundSample g = g_stones(p, fine);
		s.col += g.col * k.z;
		s.h += g.h * k.z;
		s.rough += g.rough * k.z;
	}
	if (k.w > 0.01) {
		GroundSample g = g_dirt(p, fine);
		s.col += g.col * k.w;
		s.h += g.h * k.w;
		s.rough += g.rough * k.w;
	}
	float total = (k.x > 0.01 ? k.x : 0.0) + (k.y > 0.01 ? k.y : 0.0) + (k.z > 0.01 ? k.z : 0.0) + (k.w > 0.01 ? k.w : 0.0);
	s.col = pow(s.col / total, vec3(2.2));
	s.h /= total;
	s.rough /= total;
	return s;
}

// Normale perturbée par un relief fin `h` (m) sans tangentes : gradient de
// surface d'après les dérivées à l'écran (Mikkelsen). N et V en repère vue.
vec3 g_bump(vec3 n, vec3 v, float h) {
	vec3 dpdx = dFdx(v);
	vec3 dpdy = dFdy(v);
	vec3 r1 = cross(dpdy, n);
	vec3 r2 = cross(n, dpdx);
	float det = dot(dpdx, r1);
	vec3 grad = sign(det) * (dFdx(h) * r1 + dFdy(h) * r2);
	return normalize(abs(det) * n - grad);
}
