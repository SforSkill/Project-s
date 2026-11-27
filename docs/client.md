# Client (`game-client`)

Binaire `Project-S` (Bevy 0.20 + egui). Il ne contient aucune règle de jeu : il échantillonne les entrées, les envoie par `game-netclient` et affiche l'état reçu. Jouer seul lance un serveur embarqué (`game_server::spawn_local`) sur 127.0.0.1, avec la même boucle 128 Hz que le serveur dédié. Commandes et réglages côté joueur : [`QUICKSTART.md`](../QUICKSTART.md).

## Options de lancement

| Option | Effet |
|---|---|
| `--version` | affiche la version et quitte |
| `--data <dossier>` | dossier contenant `configs/` et `maps/` |
| `--check` | valide les données (nombre de cartes, empreinte) et quitte |
| `--connect <adresse>` | ouvre le menu « Rejoindre » sur cette adresse |
| `--character-demo` | scène de test des personnages « peinture liquide » ([`characters.md`](characters.md)) |
| `--autotest …` | partie locale scriptée avec captures et rapport JSON ([`validation.md`](validation.md)) |

## Ordre d'une image (`main.rs`)

| Étape Bevy | Systèmes | Fichier |
|---|---|---|
| `Startup` | caméras, sons synthétisés | `game.rs`, `audio.rs` |
| `PreUpdate` | réception réseau | `net.rs` |
| `Update` / `Input` | entrées, puis autotest | `game.rs`, `autotest.rs` |
| `Update` / `Send` | envoi des `UserCmd` | `net.rs` |
| `Update` / `Present` | carte, scène (joueurs, grenades, fumées, feux, bombe, traceurs), personnages et effets liquides, événements, caméra, volume, fenêtre (F11 / Alt+Entrée) | `game.rs`, `world.rs`, `character/` |
| `EguiPrimaryContextPass` | HUD puis menus | `ui.rs` |
| `Last` | limite d'images, arrêt propre | `main.rs` |

Écrans : `Screen::Menu` et `Screen::Game`. Ressources principales : `Net` (session et `NetClient`), `Hud`, `Scene`, `Settings`, `MenuState`.

## Modules

| Fichier | Rôle |
|---|---|
| `net.rs` | connexion à un serveur distant ou embarqué, horloge, envoi/réception |
| `game.rs` | présentation en jeu : entrées, caméra, joueurs distants, effets, viewmodel |
| `world.rs` | maillage de la carte (un par matériau), lumières, ciel |
| `ui.rs` | menu principal, HUD, tableau des scores, achat, discussion, réglages, graphe réseau, inspecteur de tirs (F3) |
| `settings.rs` | réglages du joueur (`settings.toml`) : sensibilité, FOV, viewmodel, réticule, touches `[keys]`, son, images, réseau, échelle de l'interface |
| `audio.rs` | sons procéduraux (aucun asset tiers), spatialisés |
| `autotest.rs` | test de bout en bout d'un build sur une vraie machine |
| `character/` | personnages « peinture liquide » : squelette (`rig.rs`), maillage procédural (`mesh.rs`), animation (`anim.rs`), matériau et shader (`liquid.rs`, `liquid.wesl`), effets cosmétiques (`fx.rs`), palettes et qualité (`style.rs`), armes tenues (`weapon_mesh.rs`), branchement ECS (`runtime.rs`, alimenté par `game.rs` via `CharacterFeed`) ; détail : [`characters.md`](characters.md) |
| `demo.rs` | `--character-demo` : scène de test des personnages |

Les touches se changent dans la section `[keys]` de `settings.toml`, pas encore depuis le menu.
