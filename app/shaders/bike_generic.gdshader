// Matériaux simples du vélo (bike.gd) : couleur de sommet COLOR.rgb (sRGB) et
// rugosité COLOR.a ; le magenta pur prend la couleur de la tenue (bidons).
// Entre un sommet magenta et un sommet gris, la part de magenta interpolée
// sert de poids : pas de liseré violet à la frontière.
shader_type spatial;

#include "res://shaders/bike.gdshaderinc"

uniform float metallic = 0.0;
uniform float clearcoat = 0.0;
uniform vec3 kit_color : source_color = vec3(1.0);

void fragment() {
	vec3 c = COLOR.rgb;
	float t = clamp(min(c.r, c.b) - c.g, 0.0, 1.0);
	t = t > 0.05 ? t : 0.0;
	vec3 base = clamp((c - t * vec3(1.0, 0.0, 1.0)) / max(1.0 - t, 1e-3), 0.0, 1.0);
	ALBEDO = mix(bike_srgb(base), kit_color, t);
	ROUGHNESS = COLOR.a;
	METALLIC = metallic;
	SPECULAR = 0.5;
	CLEARCOAT = clearcoat;
	CLEARCOAT_ROUGHNESS = 0.1;
}
