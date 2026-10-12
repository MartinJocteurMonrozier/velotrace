extends Label3D
## Chronomètre du portique d'arrivée : temps de la sortie en cours (celui de
## l'écran principal), sinon temps écoulé depuis son apparition.

var _elapsed := 0.0
var _shown := -1


@warning_ignore("integer_division")
func _process(delta: float) -> void:
	_elapsed += delta
	var t := _elapsed
	var scene := get_tree().current_scene
	if scene != null:
		var ride_time: Variant = scene.get("_ride_time")
		if ride_time is float:
			t = ride_time
	var s := int(t)
	if s == _shown:
		return
	_shown = s
	text = "%d:%02d:%02d" % [s / 3600, (s / 60) % 60, s % 60]
