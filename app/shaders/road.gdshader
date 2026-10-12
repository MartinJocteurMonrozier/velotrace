// Chaussée : asphalte, lignes de rive et ligne centrale discontinue.
// UV.x : position en travers (0 à 1), UV.y : distance le long de la route (m).
shader_type spatial;

uniform vec3 asphalt : source_color = vec3(0.16, 0.16, 0.17);
uniform vec3 paint : source_color = vec3(0.92, 0.92, 0.88);

float hash(vec2 p) {
	return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

void fragment() {
	vec2 grain = floor(vec2(UV.x * 60.0, UV.y * 10.0));
	vec3 col = asphalt * (0.9 + 0.2 * hash(grain));
	float edge = step(abs(UV.x - 0.04), 0.012) + step(abs(UV.x - 0.96), 0.012);
	float centre = step(abs(UV.x - 0.5), 0.01) * step(fract(UV.y / 9.0), 0.5);
	col = mix(col, paint, clamp(edge + centre, 0.0, 1.0));
	ALBEDO = col;
	ROUGHNESS = 0.85;
}
