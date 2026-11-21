//! Bandeau LED MAX7219 vu comme un `Screen`.

use embedded_hal::digital::OutputPin;
use max7219::{
    connectors::{Connector, PinConnector},
    Command, DataError, DecodeMode, MAX7219,
};
use timeflow_core::{
    frame::{to_modules, Frame, ModuleLayout, MODULES},
    screen::Screen,
};

/// Convertit une erreur du pilote MAX7219, qui n'implémente pas
/// `std::error::Error`, en `anyhow::Error`.
fn driver_error(e: DataError) -> anyhow::Error {
    anyhow::anyhow!("MAX7219 driver error: {e:?}")
}

/// Valeur du registre « scan limit » qui active les 8 lignes d'un module.
const SCAN_ALL_ROWS: u8 = 0x07;

/// Bandeau MAX7219 de `MODULES` modules 8x8 chaînés.
///
/// Pattern Adapter : il traduit l'interface `Screen` utilisée par la logique de
/// l'horloge en appels au pilote `max7219`, pour que le reste du code ne dépende
/// jamais de ce pilote.
pub struct Max7219Screen<C> {
    driver: MAX7219<C>,
    intensity: u8,
    layout: ModuleLayout,
}

/// Construction à partir de trois broches GPIO (liaison logicielle, sans SPI).
impl<DATA, CS, SCK> Max7219Screen<PinConnector<DATA, CS, SCK>>
where
    DATA: OutputPin,
    CS: OutputPin,
    SCK: OutputPin,
{
    /// Crée le pilote sur les broches `din`, `cs` et `clk`, puis écrit toute la
    /// configuration du bandeau et l'allume (voir `configure`).
    ///
    /// `intensity` va de 0x00 (minimum) à 0x0F (maximum) ; `layout` décrit le
    /// montage des modules sur le bandeau.
    ///
    /// # Errors
    ///
    /// Renvoie une erreur si le pilote n'arrive pas à communiquer avec le bandeau.
    pub fn from_pins(
        din: DATA,
        cs: CS,
        clk: SCK,
        intensity: u8,
        layout: ModuleLayout,
    ) -> anyhow::Result<Self> {
        let driver = MAX7219::from_pins(MODULES, din, cs, clk).map_err(driver_error)?;
        let mut screen = Self {
            driver,
            intensity,
            layout,
        };
        screen.configure()?;
        Ok(screen)
    }
}

/// Configuration des registres du bandeau.
impl<C: Connector> Max7219Screen<C> {
    /// Écrit tous les registres de configuration de chaque module, puis
    /// allume le bandeau.
    ///
    /// Un MAX7219 qui a perdu son alimentation redémarre avec ses registres
    /// remis à zéro : éteint (mode « shutdown »), sans image et avec une seule
    /// ligne active. Le pilote ne réécrit pas ces registres après sa création :
    /// on les renvoie donc tous ici. L'affichage n'est pas effacé, pour éviter
    /// un clignotement quand le bandeau n'a, en fait, rien perdu.
    ///
    /// Après une vraie coupure, les lignes ont un contenu aléatoire au moment
    /// de l'allumage : un bref éclair de pixels est possible, jusqu'à l'image
    /// envoyée par le `draw` suivant (quelques millisecondes).
    fn configure(&mut self) -> anyhow::Result<()> {
        for addr in 0..MODULES {
            self.driver.test(addr, false).map_err(driver_error)?;
            self.driver
                .write_raw_byte(addr, Command::ScanLimit as u8, SCAN_ALL_ROWS)
                .map_err(driver_error)?;
            self.driver
                .set_decode_mode(addr, DecodeMode::NoDecode)
                .map_err(driver_error)?;
            self.driver
                .set_intensity(addr, self.intensity)
                .map_err(driver_error)?;
        }
        self.driver.power_on().map_err(driver_error)
    }
}

/// Affichage d'une image : découpage en modules, puis écriture de chacun.
impl<C: Connector> Screen for Max7219Screen<C> {
    type Error = anyhow::Error;

    fn draw(&mut self, frame: &Frame) -> anyhow::Result<()> {
        for (addr, block) in to_modules(frame, &self.layout).iter().enumerate() {
            self.driver
                .write_raw(addr, block)
                .map_err(|e| driver_error(e).context(format!("module {addr}")))?;
        }
        Ok(())
    }

    fn reinitialize(&mut self) -> anyhow::Result<()> {
        self.configure()
    }
}
