# Laboratoire réseau A/B/C/D

`tools/netlab` exécute dans un seul processus le **vrai serveur** et **deux vrais
clients** (`game-server`, `game-netclient`), reliés par des liens simulés
déterministes (`engine_network::linksim`). Chaque valeur écrite dans les
résultats provient d'une exécution : rien n'est estimé ni extrapolé.

```sh
cargo run --release -p netlab -- --seconds 30 --seeds 3 --out docs/results/netlab
# NETLAB_ONLY=bad:D   ne lance qu'un couple profil:variante
# NETLAB_DEBUG=1      état des clients en fin d'exécution ; NETLAB_TRACE=1 : trace 4 fois par seconde
```

Sorties : `netlab.csv`, avec une ligne par exécution, et `netlab.json`, qui
contient les exécutions plus une moyenne par couple profil × variante.

## Scénario

- Carte `ps_duel_lab`, mode deathmatch, sans bots. Le filtrage de visibilité est
  coupé : les deux joueurs doivent rester répliqués pour qu'on mesure ce que
  l'un voit de l'autre.
- Le joueur 0 fait des allers-retours latéraux. Il change de sens à des
  intervalles aléatoires de 0,15 à 0,8 s, comme lors d'un peek.
- Le joueur 1 est immobile et observe.
- Les mesures commencent après 4 s d'échauffement, à chaque frame client (240 Hz).

