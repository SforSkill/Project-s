# État d'implémentation — v0.1.0-alpha

État du jeu tel qu'il est livré par ce client. Depuis la séparation en dépôts, chaque élément est maintenu par son dépôt responsable ([`README.md`](README.md)) ; ce tableau reste la vue d'ensemble côté joueur.

Les colonnes ne se recouvrent pas :
- **Implémenté** : le code existe et compile.
- **Testé** : vérifié par un test automatique, exécuté et réussi.
- **Validé** : observé en exécution réelle, avec une vraie fenêtre, un vrai GPU,
  un vrai réseau UDP ou un vrai conteneur.

Les mesures sont dans le [rapport de validation](https://github.com/SforSkill/Project-s-docs/blob/main/VALIDATION_REPORT.md).

## P0 — cœur jouable

| Élément | Implémenté | Testé | Validé | Notes |
|---|---|---|---|---|
| Compilation Windows (client + serveur) | oui | — | oui | `cargo build --release` local |
| Compilation Linux (client + serveur) | oui | CI | serveur : oui (Docker) | le client Linux n'a été construit qu'en CI, jamais lancé |
| Serveur dédié headless 128 Hz | oui | oui | oui | conteneur Linux, 2 clients réels |
| Multijoueur UDP chiffré | oui | oui | oui | 2 clients Windows vers serveur Linux (Docker) |
| Déplacement Source (air strafe, marches, pentes) | oui | oui | oui | |
| Sensibilité type Source (0,022) | oui | — | oui | pas de test automatique de conversion |
| Contre-strafe (2 modes) | oui | oui | — | tableau comparatif : [movement.md](https://github.com/SforSkill/Project-s-simulation/blob/main/docs/movement.md) |
| Armes : tir, recul fixe, dispersion en mouvement | oui | oui | oui | |
| Hitreg + compensation de latence bornée | oui | oui | oui | |
| Santé, mort, réapparition | oui | oui | oui | |
| Carte jouable | oui | oui | oui | 3 cartes |
| HUD (vie, armure, munitions, argent, chrono, kill feed, scores) | oui | — | oui | captures de l'autotest |
| Jeu local (serveur embarqué) | oui | oui | oui | |
| Bots | oui | oui | oui | navigation sur grille, visée avec temps de réaction |
| Deathmatch | oui | oui | oui | |
| Déploiement Docker | oui | CI | oui | image de 119 Mo, santé et 401 vérifiés |
| Déploiement systemd | oui | — | oui | VPS Debian 12, client connecté par Internet |

## P1 — contenu compétitif

| Élément | Implémenté | Testé | Validé | Notes |
|---|---|---|---|---|
| 6 types d'arme (couteau, pistolet, SMG, fusil, 2 fusils de précision avec lunette) | oui | oui | oui | |
| 4 grenades (flash, fumigène, molotov, explosive) | oui | fumigène, explosive | — | flash et molotov sans test dédié ; les bots n'en lancent pas |
| Fumigène hybride (voxel opaque, percé par l'explosive) | oui | oui | — | |
| Armure (gilet, casque) | oui | oui | — | |
| Tagging (ralentissement à l'impact) | oui | oui | — | |
| Pénétration par matériau et épaisseur | oui | oui | — | détail visible dans l'inspecteur de tirs |
| Économie (récompenses, bonus de défaite, achats) | oui | oui | — | menu d'achat (B) ; les bots achètent |
| Bombe (pose, désamorçage, kit) | oui | oui | — | test `bomb_plant_and_defuse` ; les bots savent poser et désamorcer |
| MR15, MR12, MR16, prolongations | oui | oui | MR15 : début de partie seulement | aucun match complet de 30 manches joué à la main |
| Carte compétitive (Citadelle : 2 sites, zones d'achat) | oui | navigation testée | oui | |
| Viewmodel | oui | — | oui | arme simplifiée (boîtes) |
| Personnages « peinture liquide » (modèle, animations, shader, effets, palettes, 4 qualités) | oui | tests unitaires + démo `--character-demo --shots` | captures de la démo seulement | voir [characters.md](https://github.com/SforSkill/Project-s-engine/blob/main/docs/characters.md) ; pas encore vu en partie multijoueur réelle |
| Paramètres (sensibilité, viewmodel, réticule, son, vidéo, touches) | oui | — | oui | — |
| 5v5 | oui | oui (bots) | oui (1 humain + 9 bots) | jamais avec 10 humains |

## P2 — fonctions avancées

| Élément | Implémenté | Testé | Validé | Notes |
|---|---|---|---|---|
| Flux urgent (tirs, dégâts, morts) | oui | oui | oui | 275 événements en compétitif, délai mesuré |
| Ordonnancement (128 Hz fixe, commandes horodatées en sous-tick) | oui | oui | oui | 0 tick en retard en conteneur |
| Visibilité prédictive (PIM) | oui | oui | oui | |
| Extrapolation adaptative bornée | oui | oui | laboratoire | |
| Inspecteur de tirs (F3) | oui | — | oui | |
| Laboratoire réseau A/B/C/D, CSV et JSON | oui | — | oui | 60 exécutions réelles, Project-s-lab (dépôt privé) |
| Benchmarks (criterion) | oui | — | oui | [rapport de validation](https://github.com/SforSkill/Project-s-docs/blob/main/VALIDATION_REPORT.md) |
| Replays : enregistrement et relecture déterministe | oui | oui | enregistrement : oui | **pas de lecteur dans le client** |

## P3 — exploitation

| Élément | Implémenté | Testé | Validé | Notes |
|---|---|---|---|---|
| API admin (`/health`, `/metrics`, `/status`, `/command`) | oui | CI (santé, 401) | oui | jeton Bearer, temps constant |
| Journaux JSON, métriques Prometheus | oui | — | oui | |
| Sécurité (chiffrement, cookie, autorité serveur) | oui | oui | — | faiblesses connues : [security.md](https://github.com/SforSkill/Project-s-server/blob/main/docs/security.md) |
| Helm | squelette | `helm lint`, `helm template` | non | aucun cluster |
| Agones | manifeste et doc | — | non | SDK non intégré |
| CI/CD GitHub Actions | oui | — | voir le rapport final | |
| Documentation | oui | — | — | `docs/` et dépôts responsables |

## Expérimental

- Variante réseau D (256 Hz) : elle fonctionne, mais double la bande passante.
- Mode contre-strafe `buffered` : c'est le mode par défaut ; il peut être
  désactivé (`counter_strafe = "source"`).
- Bots : ils jouent les manches, achètent, posent et désamorcent la bombe, mais
  ne lancent pas de grenades et n'ont pas de tactique d'équipe.

## Limites et travail restant

1. Aucun lecteur de replay, aucun mode spectateur libre hors de la mort.
2. Les cartes sont des primitives géométriques. Les joueurs sont des humanoïdes
   procéduraux animés (Project-s-engine), sans export glTF ni assets
   Blender.
3. Le client Linux est construit en CI, mais n'a jamais été lancé.
4. Le serveur tourne sur un VPS réel et un client s'y est connecté par Internet,
   mais aucune partie entre plusieurs humains n'a été jouée.
5. Le serveur n'est pas authentifié par une clé longue durée, et il n'y a pas
   d'anti-triche client.
6. Il n'y a pas de liste de serveurs, de matchmaking ni de comptes.
7. Le scénario du laboratoire ne produit aucune correction de prédiction : les
   variantes B et C n'y sont pas départagées.
