//! Firmware TimeFlow : horloge sur bandeau MAX7219 de 4 modules (32x8) piloté
//! par un ESP32-WROOM-32. Affiche "HH:MM" en chiffres 5x7, avec l'heure obtenue
//! par NTP via le Wi-Fi.
//!
//! Ce crate ne fait que brancher le matériel (GPIO, Wi-Fi, SNTP) sur la logique
//! indépendante du matériel de `timeflow-core`.

mod display;
mod wifi;

use display::Max7219Screen;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{gpio::PinDriver, prelude::Peripherals},
    nvs::EspDefaultNvsPartition,
    sntp::{EspSntp, SyncStatus},
};
use log::info;
use std::{
    thread::sleep,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use timeflow_core::{
    frame::{render, waiting_pattern, ModuleLayout, Rotation, Style},
    health::FailureLatch,
    screen::{CachedScreen, Screen},
    time::{EuropeParis, Time},
    timer::Interval,
};
use wifi::WifiLink;

// ---- À adapter ----
/// SSID du Wi-Fi, défini dans `wifi.toml` et injecté par `build.rs`.
const WIFI_SSID: &str = env!("WIFI_SSID");
/// Mot de passe du Wi-Fi, défini dans `wifi.toml` et injecté par `build.rs`.
const WIFI_PASS: &str = env!("WIFI_PASS");
/// Nom de l'horloge sur le réseau, tel que la box l'affiche (DHCP).
const HOSTNAME: &str = "timeflow";
/// Luminosité, de 0x00 (minimum) à 0x0F (maximum).
const INTENSITY: u8 = 0x00;
/// Options d'affichage du cadran.
const STYLE: Style = Style { blink_colon: false };
/// Montage des modules sur le bandeau.
const LAYOUT: ModuleLayout = ModuleLayout {
    rotation: Rotation::None, // à ajuster si les chiffres sont tournés
    reverse_modules: false,   // à inverser si les chiffres sont dans le désordre
};
// ------------------

/// Délai entre deux rafraîchissements de l'affichage.
const REFRESH_PERIOD: Duration = Duration::from_millis(200);
/// Période de clignotement des deux points pendant l'attente : un multiple de
/// `REFRESH_PERIOD`, pour que le clignotement reste régulier.
const WAITING_BLINK_PERIOD_MS: u128 = 400;
/// Période de renvoi de la configuration du bandeau : délai maximal avant que
/// le bandeau se rallume après une coupure de sa seule alimentation.
const SCREEN_REINIT_PERIOD: Duration = Duration::from_secs(10);

/// Point d'entrée : initialise le matériel, puis rafraîchit l'affichage en
/// boucle tout en maintenant le Wi-Fi.
///
/// # Errors
///
/// Renvoie une erreur, ce qui arrête le firmware, si un périphérique, le
/// bandeau, le Wi-Fi ou SNTP ne peut pas être initialisé. Les erreurs qui
/// surviennent ensuite dans la boucle sont journalisées sans l'arrêter.
fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let p = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // Câblage du bandeau : DIN = GPIO23, CS = GPIO5, CLK = GPIO18
    let din = PinDriver::output(p.pins.gpio23)?;
    let cs = PinDriver::output(p.pins.gpio5)?;
    let clk = PinDriver::output(p.pins.gpio18)?;
    let mut screen = CachedScreen::new(Max7219Screen::from_pins(din, cs, clk, INTENSITY, LAYOUT)?);

    let mut wifi = WifiLink::new(p.modem, sysloop, nvs, WIFI_SSID, WIFI_PASS, HOSTNAME)?;

    // SNTP est lancé tout de suite : la pile réseau est prête depuis la
    // création du Wi-Fi, et SNTP réessaie seul tant que le réseau manque. Il se
    // resynchronise ensuite en tâche de fond, même après une coupure.
    let sntp = EspSntp::new_default()?;
    // `get_sync_status` ne renvoie `Completed` qu'une fois par
    // synchronisation : le premier succès est donc mémorisé ici.
    let mut time_synced = false;

    let mut screen_health = FailureLatch::new("display");
    let mut reinit_health = FailureLatch::new("display configuration");
    let mut clock_health = FailureLatch::new("system clock");
    let boot = Instant::now();
    let mut screen_reinit = Interval::new(SCREEN_REINIT_PERIOD, boot.elapsed());

    loop {
        let uptime = boot.elapsed();
        wifi.poll(uptime);

        if !time_synced && sntp.get_sync_status() == SyncStatus::Completed {
            info!("Time synchronised");
            time_synced = true;
        }

        // Avant la première synchronisation, ou si l'horloge système est
        // invalide, on affiche l'attente plutôt qu'une heure fausse.
        let utc = time_synced
            .then(|| clock_health.check(SystemTime::now().duration_since(UNIX_EPOCH)))
            .flatten();
        let frame = match utc {
            Some(utc) => render(Time::from_utc(utc.as_secs(), &EuropeParis), &STYLE),
            None => {
                // La conversion u128 -> u64 ne tronque rien avant un demi-milliard
                // d'années de fonctionnement.
                waiting_pattern((uptime.as_millis() / WAITING_BLINK_PERIOD_MS) as u64)
            }
        };
        // Le MAX7219 ne peut pas être relu : on ne sait pas s'il a perdu son
        // alimentation. On renvoie donc périodiquement sa configuration, puis
        // l'image (le cache est vidé par `reinitialize`).
        if screen_reinit.is_due(uptime) {
            reinit_health.check(screen.reinitialize());
        }
        screen_health.check(screen.draw(&frame));

        sleep(REFRESH_PERIOD);
    }
}
