# Changelog

Tenu par release-please à partir des titres de PR (Conventional Commits) : ne pas éditer à la main.

## Non publié

Séparation en dépôts : ce dépôt ne garde que le client. Comportement de jeu, format réseau et réglages du joueur inchangés.

### Modifié
- `game-client` → `ps-client` (binaire `Project-S` inchangé), `game-netclient` → `ps-netclient`.
- Les crates partagées viennent des dépôts publiés, épinglés par tag : Project-s-protocol, Project-s-netcode, Project-s-simulation (`ps-sim`, `ps-sim-net`…), Project-s-server (`ps-server` sans la feature `admin`, serveur local embarqué) et Project-s-engine (`ps-engine-character` remplace le module `character/` du client, qui garde l'intégration au jeu et le test `hitbox_fit`).
- Le contenu vient de Project-s-content (`.content-version`) : `--data`, `PS_CONTENT_DIR`, dossier de l'exécutable ou dépôt voisin ; les archives du client l'embarquent.
- Les archives du client ne contiennent plus le serveur dédié, publié par Project-s-server.

### Retiré
- `engine-*`, `game-rules`, `game-sim`, `game-server`, `configs/`, `maps/`, `tools/mapgen`, `deploy/` (dans leurs dépôts respectifs) ; `tools/netlab` et ses résultats (Project-s-lab) ; documentation transversale (Project-s-docs).

## [0.1.0-alpha.1](https://github.com/SforSkill/Project-s/compare/v0.1.0-alpha...v0.1.0-alpha.1) (2026-11-27)


### Features

* **client:** personnages « peinture liquide » à la place des cubes ([48f7ee3](https://github.com/SforSkill/Project-s/commit/48f7ee3c2732421018f768f5b04aee71b0b558e4))
* Projet S v0.1.0-alpha ([bc97fe0](https://github.com/SforSkill/Project-s/commit/bc97fe027085047dee12aa68f284bb53d87f02cd))


### Bug Fixes

* **client:** plein écran F11/Alt+Entrée et capture du curseur fiable ([25aa37c](https://github.com/SforSkill/Project-s/commit/25aa37c71a0f189185ad0f364771f3e9d28b53e0))

## 0.1.0-alpha (2026-11-26)

Première alpha jouable : déplacement de type Source, tir déterministe, serveur autoritaire 128 Hz, fumigènes hybrides, compétitif 5v5 avec bombe et économie, bots, replays, serveur dédié Linux. Détail : [`docs/release-notes.md`](docs/release-notes.md) et [`docs/status.md`](docs/status.md).
