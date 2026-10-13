// Chaîne (bike.gd) : rubans le long du trajet de la chaîne, découpés en
// maillons (plaques extérieures et intérieures en huit, rouleaux) et animés
// par `chain_offset` (distance parcourue, m). UV.x : position le long de la
// chaîne (m) ; UV.y : hauteur sur le ruban (m) ; COLOR.r : 0 plaque
// extérieure, 0,5 plaque intérieure, 1 rouleaux.
shader_type spatial;
render_mode cull_disabled;

#include "res://shaders/bike.gdshaderinc"

const float PITCH = 0.0127;

instance uniform float chain_offset = 0.0;
// Longueur du trajet ramenée à un nombre pair de maillons.
uniform float scale = 1.0;

// Demi-hauteur d'une plaque à la distance `t` de son centre (axes à ±PITCH/2).
float plate(float t, float r, float waist) {
	float d = abs(abs(t) - PITCH * 0.5);
	float round_part = sqrt(max(r * r - d * d, 0.0));
	return abs(t) < PITCH * 0.5 ? max(round_part, waist) : round_part;
}

void fragment() {
	float s = UV.x * scale + chain_offset;
	float y = UV.y;
	float kind = COLOR.r;
	vec3 col;
	float metal = 0.9;
	float rough = 0.32;
	if (kind < 0.25) {
		// Plaques extérieures : maillons centrés sur (2m + 0,5) pas.
		float t = s - PITCH * (2.0 * round((s / PITCH - 0.5) * 0.5) + 0.5);
		if (abs(y) > plate(t, 0.0044, 0.0037)) {
			discard;
		}
		float pin = abs(abs(t) - PITCH * 0.5);
		col = pin < 0.0018 ? vec3(0.5) : vec3(0.3, 0.3, 0.32);
		rough = pin < 0.0018 ? 0.2 : 0.3;
	} else if (kind < 0.75) {
		float t = s - PITCH * (2.0 * round((s / PITCH - 1.5) * 0.5) + 1.5);
		if (abs(y) > plate(t, 0.0041, 0.0031)) {
			discard;
		}
		col = vec3(0.16, 0.16, 0.17);
	} else {
		// Rouleaux : seulement autour des axes.
		float d = abs(fract(s / PITCH + 0.5) - 0.5) * PITCH;
		if (d > 0.0032) {
			discard;
		}
		col = vec3(0.3, 0.3, 0.31);
		rough = 0.45;
	}
	ALBEDO = col;
	METALLIC = metal;
	ROUGHNESS = rough;
}
