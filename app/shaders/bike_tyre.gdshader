// Pneus (bike.gd) : bande de roulement noire à sculptures fines (« file
// tread ») sur les épaules, centre lisse, flancs de couleur COLOR.rgb (sRGB)
// avec la trame de la carcasse et une bande sans inscription.
// UV.x : position sur la circonférence (m) ; UV.y : profil, 0 et 1 aux
// talons, 0,5 au centre de la bande de roulement.
shader_type spatial;

#include "res://shaders/bike.gdshaderinc"

void fragment() {
	float y = UV.y;
	float c = abs(y - 0.5);
	vec3 col = bike_srgb(COLOR.rgb);
	float h = 0.0;
	float rough = 0.78;
	// Sculptures en losanges sur les épaules de la bande de roulement.
	vec2 q = vec2(UV.x * 420.0, y * 60.0);
	float knurl = abs(fract(q.x + q.y) - 0.5) + abs(fract(q.x - q.y) - 0.5);
	float shoulder = smoothstep(0.035, 0.05, c) * (1.0 - smoothstep(0.17, 0.19, c));
	h += knurl * 0.00012 * shoulder * bike_fade2(q, 1.0);
	// Flancs : trame de la carcasse et bande lisse un peu plus sombre.
	float side = smoothstep(0.2, 0.23, c);
	float threads = sin(UV.x * 2400.0 + y * 40.0);
	h += threads * 0.00003 * side * bike_fade2(vec2(UV.x * 2400.0 / 6.283, y), 1.0);
	float band = smoothstep(0.255, 0.265, c) * (1.0 - smoothstep(0.30, 0.31, c));
	col *= 1.0 - band * 0.18;
	rough = mix(rough, 0.62, side);
	rough = mix(rough, 0.45, band);
	ALBEDO = col;
	NORMAL = bike_bump(VERTEX, NORMAL, h);
	ROUGHNESS = rough;
	SPECULAR = 0.35;
}
