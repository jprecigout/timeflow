# Architecture

Le projet est un workspace Cargo de deux crates :

- **`timeflow-core`** : toute la logique indépendante du matériel. Il est
  `no_std` hors tests, ne dépend que de `log`, compile sur PC et contient les
  tests unitaires (`cargo test-core`).
- **`timeflow`** (racine) : le firmware ESP32. Il ne fait que brancher le
  matériel (GPIO, Wi-Fi, SNTP) sur les abstractions de `timeflow-core`.

## Composants

| Fichier                        | Rôle                                                                 |
|--------------------------------|----------------------------------------------------------------------|
| `timeflow-core/src/time.rs`    | `Time` (heure validée), trait `TimeZone` et fuseau `EuropeParis` (été / hiver). |
| `timeflow-core/src/frame.rs`   | Police 5x7, rendu de `HH:MM` et de l'attente `--:--` en image 32x8 (`Style`), découpage en modules 8x8 selon le montage (`ModuleLayout`, `Rotation`). |
| `timeflow-core/src/screen.rs`  | Trait `Screen` (afficher une image) et `CachedScreen` (n'envoie que les images qui changent). |
| `timeflow-core/src/link.rs`    | `Reconnector` : machine à états qui décide quand relancer la connexion réseau. |
| `timeflow-core/src/health.rs`  | `FailureLatch` : journalise une panne et son rétablissement, sans inonder les logs. |
| `src/main.rs`                  | Initialisation et boucle principale du firmware.                     |
| `src/display.rs`               | `Max7219Screen` : adaptateur du pilote `max7219` vers le trait `Screen`. |
| `src/wifi.rs`                  | `WifiLink` : connexion Wi-Fi non bloquante, pilotée par le `Reconnector`. |
| `build.rs`                     | Lit `wifi.toml` et injecte le SSID et le mot de passe à la compilation. |

## Dépendances entre composants

Les flèches vont de l'utilisateur vers ce qu'il utilise. Aucune flèche ne part
de `timeflow-core` vers le matériel, et `main.rs` ne connaît pas le pilote
`max7219` : seul `display.rs` l'utilise.

```mermaid
flowchart LR
    subgraph firmware["timeflow (firmware ESP32)"]
        main[main.rs]
        display[display.rs<br/>Max7219Screen]
        wifi[wifi.rs<br/>WifiLink]
    end
    subgraph core["timeflow-core (testable sur PC)"]
        time[time.rs]
        frame[frame.rs]
        screen[screen.rs]
        link[link.rs]
        health[health.rs]
    end
    main --> display & wifi
    main --> time & frame & screen & health
    display -- "implémente Screen" --> screen
    display --> frame
    wifi --> link & health
    screen --> frame
    frame --> time
    display --> max[max7219]
    wifi --> esp[esp-idf-svc]
    main --> esp
    wifitoml[wifi.toml] --> build[build.rs] -- "env! WIFI_SSID / WIFI_PASS" --> main
```

## Boucle principale

Tout se passe dans une seule boucle, rafraîchie toutes les 200 ms, sans appel
bloquant : l'affichage continue même pendant une coupure du Wi-Fi.

```mermaid
sequenceDiagram
    participant M as main.rs
    participant W as WifiLink
    participant N as SNTP
    participant C as timeflow-core
    participant S as CachedScreen / Max7219Screen

    M->>S: from_pins (power_on, set_intensity)
    M->>W: new (configuration, démarrage du pilote)
    M->>N: EspSntp::new_default
    loop toutes les 200 ms
        M->>W: poll(uptime)
        Note over W: relance une tentative si le lien est coupé<br/>ou si la précédente dépasse 15 s
        M->>N: get_sync_status (jusqu'au premier succès)
        alt heure synchronisée
            M->>C: Time::from_utc(utc, EuropeParis), render
        else pas encore synchronisée ou horloge invalide
            M->>C: waiting_pattern
        end
        M->>S: draw(frame)
        Note over S: écrit sur le bandeau seulement si l'image a changé
    end
```

## Reconnexion Wi-Fi

```mermaid
stateDiagram-v2
    [*] --> Down
    Down --> Up: lien déjà établi
    Down --> Connecting: lien coupé → Connect
    Connecting --> Up: lien établi (adresse IP obtenue)
    Connecting --> Connecting: 15 s sans succès → Connect
    Up --> Connecting: lien perdu → Connect
```

À chaque `Connect`, `WifiLink` déconnecte d'abord la station si elle est
associée sans adresse IP (tentative à moitié faite), puis relance une connexion
non bloquante.

## Gestion des erreurs

- Erreurs à l'initialisation (périphériques, bandeau, Wi-Fi, SNTP), toutes avant
  la boucle : `main` renvoie l'erreur et le firmware s'arrête, il n'y a rien à
  afficher sans elles.
- Erreurs dans la boucle (écriture sur le bandeau, horloge système, état du
  Wi-Fi) : elles sont journalisées une seule fois par `FailureLatch`, puis la
  boucle continue. Une image qui n'a pas pu être écrite est renvoyée au tour
  suivant.
