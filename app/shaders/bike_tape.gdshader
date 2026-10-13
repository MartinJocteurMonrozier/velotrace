// Guidoline (bike.gd) : bande enroulée en spirale, recouvrements en relief
// et grain fin. Couleur COLOR.rgb (sRGB). UV.x : position le long du cintre
// (m) ; UV.y : tour du tube (0 à 1).
shader_type spatial;

#include "res://shaders/bike.gdshaderinc"

uniform float pitch = 0.026;

void fragment() {
	float phase = fract(UV.x / pitch + UV.y);
	// Chaque tour recouvre le précédent : marche nette, puis pente douce.
	float lap = smoothstep(0.0, 0.06, phase) * (1.0 - phase * 0.4);
	vec3 g = vec3(UV.x * 1800.0, UV.y * 60.0, 0.0);
	float grain = bike_hash13(floor(g)) * bike_fade(g, 1.0);
	float h = lap * 0.0004 * bike_fade2(vec2(UV.x / pitch, UV.y), 1.0) + grain * 0.00002;
	ALBEDO = bike_srgb(COLOR.rgb) * (0.9 + 0.1 * lap);
	NORMAL = bike_bump(VERTEX, NORMAL, h);
	ROUGHNESS = 0.7;
	SPECULAR = 0.35;
}
