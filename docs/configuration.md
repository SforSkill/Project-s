# Configuration

Tout ce qui se règle sans recompiler. Les fichiers de données (`configs/`, `maps/`) sont trouvés automatiquement, ou via `--data <dossier>` (client et serveur) ou `data_dir` (serveur). `--check` valide les données et quitte.

| Fichier | Contenu | Doc |
|---|---|---|
| `configs/movement.toml` | constantes de déplacement (unités Source, secondes) : vitesses, accélération, friction, gravité, saut, marches, contre-strafe | [`movement.md`](movement.md) |
| `configs/weapons.toml` | `[equipment]` (armures, kit, limite de grenades) et `[[weapon]]` : prix, récompense, cadence, recul, dispersion, dégâts, pénétration, lunette, couteau | [`weapons.md`](weapons.md) |
| `configs/materials.toml` | matériaux de surface : pénétration, perte de dégâts, couleur, son (l'index 0 doit être `clip`) | [`weapons.md`](weapons.md) |
| `configs/rules.toml` | profils de match (format, durées, économie, bombe, prolongations) | [`rules.md`](rules.md) |
| `maps/*.ron` | cartes générées par `cargo run -p mapgen` : ne pas éditer à la main | [`maps.md`](maps.md) |
| `server.toml` (modèle : `deploy/server.example.toml`) | serveur dédié : adresses, carte, profil, tick, clients, bots, replays, MOTD ; chaque clé est aussi une option `--clé valeur` | [`deployment.md`](deployment.md), [`bots.md`](bots.md) |
| `settings.toml` du joueur | réglages du client : `%APPDATA%\ProjetS\` (Windows), `$XDG_CONFIG_HOME/ProjetS/` ou `~/.config/ProjetS/` (Linux) | [`client.md`](client.md) |

`configs/local*.toml` est ignoré par git : on peut y garder des essais locaux.

## Secrets

Aucun secret dans ces fichiers. Le jeton de l'API d'administration du serveur vient de la variable d'environnement `PROJET_S_ADMIN_TOKEN` ([`deployment.md`](deployment.md)).

## Unités

1 u = 2,54 cm (unités Source). Temps en secondes, angles en degrés. Sensibilité compatible CS:GO (`m_yaw` = `m_pitch` = 0,022).
