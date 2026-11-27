# Carte du code

But : aller droit au bon fichier sans parcourir le dépôt. Index de la doc : [`README.md`](README.md) ; vocabulaire : [glossaire](https://github.com/SforSkill/Project-s-docs/blob/main/docs/glossary.md). Lire d'abord la ligne de la tâche (« Où chercher »), puis seulement le fichier indiqué, en ciblant le symbole (`rg -n "fn predict_cmd" crates/`).

Ce dépôt ne contient que le client. La simulation, le contenu, le serveur, le transport et les personnages génériques sont des dépendances épinglées par tag dans `Cargo.toml` ([architecture](https://github.com/SforSkill/Project-s-docs/blob/main/ARCHITECTURE.md)).

## Crates

| Crate | Rôle | Dépend de (Project S) |
|---|---|---|
| `ps-netclient` | réseau client sans E/S : connexion, horloge, prédiction, interpolation, extrapolation, `InputTimeline` | `ps-sim`, `ps-sim-net`, `ps-sim-core`, `ps-physics`, `ps-movement`, `ps-weapons`, `ps-rules`, `ps-transport` ; tests : `ps-server` |
| `ps-client` | binaire `Project-S` (Bevy + egui) | `ps-netclient`, `ps-server` sans `admin` (jeu local), `ps-engine-character`, briques de simulation |

## Où chercher

| Tâche | Fichier(s) | Doc | Tests |
|---|---|---|---|
| Connexion, horloge, envoi et réception | `ps-client/src/net.rs`, `ps-netclient/src/lib.rs` | `docs/client.md` | `ps-netclient/tests/e2e.rs` |
| Prédiction, réconciliation, interpolation | `ps-netclient/src/lib.rs` (la simulation rejouée est `ps-sim-net::predict`, Project-s-simulation) | [network.md](https://github.com/SforSkill/Project-s-netcode/blob/main/docs/network.md) | `ps-netclient/tests/e2e.rs` |
| Entrées sub-tick | `ps-netclient/src/input.rs` | idem | idem |
| Serveur local embarqué | `ps-client/src/net.rs` (`ps_server::spawn_local`) | `docs/client.md` | `--autotest` |
| Rendu, caméra, effets, viewmodel | `ps-client/src/game.rs`, `world.rs` | `docs/client.md` | `--autotest` |
| Menus, HUD, réglages, raccourcis, inspecteur de tirs | `ps-client/src/ui.rs`, `settings.rs` | `docs/client.md`, `QUICKSTART.md` | idem |
| Son | `ps-client/src/audio.rs` | `docs/client.md` | — |
| Personnages en jeu (équipes, arme tenue, flux réseau) | `ps-client/src/character/` (`runtime.rs` = entrée, `settings.rs`, `mod.rs::hold_for`) | `docs/client.md` | `character/hitbox_fit.rs`, `settings.rs`, `--character-demo` |
| Démo des personnages | `ps-client/src/demo.rs` | `docs/client.md` | `--character-demo --shots` |
| Autotest | `ps-client/src/autotest.rs` | `docs/client.md` | `docs/results/autotest/` |
| Archives de release | `tools/package.py`, `.github/workflows/release.yml` | `SCOPE.md` | job `linux` de `release.yml` |

Hors de ce dépôt : valeurs de gameplay et cartes (Project-s-content), règles et simulation (Project-s-simulation), squelette, maillage, animation et shader des personnages (Project-s-engine), serveur dédié (Project-s-server), transport chiffré (Project-s-netcode), laboratoire réseau (Project-s-lab).

## Fichiers volumineux

À lire par plage, jamais en entier : `ps-client/src/ui.rs` (~1 350 lignes), `game.rs` (~1 240), `demo.rs` (~970), `character/runtime.rs` (~830), `ps-netclient/src/lib.rs` (~860). Hors contexte : `Cargo.lock`, `docs/results/`, `THIRD_PARTY_LICENSES.md` (généré par `tools/package.py third-party`).

Mettre cette carte à jour dans la même PR qu'un ajout ou un déplacement de module.
