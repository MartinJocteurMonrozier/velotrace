# Projet « Vélo » : architecture et plan jusqu'au prototype jouable

Une application de bureau (Windows et Linux) dans l'esprit de Zwift et MyWhoosh. On importe un GPX, l'application génère un monde 3D fidèle autour de la trace, et on la parcourt sur le home trainer avec les vrais rapports et les vraies pentes de ton vélo.

---

## 1. Vue d'ensemble

```
┌──────────────────────────── Application (Godot 4) ─────────────────────────────┐
│  Scène 3D (terrain, route, bâtiments, arbres, avatar)   │  Overlay / HUD        │
│                     ▲                                   │  puissance, cadence,  │
│                     │ position sur la trace, vitesse    │  vitesse, pente,      │
│                     │                                   │  rapport, minicarte,  │
├─────────────────────┴───────────────────────────────────┴─ profil, FC ─────────┤
│                 velo-core (Rust, exposé à Godot via godot-rust/gdext)          │
│  ┌──────────────┐  ┌───────────────────┐  ┌───────────────┐  ┌───────────────┐ │
│  │ Capteurs BLE │→ │ Moteur physique   │→ │ Commande du   │  │ Enregistrement│ │
│  │ (btleplug)   │  │ (vitesse, rapports│  │ home trainer  │  │ .FIT          │ │
│  │ FTMS, CSC,   │  │  pente effective) │  │ (FTMS)        │  │               │ │
│  │ HRS, Click   │  └───────────────────┘  └───────────────┘  └───────────────┘ │
│  └──────────────┘                                                              │
└────────────────────────────────────────────────────────────────────────────────┘
          ▲
          │ fichiers de monde pré-calculés (par tronçons)
┌─────────┴──────────────── Import d'un parcours (hors ligne, Rust) ─────────────┐
│ GPX → rééchantillonnage → altitude MNT (IGN / Copernicus) → lissage des pentes │
│ OpenStreetMap (Overpass) → routes, bâtiments, forêts, eau                      │
│ Orthophoto (IGN BD ORTHO) → textures du sol                                     │
└────────────────────────────────────────────────────────────────────────────────┘
```

## 2. Pile technique recommandée

