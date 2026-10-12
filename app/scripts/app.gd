extends Node
## Services partagés par tous les écrans (chargé automatiquement, nom « App ») :
## sortie et profil, appareils Bluetooth, Strava et historique. Ils survivent
## au retour au menu, si bien que les appareils restent connectés.

const EXAMPLES := {
	"Alpe d'Huez depuis Le Bourg-d'Oisans (14 km)": "res://../data/alpe-d-huez.gpx",
	"Vallée puis lacets (7,5 km)": "res://../data/exemple-lacets.gpx",
	"Plat puis col à 7 % (8 km)": "res://../data/exemple-col.gpx",
	"Rond-point du Bourg-d'Oisans (0,7 km)": "res://../data/exemple-rond-point.gpx",
}

var ok := false
var error := ""
## VeloRide : parcours, profil, sortie en cours et décor.
var ride: Node
## VeloDevices : home trainer, capteurs et Click en Bluetooth.
var devices: Node
## VeloStrava : connexion au compte et envoi des sorties.
var strava: Node
## VeloHistory : sorties enregistrées et suivi à long terme.
var history: Node


func _ready() -> void:
	# Sans import préalable par l'éditeur, Godot ne connaît pas encore l'extension.
	if not ClassDB.class_exists("VeloRide"):
		GDExtensionManager.load_extension("res://velotrace.gdextension")
	for cls in ["VeloRide", "VeloDevices", "VeloStrava", "VeloHistory"]:
		if not ClassDB.class_exists(cls):
			error = "Extension Rust absente ou ancienne : lance « cargo build -p velo-godot » à la racine du dépôt, puis relance."
			return
	ride = _service("VeloRide")
	devices = _service("VeloDevices")
	strava = _service("VeloStrava")
	history = _service("VeloHistory")
	ride.set_devices(devices)
	ok = true
	# Hors mode test, on se reconnecte tout de suite aux appareils appairés.
	if not ride.profile_get("app.test_mode"):
		devices.connect_remembered(ride.remembered_devices())


func _service(cls: String) -> Node:
	var n: Node = ClassDB.instantiate(cls)
	n.name = cls
	add_child(n)
	return n


## Envoie la sortie sur Strava si l'envoi automatique est activé.
func auto_upload(ride_id: int) -> void:
	if ride_id >= 0 and ride.profile_get("strava.auto_upload") and strava.status().connected:
		strava.upload_ride(ride_id)
