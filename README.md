# velotrace

Application de home trainer pour Windows et Linux, dans l'esprit de Zwift et MyWhoosh : on importe un GPX, l'application génère le monde 3D autour de la trace, et on la parcourt avec les vrais rapports et les vraies pentes de son vélo.

Architecture et plan : [docs/architecture.md](docs/architecture.md).

## État

Étape 1 en cours : le cœur `velo-core` (Rust) contient

- le profil cycliste, vélo et home trainer, sauvegardé en TOML ;
- la transmission et le changement de vitesse virtuel (Zwift Click) ;
- le modèle physique et le calcul de la pente envoyée au home trainer ;
- la lecture d'un GPX, le rééchantillonnage et le lissage des pentes ;
- l'encodage et le décodage des trames BLE FTMS, CSC et HRS ;
- le **mode test**, qui simule le home trainer, le capteur de cadence, la FC et les Zwift Click.

## Prototype jouable (Godot, mode test)

![En route vers les lacets](docs/captures/prototype-lacets.png)

Il faut [Godot 4.5 ou plus récent](https://godotengine.org/download) et Rust 1.94 ou plus récent (`rustup update stable`).

```sh
cargo build -p velo-godot          # compile l'extension Rust chargée par Godot
godot --path app                   # ou ouvrir app/project.godot dans l'éditeur Godot et appuyer sur F5
```

Au menu, choisis un parcours d'exemple ou ton propre GPX, puis le cycliste virtuel :

- **Cadence 88 rpm, Click automatiques** : la puissance dépend du rapport et de la pente, le cycliste change de vitesse tout seul ;
- **Puissance fixe 200 W** : ← et → ajustent la puissance.

Pendant la sortie : ↑ et ↓ changent de vitesse (Click), A active ou coupe les Click automatiques, C change de caméra, Espace met en pause, Échap revient au menu.

Pour démarrer directement sur un parcours : `godot --path app -- --gpx /chemin/vers/sortie.gpx [--puissance 220]`.

## Essayer le cœur en ligne de commande

```sh
cargo test
cargo run -p velo-core --example gears                     # rapports virtuels
cargo run -p velo-core --example simulate -- data/exemple-col.gpx              # mode test, cadence 88 rpm et Click automatiques
cargo run -p velo-core --example simulate -- data/exemple-col.gpx --power 200  # mode test à 200 W
```

## Vérifier son matériel (velo-probe)

Sous Linux, installer d'abord `libdbus-1-dev` et `pkg-config` (`sudo apt install libdbus-1-dev pkg-config`), puis, près du vélo :

```sh
cargo run --release -p velo-probe -- scan       # appareils Bluetooth à portée
cargo run --release -p velo-probe -- trainer    # données du home trainer, puis pentes de 0 %, 2 % et 8 %
cargo run --release -p velo-probe -- capteurs   # cadence et fréquence cardiaque
cargo run --release -p velo-probe -- click      # boutons des Zwift Click
```

Fermer Zwift, MyWhoosh ou l'appli Decathlon avant : un home trainer n'accepte souvent qu'une seule connexion Bluetooth. Le Click gauche V2 peut exiger un déverrouillage quotidien (se connecter une fois à l'appli Zwift avec le Click, puis la fermer) ; le Click droit fonctionne sans.
