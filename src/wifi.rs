//! Lien Wi-Fi en mode station, maintenu par une reconnexion non bloquante.

use core::time::Duration;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::modem::Modem,
    nvs::EspDefaultNvsPartition,
    sys::EspError,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use log::{info, warn};
use timeflow_core::{
    health::FailureLatch,
    link::{LinkAction, LinkStatus, LinkTimeouts, Reconnector},
};

/// Délais d'une tentative de connexion. L'association prend quelques
/// secondes ; le DHCP peut être bien plus lent avec un signal faible, et
/// réessaie de lui-même tant que la station reste associée.
const TIMEOUTS: LinkTimeouts = LinkTimeouts {
    association: Duration::from_secs(15),
    address: Duration::from_secs(60),
};

/// Lien Wi-Fi vers le point d'accès configuré dans `wifi.toml`.
///
/// La connexion est pilotée par un `Reconnector` : le lien est vérifié à chaque
/// `poll` et une nouvelle tentative est lancée quand il est coupé, au démarrage
/// comme après une perte de connexion. Les tentatives ne sont pas bloquantes :
/// l'affichage continue pendant que l'horloge est hors ligne.
pub struct WifiLink<'d> {
    wifi: BlockingWifi<EspWifi<'d>>,
    reconnector: Reconnector,
    /// État du lien au dernier `poll`, pour ne journaliser que les changements.
    last_status: LinkStatus,
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
            reconnector: Reconnector::new(TIMEOUTS),
            last_status: LinkStatus::Down,
            status_health: FailureLatch::new("Wi-Fi status"),
        })
    }

    /// Vérifie le lien et lance une nouvelle tentative de connexion si besoin.
    ///
    /// `now` est le temps écoulé depuis le démarrage.
    pub fn poll(&mut self, now: Duration) {
        // Une erreur du pilote est journalisée une fois, puis traitée comme un
        // lien coupé : la prochaine tentative tranchera.
        let status = self
            .status_health
            .check(self.read_status())
            .unwrap_or(LinkStatus::Down);
        if status != self.last_status {
            match (self.last_status, status) {
                (_, LinkStatus::Up) => info!("Wi-Fi connected"),
                (LinkStatus::Up, LinkStatus::Associated) => {
                    warn!("Wi-Fi IP address lost, waiting for DHCP")
                }
                (_, LinkStatus::Associated) => info!("Wi-Fi associated, waiting for an IP address"),
                (_, LinkStatus::Down) => warn!("Wi-Fi connection lost"),
            }
            self.last_status = status;
        }
        if self.reconnector.poll(now, status) == LinkAction::Connect {
            self.start_attempt();
        }
    }

    /// Lit l'état du lien auprès du pilote.
    fn read_status(&self) -> Result<LinkStatus, EspError> {
        Ok(if self.wifi.is_up()? {
            LinkStatus::Up
        } else if self.wifi.is_connected()? {
            LinkStatus::Associated
        } else {
            LinkStatus::Down
        })
    }

    /// Abandonne la tentative en cours et en lance une nouvelle, sans attendre.
    fn start_attempt(&mut self) {
        info!("Connecting to Wi-Fi");
        // On utilise ici les appels non bloquants d'`EspWifi` : ceux de
        // `BlockingWifi` figeraient l'affichage jusqu'à la fin du délai.
        // Une station associée mais sans adresse IP au-delà du délai est
        // d'abord déconnectée. Cette déconnexion est asynchrone : tant
        // qu'ESP-IDF ne l'a pas traitée, `connect` peut échouer, et le `poll`
        // suivant peut encore lire « associé » puis « coupé » et relancer une
        // seconde tentative. Ce n'est pas grave : au pire, une tentative de
        // trop et un message « connection lost » superflu.
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
