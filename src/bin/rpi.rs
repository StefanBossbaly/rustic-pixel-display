use anyhow::Result;
use rustic_pixel_display::{
    config::{HardwareConfig, HardwareMapping, LedSequence, RowAddressSetterType},
    driver::{self, RustHardwareDriver},
};

use rustic_pixel_examples::renders::upcoming_arrivals::{UpcomingArrivals, UpcomingArrivalsConfig};
use septa_api::types::RegionalRailStop;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let _led_driver = driver::MatrixDriver::with_single_render::<RustHardwareDriver, _>(
        UpcomingArrivals::new(UpcomingArrivalsConfig {
            septa_station: RegionalRailStop::SuburbanStation,
            amtrak_station: None,
            results: Some(20),
        }),
        HardwareConfig {
            hardware_mapping: HardwareMapping::Regular,
            rows: 64,
            cols: 128,
            refresh_rate: 120,
            pi_chip: None,
            pwm_bits: 4,
            pwm_lsb_nanoseconds: 130,
            slowdown: Some(2),
            interlaced: false,
            dither_bits: 0,
            chain_length: 2,
            parallel: 1,
            panel_type: None,
            multiplexing: None,
            row_setter: RowAddressSetterType::Direct,
            led_sequence: LedSequence::Bgr,
        },
    )?;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Ctrl+C received!");
        }
    }

    Ok(())
}
