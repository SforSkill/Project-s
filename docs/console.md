# Console développeur

Ouverte depuis le menu et en jeu avec la touche **Console** (réglable dans Paramètres, touches ; par défaut la touche physique sous Échap, `²` en AZERTY, `~` en QWERTY). Échap ou la même touche la ferme. Ouverte, elle bloque clavier et souris du jeu.

- Entrée exécute, ↑/↓ parcourent l'historique (100 lignes), TAB complète le nom de commande et liste les suggestions, `clear` efface. Copier-coller natif dans la ligne de saisie.
- Une commande de réglage sans argument affiche sa valeur, son rôle et ses bornes. Les nombres acceptent la virgule. Les booléens : `0/1`, `on/off`, `true/false`.
- Les réglages sont les mêmes champs que les menus (`settings.toml`) : un changement ici apparaît tout de suite dans le menu, et l'inverse. Les changements persistants sont sauvegardés immédiatement.
- Aucune commande client ne touche santé, vitesse, dégâts, hitboxes, tickrate, sanctions, droits ou positions : le serveur reste autoritaire. Les commandes serveur passent par `host`, qui envoie `ClientMsg::Host` ; le serveur décide.
- Code : `crates/ps-client/src/console/` (`mod.rs` registre et exécution, `commands.rs` commandes, `ui.rs` egui, `tests.rs`). Pour ajouter une commande : une entrée dans `commands::builtin()` (nom, alias, aide, arguments bornés, permission, persistance).

## Permissions

| Permission | Portée | Depuis un fichier `.cfg` |
|---|---|---|
| Client | console et session (`quit`, `connect`...) | refusée |
| Config | réglages du joueur | oui |
| Dev | diagnostics (compteur d'images, graphe réseau, ping) | oui |
| ServerAdmin | envoi d'une commande au serveur (session requise) | refusée |
| TestOnly | tests uniquement | refusée |

## Commandes

| Commande (alias) | Arguments | Description | Permission |
|---|---|---|---|
| `help` (`?`) | `[commande]` | liste, ou détail d'une commande | Client |
| `clear` (`cls`) | | efface la console | Client |
| `quit` (`exit`) | | quitte le jeu | Client |
| `disconnect` | | quitte la partie en cours | Client |
| `connect` | `<hôte[:port]>` | rejoint un serveur (adresse normalisée comme dans le menu) | Client |
| `retry` (`reconnect`) | | dernier serveur | Client |
| `version` | | version du client | Client |
| `status` | | connexion, ping, tick, images/s | Client |
| `host` | `<commande...>` | commande serveur via `ClientMsg::Host` | ServerAdmin |
| `exec` | `<nom>` | exécute `cfg/<nom>.cfg` | Config |
| `writeconfig` | `[nom]` | écrit les réglages dans `cfg/<nom>.cfg` (défaut `config`) | Config |
| `sensitivity` (`sensibility`) | `0.05..10` | sensibilité | Config |
| `m_yaw`, `m_pitch` | `0.001..0.1` | degrés par unité de souris | Config |
| `zoom_sensitivity_ratio` | `0.2..2` | sensibilité en zoom | Config |
| `volume` (`snd_master_volume`) | `0..1` | volume général | Config |
| `snd_game_volume` | `0..1` | armes, pas, grenades, bombe (un seul volume commun) | Config |
| `snd_ui_volume` | `0..1` | clics et retours de touche/mort | Config |
| `snd_mute` | `0/1` | coupe le son | Config |
| `snd_music_volume`, `snd_voice_volume` | | non disponible (ni musique ni voix) | Config |
| `fps_max` | `0` ou `30..1000` | limite d'images | Config |
| `cl_showfps` | `0/1` | compteur d'images (en partie) | Dev |
| `net_graph` | `0/1` | graphe réseau | Dev |
| `ping` | | latence | Dev |
| `viewmodel_fov` | `54..90` | FOV de l'arme | Config |
| `viewmodel_offset_x/y/z` | `-10..10` | position de l'arme | Config |
| `viewmodel_opacity` | | non supporté | Config |
| `cl_crosshairsize` | `0..30` | taille | Config |
| `cl_crosshairgap` | `-5..30` | écart | Config |
| `cl_crosshairthickness` | `0..10` | épaisseur | Config |
| `cl_crosshairdot`, `cl_crosshairoutline` | `0/1` | point, contour | Config |
| `cl_crosshaircolor` | `r g b` (0..255) | couleur | Config |

## Fichiers `.cfg`

Dossier `cfg/` de la configuration du joueur (à côté de `settings.toml`). Nom : lettres, chiffres, `_` et `-` (suffixe `.cfg` facultatif) ; `..`, séparateurs, chemins absolus et noms réservés Windows sont refusés. Une commande par ligne, `//` et `#` pour les commentaires. `exec` utilise le même registre, limité à 3 niveaux d'imbrication, 500 lignes par fichier, 64 Ko, 2000 commandes au total, et seules les commandes Config et Dev y passent.

## Audio

Trois volumes : général, jeu, interface. Gain = (volume)² pour chacun (courbe perceptuelle, le fichier stocke le curseur). Jeu : tirs, couteau, pas, atterrissage, grenades, rechargement, bombe. Interface : `Click`, `Beep`, retours touché, tête et mort.
