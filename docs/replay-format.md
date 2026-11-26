# Format des replays (`.psrp`)

Un serveur lancé avec `--replays DOSSIER` enregistre un fichier par match. Le
serveur embarqué du client enregistre dans `<dossier de config>/replays`.

## Conteneur (`engine-replay`)

```text
"PSRP" | u32 version | u32 header_len | header
       | { u32 len | u32 fnv1a(payload) | payload }*
```

Tous les entiers sont en little-endian. Chaque trame porte sa propre somme de
contrôle : un fichier coupé net, par exemple lors d'un plantage, se lit jusqu'à
la dernière trame complète.

## Contenu (`game_sim::record`)

Un replay enregistre les **entrées** de la simulation, pas son état :
- l'en-tête (`ReplayHeader`) contient la version du jeu, la carte, le profil et
  les règles, la configuration de simulation, la graine, l'empreinte des données
  (`GameData::fingerprint`) et l'heure de début ;
- chaque trame contient les entrées d'un tick : commandes des joueurs,
  arrivées et départs, commandes d'administration.

La simulation est déterministe pour une graine et un flux d'entrées donnés,
bots compris. Rejouer les entrées sur une `Sim` neuve, avec les mêmes données,
reproduit donc le match à l'identique. Le test `replay_reproduces_the_match`
(dans `crates/game-sim/tests/sim.rs`) le vérifie.

## Limites

- L'alpha n'a **pas de lecteur de replay** dans le client : le format,
  l'enregistrement et la relecture existent, mais seuls les tests les utilisent.
- Si les données du jeu (`configs/`, `maps/`) changent, les anciens replays ne
  sont plus rejouables. L'empreinte de l'en-tête permet de le détecter.
- Les fichiers ne sont pas signés (voir `docs/security.md`).
- L'en-tête contient la graine du match, qui fixe la dispersion des tirs : un
  replay ne doit être diffusé qu'une fois le match terminé.
