//! Lien Wi-Fi en mode station, maintenu par une reconnexion non bloquante.

use core::time::Duration;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::modem::Modem,
    ipv4,
    netif::{EspNetif, NetifConfiguration, NetifStack},
    nvs::EspDefaultNvsPartition,
    sys::{esp, esp_wifi_set_ps, wifi_ps_type_t_WIFI_PS_NONE, EspError},
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi, WifiDriver},
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
    /// de passe vide désigne un réseau ouvert. `hostname` est le nom envoyé à
    /// la box dans les requêtes DHCP (30 caractères au plus).
    ///
    /// # Errors
    ///
    /// Renvoie une erreur si les identifiants ou le nom sont trop longs, ou si
    /// le pilote ne peut pas démarrer.
    pub fn new(
        modem: Modem,
        sysloop: EspSystemEventLoop,
        nvs: EspDefaultNvsPartition,
        ssid: &str,
        password: &str,
        hostname: &str,
    ) -> anyhow::Result<Self> {
        // Interface station créée à la main pour fixer le nom DHCP dans le
        // code, sans dépendre de `CONFIG_LWIP_LOCAL_HOSTNAME` (valeur de repli
        // lue par ESP-IDF quand l'interface n'a pas de nom).
        let sta_netif = EspNetif::new_with_conf(&NetifConfiguration {
            ip_configuration: Some(ipv4::Configuration::Client(
                ipv4::ClientConfiguration::DHCP(ipv4::DHCPClientSettings {
                    hostname: Some(
                        hostname
                            .try_into()
                            .map_err(|_| anyhow::anyhow!("hostname is too long"))?,
                    ),
                }),
            )),
            ..NetifConfiguration::wifi_default_client()
        })?;
        let driver = WifiDriver::new(modem, sysloop.clone(), Some(nvs))?;
        let esp_wifi = EspWifi::wrap_all(driver, sta_netif, EspNetif::new(NetifStack::Ap)?)?;
        // Simple information de diagnostic : une erreur de lecture ne doit pas
        // empêcher le Wi-Fi de démarrer.
        match esp_wifi.sta_netif().get_hostname() {
            Ok(name) => info!("Network hostname: {name}"),
            Err(e) => warn!("Cannot read network hostname ({e})"),
        }
        let mut wifi = BlockingWifi::wrap(esp_wifi, sysloop)?;
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
        disable_power_save()?;
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

/// Désactive l'économie d'énergie du modem Wi-Fi (mode « modem sleep », actif
/// par défaut).
///
/// Dans ce mode, la radio dort l'essentiel du temps (environ 90 % mesurés sur
/// la carte) et ne se réveille qu'aux balises DTIM du point d'accès. Sur le
/// réseau maillé de l'utilisateur, la station restait associée sans obtenir
/// d'adresse IP : on suppose que les réponses DHCP étaient perdues pendant ce
/// sommeil (voir `docs/architecture.md`). L'horloge est alimentée en
/// permanence, l'économie d'énergie ne lui apporte rien.
///
/// # Errors
///
/// Renvoie l'erreur d'ESP-IDF si le pilote Wi-Fi refuse le réglage.
fn disable_power_save() -> Result<(), EspError> {
    // SAFETY: `esp_wifi_set_ps` ne reçoit qu'une valeur d'énumération, sans
    // pointeur. ESP-IDF exige seulement que le pilote Wi-Fi soit initialisé,
    // ce qui est le cas : la fonction est appelée après `wifi.start()`.
    esp!(unsafe { esp_wifi_set_ps(wifi_ps_type_t_WIFI_PS_NONE) })?;
    info!("Wi-Fi power save disabled");
    Ok(())
}
