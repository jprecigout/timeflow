# Architecture

## Composants

| Fichier        | Rôle                                                                 |
|----------------|----------------------------------------------------------------------|
| `src/main.rs`  | Initialise le matériel, le Wi-Fi et le NTP, puis rafraîchit l'affichage. |
| `src/clock.rs` | Logique pure : police 5x7, heure de Paris (été / hiver), rendu d'une image 32x8, découpage en modules 8x8. |
| `build.rs`     | Lit `wifi.toml` et injecte le SSID et le mot de passe à la compilation. |

`clock.rs` ne dépend d'aucune bibliothèque matérielle : c'est ce qui permet de
le tester de façon isolée.

## Dépendances entre composants

```mermaid
flowchart LR
    wifi[wifi.toml] --> build[build.rs]
    build -- "env! WIFI_SSID / WIFI_PASS" --> main[main.rs]
    main --> clock[clock.rs]
    main --> esp[esp-idf-svc<br/>Wi-Fi, SNTP, GPIO]
    main --> max[max7219<br/>pilote du bandeau]
```

## Démarrage

```mermaid
sequenceDiagram
    participant M as main.rs
    participant W as Wi-Fi
    participant N as SNTP
    participant C as clock.rs
    participant D as MAX7219

    M->>D: power_on, set_intensity
    loop tant que non connecté
        M->>C: waiting_pattern(step)
        M->>D: write_raw
        M->>W: connect (nouvel essai toutes les 5 s)
    end
    loop tant que non synchronisé
        M->>C: waiting_pattern(step)
        M->>D: write_raw
        M->>N: get_sync_status
    end
    loop toutes les 200 ms
        M->>C: paris_time(utc), render, to_modules
        M->>D: write_raw
    end
```
