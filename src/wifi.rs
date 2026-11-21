//! Lien Wi-Fi en mode station, maintenu par une reconnexion non bloquante.

use core::time::Duration;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::modem::Modem,
    nvs::EspDefaultNvsPartition,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use log::{info, warn};
use timeflow_core::{
    health::FailureLatch,
    link::{LinkAction, Reconnector},
};

/// Temps laissé à chaque tentative de connexion (association et DHCP) avant
/// qu'elle soit abandonnée et relancée.
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(15);

/// Lien Wi-Fi vers le point d'accès configuré dans `wifi.toml`.
///
/// La connexion est pilotée par un `Reconnector` : le lien est vérifié à chaque
/// `poll` et une nouvelle tentative est lancée quand il est coupé, au démarrage
/// comme après une perte de connexion. Les tentatives ne sont pas bloquantes :
/// l'affichage continue pendant que l'horloge est hors ligne.
pub struct WifiLink<'d> {
    wifi: BlockingWifi<EspWifi<'d>>,
    reconnector: Reconnector,
    status_health: FailureLatch,
}

/// Configuration et supervision du lien.
impl<'d> WifiLink<'d> {
    /// Configure la station avec `ssid` et `password` et démarre le pilote
    /// Wi-Fi. La connexion elle-même est lancée par le premier `poll`. Un mot
    /// de passe vide désigne un réseau ouvert.
    ///
    /// # Errors
    ///
    /// Renvoie une erreur si les identifiants sont trop longs ou si le pilote
    /// ne peut pas démarrer.
    pub fn new(
        modem: Modem,
        sysloop: EspSystemEventLoop,
        nvs: EspDefaultNvsPartition,
        ssid: &str,
        password: &str,
    ) -> anyhow::Result<Self> {
        let mut wifi =
            BlockingWifi::wrap(EspWifi::new(modem, sysloop.clone(), Some(nvs))?, sysloop)?;
        // Avec WPA2 (valeur par défaut), un réseau ouvert serait refusé.
        let auth_method = if password.is_empty() {
            AuthMethod::None
        } else {
            AuthMethod::WPA2Personal
        };
        wifi.set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: ssid
                .try_into()
                .map_err(|_| anyhow::anyhow!("Wi-Fi SSID is too long"))?,
            password: password
                .try_into()
                .map_err(|_| anyhow::anyhow!("Wi-Fi password is too long"))?,
            auth_method,
            ..Default::default()
        }))?;
        wifi.start()?;
        Ok(Self {
            wifi,
            reconnector: Reconnector::new(ATTEMPT_TIMEOUT),
            status_health: FailureLatch::new("Wi-Fi status"),
        })
    }

    /// Vérifie le lien et lance une nouvelle tentative de connexion si besoin.
    ///
    /// `now` est le temps écoulé depuis le démarrage.
    pub fn poll(&mut self, now: Duration) {
        let was_up = self.reconnector.is_up();
        // Une erreur du pilote est journalisée une fois, puis traitée comme un
        // lien coupé : la prochaine tentative tranchera.
        let is_up = self.status_health.check(self.wifi.is_up()).unwrap_or(false);
        match (was_up, is_up) {
            (false, true) => info!("Wi-Fi connected"),
            (true, false) => warn!("Wi-Fi connection lost"),
            _ => {}
        }
        if self.reconnector.poll(now, is_up) == LinkAction::Connect {
            self.start_attempt();
        }
    }

    /// Abandonne la tentative en cours et en lance une nouvelle, sans attendre.
    fn start_attempt(&mut self) {
        info!("Connecting to Wi-Fi");
        // On utilise ici les appels non bloquants d'`EspWifi` : ceux de
        // `BlockingWifi` figeraient l'affichage jusqu'à la fin du délai.
        // Une station associée mais sans adresse IP (tentative à moitié faite)
        // est d'abord déconnectée. Si ESP-IDF n'a pas encore traité cette
        // déconnexion, `connect` peut échouer : la tentative suivante, 15 s plus
        // tard, prend alors le relais.
        let wifi = self.wifi.wifi_mut();
        if wifi.is_connected().unwrap_or(false) {
            if let Err(e) = wifi.disconnect() {
                warn!("Wi-Fi disconnection failed ({e})");
            }
        }
        if let Err(e) = wifi.connect() {
            warn!("Wi-Fi connection could not be started ({e}), retrying later");
        }
    }
}
