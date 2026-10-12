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

## Essayer

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
