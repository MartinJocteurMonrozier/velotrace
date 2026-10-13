// Outils communs aux matériaux du vélo (bike.gd) : bruit et relief procédural.

float bike_hash13(vec3 p) {
	p = fract(p * 0.1031);
	p += dot(p, p.zyx + 31.32);
	return fract((p.x + p.y) * p.z);
}

// Relief d'une hauteur `h` (m) calculé à l'écran, sans tangentes : normale
// perturbée en repère de vue (`pos` : VERTEX, `n` : NORMAL).
vec3 bike_bump(vec3 pos, vec3 n, float h) {
	vec3 dpdx = dFdx(pos);
	vec3 dpdy = dFdy(pos);
	float dhdx = dFdx(h);
	float dhdy = dFdy(h);
	vec3 r1 = cross(dpdy, n);
	vec3 r2 = cross(n, dpdx);
	float det = dot(dpdx, r1);
	if (abs(det) < 1e-14) {
		return n;
	}
	vec3 grad = sign(det) * (dhdx * r1 + dhdy * r2);
	return normalize(abs(det) * n - grad);
}

// Atténuation d'un motif de période `period` (dans les unités de `coord`)
// quand il devient plus fin qu'un pixel, pour éviter le moiré.
float bike_fade(vec3 coord, float period) {
	float px = length(fwidth(coord)) / period;
	return clamp(2.0 - 2.5 * px, 0.0, 1.0);
}

float bike_fade2(vec2 coord, float period) {
	float px = length(fwidth(coord)) / period;
	return clamp(2.0 - 2.5 * px, 0.0, 1.0);
}

vec3 bike_srgb(vec3 c) {
	return pow(c, vec3(2.2));
}
