// TODO: Remove once code is more stable
#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

use anyhow::anyhow;
use anyhow::Result;
use led_driver::LedDriver;
use log::Metadata;
use log::Record;
use tokio::signal;

use crate::transit::TransitRender;

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

#[rocket::main]
async fn main() -> Result<()> {
    let transit_render: Box<TransitRender> = Box::new(
        TransitRender::from_config()
            .map_err(|e| anyhow!("Failed to create transit tracker: {e}"))?,
    );
    let led_driver = LedDriver::new(transit_render)?;

    log::set_max_level(log::LevelFilter::Debug);
    log::set_logger(&MY_LOGGER)?;

    tokio::select! {
        _ = signal::ctrl_c() => {
            println!("Ctrl+C received!");
        },
        // _ = http_server::build_rocket()
        // .ignite()
        // .await?
        // .launch() => {
        //     println!("HTTP server exited!");
        // },
    }

    drop(led_driver);

    Ok(())
}
