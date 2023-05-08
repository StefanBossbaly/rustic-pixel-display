use led_driver::LedDriver;

use std::sync::Arc;
use std::sync::Mutex;

#[cfg(feature = "http_server")]
mod http_server;

#[cfg(feature = "http_server")]
#[macro_use]
extern crate rocket;

mod config;
mod led_driver;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut led_driver = LedDriver::new();
    led_driver.start();

    #[cfg(feature = "http_server")]
    let led_driver = Arc::new(Mutex::new(led_driver));

    #[cfg(feature = "http_server")]
    http_server::build_rocket(led_driver)
        .ignite()
        .await?
        .launch()
        .await?;

    Ok(())
}
