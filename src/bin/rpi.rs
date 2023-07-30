use anyhow::Result;
use rustic_pixel_display::{
    config::{HardwareConfig, HardwareMapping, LedSequence, RowAddressSetterType},
    driver::{self, RustHardwareDriver},
};
use rustic_pixel_examples::renders::upcoming_arrivals::UpcomingArrivals;

#[cfg(feature = "http_server")]
use rustic_pixel_display::http_server;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let transit_render = Box::new(UpcomingArrivals::new(
        septa_api::types::RegionalRailStop::SuburbanStation,
        20,
    ));

    #[cfg(not(feature = "http_server"))]
    {
        let _led_driver = driver::MatrixDriver::<RustHardwareDriver>::new(
            transit_render,
            HardwareConfig {
                hardware_mapping: HardwareMapping::Regular,
                rows: 64,
                cols: 128,
                refresh_rate: 120,
                pi_chip: None,
                pwm_bits: 6,
                pwm_lsb_nanoseconds: 1,
                slowdown: Some(1),
                interlaced: false,
                dither_bits: 0,
                chain_length: 1,
                parallel: 1,
                panel_type: None,
                multiplexing: None,
                row_setter: RowAddressSetterType::Direct,
                led_sequence: LedSequence::Rbg,
            },
            None,
        )?;

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("Ctrl+C received!");
            }
        }
    }

    Ok(())
}
