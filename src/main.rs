use anyhow::Result;
use led_driver::LedDriver;
use std::sync::{Arc, Mutex};

#[cfg(feature = "http_server")]
mod http_server;

#[cfg(feature = "http_server")]
#[macro_use]
extern crate rocket;

mod config;
mod led_driver;
mod render;

#[tokio::main]
async fn main() -> Result<()> {
    let render = Arc::new(Mutex::new(Box::default()));

    let mut led_driver = LedDriver::new(render.clone());
    led_driver.start()?;

    #[cfg(feature = "http_server")]
    let led_driver = Arc::new(Mutex::new(led_driver));

    #[cfg(feature = "http_server")]
    http_server::build_rocket(led_driver, render)
        .ignite()
        .await?
        .launch()
        .await?;

    Ok(())
}
