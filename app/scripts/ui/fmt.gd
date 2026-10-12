extends RefCounted
## Mise en forme à la française : virgule décimale, espaces fines entre les
## milliers, dates et durées.

const DAYS := ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."]
const MONTHS := ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."]
const MODES := {
	"libre": "Sortie libre",
	"seance": "Séance",
	"adversaires": "Coureurs virtuels",
	"test": "Séance test",
}
## Espace insécable entre les milliers (présente dans toutes les polices).
const THIN := "\u00a0"


## Nombre avec `decimals` décimales, virgule et séparateur de milliers.
static func num(v: float, decimals := 0) -> String:
	if is_nan(v) or is_inf(v):
		return "--"
	var neg := v < 0.0 and absf(v) >= 0.5 * pow(10.0, -decimals)
	var s := ("%." + str(decimals) + "f") % absf(v)
	var parts := s.split(".")
	var whole := parts[0]
	var grouped := ""
	while whole.length() > 3:
		grouped = THIN + whole.right(3) + grouped
		whole = whole.left(whole.length() - 3)
	grouped = whole + grouped
	if parts.size() > 1:
		grouped += "," + parts[1]
	return ("−" if neg else "") + grouped


## Valeur inconnue (négative ou nulle) affichée « -- ».
static func opt(v: float, decimals := 0) -> String:
	return num(v, decimals) if v > 0.0 else "--"


static func km(m: float, decimals := 1) -> String:
	return num(m / 1000.0, decimals) + " km"


static func watts(w: float) -> String:
	return num(w) + " W"


static func percent(fraction: float, decimals := 0) -> String:
	return num(fraction * 100.0, decimals) + " %"


## Durée au format h:mm:ss.
static func hms(seconds: float) -> String:
	var s := maxi(0, roundi(seconds))
	return "%d:%02d:%02d" % [s / 3600, (s / 60) % 60, s % 60]


## Durée au format m:ss (étapes de séance), avec les heures au-delà.
static func mss(seconds: float) -> String:
	var s := maxi(0, roundi(seconds))
	if s >= 3600:
		return hms(s)
	return "%d:%02d" % [s / 60, s % 60]


## Durée lisible : « 45 s », « 5 min », « 1 h 05 », « 2 h ».
static func duration(seconds: float) -> String:
	var s := maxi(0, roundi(seconds))
	if s < 60:
		return "%d s" % s
	if s < 3600:
		var m := s / 60
		var r := s % 60
		return "%d min" % m if r == 0 else "%d min %02d" % [m, r]
	var h := s / 3600
	var mm := (s / 60) % 60
	return "%d h" % h if mm == 0 else "%d h %02d" % [h, mm]


## Durée en heures décimales (« 3,5 h »).
static func hours(seconds: float, decimals := 1) -> String:
	return num(seconds / 3600.0, decimals) + " h"


static func _local(unix: float) -> Dictionary:
	var bias: int = Time.get_time_zone_from_system().get("bias", 0)
	return Time.get_datetime_dict_from_unix_time(int(unix) + bias * 60)


## « lun. 12 oct. 2026 »
static func date_long(unix: float) -> String:
	var d := _local(unix)
	return "%s %d %s %d" % [DAYS[d.weekday], d.day, MONTHS[d.month - 1], d.year]


## « 12 oct. »
static func date_short(unix: float) -> String:
	var d := _local(unix)
	return "%d %s" % [d.day, MONTHS[d.month - 1]]


## « oct. 2026 »
static func month_year(unix: float) -> String:
	var d := _local(unix)
	return "%s %d" % [MONTHS[d.month - 1], d.year]


## « 18:30 »
static func clock(unix: float) -> String:
	var d := _local(unix)
	return "%02d:%02d" % [d.hour, d.minute]


static func mode(m: String) -> String:
	return MODES.get(m, m.capitalize())


## Lit un nombre tapé à la française ou à l'anglaise ; NAN si invalide.
static func parse(text: String) -> float:
	var s := text.strip_edges().replace(THIN, "").replace(" ", "").replace("\u202f", "").replace(",", ".").replace("−", "-")
	for unit in ["%", "W/kg", "W", "kg", "km", "rpm", "m"]:
		if s.ends_with(unit):
			s = s.left(s.length() - unit.length())
	if s == "" or not s.is_valid_float():
		return NAN
	return s.to_float()


## Lit une durée « 5 », « 5:30 » ou « 1:05:00 » (minutes par défaut) ; NAN si invalide.
static func parse_duration(text: String) -> float:
	var parts := text.strip_edges().replace(" ", "").split(":")
	if parts.is_empty() or parts.size() > 3:
		return NAN
	var total := 0.0
	for p in parts:
		if p == "" or not p.replace(",", ".").is_valid_float():
			return NAN
	if parts.size() == 1:
		return parts[0].replace(",", ".").to_float() * 60.0
	for p in parts:
		total = total * 60.0 + p.to_float()
	return total


## Liste d'entiers « 36, 52 » ; tableau vide si invalide.
static func parse_int_list(text: String) -> Array:
	var out := []
	for p in text.replace(";", ",").replace(" ", ",").split(",", false):
		if not p.strip_edges().is_valid_int():
			return []
		out.append(p.strip_edges().to_int())
	return out


static func int_list(values: Variant) -> String:
	var parts := PackedStringArray()
	if values is Array or values is PackedInt32Array or values is PackedInt64Array or values is PackedFloat32Array:
		for v in values:
			parts.append(str(int(v)))
	return ", ".join(parts)