| Mesure | Définition |
|---|---|
| `view_error` | distance entre la position du joueur 0 affichée chez le joueur 1 et sa position réelle sur le serveur **au même instant** : coût de la latence et de l'interpolation |
| `rewind_error` | distance entre la position affichée et la position serveur au `view_time` que le client déclare dans ses commandes, c'est-à-dire là où la compensation de latence testera un tir (« j'avais le viseur dessus ») |
| `extrapolated_pct` | part des frames où le joueur distant est extrapolé |
| `corrections`, `own_snap_max` | corrections de prédiction du joueur 0 ; plus grand saut visible de sa propre vue |
| `missing_cmds` | commandes arrivées trop tard pour leur tick (comptées sur le serveur, après l'échauffement) |
| `down_kbps`, `up_kbps` | débit par client, en-têtes IP/UDP de 28 octets inclus |
| `server_tick_us_*` | durée d'un tick serveur (2 joueurs, sans bots) |

Profils de lien (latence aller simple / gigue / pertes / doublons) :

| Profil | Latence | Gigue | Pertes | Doublons |
|---|---|---|---|---|
| lan | 1 ms | 0,5 ms | 0 % | 0 % |
| good | 15 ms | 2 ms | 0 % | 0 % |
| average | 35 ms | 8 ms | 1 % | 0 % |
| bad | 60 ms | 25 ms | 5 % | 1 % |
| harsh | 80 ms | 40 ms | 10 % | 2 % |

## Résultats (v0.1.0-alpha)

Moyenne sur 3 graines, 30 s simulées par exécution, soit 60 exécutions.
Distances en unités de jeu : 1 u = 1 pouce, un joueur fait 32 u de large.
Fichiers bruts : `docs/results/netlab/`.

| Profil | Var. | view p95 | rewind p95 | rewind max | extrap. % | corr. | kbit/s ↓ | tick µs |
|---|---|---|---|---|---|---|---|---|
| lan | A | 4.61 | 0.061 | 0.070 | 0.00 | 0 | 291 | 6.2 |
| lan | B | 4.61 | 0.061 | 0.070 | 0.00 | 0 | 291 | 6.4 |
| lan | C | 4.61 | 0.061 | 0.070 | 0.00 | 0 | 291 | 6.3 |
| lan | D | 3.17 | 0.060 | 0.070 | 0.00 | 0 | 582 | 6.5 |
| good | A | 7.50 | 0.061 | 0.070 | 0.00 | 0 | 292 | 6.3 |
| good | B | 7.50 | 0.061 | 0.070 | 0.00 | 0 | 292 | 6.4 |
| good | C | 7.50 | 0.061 | 0.070 | 0.00 | 0 | 292 | 7.4 |
| good | D | 6.08 | 0.061 | 0.127 | 0.00 | 0 | 582 | 6.4 |
| average | A | 12.97 | 0.061 | 0.097 | 0.00 | 0 | 292 | 6.6 |
| average | B | 12.97 | 0.061 | 0.097 | 0.00 | 0 | 292 | 6.5 |
| average | C | 12.97 | 0.061 | 0.097 | 0.00 | 0 | 292 | 6.4 |
| average | D | 11.43 | 0.061 | 0.098 | 0.00 | 0 | 583 | 6.4 |
| bad | A | 20.70 | 0.070 | 0.396 | 0.00 | 0 | 292 | 6.4 |
| bad | B | 20.70 | 0.069 | 0.305 | 0.10 | 0 | 292 | 6.5 |
| bad | C | 20.70 | 0.069 | 0.305 | 0.10 | 0 | 292 | 6.8 |
| bad | D | 18.38 | 0.066 | 0.966 | 0.00 | 0 | 583 | 6.6 |
| harsh | A | 26.41 | 0.113 | 2.037 | 0.00 | 0 | 292 | 6.4 |
| harsh | B | 26.41 | 0.109 | 0.593 | 0.68 | 0 | 292 | 6.5 |
| harsh | C | 26.41 | 0.109 | 0.593 | 0.68 | 0 | 292 | 6.7 |
| harsh | D | 23.65 | 0.088 | 1.783 | 0.00 | 0 | 583 | 6.5 |

Commandes manquantes après l'échauffement, sur les 60 exécutions : entre 0 et 5.

## Lecture

- **Le hitreg reste cohérent quel que soit le lien.** Entre ce que le joueur voit
  et ce que le serveur rembobine, l'écart au p95 va de 0,06 à 0,11 unité, soit
  moins de 3 mm. C'est bien moins que le rayon d'une tête. Tant que la
  compensation n'est pas bornée (200 ms), un tir placé sur la cible affichée est
  testé à cet endroit.
- **`view_error` est le prix de la latence**, pas une erreur du hitreg : l'adversaire
  est affiché là où il était il y a RTT/2 plus `interp`. Ce prix croît avec la
  latence, de 4,6 u en LAN à 26 u sur le profil harsh, au p95.
- **D (256 Hz)** réduit `view_error` de 10 à 30 % : le délai d'interpolation est
  plus court en temps. En contrepartie, la bande descendante double (582 contre
  291 kbit/s). Sur les profils dégradés, son `rewind max` est moins bon que celui
  de B/C, car elle n'extrapole pas.
- **B et C (extrapolation)** ne changent rien tant que les snapshots arrivent à
  temps : l'extrapolation ne se déclenche jamais en LAN, good ou average. Sur bad
  et harsh, elle divise l'écart maximal par 1,3 à 3,4 (harsh : de 2,04 à 0,59 u).
- **B et C donnent des chiffres identiques ici.** C ne diffère de B que par le
  lissage des corrections de prédiction, et ce scénario n'en produit aucune
  (aucun contact ni obstacle). La différence se verrait dans un scénario avec
  collisions entre joueurs, qui n'est pas encore écrit.

## Correctifs issus du laboratoire

Les premières exécutions ont révélé deux défauts. Ils sont corrigés et les
chiffres ci-dessus sont postérieurs aux corrections.

1. Sur le profil bad, environ 34 % des commandes arrivaient trop tard et le
   client se resynchronisait 30 fois en 10 s. Deux causes :
   - le serveur annonçait un tampon de −1000 tant qu'il n'avait reçu aucune
     commande ;
   - le tampon cible était fixe (2 ticks), plus court que la gigue montante.

   Correctifs :
   - valeur « inconnu » (`i16::MIN`) ;
   - cible adaptative à la gigue ;
   - les snapshots antérieurs à une resynchronisation sont ignorés.
2. À 256 Hz sur lien dégradé, le client ne recevait jamais de retour d'horloge :
   le serveur ne comptait que les commandes acceptées, donc aucune quand elles
   arrivaient toutes en retard. Le retour d'horloge utilise désormais la
   commande la plus récente du dernier paquet, même si elle est en retard.

## Limites

- Un seul scénario de mouvement, deux clients, pas de bots : la charge serveur
  avec 10 joueurs est mesurée ailleurs (`VALIDATION_REPORT.md`).
- Les liens sont simulés (pertes indépendantes, gigue uniforme). Les vraies
  connexions ont des pertes en rafales et des changements de route.
