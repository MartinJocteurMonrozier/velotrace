# Cycliste (rider.bin)

`rider.bin` est généré par `tools/rider/build_rider.py`, qui le reconstruit à
l'identique (`python3 tools/rider/build_rider.py`).

Le corps est tiré des fichiers de MakeHuman (maillage de base hm08, cibles de
morphologie, squelette et poids de peau par défaut), dépôt
makehumancommunity/makehuman, commit a8bc2d54ff0ac92e78ff71431b1023eda42bf482.
L'équipe MakeHuman les place sous licence **CC0 1.0** (domaine public, voir
`LICENSE.md` de ce dépôt, section « The license for the bundled assets »).
Aucune attribution n'est exigée ; elle est donnée ici par courtoisie.

Le casque, les lunettes, les chaussures et les cales sont construits par le
script lui-même ; la tenue est peinte par `app/shaders/rider.gdshader`. Ni l'un
ni l'autre ne reprend d'autre source.
