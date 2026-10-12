// Bâtiments en couleurs de sommet (sRGB). Vus de loin, ils disparaissent là
// où le carré détaillé (avec ses propres bâtiments) est chargé.
shader_type spatial;

uniform bool use_mask = false;
uniform sampler2D near_mask : filter_nearest, repeat_disable;
uniform vec4 mask_rect = vec4(0.0, 0.0, 1.0, 1.0);

varying vec2 world_xz;
varying vec3 world_pos;
varying vec3 world_normal;

void vertex() {
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	world_xz = world_pos.xz;
	world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
}

void fragment() {
	if (use_mask) {
		vec2 m = (world_xz - mask_rect.xy) / mask_rect.zw;
		if (m.x >= 0.0 && m.y >= 0.0 && m.x < 1.0 && m.y < 1.0 && texture(near_mask, m).r > 0.5) {
			discard;
		}
	}
	vec3 c = pow(COLOR.rgb, vec3(2.2));
	float rough = 0.85;
	// Façades : rangées de fenêtres tous les 3 m, étages de 3 m.
	if (abs(world_normal.y) < 0.3) {
		float along = abs(world_normal.x) > abs(world_normal.z) ? world_pos.z : world_pos.x;
		vec2 cell = fract(vec2(along / 3.0, world_pos.y / 3.0));
		float px = length(fwidth(vec2(along, world_pos.y))) ;
		float win = step(0.3, cell.x) * step(cell.x, 0.7) * step(0.35, cell.y) * step(cell.y, 0.8);
		// Au loin, les fenêtres se fondent en un léger assombrissement.
		float far = smoothstep(0.15, 0.6, px);
		vec3 glass = vec3(0.05, 0.07, 0.09);
		c = mix(c, mix(glass, c * 0.82, far), win * (1.0 - far) + far * 0.0);
		c *= mix(1.0, 0.9, far);
		rough = mix(rough, 0.25, win * (1.0 - far));
	}
	ALBEDO = c;
	ROUGHNESS = rough;
	SPECULAR = 0.3;
}
