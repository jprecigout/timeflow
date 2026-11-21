//! Horloge sur bandeau MAX7219 4 modules (32x8) pour ESP32-WROOM-32.
//! Affiche "HH:MM" en chiffres 5x7, heure récupérée par NTP via le Wi-Fi.

mod clock;

use clock::{paris_time, render, to_modules, waiting_pattern, Frame, Style, MODULES};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{gpio::PinDriver, prelude::Peripherals},
    nvs::EspDefaultNvsPartition,
    sntp::{EspSntp, SyncStatus},
    wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use log::{info, warn};
use max7219::MAX7219;
use std::{
    thread::sleep,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

// ---- À adapter ----
const WIFI_SSID: &str = env!("WIFI_SSID"); // défini dans wifi.toml
const WIFI_PASS: &str = env!("WIFI_PASS");
const INTENSITY: u8 = 0x00; // luminosité de 0x00 (minimum) à 0x0F
const STYLE: Style = Style {
    blink_colon: false,
    rotation: 0,            // 0 à 3 : à ajuster si les chiffres sont tournés
    reverse_modules: false, // à inverser si les chiffres sont dans le désordre
};
// -------------------

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let p = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // Matrice : DIN = GPIO23, CS = GPIO5, CLK = GPIO18
    let din = PinDriver::output(p.pins.gpio23)?;
    let cs = PinDriver::output(p.pins.gpio5)?;
    let clk = PinDriver::output(p.pins.gpio18)?;
    let mut display =
        MAX7219::from_pins(MODULES, din, cs, clk).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    display.power_on().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    for addr in 0..MODULES {
        display
            .set_intensity(addr, INTENSITY)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    let mut show = |frame: &Frame| {
        for (addr, rows) in to_modules(frame, &STYLE).iter().enumerate() {
            if let Err(e) = display.write_raw(addr, rows) {
                warn!("Erreur d'écriture sur le module {addr} : {e:?}");
            }
        }
    };

    // Wi-Fi
    let mut wifi = BlockingWifi::wrap(EspWifi::new(p.modem, sysloop.clone(), Some(nvs))?, sysloop)?;
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID
            .try_into()
            .map_err(|_| anyhow::anyhow!("SSID trop long"))?,
        password: WIFI_PASS
            .try_into()
            .map_err(|_| anyhow::anyhow!("mot de passe trop long"))?,
        ..Default::default()
    }))?;
    wifi.start()?;

    let mut step = 0;
    loop {
        show(&waiting_pattern(step));
        step += 1;
        match wifi.connect().and_then(|_| wifi.wait_netif_up()) {
            Ok(()) => break,
            Err(e) => {
                warn!("Connexion Wi-Fi échouée ({e}), nouvel essai dans 5 s");
                sleep(Duration::from_secs(5));
            }
        }
    }
    info!("Wi-Fi connecté");

    // Synchronisation NTP (le service continue de resynchroniser en tâche de fond)
    let sntp = EspSntp::new_default()?;
    while sntp.get_sync_status() != SyncStatus::Completed {
        show(&waiting_pattern(step));
        step += 1;
        sleep(Duration::from_millis(500));
    }
    info!("Heure synchronisée");

    loop {
        let utc = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let (h, m, s) = paris_time(utc);
        show(&render(h, m, s, &STYLE));
        sleep(Duration::from_millis(200));
    }
}
