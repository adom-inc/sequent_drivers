use rppal::i2c::I2c;

const MIN_STACK_INDEX: u8 = 0;
const MAX_STACK_INDEX: u8 = 7;

/// Channels are numbered from 1, as on the board.
pub const CHANNEL_COUNT: usize = 8;

const CARD_BASE_ADDRESS: u8 = 0x40;

const VALUE_BLOCK_LEN: usize = CHANNEL_COUNT * size_of::<f32>();

/// The registers this driver reads. The board also has watchdog, calibration and LED registers,
/// none of which are touched: reads never change anything on the board.
#[repr(u8)]
enum Register {
    Temperatures = 0,
    CpuTemperature = 32,
    Supply = 33,
    HardwareRevision = 55,
    Resistances = 59,
    SensorType = 133,
}

/// Below this, a PT100 would be colder than its -200 °C range: a shorted input.
const MIN_PT100_OHMS: f32 = 18.0;
/// The input saturates at +/-366 ohm with nothing connected (measured on hardware 6.0, firmware
/// 3.00), which converts to a very plausible looking 690.9 °C. Well under that, and still far
/// above anything this board's sensors should see.
const MAX_PT100_OHMS: f32 = 300.0;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Provided stack index ({0}) was out of range")]
    InvalidStackIndex(u8),
    #[error("The board's sensor type is PT1000, and only PT100 is supported")]
    UnsupportedSensorType,
    #[error("I2C error: {0}")]
    I2c(#[from] rppal::i2c::Error),
}

/// What's on the end of a channel, judged from its resistance. The temperature alone can't tell:
/// an empty channel reads 690.9 °C.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeStatus {
    Ok,
    /// Nothing connected, or a broken wire: the input has saturated
    Open,
    /// Near zero ohms. On a 3-wire probe, often the two same-colour leads across the sense pair.
    Shorted,
}

impl ProbeStatus {
    pub fn from_resistance(ohms: f32) -> Self {
        if ohms.is_nan() || ohms.abs() > MAX_PT100_OHMS {
            Self::Open
        } else if ohms < MIN_PT100_OHMS {
            Self::Shorted
        } else {
            Self::Ok
        }
    }
}

/// One channel's measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    /// Only meaningful when `status` is [`ProbeStatus::Ok`]
    pub temperature_c: f32,
    pub resistance_ohm: f32,
    pub status: ProbeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Revisions {
    pub hardware_major: u8,
    pub hardware_minor: u8,
    pub firmware_major: u8,
    pub firmware_minor: u8,
}

/// https://sequentmicrosystems.com/products/rtd-data-acquisition-card-for-rpi
///
/// Register map from https://github.com/SequentMicrosystems/rtd-rpi. Each read is one combined
/// I2C transaction, so it's safe alongside other boards on the bus.
pub struct RtdDataAcquisition {
    bus: I2c,
}

#[derive(Debug, Clone)]
pub struct RtdDataAcquisitionConfig {
    /// As set by the board's address jumpers, 0 to 7
    pub stack_index: u8,
    pub i2c_bus: u8,
}

impl RtdDataAcquisition {
    /// Opens the board and checks that it's there and set up for PT100 probes. Only reads.
    pub fn new(config: RtdDataAcquisitionConfig) -> Result<Self, Error> {
        if !(MIN_STACK_INDEX..=MAX_STACK_INDEX).contains(&config.stack_index) {
            return Err(Error::InvalidStackIndex(config.stack_index));
        }

        let mut bus = I2c::with_bus(config.i2c_bus)?;
        bus.set_slave_address((CARD_BASE_ADDRESS + config.stack_index) as u16)?;

        let this = Self { bus };

        let mut sensor_type = [0];
        this.read(Register::SensorType, &mut sensor_type)?;
        if sensor_type[0] & 0x0f != 0 {
            return Err(Error::UnsupportedSensorType);
        }

        Ok(this)
    }

