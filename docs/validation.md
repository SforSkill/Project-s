# Reproduire la validation

Chaque chiffre de `docs/validation-report.md` vient d'une des commandes ci-dessous.

## Contrôles du code

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --release --locked
cargo run --release -p game-server -- --data . --config deploy/server.example.toml --check
```

Le test `bot_match_report` est ignoré par défaut ; c'est un outil de diagnostic.
Pour le lancer : `cargo test --release -p game-sim --test sim bot_match_report -- --ignored --nocapture`.

## Autotest du client (vraie fenêtre, vrai GPU)

```sh
target/release/Project-S --autotest --map ps_citadel --mode deathmatch --bots 5 --seconds 25 --out autotest-dm
target/release/Project-S --autotest --map ps_citadel --mode competitive --bots 9 --seconds 25 --out autotest-comp
target/release/Project-S --autotest --connect 127.0.0.1:27015 --seconds 25 --out autotest-net
```

Le client joue avec des entrées scriptées : il se déplace, tourne, tire et
ouvre l'inspecteur. Il prend des captures d'écran, puis écrit
`autotest-report.json` et quitte. Le rapport contient :
- les images : nombre, FPS moyen, p50, p99 et max ;
- le réseau : snapshots, pertes, RTT, gigue, interpolation, tampon de
  commandes, corrections, événements urgents, erreurs de décodage ;
- le nombre maximal de joueurs distants affichés.

La première exécution après un build compile les shaders : ses temps de frame
maximaux sont plus élevés.

## Serveur Linux (Docker) et deux clients

```sh
docker build -f deploy/docker/Dockerfile -t projet-s-server:0.1.0-alpha .
docker run -d --name ps -p 27015:27015/udp -p 127.0.0.1:27016:27016 \
  projet-s-server:0.1.0-alpha --map ps_citadel --mode deathmatch --bots 4
curl -s localhost:27016/health; curl -s -o /dev/null -w "%{http_code}\n" localhost:27016/status
target/release/Project-S --autotest --connect 127.0.0.1:27015 --seconds 25 --out mp-a &
target/release/Project-S --autotest --connect 127.0.0.1:27015 --seconds 25 --out mp-b
curl -s localhost:27016/metrics | grep projet_s_
docker rm -f ps
```

## Laboratoire réseau

Voir `docs/network-lab.md`.

## Benchmarks

```sh
cargo bench -p engine-physics -p engine-movement -p engine-hitreg -p game-sim
```
