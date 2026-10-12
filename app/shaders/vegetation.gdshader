// Arbres et buissons : couleurs de sommet (sRGB) teintées par instance,
// léger balancement au vent, plus sombre au cœur du feuillage.
shader_type spatial;
render_mode cull_back;

uniform float sway = 0.035;

void vertex() {
	vec3 origin = MODEL_MATRIX[3].xyz;
	float phase = origin.x * 0.21 + origin.z * 0.17;
	float h = max(VERTEX.y, 0.0);
	float wind = sin(TIME * 1.3 + phase) * 0.6 + sin(TIME * 2.9 + phase * 1.7) * 0.4;
	VERTEX.x += wind * sway * h * h;
	VERTEX.z += wind * sway * 0.5 * h * h;
}

void fragment() {
	vec3 c = COLOR.rgb;
	// Couleurs fournies en sRGB.
	ALBEDO = pow(c, vec3(2.2));
	ROUGHNESS = 0.95;
	SPECULAR = 0.2;
	BACKLIGHT = vec3(0.25, 0.35, 0.12);
}
