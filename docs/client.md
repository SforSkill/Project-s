# Client (`ps-client`)

Binaire `Project-S` (Bevy 0.20 + egui). Il ne contient aucune règle de jeu : il échantillonne les entrées, les envoie par `ps-netclient` et affiche l'état reçu. Jouer seul lance un serveur embarqué (`ps_server::spawn_local`, Project-s-server compilé sans la feature `admin`) sur 127.0.0.1, avec la même boucle 128 Hz que le serveur dédié. Commandes et réglages côté joueur : [`QUICKSTART.md`](../QUICKSTART.md).

## Options de lancement

| Option | Effet |
|---|---|
| `--version` | affiche la version et quitte |
| `--data <dossier>` | dossier Project-s-content (`configs/`, `maps/`) ; sinon `PS_CONTENT_DIR`, le dossier de l'exécutable ou un dépôt `Project-s-content` voisin |
| `--check` | valide les données (nombre de cartes, empreinte) et quitte |
| `--connect <adresse>` | ouvre le menu « Rejoindre » sur cette adresse |
| `--character-demo` | scène de test des personnages « peinture liquide » ([personnages](#personnages)) |
| `--autotest …` | partie locale scriptée avec captures et rapport JSON ([autotest](#autotest)) |

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
| `character/` | intégration des personnages `ps-engine-character` : branchement ECS (`runtime.rs`, alimenté par `game.rs` via `CharacterFeed`), couleurs d'équipe (`settings.rs`), arme tenue (`hold_for`), test `hitbox_fit.rs` ; voir [personnages](#personnages) |
| `demo.rs` | `--character-demo` : scène de test des personnages |

Les touches se changent dans la section `[keys]` de `settings.toml`, pas encore depuis le menu.

## Personnages

Le squelette, le maillage, l'animation, le matériau liquide, les palettes et les effets viennent de `ps-engine-character` ([Project-s-engine `docs/characters.md`](https://github.com/SforSkill/Project-s-engine/blob/main/docs/characters.md) : garanties compétitives, performance, démo, limites). Le client y ajoute :

- `character/runtime.rs` : un personnage par joueur visible, nourri par l'état réseau (`CharacterFeed`) ; bras et arme en première personne sur le viewmodel ;
- `character/settings.rs` : réglages **Paramètres → Personnages** (qualité, mode de couleur « Par camp » ou « Alliés / ennemis », deux palettes), deux équipes n'ont jamais la même palette ;
- `character/hitbox_fit.rs` : test qui vérifie que plus de 90 % de la peau reste à moins de 3 unités des hitboxes `ps-hitreg` (debout, accroupi, course, pistolet).

```
Project-S.exe --character-demo
Project-S.exe --character-demo --shots <dossier>   # captures + report.json, puis quitte
```

## Autotest

Partie locale (ou distante avec `--connect`) jouée avec des entrées scriptées sur une vraie fenêtre et un vrai GPU : déplacement, rotation, tir, inspecteur de tirs ; captures d'écran puis `autotest-report.json` (images : nombre, FPS moyen, p50, p99, max ; réseau : snapshots, pertes, RTT, gigue, interpolation, tampon de commandes, corrections, événements urgents, erreurs de décodage ; joueurs distants affichés).

```sh
target/release/Project-S --autotest --map ps_citadel --mode deathmatch --bots 5 --seconds 25 --out autotest-dm
target/release/Project-S --autotest --map ps_citadel --mode competitive --bots 9 --seconds 25 --out autotest-comp
target/release/Project-S --autotest --connect 127.0.0.1:27015 --seconds 25 --out autotest-net
```

La première exécution après un build compile les shaders : ses temps maximaux sont plus élevés. Résultats de référence : [`results/autotest/`](results/autotest/) ; méthode complète de validation : [Project-s-docs `docs/validation.md`](https://github.com/SforSkill/Project-s-docs/blob/main/docs/validation.md).
