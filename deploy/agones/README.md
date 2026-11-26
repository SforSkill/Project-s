# Agones (squelette)

`fleet.yaml` décrit une flotte de serveurs Projet S pour [Agones](https://agones.dev).
**Statut : non testé sur un cluster.** Il sert de point de départ.

## Principe

Agones lance des pods `GameServer` à partir de la flotte. Un matchmaker *alloue*
un serveur prêt à une partie. Le serveur doit dialoguer avec le SDK Agones
(sidecar gRPC/HTTP sur `localhost:9358`) :

| Appel SDK | Moment | État dans Projet S 0.1.0-alpha |
|---|---|---|
| `Ready()` | carte chargée, port UDP ouvert | absent |
| `Health()` | toutes les quelques secondes | absent (on a `/health` HTTP à la place) |
| `Allocate()` / watch `GameServer` | début de match, configuration (carte, mode) | absent |
| `Shutdown()` | fin de match (`GameOver`) | absent |

## Manques à combler avant usage réel

1. Appels SDK : un petit client HTTP vers `http://localhost:${AGONES_SDK_HTTP_PORT}`
   (`POST /ready`, `POST /health`, `POST /shutdown`) dans `game-server/src/main.rs`,
   derrière une option `--agones`.
2. Fin de match : appeler `Shutdown()` à la fin d'un match compétitif pour que
   le pod soit recyclé.
3. Lire la carte et le mode depuis les labels/annotations du `GameServer`
   alloué (watch SDK) plutôt que des arguments fixes.

## Essai (cluster de test)

```sh
kubectl apply -f deploy/agones/fleet.yaml
kubectl get gameservers
```

Sans `Ready()`, les `GameServer` restent `Scheduled` : c'est le comportement
attendu tant que le point 1 n'est pas fait.
