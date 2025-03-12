use rppal::i2c::I2c;

const MIN_STACK_INDEX: u8 = 0;
const MAX_STACK_INDEX: u8 = 7;

const MIN_RELAY_INDEX: u8 = 1;
const MAX_RELAY_INDEX: u8 = 16;

const CARD_BASE_ADDRESS: u8 = 0x20;

#[allow(unused)]
#[repr(u8)]
enum Register {
    Input = 0x00,
    Output = 0x02,
    PolarityInvert = 0x04,
    Configuration = 0x06,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Provided stack index ({0}) was out of range")]
    InvalidStackIndex(u8),
    #[error("Provided relay index ({0}) was out of range")]
    InvalidRelayNumber(u8),
    #[error("I2C error: {0}")]
    I2c(#[from] rppal::i2c::Error),
}

/// https://sequentmicrosystems.com/products/solid-state-relays-8-layer-stackable-hat-for-raspberry-pi
pub struct SolidStateRelay16 {
    bus: I2c,
}

pub struct SolidStateRelay16Config {
    pub stack_index: u8,
    pub i2c_bus: u8,
}

impl Default for SolidStateRelay16Config {
    fn default() -> Self {
        Self {
            stack_index: 0,
            i2c_bus: 1,
        }
    }
}

impl SolidStateRelay16 {
    pub fn new(config: Option<SolidStateRelay16Config>) -> Result<Self, Error> {
        let config = config.unwrap_or_default();

        /* I2C Bus Initialization */
        if !(MIN_STACK_INDEX..=MAX_STACK_INDEX).contains(&config.stack_index) {
            return Err(Error::InvalidStackIndex(config.stack_index));
        }

        let slave_address = CARD_BASE_ADDRESS + (0x07 ^ config.stack_index);

        let mut bus = I2c::with_bus(config.i2c_bus)?;
        bus.set_slave_address(slave_address as u16)?;

        /* Device Initialization */

        bus.smbus_write_word(Register::Configuration as _, 0)?;
        bus.smbus_write_word(Register::Output as _, 0)?;

        Ok(Self { bus })
    }

    pub fn set(&self, relay: u8, state: bool) -> Result<(), Error> {
        if !(MIN_RELAY_INDEX..=MAX_RELAY_INDEX).contains(&relay) {
            return Err(Error::InvalidRelayNumber(relay));
        }

        let mut output = self.bus.smbus_read_word(Register::Output as _)?;
        let bit_index = MAX_RELAY_INDEX - relay;

        if state {
            output |= 1 << bit_index;
        } else {
            output &= !(1 << bit_index);
        }

        self.bus.smbus_write_word(Register::Output as _, output)?;

        Ok(())
    }

    pub fn get(&self, relay: u8) -> Result<bool, Error> {
        if !(MIN_RELAY_INDEX..=MAX_RELAY_INDEX).contains(&relay) {
            return Err(Error::InvalidRelayNumber(relay));
        }

        let output = self.bus.smbus_read_word(Register::Output as _)?;
        let bit_index = MAX_RELAY_INDEX - relay;

        Ok(output & (1 << bit_index) != 0)
    }
}
