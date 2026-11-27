<p align="center"><img src="assets/brand/banner.png" alt="Project S — FPS tactique compétitif" width="100%"></p>

# Projet S

FPS tactique compétitif en Rust : déplacement de type Source, tir déterministe,
serveur autoritaire à 128 Hz, grenades fumigènes hybrides (volume voxel opaque
qui réagit aux explosions), mode compétitif 5v5 avec bombe et économie.

**Alpha** (dernière version : [Releases](https://github.com/SforSkill/Projet-S/releases)) — jouable en local contre des bots et en réseau sur un
serveur dédié. Voir [`docs/status.md`](docs/status.md) pour
ce qui est fait et ce qui ne l'est pas, et [`docs/validation-report.md`](docs/validation-report.md)
pour les mesures réelles.

- Jouer : [`QUICKSTART.md`](QUICKSTART.md)
- Héberger un serveur : [`docs/deployment.md`](docs/deployment.md)
- Architecture : [`docs/architecture.md`](docs/architecture.md)
- Réseau : [`docs/network.md`](docs/network.md), laboratoire A/B/C/D : [`docs/network-lab.md`](docs/network-lab.md)
- Sécurité : [`docs/security.md`](docs/security.md)
- Toute la documentation : [`docs/README.md`](docs/README.md) ; carte du code : [`docs/codemap.md`](docs/codemap.md)
- Contribuer : [`CONTRIBUTING.md`](CONTRIBUTING.md) ; historique : [`CHANGELOG.md`](CHANGELOG.md)

## Construire

Rust 1.100 (fixé par `rust-toolchain.toml`).

```sh
cargo build --release -p game-client      # target/release/Project-S(.exe)
cargo build --release -p game-server      # target/release/projet-s-server(.exe)
cargo test --workspace --release
cargo clippy --workspace --all-targets -- -D warnings
```

Linux (client) : `libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`.
Le serveur dédié n'a aucune dépendance système et ne compile pas Bevy.

## Dépôt

| Dossier | Contenu |
|---|---|
| `crates/engine-*` | moteur partagé sans rendu : cœur, collisions, déplacement, armes, hitreg, visibilité, réseau chiffré, replays |
| `crates/game-rules` | règles de match (MR12/15/16, prolongations, économie) |
| `crates/game-sim` | simulation autoritaire (serveur et prédiction client) |
| `crates/game-server` | serveur sans E/S + boucle UDP 128 Hz + binaire dédié |
| `crates/game-netclient` | réseau client sans E/S : prédiction, interpolation, extrapolation |
| `crates/game-client` | client Bevy + egui (`Project-S`) |
| `configs/`, `maps/` | données du jeu (TOML, RON générés par `tools/mapgen`) |
| `tools/mapgen`, `tools/netlab`, `tools/package.py` | générateur de cartes, banc réseau, empaquetage |
| `deploy/` | Docker, systemd, Helm, Agones |
| `docs/` | documentation, décisions (ADR), résultats mesurés |

## Licence

Code propriétaire, tous droits réservés (voir [`LICENSE`](LICENSE)). Les
dépendances tierces sont listées dans `THIRD_PARTY_LICENSES.md`. Aucun asset ne
provient d'un jeu existant.
