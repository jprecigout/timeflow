# 0001 — Fuseau horaire calculé en Rust pur

- **Date** : 2026-11-21
- **Statut** : accepté

## Contexte

L'horloge affiche l'heure de Paris : UTC+1 en hiver, UTC+2 en été, avec le
changement d'heure européen (dernier dimanche de mars et dernier dimanche
d'octobre, à 01:00 UTC). L'ESP32 ne reçoit que l'heure UTC par NTP. Il faut donc
convertir l'heure UTC en heure locale.

## Décision

On calcule l'heure locale en Rust pur, dans `timeflow-core/src/time.rs` (dates
du calendrier grégorien et règle de l'Union européenne), derrière le trait
`TimeZone` (pattern Strategy).

## Alternatives

### Support `TZ` POSIX d'ESP-IDF (écartée)

On appelle `setenv("TZ", "CET-1CEST,M3.5.0,M10.5.0/3")`, puis `tzset()`, puis
`localtime_r()`, fournis par la newlib d'ESP-IDF.

- **Avantages** :
  - aucun calcul de dates à écrire ni à maintenir ;
  - règle POSIX standard, éprouvée ;
  - changer de fuseau revient à changer une chaîne de caractères, qui pourrait
    même se régler sans recompiler (stockée en NVS, par exemple).
- **Inconvénients** :
  - le calcul ne tourne que sur l'ESP32 : impossible de tester les changements
    d'heure sur PC ;
  - appels FFI à `setenv`, `tzset` et `localtime_r`, donc du code `unsafe` ;
  - la logique de l'heure dépendrait d'ESP-IDF, contrairement à la règle de
    découplage du projet.

### Décalage fixe (écartée)

On ajoute toujours une heure. C'est simple, mais l'affichage est faux de fin mars
à fin octobre.

## Raisons du choix

- Le code reste **testable sur PC** : les changements d'heure, le passage à la
  nouvelle année et les cas limites sont couverts par des tests unitaires.
- Pas de `unsafe` (le crate `timeflow-core` l'interdit) ni de dépendance à
  ESP-IDF.
- Grâce au trait `TimeZone`, la règle n'est pas figée : un autre fuseau, ou une
  implémentation basée sur `TZ` côté firmware, s'ajoute sans toucher au reste.

## Conséquences

- Une cinquantaine de lignes de calcul de dates à maintenir, couvertes par les
  tests.
- Changer de fuseau demande d'écrire une implémentation de `TimeZone` et de
  recompiler.
- Si la règle européenne change (fin du changement d'heure, par exemple), il
  faudra ajouter une nouvelle implémentation de `TimeZone`.
