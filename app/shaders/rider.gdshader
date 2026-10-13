// Cycliste : peau, tenue et accessoires dans un seul matériau. Sur le corps,
// la tenue suit des champs calculés dans la pose de repos (CUSTOM0, positifs à
// l'intérieur du vêtement) et des coordonnées de décor (CUSTOM1) ; les limites
// restent nettes quelle que soit la finesse du maillage. Les accessoires
// portent un identifiant de matière (CUSTOM2.r) ; le corps porte 0.
shader_type spatial;
render_mode cull_back;

uniform vec3 jersey : source_color = vec3(0.06, 0.26, 0.70);
uniform vec3 jersey_dark : source_color = vec3(0.03, 0.12, 0.38);
uniform vec3 accent : source_color = vec3(0.86, 0.07, 0.09);

const vec3 SKIN = vec3(0.52, 0.28, 0.17);
const vec3 HAIR = vec3(0.045, 0.03, 0.02);
const vec3 BIB = vec3(0.018, 0.018, 0.02);
const vec3 BIB_BAND = vec3(0.07, 0.07, 0.075);
const vec3 SOCK = vec3(0.86, 0.86, 0.86);
const vec3 GLOVE = vec3(0.025, 0.025, 0.028);
const vec3 WHITE = vec3(0.85, 0.85, 0.86);
const vec3 BLACK = vec3(0.02, 0.02, 0.022);

varying vec4 f0;
varying vec4 f1;
varying flat int mat_id;
varying float arm;

void vertex() {
	f0 = CUSTOM0;
	f1 = CUSTOM1;
	mat_id = int(CUSTOM2.r * 255.0 + 0.5);
	arm = CUSTOM2.g;
}

// Masque net mais anticrénelé de la zone f > 0.
float inside(float f) {
	return clamp(f / max(fwidth(f), 1e-5) + 0.5, 0.0, 1.0);
}

// Aération du casque : capsule le long d'un méridien (angle phc, de v0 à v1,
// demi-largeur hw en radians), distance signée.
float slot(float ph, float v, float k, float phc, float v0, float v1, float hw) {
	float dp = abs(mod(ph - phc + PI, TAU) - PI) * k;
	float dv = (v - clamp(v, v0, v1)) * 1.9;
	float w = hw * (0.6 + 0.4 * smoothstep(v0, v0 + 0.15, v) * smoothstep(v1, v1 - 0.15, v));
	return length(vec2(dp, dv)) - w;
}

