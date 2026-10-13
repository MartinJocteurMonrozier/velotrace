// Peinture du cadre, de la fourche et de la tige de selle (bike.gd) : couleur
// du cadre de la tenue, dégradé vers le noir sur le triangle arrière et les
// fourreaux, liseré de la couleur d'accent, paillettes métalliques sous un
// vernis brillant. Repère de l'objet : x à droite, y en haut, z vers l'arrière.
// COLOR.a = 1 : inscription (couleur `decal_color`).
shader_type spatial;

#include "res://shaders/bike.gdshaderinc"

uniform vec3 frame_color : source_color = vec3(0.72, 0.04, 0.06);
uniform vec3 fade_color : source_color = vec3(0.025, 0.025, 0.03);
uniform vec3 decal_color : source_color = vec3(0.95, 0.95, 0.95);
uniform vec3 accent_color : source_color = vec3(0.95, 0.95, 0.95);
// Début et fin du dégradé vers l'arrière (z, m), et vers le bas des fourreaux (y, m).
uniform vec2 rear_fade = vec2(0.17, 0.32);
uniform vec2 fork_fade = vec2(0.42, 0.22);
uniform float fork_z = -0.3;

varying vec3 obj;

void vertex() {
	obj = VERTEX;
}

void fragment() {
	// Dégradé en biais : il descend vers l'arrière comme un coup de pinceau.
	float rz = obj.z + (obj.y - 0.5) * 0.35;
	float rear = smoothstep(rear_fade.x, rear_fade.y, rz);
	float fork = smoothstep(fork_fade.x, fork_fade.y, obj.y) * smoothstep(fork_z + 0.05, fork_z - 0.05, obj.z);
	float fade = max(rear, fork);
	vec3 base = mix(frame_color, fade_color, fade);
	// Liseré fin à l'amorce du dégradé arrière.
	float stripe = 1.0 - smoothstep(0.0025, 0.0045, abs(rz - rear_fade.x + 0.012));
	base = mix(base, accent_color, stripe * 0.9);
	float decal = step(0.5, COLOR.a);
	ALBEDO = mix(base, decal_color, decal);

	// Paillettes : petites facettes qui renvoient la lumière une à une.
	vec3 cell = floor(obj * 2600.0);
	float h = bike_hash13(cell);
	float sparkle = step(0.86, h) * (1.0 - decal) * bike_fade(obj, 1.0 / 2600.0);
	vec3 rnd = vec3(bike_hash13(cell + 17.0), bike_hash13(cell + 41.0), bike_hash13(cell + 73.0)) - 0.5;
	vec3 tilt = (VIEW_MATRIX * MODEL_MATRIX * vec4(rnd, 0.0)).xyz;
	NORMAL = normalize(NORMAL + tilt * 0.9 * sparkle);
	METALLIC = mix(0.2, 0.0, decal);
	ROUGHNESS = mix(0.42, 0.3, decal) - sparkle * 0.25;
	SPECULAR = 0.5;
	CLEARCOAT = 1.0;
	CLEARCOAT_ROUGHNESS = 0.06;
}
