use rppal::i2c::I2c;

const MIN_STACK_INDEX: u8 = 0;
const MAX_STACK_INDEX: u8 = 7;

const CARD_BASE_ADDRESS: u8 = 0x38;

const MIN_MOSFET_INDEX: u8 = 1;
const MAX_MOSFET_INDEX: u8 = 8;

#[allow(unused)]
#[repr(u8)]
enum Register {
    Input = 0x00,
    Output = 0x01,
    PolarityInvert = 0x02,
    Configuration = 0x03,
    PwmBase = 0x07,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Provided stack index ({0}) was out of range")]
    InvalidStackIndex(u8),
    #[error("Provided mosfet index ({0}) was out of range")]
    InvalidMosfetNumber(u8),
    #[error("I2C error: {0}")]
    I2c(#[from] rppal::i2c::Error),
}

/// https://sequentmicrosystems.com/products/eight-mosfets-8-layer-stackable-card-for-raspberry-pi
pub struct Mosfet8 {
    bus: I2c,
}

pub struct Mosfet8Config {
    pub stack_index: u8,
    pub i2c_bus: u8,
}

impl Default for Mosfet8Config {
    fn default() -> Self {
        Self {
            stack_index: 0,
            i2c_bus: 1,
        }
    }
}

impl Mosfet8 {
    pub fn new(config: Option<Mosfet8Config>) -> Result<Self, Error> {
        let config = config.unwrap_or_default();

        /* I2C Bus Initialization */
        if !(MIN_STACK_INDEX..=MAX_STACK_INDEX).contains(&config.stack_index) {
            return Err(Error::InvalidStackIndex(config.stack_index));
        }

        let slave_address = CARD_BASE_ADDRESS + (0x07 ^ config.stack_index);

        let mut bus = I2c::with_bus(config.i2c_bus)?;
        bus.set_slave_address(slave_address as u16)?;

        /* Device Initialization */

        bus.smbus_write_byte(Register::Configuration as _, 0)?;
        bus.smbus_write_byte(Register::Output as _, 0xff)?;

        Ok(Self { bus })
    }

    pub fn set(&self, mosfet: u8, state: bool) -> Result<(), Error> {
        if !(MIN_MOSFET_INDEX..=MAX_MOSFET_INDEX).contains(&mosfet) {
            return Err(Error::InvalidMosfetNumber(mosfet));
        }

        let mut output = self.bus.smbus_read_byte(Register::Output as _)? ^ 0xff;
        let bit_index = mosfet - 1;

        if state {
            output |= 1 << bit_index;
        } else {
            output &= !(1 << bit_index);
        }

        self.bus
            .smbus_write_byte(Register::Output as _, output ^ 0xff)?;

        Ok(())
    }

    pub fn set_pwm(&self, mosfet: u8, value: f32) -> Result<(), Error> {
        if !(MIN_MOSFET_INDEX..=MAX_MOSFET_INDEX).contains(&mosfet) {
            return Err(Error::InvalidMosfetNumber(mosfet));
        }

        self.bus.smbus_write_word(
            Register::PwmBase as u8 + (2 * (mosfet - 1)),
            (value.clamp(0., 100.) * 10.) as _,
        )?;

        Ok(())
    }

    pub fn get(&self, mosfet: u8) -> Result<bool, Error> {
        if !(MIN_MOSFET_INDEX..=MAX_MOSFET_INDEX).contains(&mosfet) {
            return Err(Error::InvalidMosfetNumber(mosfet));
        }

        let output = self.bus.smbus_read_byte(Register::Output as _)?;
        let bit_index = mosfet - 1;

        Ok((output ^ 0xff) & (1 << bit_index) != 0)
    }

    pub fn get_pwm(&self, mosfet: u8) -> Result<f32, Error> {
        if !(MIN_MOSFET_INDEX..=MAX_MOSFET_INDEX).contains(&mosfet) {
            return Err(Error::InvalidMosfetNumber(mosfet));
        }

        let value = self
            .bus
            .smbus_read_word(Register::PwmBase as u8 + (2 * (mosfet - 1)))?;

        Ok(value as f32 / 10.)
    }
}
