Première alpha jouable de Projet S, un FPS tactique compétitif écrit en Rust.

**Archives**
- `Project-S-Windows-x64.zip` : le client Windows, avec un serveur intégré pour
  jouer en local contre des bots ;
- `Project-S-Linux-x64.tar.gz` : le client Linux, construit en CI mais pas
  encore testé sur une vraie machine ;
- `Project-S-Dedicated-Server-Linux-x64.tar.gz` : le serveur dédié 128 Hz,
  livré avec un fichier de configuration d'exemple et une unité systemd.

Chaque archive a un fichier `.sha256`.

**Pour commencer** : lire `QUICKSTART.md`, inclus dans chaque archive.

**Ce qui est fait et ce qui manque** :
- `docs/status.md` ;
- les mesures réelles sont dans `docs/validation-report.md`.

Version alpha : les exécutables ne sont pas signés. Le jeu n'a pas encore été
testé avec des joueurs humains sur Internet.
