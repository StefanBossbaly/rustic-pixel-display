#[macro_use]
extern crate rocket;

use led_driver::LedDriver;
use rpi_led_panel::{HardwareMapping, LedSequence, RGBMatrixConfig, RowAddressSetterType};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::Mutex;

mod http_server;
mod led_driver;

#[derive(Serialize, Deserialize, Debug)]
struct HardwareConfig {
    hardware_mapping: String,

    rows: usize,

    cols: usize,

    refresh_rate: usize,

    pi_chip: String,

    pwm_bits: usize,

    pwm_lsb_nanoseconds: u32,

    slowdown: u32,

    interlaced: bool,

    dither_bits: usize,

    chain_length: usize,

    parallel: usize,

    panel_type: String,

    multiplexing: String,

    row_setter: String,

    led_sequence: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut led_driver = LedDriver::new();
    led_driver.start(RGBMatrixConfig {
        hardware_mapping: HardwareMapping::regular(),
        rows: 64,
        cols: 64,
        refresh_rate: 120,
        pi_chip: None,
        pwm_bits: 6,
        pwm_lsb_nanoseconds: 130,
        slowdown: None,
        interlaced: false,
        dither_bits: 0,
        chain_length: 1,
        parallel: 1,
        panel_type: None,
        multiplexing: None,
        row_setter: RowAddressSetterType::Direct,
        led_sequence: LedSequence::Rgb,
    });

    let led_driver = Arc::new(Mutex::new(led_driver));

    http_server::build_rocket(led_driver)
        .ignite()
        .await?
        .launch()
        .await?;

    Ok(())
}
