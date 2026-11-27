# Projet S (contexte pour les agents de code)

Repo `SforSkill/Projet-S` : FPS tactique compétitif en Rust. Une simulation autoritaire sans E/S (`game-sim`) tourne sur le serveur dédié, dans le serveur embarqué du client et, pour le déplacement, en prédiction côté client. Le rendu (Bevy) ne contient aucune règle de jeu.

## Coût et contexte (à appliquer à chaque tâche)

La politique commune des agents s'applique : `Thaskow/.github` → `ai/AGENTS.md` (`gh api repos/Thaskow/.github/contents/ai/AGENTS.md -H "Accept: application/vnd.github.raw"`). L'essentiel :

- **Carte d'abord** : [`docs/codemap.md`](docs/codemap.md) donne le fichier, la doc et les tests de chaque tâche. La lire avant de chercher, puis `rg -n` sur le symbole et lecture de la plage utile.
- **Jamais dans le contexte** : `Cargo.lock`, `target/`, `maps/*.ron`, `docs/results/`, `THIRD_PARTY_LICENSES.md`. Les gros fichiers (`game-sim/src/sim.rs`, `game-server/src/server.rs`, `game-client/src/ui.rs`, `game.rs`) se lisent par plage.
- **Sorties courtes** : `cargo build/test/clippy` sont bavards. `python <Thaskow/.github>/tools/ai/quiet.py -- <cmd>` s'il est présent, sinon sortie redirigée hors du repo dont on ne garde que `error`, `FAILED`, `failures:` et la fin. Jamais masquer une erreur.
- **Tests ciblés** pendant le travail (`cargo test -p <crate>`, `cargo clippy -p <crate>`) ; `-p game-server` compile sans Bevy, donc vite. Workspace complet une fois à la fin.
- **Mode selon le risque** : FAST (doc, texte, petit bug), STANDARD (fonctionnalité, refactor), DEEP (`engine-network`, `crypto.rs`, `engine-visibility`, `engine-hitreg`, protocole, déploiement, workflows de release).
- Sous-agent seulement pour une recherche sur plusieurs crates ; une tâche par session, `/clear` entre deux, `/handoff` pour une tâche longue.

## Stack

Rust 1.100 (`rust-toolchain.toml`, édition 2024, MSRV 1.90), workspace Cargo de 15 crates, Bevy 0.20 + egui (client seulement), TOML/RON pour les données, criterion pour les benchmarks. CI GitHub Actions (Ubuntu 22.04), release-please, image Docker du serveur.

## Architecture

| Chemin | Rôle |
|---|---|
| `crates/engine-*` | Moteur partagé sans rendu ni E/S : cœur, collisions, déplacement, armes, hitreg, visibilité, réseau chiffré, replays |
| `crates/game-rules` | Machine d'état pure du match (formats, phases, économie, bombe) |
| `crates/game-sim` | Simulation autoritaire (`Sim::step` = un tick), protocole, prédiction, bots, replays |
| `crates/game-server` | `Server` sans E/S + boucle UDP 128 Hz (`run.rs`) + binaire `projet-s-server` |
| `crates/game-netclient` | Réseau client sans E/S : prédiction, interpolation, entrées sub-tick |
| `crates/game-client` | Binaire `Project-S` (Bevy) : rendu, HUD, son, réglages, personnages liquides (`character/`, purement cosmétiques), `--autotest`, `--character-demo` |
| `configs/*.toml` | Toutes les valeurs de gameplay |
| `maps/*.ron` | Cartes **générées** par `tools/mapgen` |
| `tools/` | `mapgen`, `netlab` (laboratoire réseau), `package.py` (archives, licences tierces) |
| `deploy/` | Docker, systemd, Helm, Agones, `server.example.toml` |
| `docs/` | Index `docs/README.md` ; carte `codemap.md`, glossaire, configuration, règles, réseau, sécurité, ADR (`decisions/`), état (`status.md`), mesures (`validation-report.md`) |

Détail par fichier et dépendances entre crates : [`docs/codemap.md`](docs/codemap.md) ; vocabulaire (tick, sub-tick, PIM…) : [`docs/glossary.md`](docs/glossary.md).

## Règles

- `engine-*` ne dépend jamais de `game-*` ; Bevy reste confiné à `game-client`.
- Simulation déterministe et sans E/S, partagée serveur/client ([ADR 0001](docs/decisions/0001-rust-bevy.md)) : l'aléa passe par `DetRng`, le temps par `TickRate`/`SimTime`, les E/S restent dans `run.rs` et `game-client`.
- Personnages (`game-client/src/character/`) purement cosmétiques : ils ne lisent que l'état réseau, n'influencent jamais hitboxes ni réseau ([`docs/characters.md`](docs/characters.md), ADR 0005).
- La prédiction (`game-sim/src/predict.rs`) suit exactement le chemin de `Sim::run_cmd` : modifier l'un, c'est vérifier l'autre.
- Valeurs de gameplay dans `configs/`, jamais en dur (exception connue : grenades et fumée, encore dans `GrenadeConfig`/`SmokeConfig`, cf. `docs/grenades.md`). Cartes : modifier `tools/mapgen`, puis `cargo run -p mapgen`, jamais les `.ron` à la main.
- Changement de `proto.rs`, du format de replay ou de `engine-network` : doc associée (`docs/network.md`, `docs/replay-format.md`) dans la même PR.
- Une décision qui dure va dans `docs/decisions/` (ADR numérotée).
- Repo public : aucun secret, adresse ou chemin de serveur.

## Vérification

Celles de la CI (`.github/workflows/ci.yml`) :

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --release --locked
cargo run --release --locked -p game-server -- --data . --config deploy/server.example.toml --check
```

## Conventions

- Pas de commentaire sauf raison non évidente, en une courte clause ; les `//!` de module décrivent le rôle du fichier.
- Git : trunk-based, détail dans [`docs/git-workflow.md`](docs/git-workflow.md). Branche courte depuis `main`, PR vers `main` au titre Conventional Commits, fusion en squash ; release-please gère versions, changelog et tags (ne jamais les modifier à la main).
- Mettre `docs/codemap.md` à jour dans la PR qui ajoute ou déplace un module.

## Auteur

Commits au nom du noreply GitHub du compte (`<id>+<login>@users.noreply.github.com`), jamais une adresse professionnelle ou d'école. L'utilisateur gère git : ne pas créer de branche, commiter ni pousser sans demande explicite.
