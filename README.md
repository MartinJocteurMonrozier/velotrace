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

Le décor réel est construit par `velo-world` (relief, images aériennes, OpenStreetMap).

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

![En danseuse dans la montée, caméra proche](docs/captures/cycliste-danseuse.jpg)

Le cycliste roule sur un vélo de route détaillé (cadre à tube incliné, cintre route, transmission, roues carbone à rayons, pneus à flancs beiges), en maillot, cuissard, chaussettes blanches, chaussures à cales, casque et lunettes. Sa position suit l'effort : pied à terre à l'arrêt, assis les mains aux cocottes, en position aéro les mains en bas du cintre sur le plat et en descente quand la puissance est élevée, en danseuse en sprint et dans les montées appuyées, le vélo balancé au rythme du pédalage. La caméra par défaut le suit de près, juste derrière, et glisse vers l'extérieur dans les virages pour le montrer un peu de côté ; C passe ensuite aux caméras arrière, de côté et haute.

Pour démarrer directement sur un parcours : `godot --path app -- --gpx /chemin/vers/sortie.gpx [--puissance 220]`.

### Décor réel

![Montée de l'Alpe d'Huez, décor réel](docs/captures/decor-reel-alpe.jpg)

Au menu, **Décor réel** télécharge une fois, puis garde en cache (`~/.cache/velotrace` sous Linux, `%LOCALAPPDATA%\velotrace` sous Windows) :

- le **relief** (tuiles « terrarium » sur AWS, ~13 m) ; l'altitude du parcours en est tirée puis lissée, pour des pentes fidèles ;
- les **images aériennes** de l'IGN (BD ORTHO®, France) plaquées sur le sol ; ailleurs ou hors connexion, le sol est coloré d'après l'altitude, la pente et OpenStreetMap ;
- **OpenStreetMap** via Overpass (avec deux serveurs de secours si le premier ne répond pas) : forêts, eau, bâtiments, routes et rivières.

![Route réaliste : accotements, buissons et balises](docs/captures/route-realiste.jpg)

La route suit un tracé lissé, rééchantillonné tous les mètres : enrobé avec grain, rustines, traces de roues et marquages usés, accotements en gravier puis en herbe, buissons sur les bords et balises J6 tous les 50 m. Le relief est aplani sous la chaussée puis rejoint le terrain sur une trentaine de mètres, il ne la coupe plus. Le cycliste tient sa droite en montée et coupe les virages à la corde sur le plat et en descente, en s'inclinant selon la vitesse.

![Rond-point et routes adjacentes d'après OpenStreetMap](docs/captures/rond-point.jpg)

Avec OpenStreetMap, la route devient celle de la réalité : le tracé du GPX est recalé sur la route OSM, la largeur et le nombre de voies viennent des étiquettes `width` et `lanes` (sinon du type de route), avec axe discontinu, continu dans les lacets, et séparations de voies. Les trottoirs viennent de `sidewalk` ; quand OSM ne dit rien, une rue bordée de façades en reçoit un. Les routes qui débouchent sur le parcours partent de leur vrai nœud de jonction, sous la chaussée, avec la même pente qu'elle : amorce de 30 m évasée au carrefour (moins quand elle arrive en biais), ligne « cédez le passage » et rive discontinue sur la route principale ; au-delà, elles continuent drapées sur le relief avec le même enrobé (ou en piste de terre). Une route qui longe le parcours ou le croise sans s'y raccorder (pont, voie parallèle) reste dessinée telle quelle. Les ronds-points (`junction=roundabout`, ou anneaux presque circulaires) gardent leur vraie forme, souvent ovale, et leur pente ; le cycliste y tient sa droite. L'exemple « Rond-point du Bourg-d'Oisans » en traverse un.

![Sol détaillé au bord de la route](docs/captures/sol-bord-route.jpg)

Près de la route, l'image aérienne, trop floue de près, laisse place à un sol dessiné : prairie avec ses touffes, ses brins et quelques fleurs, cailloux et éboulis sur les pentes raides, terre et aiguilles en forêt, enrobé, dalles ou pavés autour des maisons. La nature du sol vient de l'occupation OSM, de la pente et de la proximité des bâtiments. L'accotement utilise exactement la même texture, si bien qu'il se fond dans le sol sans couture ; plus loin, le sol dessiné rejoint l'image par une lisière irrégulière.

![Mur de soutènement dans la montée de l'Alpe d'Huez](docs/captures/mur-soutenement.jpg)

Là où la montagne est raide au bord de la route, un mur de soutènement en moellons la retient (jusqu'à 4 m, avec une terrasse herbeuse au-dessus) ; au-dessus du vide, un parapet de 75 cm borde la chaussée.

![Falaise sculptée au bord de la route](docs/captures/falaise.jpg)

Au-delà de 45°, une falaise sculptée se dresse au bord de la route : paroi en relief jusqu'à 30 m de haut, avec ses bancs inclinés en corniches, ses ravines, ses surplombs, ses coulures, ses lichens et un rebord herbeux qui rejoint la pente.

![Portique d'arrivée, barrières et public](docs/captures/arrivee.jpg)

Comme sur le Tour de France : arche gonflable bleue et blanche au départ, banderole à pois au pied de chaque ascension (longueur, pente moyenne et catégorie, de la 4e à hors catégorie), arche à pois « Grand prix de la montagne » au sommet, banderoles 3 km et 2 km, flamme rouge au dernier kilomètre et, à l'arrivée, un portique en treillis métallique avec son chronomètre, sa banderole « ARRIVÉE » et une ligne peinte à damier. Des barrières habillées de bâches imprimées (slogans inventés, aucune vraie marque) bordent ces endroits, les 300 derniers mètres en entier, avec du public derrière. Une ascension compte à partir de 80 m de dénivelé à 3 % de moyenne.

Près du parcours, des sapins et des feuillus poussent là où OSM indique une forêt ou là où l'image aérienne est vert sombre (version détaillée de près, simplifiée au-delà) ; les bâtiments sont extrudés à leur hauteur avec des fenêtres et restent visibles au loin, les routes et rivières sont drapées sur le relief, les lacs posés à plat. Près de la route, l'herbe des accotements et un grain de détail sur le sol masquent le flou des images aériennes.

**Qualité** basse, moyenne ou haute : distance du décor, finesse des images (2 m, 1 m ou 0,5 m par pixel), densité et portée des arbres, ombres, anticrénelage et échelle de rendu (FSR). L'affichage est plafonné à 60 images/s. Les carrés de décor détaillés (512 m) sont générés en arrière-plan autour du cycliste et libérés derrière lui ; au loin, des carrés de 2 km peu détaillés. Sur une Intel HD 630, commence en qualité moyenne.

Options en ligne de commande : `--decor reel|cache|simple`, `--qualite 0|1|2`, `--osm-fichier data/osm-alpe-d-huez.json` (OSM depuis un fichier au lieu d'Overpass), `--images <url {z}/{x}/{y}>` (autre source d'images, `file://` accepté), `--aller-a <m>`.

Sans Godot : `cargo run --release -p velo-world --example decor -- data/alpe-d-huez.gpx --osm data/osm-alpe-d-huez.json` construit le décor et affiche ce qui a été trouvé. Ce fichier OSM d'essai vient d'Overture Maps : il a la géométrie des routes OSM mais pas leurs étiquettes de voies, de trottoirs ni de ronds-points, qui y sont donc déduites ; avec Overpass, ce sont les vraies.

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
