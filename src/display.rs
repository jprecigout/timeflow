//! Bandeau LED MAX7219 vu comme un `Screen`.

use embedded_hal::digital::OutputPin;
use max7219::{
    connectors::{Connector, PinConnector},
    DataError, MAX7219,
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

/// Bandeau MAX7219 de `MODULES` modules 8x8 chaînés.
///
/// Pattern Adapter : il traduit l'interface `Screen` utilisée par la logique de
/// l'horloge en appels au pilote `max7219`, pour que le reste du code ne dépende
/// jamais de ce pilote.
pub struct Max7219Screen<C> {
    driver: MAX7219<C>,
    layout: ModuleLayout,
}

/// Construction à partir de trois broches GPIO (liaison logicielle, sans SPI).
impl<DATA, CS, SCK> Max7219Screen<PinConnector<DATA, CS, SCK>>
where
    DATA: OutputPin,
    CS: OutputPin,
    SCK: OutputPin,
{
    /// Crée le pilote sur les broches `din`, `cs` et `clk`, allume le bandeau
    /// et règle la luminosité de chaque module.
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
        let mut driver = MAX7219::from_pins(MODULES, din, cs, clk).map_err(driver_error)?;
        driver.power_on().map_err(driver_error)?;
        for addr in 0..MODULES {
            driver
                .set_intensity(addr, intensity)
                .map_err(driver_error)?;
        }
        Ok(Self { driver, layout })
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
}