void fragment() {
	vec3 c = SKIN;
	float rough = 0.55;
	float spec = 0.4;
	float metal = 0.0;
	float rim = 0.0;
	float sss = 0.0;
	float ao = 1.0;
	if (mat_id == 0) {
		// Peau, cheveux et lèvres.
		float hair = smoothstep(0.4, 0.6, f1.w);
		c = mix(SKIN, HAIR, hair);
		c = mix(c, SKIN * vec3(0.95, 0.72, 0.68), smoothstep(0.3, 0.6, -f1.w));
		rough = mix(0.5, 0.8, hair);
		sss = 0.5 * (1.0 - hair);
		// Chaussettes, bande du haut au liseré.
		float sock = inside(f0.z);
		vec3 sock_c = mix(SOCK, accent, inside(0.018 - f0.z) * inside(f0.z - 0.006));
		c = mix(c, sock_c, sock);
		rough = mix(rough, 0.9, sock);
		spec = mix(spec, 0.3, sock);
		sss *= 1.0 - sock;
		// Cuissard noir, bande élastique en bas.
		float bib = inside(f0.y);
		vec3 bib_c = mix(BIB, BIB_BAND, inside(0.03 - f0.y));
		c = mix(c, bib_c, bib);
		rough = mix(rough, 0.42, bib);
		rim = mix(rim, 0.35, bib);
		sss *= 1.0 - bib;
		// Maillot : flancs foncés, bande de poitrine, manches à liseré, col.
		float jer = inside(f0.x);
		vec3 jc = jersey;
		float side_panel = inside(abs(f1.x) - 0.84) * inside(1.0 - f1.z) * (1.0 - arm);
		jc = mix(jc, jersey_dark, side_panel);
		float band = inside(f1.z - 0.56) * inside(0.68 - f1.z) * (1.0 - arm);
		jc = mix(jc, accent, band);
		float cuff = arm * inside(0.03 - f0.x);
		jc = mix(jc, jersey_dark, cuff);
		jc = mix(jc, accent, arm * inside(0.03 - f0.x) * inside(f0.x - 0.016));
		jc = mix(jc, jersey_dark, inside(f1.z - 1.0) * inside(0.025 - f0.x) * (1.0 - arm));
		c = mix(c, jc, jer);
		rough = mix(rough, 0.62, jer);
		spec = mix(spec, 0.35, jer);
		rim = mix(rim, 0.2, jer);
		sss *= 1.0 - jer;
		// Mitaines.
		float glove = inside(f0.w);
		c = mix(c, GLOVE, glove);
		rough = mix(rough, 0.65, glove);
		sss *= 1.0 - glove;
	} else if (mat_id == 1) {
		// Casque : coque blanche, panneaux et bande aux couleurs de la tenue,
		// aérations noires.
		float ph = f1.x;
		float th = max(f1.y, 0.02);
		float v = f1.z;
		float k = sin(th);
		float vent = 1.0;
		// Bouches avant, canaux du dessus, sorties arrière.
		vent = min(vent, slot(ph, v, k, 0.0, 0.5, 0.82, 0.085));
		vent = min(vent, slot(ph, v, k, 0.4, 0.48, 0.82, 0.08));
		vent = min(vent, slot(ph, v, k, -0.4, 0.48, 0.82, 0.08));
		vent = min(vent, slot(ph, v, k, 0.82, 0.5, 0.8, 0.07));
		vent = min(vent, slot(ph, v, k, -0.82, 0.5, 0.8, 0.07));
		vent = min(vent, slot(ph, v, k, 0.2, 0.1, 0.4, 0.06));
		vent = min(vent, slot(ph, v, k, -0.2, 0.1, 0.4, 0.06));
		vent = min(vent, slot(ph, v, k, 0.75, 0.16, 0.42, 0.06));
		vent = min(vent, slot(ph, v, k, -0.75, 0.16, 0.42, 0.06));
		vent = min(vent, slot(ph, v, k, 1.5, 0.3, 0.62, 0.06));
		vent = min(vent, slot(ph, v, k, -1.5, 0.3, 0.62, 0.06));
		vent = min(vent, slot(ph, v, k, 2.3, 0.4, 0.8, 0.07));
		vent = min(vent, slot(ph, v, k, -2.3, 0.4, 0.8, 0.07));
		vent = min(vent, slot(ph, v, k, 2.85, 0.42, 0.84, 0.07));
		vent = min(vent, slot(ph, v, k, -2.85, 0.42, 0.84, 0.07));
		// Couleur de l'équipe sur le bas et l'arrière, liseré, bord noir.
		float aph = abs(ph) / PI;
		float sweep = v - (0.78 - 0.5 * aph * aph);
		c = mix(WHITE, jersey, inside(sweep));
		c = mix(c, accent, inside(sweep + 0.035) * inside(-sweep));
		rough = 0.18;
		spec = 0.6;
		// Aérations percées : on voit la mousse au fond ; de biais, la paroi
		// de l'aération (sombre) cache le fond.
		if (vent < 0.0) {
			if (dot(NORMAL, VIEW) > 0.4) {
				discard;
			}
			c = vec3(0.02);
			rough = 1.0;
			spec = 0.0;
			ao = 0.2;
		}
	} else if (mat_id == 4) {
		// Mousse et bord du casque, à l'ombre de la coque.
		c = vec3(0.025);
		rough = 1.0;
		spec = 0.1;
		ao = 0.2;
	} else if (mat_id == 5) {
		// Chaussure : tige blanche, talon et molette noirs, bande, liseré, semelle.
		float u = f1.x;
		float h = f1.y;
		float lat = f1.z;
		c = WHITE;
		c = mix(c, BLACK, inside(0.13 - u) * inside(0.85 - h));
		c = mix(c, jersey_dark, inside(u - 0.5) * inside(0.6 - u) * inside(h - 0.5));
		float d = u - 0.2;
		float stripe = inside(d) * inside(0.58 - d) * inside(h - 0.3 - 0.3 * d) * inside(0.42 + 0.3 * d - h) * inside(lat - 0.3);
		c = mix(c, jersey, stripe);
		float dial = inside(0.4 - length(vec2((u - 0.4) * 9.0, (h - 0.82) * 2.4))) * inside(lat - 0.1);
		c = mix(c, BLACK, dial);
		// Semelle carbone sur le dernier centimètre.
		float sole = inside(0.011 - f1.w);
		c = mix(c, BLACK, sole);
		rough = mix(0.3, 0.2, sole);
		spec = 0.5;
	} else if (mat_id == 7) {
		// Verres miroir, teinte dégradée.
		c = mix(vec3(0.05, 0.07, 0.16), vec3(0.32, 0.1, 0.3), f1.y);
		rough = 0.04;
		metal = 0.9;
		spec = 0.9;
	} else if (mat_id == 8) {
		// Monture.
		c = BLACK;
		rough = 0.25;
		spec = 0.6;
	} else if (mat_id == 9) {
		// Cale.
		c = mix(accent, vec3(0.35), 0.5);
		rough = 0.5;
		spec = 0.4;
	} else {
		// Sangles.
		c = vec3(0.02);
		rough = 0.7;
		spec = 0.3;
	}
	ALBEDO = c;
	ROUGHNESS = rough;
	SPECULAR = spec;
	METALLIC = metal;
	RIM = rim;
	RIM_TINT = 0.6;
	SSS_STRENGTH = sss;
	AO = ao;
}
