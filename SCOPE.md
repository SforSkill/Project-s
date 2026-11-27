# Périmètre de Project-s

## 1. Mission
Livrer le jeu sur le poste du joueur : le client natif `Project-S` (Bevy + egui), qui affiche l'état reçu du serveur, envoie les entrées et lance un serveur local embarqué pour jouer seul ou contre des bots.

## 2. Responsabilités
| Crate | Contenu |
|---|---|
| `ps-client` | Binaire `Project-S` : point d'entrée, menus, HUD, interface, paramètres (`settings.toml`), entrées, connexion (serveur distant ou `ps-server` embarqué), rendu de la carte, caméra, viewmodel, son procédural, intégration des personnages `ps-engine-character` (`character/` : équipes, arme tenue, événements), inspecteur de tirs (F3), `--autotest`, `--character-demo` |
| `ps-netclient` | Réseau client sans E/S : connexion `ps-transport`, horloge, prédiction et réconciliation (`ps-sim-net::predict`), interpolation, extrapolation bornée, entrées sub-tick (`InputTimeline`) ; tests de bout en bout client ↔ serveur (`tests/e2e.rs`, `ps-server` en bibliothèque) |
| `assets/brand/` | Identité visuelle (logo, icônes, bannière) |
| `tools/package.py` | Archives de release du client (binaire + contenu épinglé + licences tierces) |

## 3. Ne contient jamais
Règle de jeu ou valeur de gameplay (simulation : Project-s-simulation ; données : Project-s-content), code du serveur dédié ou de son déploiement, accès à une base de données, logique officielle de comptes, matchmaking, classement ou sanctions, secret ou adresse de serveur réel.

## 4. Dépendances autorisées
Bevy 0.20 et `bevy_egui` ; Project-s-engine (`ps-engine-character`) ; Project-s-simulation (`ps-sim`, `ps-sim-net` et leurs briques) ; Project-s-netcode (`ps-transport`) ; Project-s-server (`ps-server` **sans** la feature `admin`, pour le jeu local) ; contenu Project-s-content lu à l'exécution.

## 5. Dépendances interdites
Clients SQL (`sqlx`, `postgres`, `diesel`), `ps-server` avec la feature `admin` (`axum`, `prometheus-client`). Vérifié par `boundaries.toml` en CI.

## 6. Interfaces publiques
Le binaire `Project-S` et ses options (`--data`, `--check`, `--connect`, `--character-demo`, `--autotest`, `--version`, [`docs/client.md`](docs/client.md)) ; `settings.toml` du joueur ; `ps-netclient::{NetClient, ClientConfig, input::InputTimeline}`.

## 7. Contrats
- Serveur : messages `ps-sim-net::proto` sur `ps-transport` ; le client et le serveur partagent `PROTOCOL_VERSION` et l'empreinte du contenu.
- Contenu : dossier Project-s-content à la version de `.content-version` (`--data`, `PS_CONTENT_DIR`, à côté de l'exécutable ou dépôt voisin).
- Personnages : purement cosmétiques, ils ne lisent que l'état réseau ; les poses du moteur restent dans les hitboxes de `ps-hitreg` (test `character/hitbox_fit.rs`).
- Replays : format `ps-replay` (Project-s-protocol), enregistrés par le serveur.

## 8. Performance
Aucune règle de jeu dans le rendu ; la prédiction rejoue exactement `Sim::run_cmd`. Limite d'images et V-Sync réglables ; le serveur embarqué garde sa boucle 128 Hz dans son propre fil.

## 9. Sécurité
Le client n'a aucune autorité : le serveur valide tout. Transport chiffré (`ps-transport`) ; le serveur embarqué écoute sur 127.0.0.1 et n'a pas d'API d'administration. Pas d'anti-triche client. Modèle de menace : [Project-s-server `docs/security.md`](https://github.com/SforSkill/Project-s-server/blob/main/docs/security.md).

## 10. Compilation
`cargo build --release -p ps-client` (Linux : `libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`). Contenu : cloner Project-s-content à côté du dépôt.

## 11. Tests
`cargo test --workspace --all-features --release` (bout en bout client ↔ serveur dans `ps-netclient`, `hitbox_fit` et réglages dans `ps-client`) ; `--autotest` et `--character-demo --shots` sur une vraie machine ([`docs/client.md`](docs/client.md)).

## 12. Versionnement
SemVer pré-release (`0.1.0-alpha.N`), tag `vX.Y.Z` par release-please, `Cargo.lock` versionné et produit sans patch local. Les dépôts dépendus sont épinglés par tag dans `Cargo.toml`, le contenu par `.content-version`.

## 13. Déploiement
Archives de release Windows et Linux (`.github/workflows/release.yml`, `tools/package.py`) avec le contenu épinglé. Le serveur dédié est livré par Project-s-server.

## 14. Limites connues
- Pas de lecteur de replay ni de mode spectateur libre ; touches modifiables seulement dans `settings.toml`.
- Le client Linux est construit en CI mais n'a jamais été lancé sur une machine avec écran.
- Une partie de la présentation (caméras, viewmodel, HUD, audio, maillage des cartes) reste mêlée à l'état du jeu ; elle rejoindra Project-s-engine module par module.