    /// Every channel, index 0 being channel 1. An empty channel is not an error: it's a reading
    /// whose status says so.
    pub fn read_all(&self) -> Result<[Reading; CHANNEL_COUNT], Error> {
        let mut temperatures = [0; VALUE_BLOCK_LEN];
        let mut resistances = [0; VALUE_BLOCK_LEN];
        self.read(Register::Temperatures, &mut temperatures)?;
        self.read(Register::Resistances, &mut resistances)?;
        Ok(decode_readings(&temperatures, &resistances))
    }

    pub fn revisions(&self) -> Result<Revisions, Error> {
        let mut bytes = [0; 4];
        self.read(Register::HardwareRevision, &mut bytes)?;
        Ok(Revisions {
            hardware_major: bytes[0],
            hardware_minor: bytes[1],
            firmware_major: bytes[2],
            firmware_minor: bytes[3],
        })
    }

    /// The board's own processor temperature, not a probe.
    pub fn cpu_temperature_c(&self) -> Result<i8, Error> {
        let mut bytes = [0];
        self.read(Register::CpuTemperature, &mut bytes)?;
        Ok(bytes[0] as i8)
    }

    /// The board's 5 V supply, in millivolts.
    pub fn supply_mv(&self) -> Result<u16, Error> {
        let mut bytes = [0; 2];
        self.read(Register::Supply, &mut bytes)?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn read(&self, register: Register, buffer: &mut [u8]) -> Result<(), Error> {
        self.bus.write_read(&[register as u8], buffer)?;
        Ok(())
    }
}

fn decode_floats(bytes: &[u8; VALUE_BLOCK_LEN]) -> [f32; CHANNEL_COUNT] {
    core::array::from_fn(|i| {
        let start = i * size_of::<f32>();
        f32::from_le_bytes(bytes[start..start + size_of::<f32>()].try_into().unwrap())
    })
}

fn decode_readings(
    temperatures: &[u8; VALUE_BLOCK_LEN],
    resistances: &[u8; VALUE_BLOCK_LEN],
) -> [Reading; CHANNEL_COUNT] {
    let temperatures = decode_floats(temperatures);
    let resistances = decode_floats(resistances);
    core::array::from_fn(|i| Reading {
        temperature_c: temperatures[i],
        resistance_ohm: resistances[i],
        status: ProbeStatus::from_resistance(resistances[i]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_probe_at_room_temperature_is_ok() {
        assert_eq!(ProbeStatus::from_resistance(108.6), ProbeStatus::Ok);
    }

    /// The values read from a real board on 2026-11-04.
    #[test]
    fn empty_channels_read_as_open_though_the_temperature_looks_plausible() {
        assert_eq!(ProbeStatus::from_resistance(366.0), ProbeStatus::Open);
        assert_eq!(ProbeStatus::from_resistance(-366.0), ProbeStatus::Open);
    }

    #[test]
    fn near_zero_ohms_reads_as_shorted() {
        assert_eq!(ProbeStatus::from_resistance(0.016), ProbeStatus::Shorted);
    }

    #[test]
    fn nan_is_not_a_reading() {
        assert_eq!(ProbeStatus::from_resistance(f32::NAN), ProbeStatus::Open);
    }

    #[test]
    fn channels_decode_in_order_as_little_endian_floats() {
        let mut temperatures = [0; VALUE_BLOCK_LEN];
        let mut resistances = [0; VALUE_BLOCK_LEN];
        for i in 0..CHANNEL_COUNT {
            let start = i * 4;
            temperatures[start..start + 4].copy_from_slice(&(20.0 + i as f32).to_le_bytes());
            resistances[start..start + 4].copy_from_slice(&(100.0 + i as f32).to_le_bytes());
        }
        let readings = decode_readings(&temperatures, &resistances);
        assert_eq!(readings[0].temperature_c, 20.0);
        assert_eq!(readings[7].temperature_c, 27.0);
        assert_eq!(readings[7].resistance_ohm, 107.0);
        assert!(readings.iter().all(|r| r.status == ProbeStatus::Ok));
    }
}
