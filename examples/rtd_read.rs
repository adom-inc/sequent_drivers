//! Prints what an RTD Data Acquisition card reads. Only reads, so it's safe beside a running
//! tungsten: `rtd_read <stack index> [i2c bus]`

use sequent::rtd::{RtdDataAcquisition, RtdDataAcquisitionConfig};

fn main() -> Result<(), sequent::rtd::Error> {
    let mut args = std::env::args().skip(1);
    let stack_index = args.next().map_or(0, |a| a.parse().expect("stack index"));
    let i2c_bus = args.next().map_or(1, |a| a.parse().expect("i2c bus"));

    let card = RtdDataAcquisition::new(RtdDataAcquisitionConfig {
        stack_index,
        i2c_bus,
    })?;

    let revisions = card.revisions()?;
    println!(
        "hardware {}.{}, firmware {}.{:02}, cpu {} °C, supply {} mV",
        revisions.hardware_major,
        revisions.hardware_minor,
        revisions.firmware_major,
        revisions.firmware_minor,
        card.cpu_temperature_c()?,
        card.supply_mv()?
    );

    for (i, reading) in card.read_all()?.iter().enumerate() {
        println!(
            "channel {}: {:?}, {:.3} ohm, {:.3} °C",
            i + 1,
            reading.status,
            reading.resistance_ohm,
            reading.temperature_c
        );
    }

    Ok(())
}
