# Cartes

Les cartes sont **écrites en code** dans `tools/mapgen/src/main.rs`, puis générées
dans `maps/*.ron` avec `cargo run -p mapgen`. Le code se relit, la génération est
reproductible, et toute la géométrie est originale.

La construction se fait en trois temps :
1. une grille de cases de 128 unités est creusée dans la roche ;
2. les murs sont fusionnés en un minimum de boîtes ;
3. les éléments sont ajoutés : caisses, plates-formes, murs minces traversables,
   rampes.

| Carte | Nom | Briques | Modes | Usage |
|---|---|---|---|---|
| `ps_citadel` | Citadelle | 50 | competitive, deathmatch, training | carte compétitive : 2 sites de bombe, zones d'achat, routes longues et courtes, murs traversables |
| `ps_duel_lab` | Duel Lab | 23 | duel, deathmatch, training | arène 1v1 ; utilisée par les tests réseau et le laboratoire |
| `ps_training_lab` | Training Lab | 56 | training, deathmatch | stand de tir : cibles, murs de chaque matériau pour tester la pénétration |

## Format (`MapDef`, RON)

- `brushes` contient trois sortes de briques :
  - `Box { min, max, mat }` ;
  - `Ramp { min, max, high, mat }` ;
  - `Clip { min, max }`, qui bloque le joueur sans arrêter les balles ni se dessiner.
- `spawns`, par équipe ; `dm_spawns`, pour le deathmatch ; `bomb_sites` et `buy_zones`, qui sont des boîtes ;
  `targets`, pour l'entraînement ; `labels`, les noms de zones.
- `materials` vient de `configs/materials.toml` : pénétrabilité, couleur, son.

Le test `shipped_data_and_maps_are_valid` charge toutes les cartes livrées.
`citadel_nav_connects_spawns_to_both_sites` vérifie que les bots peuvent
atteindre les deux sites depuis les deux points d'apparition.

## Modifier une carte

Éditer le code de la carte dans `tools/mapgen`, puis lancer `cargo run -p mapgen`
et `cargo test -p game-sim`. Ne pas éditer les `.ron` à la main : ils sont
régénérés.
