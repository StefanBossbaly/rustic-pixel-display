// TODO: Remove once code is more stable
#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

use crate::transit::TransitRender;
use anyhow::{anyhow, Result};
use led_driver::LedDriver;
use log::Metadata;
use log::Record;

#[cfg(not(feature = "http_server"))]
use tokio::signal;

#[cfg(feature = "http_server")]
mod http_server;

#[cfg(feature = "http_server")]
#[macro_use]
extern crate rocket;

mod config;
mod led_driver;
mod render;
mod transit;

static MY_LOGGER: MyLogger = MyLogger;

struct MyLogger;

impl log::Log for MyLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!("{} - {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

#[tokio::main]
async fn main() -> Result<()> {
    log::set_max_level(log::LevelFilter::Debug);
    log::set_logger(&MY_LOGGER)?;

    let transit_render: Box<TransitRender> = Box::new(
        TransitRender::from_config()
            .map_err(|e| anyhow!("Failed to create transit tracker: {e}"))?,
    );

    #[cfg(feature = "http_server")]
    {
        let (http_to_driver_sender, http_to_driver_receiver) = std::sync::mpsc::channel();
        let (driver_to_http_sender, driver_to_http_receiver) = std::sync::mpsc::channel();

        let _led_driver = LedDriver::new(
            transit_render,
            Some((driver_to_http_sender, http_to_driver_receiver)),
        )?;

        http_server::build_rocket(http_to_driver_sender, driver_to_http_receiver)
            .ignite()
            .await?
            .launch()
            .await?;
    }

    #[cfg(not(feature = "http_server"))]
    {
        let _led_driver = LedDriver::new(transit_render, None)?;

        tokio::select! {
            _ = signal::ctrl_c() => {
                println!("Ctrl+C received!");
            }
        }
    }

    Ok(())
}
