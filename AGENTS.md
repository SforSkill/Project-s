# Project-s (contexte agents)

Client du jeu Project S (`Project-S`, Bevy + egui) et son réseau client. Lire [`SCOPE.md`](SCOPE.md). Politique commune des agents : `Thaskow/.github` → `ai/AGENTS.md`.

- Aucune règle de jeu ici : la simulation vit dans Project-s-simulation, les valeurs et cartes dans Project-s-content, le serveur dans Project-s-server, les personnages génériques dans Project-s-engine. Un changement qui en relève va dans le dépôt responsable (liens dans [`docs/README.md`](docs/README.md)).
- Le jeu local embarque `ps-server` **sans** la feature `admin` (`boundaries.toml`) ; aucun accès SQL.
- Personnages purement cosmétiques : `character/` traduit équipes et arme tenue pour `ps-engine-character` et ne touche jamais hitboxes ni réseau ; `character/hitbox_fit.rs` vérifie que les poses du moteur restent dans les hitboxes.
- La prédiction (`ps-netclient`) rejoue `ps-sim-net::predict`, qui suit `Sim::run_cmd` : un changement de simulation se teste ici par `tests/e2e.rs`.
- Carte du code : [`docs/codemap.md`](docs/codemap.md). Gros fichiers à lire par plage : `ps-client/src/ui.rs`, `game.rs`, `demo.rs`, `character/runtime.rs`, `ps-netclient/src/lib.rs`. Hors contexte : `Cargo.lock`, `target/`, `docs/results/`, `THIRD_PARTY_LICENSES.md`, images de `assets/brand/`.
- Plusieurs dépôts à la fois : `.cargo/config.toml` local avec `[patch."https://github.com/SforSkill/…"]` vers les clones voisins (ignoré par git). Ne jamais commiter un `Cargo.lock` résolu avec ces patches : le régénérer sans le fichier, puis `cargo test --workspace --release --locked`.
- Mode DEEP : `ps-netclient` (prédiction, connexion), `net.rs`, workflows de release.
- Dépôt public : aucun secret, adresse ou chemin de serveur réel.

Vérification (Project-s-content cloné à côté) : `cargo fmt --all -- --check && cargo clippy --workspace --all-targets --all-features --locked -- -D warnings && cargo test --workspace --all-features --release --locked && python ../Project-s-docs/tools/check_boundaries.py .`

Git : branche courte depuis `main`, PR au titre Conventional Commits, fusion en squash ; release-please gère versions, changelog et tags. Commits au nom du noreply GitHub du compte.
