#[macro_use]
extern crate lazy_static;

use anyhow::Result;
use led_driver::LedDriver;
use render::DebugTextRender;
use std::sync::{Arc, Mutex};

#[cfg(feature = "http_server")]
mod http_server;

#[cfg(feature = "http_server")]
#[macro_use]
extern crate rocket;

mod config;
mod led_driver;
mod render;
mod transit;

#[rocket::main]
async fn main() -> Result<()> {
    let render = Box::new(DebugTextRender::new());

    let (led_driver, tx_bus, rx_bus_reader) = LedDriver::new(render)?;

    #[cfg(feature = "http_server")]
    let led_driver = Arc::new(Mutex::new(led_driver));

    #[cfg(feature = "http_server")]
    http_server::build_rocket(tx_bus, rx_bus_reader)
        .ignite()
        .await?
        .launch()
        .await?;

    Ok(())
}
