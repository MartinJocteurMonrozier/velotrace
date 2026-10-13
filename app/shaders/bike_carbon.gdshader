// Carbone nu (cintre, leviers, jantes, cages) : tissage sergé 3K discret en
// projection triplanaire dans le repère de l'objet, sous un vernis brillant
// ou mat. COLOR.a = 1 : inscription de couleur COLOR.rgb (sRGB), sans tissage.
shader_type spatial;

#include "res://shaders/bike.gdshaderinc"

uniform vec3 tint : source_color = vec3(0.045, 0.045, 0.05);
uniform float roughness = 0.32;
uniform float clearcoat = 1.0;
// Mèches par mètre (une mèche de 3 mm environ).
uniform float tows = 330.0;
uniform float weave_strength = 1.0;

varying vec3 obj;
varying vec3 obj_n;

void vertex() {
	obj = VERTEX;
	obj_n = NORMAL;
}

// Sergé 2/2 : hauteur et clarté d'une mèche à la position `uv` (en mèches).
vec2 twill(vec2 uv) {
	vec2 i = floor(uv);
	vec2 f = fract(uv);
	bool warp = mod(i.x - i.y, 4.0) < 2.0;
	float across = warp ? f.x : f.y;
	float h = sin(3.14159 * across);
	return vec2(h, warp ? 1.0 : 0.72);
}

void fragment() {
	float decal = step(0.5, COLOR.a);
	vec3 w = abs(normalize(obj_n));
	w = pow(w, vec3(4.0));
	w /= (w.x + w.y + w.z);
	vec3 p = obj * tows;
	vec2 a = twill(p.yz);
	vec2 b = twill(p.xz);
	vec2 c = twill(p.xy);
	vec2 t = a * w.x + b * w.y + c * w.z;
	float fade = bike_fade(p, 1.0) * weave_strength * (1.0 - decal);
	ALBEDO = mix(tint * mix(1.0, t.y * 1.25, fade), bike_srgb(COLOR.rgb), decal);
	NORMAL = bike_bump(VERTEX, NORMAL, t.x * 0.00012 * fade);
	METALLIC = 0.0;
	ROUGHNESS = mix(roughness, 0.3, decal);
	SPECULAR = 0.5;
	CLEARCOAT = clearcoat;
	CLEARCOAT_ROUGHNESS = 0.05;
}
