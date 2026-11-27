# Glossaire

| Terme | Sens dans Projet S | Où |
|---|---|---|
| Tick | un pas de simulation ; 128 par seconde par défaut (`TickRate`) | `engine-core/src/tick.rs` |
| Sub-tick | horodatage d'une entrée à l'intérieur d'un tick (`SimTime` fractionnaire) : un tir part à l'instant exact du clic | `engine-core/src/cmd.rs`, `game-netclient/src/input.rs` |
| `UserCmd` | ce qu'un client demande pour un tick (boutons, angles, segments) | `engine-core/src/cmd.rs` |
| Sans E/S (sans-IO) | code qui reçoit des datagrammes et une horloge et rend des datagrammes, sans socket : testable de façon déterministe | `engine-network`, `game-server/src/server.rs`, `game-netclient` |
| Autoritaire | seul le serveur décide de l'état du jeu ; le client prédit et se corrige | `game-sim` |
| Prédiction | le client rejoue localement son propre déplacement avec le même code que le serveur | `game-sim/src/predict.rs` |
| Interpolation / extrapolation | affichage des autres joueurs entre deux snapshots reçus, ou au-delà du dernier | `game-netclient/src/lib.rs` |
| Snapshot | état du monde envoyé par le serveur, en delta | `game-sim/src/proto.rs` |
| Lag compensation | le serveur rejoue les hitboxes à l'instant vu par le tireur, dans une limite | `engine-hitreg` |
| PIM | gestion d'intérêt prédictive : un client ne reçoit que les joueurs qu'il peut voir bientôt (anti-wallhack) | `engine-visibility` |
| Brush | volume convexe défini par des plans ; brique de base des cartes et des collisions | `engine-physics/src/brush.rs` |
| Hull | boîte du joueur balayée contre le monde (`trace_hull`) | `engine-physics/src/world.rs` |
| Contre-strafe | appuyer sur la direction opposée pour s'arrêter net et tirer précis | `engine-movement`, `docs/movement.md` |
| Pénétration (wallbang) | balle qui traverse une surface selon son matériau et son épaisseur | `engine-weapons/src/damage.rs`, `configs/materials.toml` |
| MR12 / MR15 / MR16 | nombre maximal de manches : 24, 30 ou 32 | `configs/rules.toml` |
| Freeze time | début de manche, joueurs immobiles, achat possible | `game-rules` |
| `.psrp` | fichier de replay : entrées par tick, rejouées à l'identique | `docs/replay-format.md` |
| Netlab | laboratoire réseau : vrai serveur et vrais clients sur des liens simulés | `tools/netlab`, `docs/network-lab.md` |
| u | unité de distance Source, 1 u = 2,54 cm | — |