| Brique | Choix | Pourquoi |
|---|---|---|
| Moteur 3D et interface | **Godot 4** (GDScript pour la scène et l'UI) | Gratuit et libre, exports Windows et Linux natifs, bon terrain et instanciation (MultiMesh) pour les arbres, UI intégrée pour l'overlay. Unity est une alternative plus lourde (licence), Bevy (Rust) est moins mûr côté outils. |
| Cœur métier | **Rust**, crate `velo-core`, branché dans Godot avec **godot-rust (gdext)** | Physique, Bluetooth et import testables sans lancer le jeu, performants, multiplateforme. |
| Bluetooth Low Energy | **btleplug** | Une seule API pour Windows (WinRT) et Linux (BlueZ). |
| ANT+ (optionnel, plus tard) | clé USB ANT+ via libusb | Utile seulement si tu veux brancher le Coros Dura en parallèle (voir §3). |
| Données géographiques | GPX (crate `gpx`), MNT IGN RGE ALTI 1 m / 5 m (France) ou Copernicus GLO‑30 (monde), OSM via Overpass, orthophoto IGN BD ORTHO | Tout est gratuit et réutilisable (licence Etalab pour l'IGN, ODbL pour OSM). |
| Fichiers d'activité | export **.FIT** | Importable dans Strava, Coros, Garmin Connect, Intervals.icu. |
| Distribution | installeur Windows (Inno Setup ou MSI), AppImage ou Flatpak sous Linux | Téléchargeable sans dépendance. |

Option « photoréaliste » écartée pour le prototype : Cesium + Google Photorealistic 3D Tiles donne un rendu bluffant, mais c'est payant à l'usage, dépendant d'internet, et les conditions d'utilisation interdisent la mise en cache durable. On pourra la réévaluer plus tard.

## 3. Les appareils

| Appareil | Protocole | Ce qu'on en tire | Points d'attention |
|---|---|---|---|
| **Van Rysel D500** | BLE **FTMS** (service 0x1826) ; ANT+ FE‑C probable | Puissance, cadence (si fournie), vitesse du volant. On lui envoie la pente via *Set Indoor Bike Simulation Parameters* (opcode 0x11 : vent, pente, Crr, CdA). | La plupart des home trainers n'acceptent **qu'une connexion BLE** à la fois. La pente maximale simulée est limitée par l'appareil (à vérifier sur la fiche du D500). |
| **Capteur de cadence** | BLE **CSC** (0x1816) | Cadence calculée à partir des tours de manivelle et de leur horodatage. | Sert de référence si la cadence du trainer est absente ou bruitée. |
| **Zwift Click** | BLE propriétaire (rétro‑ingénierie publique, cf. projets open source comme SwiftControl / BikeControl) | Boutons + et − pour le changement de vitesse virtuel. | **Principal risque technique** : protocole non documenté, poignée de main propre à Zwift, possibles différences entre Click v1 et v2. À valider dès l'étape 0. |
| **Coros Dura** | C'est un compteur, il **reçoit** des capteurs mais n'en diffuse pas | Pas utilisable comme capteur. Deux usages possibles : (a) le Dura lit le trainer en ANT+ pendant que l'appli le pilote en BLE, pour enregistrer aussi la sortie sur le compteur ; (b) plus simple, l'appli exporte un .FIT que tu importes dans Coros ou Strava. | Pour (a), vérifier que le D500 diffuse en ANT+ en même temps qu'en BLE. |
| Ceinture cardio (si tu en as une) | BLE **HRS** (0x180D) | Fréquence cardiaque dans l'overlay et le .FIT. | Aucun. |

## 4. Physique : reproduire le comportement réel du vélo

### 4.1 Vitesse dans le jeu

Comme Zwift, la vitesse affichée vient de la **puissance mesurée** et des forces résistantes réelles, intégrées à chaque pas de temps (≈ 20 Hz) :

```
P_roue = P_mesurée × (1 − pertes transmission ≈ 2,5 %)
F_résistante(v, θ) = m·g·(Crr·cos θ + sin θ) + ½·ρ·CdA·(v + v_vent)²
m_eff · dv/dt = P_roue / v − F_résistante(v, θ)
```

- `m` = ton poids + le vélo (≈ 8 kg), `m_eff` ajoute l'inertie des roues (≈ +1 kg).
- `Crr` ≈ 0,004 (route lisse), on pourra le moduler selon la surface OSM (gravier, pavés).
- `CdA` ≈ 0,32 m² sur les cocottes, `ρ` ≈ 1,2 kg/m³ ajusté selon l'altitude.
- Le drafting (aspiration) est inutile au début puisqu'on roule seul.

### 4.2 Pente ressentie

La pente `θ` vient du MNT (pas de l'altitude GPS du GPX, trop bruitée), lissée sur ~30 à 50 m pour éviter les à‑coups. Réglage « difficulté des pentes » à **100 % par défaut** (Zwift est à 50 %), pour coller à la réalité.

### 4.3 Rapports : deux modes

**Mode A, passage de vitesses réel.** Tu changes de vitesse sur ton vélo monté sur le D500. On envoie simplement la pente au trainer, qui calcule lui‑même la résistance à partir de la vitesse de son volant : le ressenti est naturellement celui de tes rapports. L'appli déduit le rapport engagé du ratio vitesse volant / cadence pour l'afficher.

**Mode B, passage virtuel avec les Zwift Click.** La chaîne reste sur un pignon du milieu (rapport physique `r_p`), et l'appli simule ta transmission complète (`r_v` = plateau / pignon virtuel). Pour que le couple au pédalier soit celui du rapport virtuel, on calcule à chaque pas une **pente effective** à envoyer au trainer :

```
v_p = ω_pédalier · r_p · R_roue          (vitesse réelle du volant)
v_v = ω_pédalier · r_v · R_roue          (vitesse que tu aurais sur la route)
F_cible = F_résistante(v_v, θ) · r_v / r_p      (couple pédalier = F · R_roue · r)
sin θ_envoyée = (F_cible − ½·ρ·CdA·v_p²) / (m·g) − Crr
```

`θ_envoyée` est bornée à la plage du D500 et filtrée pour rester fluide. C'est le même principe que le « virtual shifting » de Zwift. Limite connue : l'inertie ressentie reste celle du volant du trainer, on ne peut pas la changer.

## 5. Génération du monde 3D

Pipeline d'import exécuté une fois par GPX, résultat mis en cache sur disque :

1. **Trace** : lecture du GPX, projection en coordonnées métriques locales (UTM ou ENU), rééchantillonnage tous les 5 m, lissage.
2. **Relief** : MNT sur un couloir de ~2 km de part et d'autre de la trace (haute résolution) puis ~10 km en basse résolution pour l'horizon. Découpage en tuiles avec niveaux de détail.
3. **Route** : ruban 3D le long de la trace, avec le terrain aplani dessous, marquages selon le type de route OSM. Les autres routes OSM visibles sont générées aussi.
4. **Sol** : texture d'orthophoto IGN (20 cm à 50 cm) sur le couloir proche, couleurs selon l'occupation OSM (champ, forêt, eau) au loin.
5. **Bâtiments** : extrusion des emprises OSM avec hauteur (`height`, `building:levels`, sinon valeur par défaut), toits simples.
6. **Végétation** : arbres instanciés (MultiMesh) dans les polygones de forêt, haies et arbres isolés OSM.
7. **Ciel et lumière** : ciel procédural, heure du jour réglable.

**Implémentation actuelle (crate `velo-world`)** :

- relief : tuiles terrarium zoom 13 (~13 m), couloir de 2,5 à 6 km selon la qualité ; altitude du parcours = relief, médiane glissante 100 m puis moyenne 200 m (Alpe d'Huez : D+ 1 072 m) ; sol posé à plat sous la chaussée et les accotements (5 m de l'axe), puis fondu vers le relief jusqu'à 28 m ;
- deux niveaux de décor : grands carrés de 2 048 m (maille 32 à 64 m, texture 256 ou 512 px) construits au chargement, et carrés détaillés de 512 m (maille 4 à 5,3 m, texture 256 à 1 024 px, arbres, bâtiments, routes, eau) générés par des fils de travail autour du cycliste et libérés derrière lui. Un masque d'un texel par carré de 512 m retire le grand carré là où un carré détaillé est chargé, et des jupes cachent les jointures ;
- images aériennes : WMTS IGN par défaut, URL modifiable dans le profil (`[scenery]`), repli sur trois zooms moins fins puis sur une couleur calculée (altitude, pente, occupation OSM) ;
- arbres : cases fixes de 5,5 à 9 m ; un arbre y pousse selon OSM (forêt) et la couleur de l'image (vert sombre), sapin ou feuillu selon `leaf_type` et l'altitude ; MultiMesh par carré avec distance d'affichage ;
- OSM : requête Overpass « around » le long du parcours (bâtiments sur un couloir plus large, jusqu'à 2,5 km), mise en cache, avec deux miroirs de secours ; ou fichier JSON (`--osm-fichier`) ;
- route (`road.rs`) : axe lissé (gaussienne σ 5 m) puis rééchantillonné tous les mètres en Catmull-Rom ; maillage 4 sommets par mètre (chaussée de 6 m, accotements de 1,3 m abaissés de 25 cm) découpé en tronçons de 200 m ; balises J6 tous les 50 m ; le shader dessine enrobé, rustines, lignes de rive et tirets usés, gravier puis herbe sur les accotements, et peint l'herbe sur l'image aérienne jusqu'à 11 m ;
- route d'après OSM (`roadnet.rs`) : chaque point du GPX est rattaché à la route OSM la plus plausible (à moins de 14 m, même sens, de préférence la même qu'au point précédent) puis recalé dessus ; profil en travers par échantillon (demi-largeur depuis `width`, sinon `lanes` × largeur de voie selon la classe ; trottoirs `sidewalk*`, sinon trottoir dessiné à part ou au moins trois façades à moins de 28 m ; marquages selon la classe, axe continu quand la courbure lissée dépasse 1/70 m) ; amorces de 30 m pour chaque route qui sort de la chaussée du parcours ou d'un rond-point (évasement de 2,5 m au débouché, cédez-le-passage, rive discontinue et trottoir interrompu en face) ; ronds-points étiquetés ou anneaux circulaires de 5 à 45 m de rayon, dessinés à plat avec bande franchissable et îlot bombé ; relief aplani sous tout cela par un index de points (replat puis fondu) ;
- trajectoire du cycliste : à ~1,1 m du bord droit de la chaussée, décalée vers l'intérieur du virage selon la courbure (gain 50) quand la pente est nulle ou négative, sans franchir l'axe sur une route à double sens (toute la largeur sur une route étroite ou à sens unique), puis lissée sur 10 m ; cap mesuré sur ±2 m et inclinaison atan(v²κ/g) ;
- arbres et buissons : maillages faits main (sapin à 4 étages tombants, feuillu en 5 boules), vent dans le shader ; instances regroupées par cases de 128 m avec une version détaillée jusqu'à 110 à 260 m et une version simplifiée au-delà ;
- bâtiments : couleurs de murs et toits variées, fenêtres tous les 3 m dessinées par shader ; aussi présents dans les grands carrés ;
- sol : texture de bruit superposée à l'image de près (25 à 260 m) pour masquer le flou.

L'**avatar** : modèle de cycliste et de vélo libres de droits (ou réalisés sous Blender), pédalage synchronisé sur la cadence réelle avec une IK des jambes (pieds liés aux pédales), position du buste selon la pente et la vitesse. Caméras : arrière (par défaut), première personne, drone.

## 6. Overlay (HUD)

- Haut : puissance instantanée et moyenne 3 s, W/kg, cadence, vitesse, fréquence cardiaque.
- Côté : **pente** actuelle et à venir, **rapport** engagé (ex. 50×17, avec position sur la cassette).
- Bas : **profil d'altitude** du parcours avec ta position, distance parcourue et restante, temps, dénivelé.
- Coin : **minicarte** avec la trace et ta position.
- Écrans : choix du GPX, appairage des capteurs, profil cycliste (poids, transmission, roue), fin de sortie avec export .FIT.

## 7. Plan par étapes

| Étape | Contenu | Résultat vérifiable |
|---|---|---|
| **0. Validation matériel** | Petit outil en ligne de commande Rust : scan BLE, connexion au D500, lecture des données FTMS, envoi d'une pente ; lecture du capteur de cadence ; décodage des boutons du Zwift Click. | Tu sens la résistance changer quand l'outil envoie 2 %, puis 8 %. Les clics s'affichent. **On lève ici les deux plus gros risques.** |
| **1. Cœur physique et GPX** | `velo-core` : lecture GPX, altitude MNT, lissage, modèle physique, mode A, enregistrement .FIT, tests automatiques. | Une sortie « sans graphisme » sur un GPX réel, avec vitesse et pente cohérentes, et un .FIT qui s'importe dans Strava. |
| **2. Prototype jouable** | Projet Godot : terrain MNT sans texture, ruban de route, avatar simplifié qui avance, caméra arrière, overlay complet, écran d'appairage. Builds Windows et Linux. | **Premier prototype jouable** : tu charges ton GPX et tu roules dessus avec le vrai ressenti des pentes. |
| 3. Changement virtuel | Mode B (pente effective), Zwift Click, affichage du rapport, réglage de difficulté. | Tu passes les vitesses aux Click et le ressenti correspond à tes rapports. |
| 4. Monde réaliste | Orthophoto, bâtiments, forêts, eau, routes secondaires, ciel ; avatar animé avec IK. | Un parcours reconnaissable visuellement. |
| 5. Finitions et diffusion | Installeurs, réglages, profil cycliste, option Coros Dura en ANT+, performances. | Une version téléchargeable. |

## 8. Risques identifiés

- **Zwift Click** : protocole propriétaire, peut changer avec une mise à jour du firmware. Solution de repli : touches clavier ou manette.
- **Bluetooth sous Linux** : nécessite BlueZ récent et les bons droits ; sous Windows, Windows 10 ou plus récent.
- **D500** : vérifier qu'il expose bien FTMS avec le mode simulation (et pas seulement l'ERG), et sa pente maximale.
- **Données hors de France** : pas d'IGN, on bascule sur Copernicus 30 m et une imagerie moins fine.
- **Quotas Overpass** : mise en cache locale, voire extraits OSM régionaux téléchargés une fois.
- **Propriété intellectuelle** : aucun élément graphique repris de Zwift ou MyWhoosh.

## 9. Ton profil (réglages par défaut)

| Paramètre | Valeur |
|---|---|
| Plateaux | 52 / 36 |
| Cassette | 11‑34, 11 vitesses (11‑13‑15‑17‑19‑21‑23‑25‑27‑30‑34, étagement Shimano supposé) |
| Pneus | 700×40c (circonférence 2 200 mm) |
| Poids | cycliste 59 kg + vélo 11 kg = 70 kg |
| Changement de vitesse | **mode B**, virtuel avec les **Zwift Click V2** |
| Fréquence cardiaque | montre Garmin en mode « diffusion de la FC » (BLE HRS) |

En mode B, la chaîne reste sur un rapport physique du milieu (par défaut 52×19 si le D500 a sa cassette, réglable), et l'appli parcourt 16 rapports virtuels aux Click, comme un passage synchronisé Shimano : 36×34 à 36×19, puis 52×27 à 52×11, sans croisement de chaîne extrême.

## 10. Application paramétrique et soignée

Tout ce qui est réglable passe par l'interface, rien n'est codé en dur. Les réglages sont stockés dans un fichier de profil (TOML) que l'interface lit et écrit.

- **Profil cycliste** : poids, taille, FTP, zones de puissance et de FC, unités.
- **Vélo** (plusieurs vélos possibles) : plateaux, cassette (préréglages Shimano, SRAM, Campagnolo ou saisie libre), pneus, poids, CdA selon la position, Crr, pertes de transmission.
- **Home trainer** : rapport physique en mode B, difficulté des pentes (0 à 100 %), pente max, lissage, étalonnage de la puissance.
- **Capteurs** : appairage, priorité des sources (cadence trainer ou capteur), mémorisation des appareils.
- **Commandes** : affectation des boutons des Click et des touches clavier.
- **Affichage** : choix et position des champs de l'overlay, unités, caméra, qualité graphique.
- **Parcours** : bibliothèque des GPX importés, avec profil, dénivelé et aperçu de la carte.
- **Sorties** : historique, export .FIT, **envoi direct sur Strava** (connexion OAuth une fois, puis envoi automatique ou manuel en fin de sortie ; il faudra créer une « application API » Strava gratuite pour obtenir les identifiants).

Interface : thème sombre moderne, menus Godot avec composants maison (cartes, onglets, graphiques), utilisable à la souris et aux Click.

## 11. Mode test (appareils simulés)

Un interrupteur dans les réglages remplace tout le matériel par des appareils simulés, pour développer et essayer l'application sans home trainer :

- **Home trainer simulé** : il accepte la prise de contrôle FTMS et les consignes de pente, et renvoie vitesse, cadence et puissance comme le D500.
- **Capteur de cadence et FC simulés** : la fréquence cardiaque suit la puissance avec un temps de réponse réaliste.
- **Click simulés** : touches du clavier ou boutons à l'écran, plus un mode automatique où le cycliste virtuel change de vitesse pour garder sa puissance dans une plage.
- **Cycliste virtuel** : soit à puissance imposée (réglable au clavier), soit à cadence imposée ; dans ce cas la puissance dépend du rapport et de la pente, ce qui permet de tester tout le ressenti des vitesses.

Le simulateur fabrique les mêmes trames Bluetooth que le vrai matériel et les décode avec le même code : seul le lien radio n'est pas testé.
