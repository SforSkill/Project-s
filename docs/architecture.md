# Architecture

## Principe

Une seule simulation autoritaire (`game-sim`), en Rust pur et sans E/S, est
exécutée :
- par le serveur dédié (`projet-s-server`) ;
- par le serveur embarqué du client pour le jeu local (même boucle UDP 128 Hz
  sur 127.0.0.1) ;
- partiellement par le client pour prédire son propre déplacement (`game_sim::predict`),
  avec exactement le même code de mouvement et de collision.

Le rendu (Bevy 0.20 + egui) ne contient aucune règle de jeu : il affiche l'état
fourni par `game-netclient` et produit des `UserCmd`.

```text
            ┌──────────────── client (Project-S) ────────────────┐
 souris ──▶ │ game-client (Bevy) ──UserCmd──▶ game-netclient      │
 clavier    │   ▲ rendu/HUD/son      prédiction, interpolation,   │
            │   └──────── état ───── extrapolation, horloge       │
            └───────────────────────────────┬────────────────────┘
                                            │ UDP chiffré (engine-network)
            ┌───────────────────────────────▼────────────────────┐
            │ game-server : boucle 128 Hz, PIM, snapshots delta, │
            │ flux urgent, admin HTTP, replays                   │
            │      └── game-sim : Sim::step (mouvement, armes,   │
            │            hitreg compensée, grenades, fumée,      │
            │            bombe, bots, game-rules)                │
            └────────────────────────────────────────────────────┘
```

## Crates

| Crate | Rôle | Dépend de Bevy |
|---|---|---|
| `engine-core` | types communs (`UserCmd`, `SimTime` sous-tick, angles, équipes, chargement TOML/RON) | non |
| `engine-physics` | monde de collision par briques convexes, `trace_hull` balayé, matériaux, épaisseurs | non |
| `engine-movement` | déplacement Source (friction, accélération, air-strafe, marches, contre-strafe) | non |
| `engine-weapons` | armes, recul à motif fixe, dispersion uniquement en mouvement, dégâts, pénétration | non |
| `engine-hitreg` | capsules, historique des poses, compensation de latence bornée | non |
| `engine-visibility` | gestion d'intérêt prédictive (PIM) anti-wallhack | non |
| `engine-network` | transport UDP chiffré sans E/S, 4 canaux, simulateur de lien | non |
| `engine-replay` | conteneur de replay auto-vérifié | non |
| `game-rules` | machine d'état des manches et de l'économie | non |
| `game-sim` | simulation complète, bots, protocole de jeu, enregistrement | non |
| `game-server` | serveur sans E/S + boucle UDP + binaire dédié (admin, métriques) | non |
| `game-netclient` | réseau client sans E/S | non |
| `game-client` | jeu (fenêtre, rendu, audio, interface) | **oui** |

Le serveur dédié compile sans Bevy : l'image Docker fait 119 Mo, binaire et
système de base compris.

## Temps

- Tick serveur fixe, 128 Hz par défaut (`--tick`). Le laboratoire réseau mesure aussi 256 Hz.
- Les entrées sont horodatées en sous-tick : `ButtonEvent.frac` vaut 0 à 255
  dans la commande. La cadence de tir et le contre-strafe ne dépendent donc ni
  du FPS ni du tick.
- `UserCmd::view_time` indique le temps de rendu que voyait le joueur. Le serveur
  rembobine les hitboxes à ce temps, dans la limite de `LagCompPolicy`
  (200 ms maximum, tolérance de 50 ms).

## Décisions

Voir `docs/decisions/` (ADR).
