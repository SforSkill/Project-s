# Réseau

## Transport (`engine-network`)

- **UDP** uniquement, paquets de 1200 octets au plus, sans fragmentation IP.
  L'en-tête en clair fait 13 octets (type, identifiant de connexion, séquence
  64 bits), suivi de 16 octets de tag.
- **Poignée de main** en 4 messages :
  1. HELLO, complété à 512 octets ;
  2. CHALLENGE, réponse sans état serveur, avec un cookie ;
  3. CONNECT, complété lui aussi ;
  4. ACCEPT.

  Le complément (padding) fait que le serveur ne répond jamais plus gros que ce
  qu'il reçoit (pas d'amplification). Le cookie bloque les adresses usurpées.
- **Chiffrement** :
  - échange de clés X25519 éphémère ;
  - dérivation HKDF-SHA256, une clé par sens ;
  - ChaCha20-Poly1305 sur chaque paquet, avec un nonce dérivé de la séquence ;
  - fenêtre anti-rejeu.
- **Canaux** :

| Canal | Livraison | Usage |
|---|---|---|
| Input | non fiable, dernier gagne | commandes (chacune répétée dans 4 paquets successifs) |
| Snapshot | non fiable | état du monde, en delta par rapport au dernier snapshot acquitté |
| Urgent | répété jusqu'à acquittement, dédoublonné | tirs, dégâts, morts, explosions : envoyés **immédiatement**, hors snapshot |
| Reliable | fiable, ordonné | état du match, discussion, volumes de fumée, joueurs |

## Protocole de jeu (`game_sim::proto`)

- Les messages de contrôle sont encodés avec postcard (serde).
- Les snapshots utilisent un format binaire écrit à la main :
  - positions quantifiées au 1/8 d'unité ;
  - vitesses en i16 ;
  - lacet sur 16 bits ;
  - deltas par rapport à une base acquittée (une entité identique à la base coûte 1 octet).

  Moyenne mesurée : 224 à 231 octets par snapshot en deathmatch avec 5 joueurs
  visibles, et 415 à 513 octets en compétitif 5v5 (`VALIDATION_REPORT.md`).
- Chaque snapshot contient `cmd_buffer` : la différence entre la commande la plus
  récente reçue et le tick simulé. `i16::MIN` signifie qu'aucune commande n'a
  encore été reçue.

## Horloge et prédiction (`game-netclient`)

- Le client produit ses commandes pour `pred_tick`, en avance sur le serveur.
  Il accélère ou ralentit son horloge d'au plus 5 % pour tenir un tampon cible.
  Ce tampon vaut 2 ticks, plus une marge égale à deux fois l'écart moyen observé
  (gigue montante), plafonnée à 12 ticks.
- Si l'écart dépasse 24 ticks (pause, saut d'horloge), le client resynchronise
  d'un coup. Les snapshots antérieurs à la resynchronisation sont ignorés par la
  boucle de régulation.
- Le déplacement local est prédit avec le code du serveur. En cas de désaccord,
  le client revient à l'état serveur et rejoue les commandes suivantes. En
  variante C, les petites erreurs (moins de 64 unités) sont lissées en environ
  100 ms ; les variantes A, B et D corrigent d'un coup.
- Les joueurs distants sont interpolés `interp` ticks dans le passé. `interp`
  vaut au moins 2 ticks et s'adapte à la gigue (2,5 × gigue + 2).
- Les variantes B et C extrapolent au-delà du dernier snapshot reçu. Cette
  extrapolation :
  - est bornée (15 ms par défaut) ;
  - tient compte des collisions et de la gravité ;
  - a une confiance qui baisse avec la gigue, l'âge du paquet et l'accélération ;
  - ne traverse jamais un mur.

## Visibilité prédictive (PIM, `engine-visibility`)

Le serveur n'envoie un ennemi que s'il pourrait devenir visible pendant une
fenêtre d'anticipation : RTT/2 + interpolation + marge. Les deux yeux sont
extrapolés selon leur vitesse. Un wallhack n'a donc rien à afficher derrière les
murs et les fumées, et un joueur qui « peek » est déjà présent chez le client
avant d'apparaître au coin. Le test `hidden_enemies_are_not_sent` le vérifie.

## Variantes (laboratoire)

| | Interpolation + prédiction | Extrapolation adverse | Correction lissée | Tick |
|---|---|---|---|---|
| A | oui | non | non | 128 Hz |
| B | oui | oui | non | 128 Hz |
| C (défaut) | oui | oui | oui | 128 Hz |
| D | oui | non | non | 256 Hz |

Le flux urgent est une fonction du serveur, active pour toutes les variantes.
Mesures : `docs/network-lab.md`.
