#[macro_use]
extern crate lazy_static;

use anyhow::anyhow;
use anyhow::Result;
use led_driver::LedDriver;
use render::DebugTextRender;

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

    let (_led_driver, tx_bus, rx_bus_reader) = LedDriver::new(render)?;

    let mut transit_tracker = transit::TransitTracker::from_config()
        .map_err(|e| anyhow!("Failed to create transit tracker"))?;

    tokio::spawn(async move {
        loop {
            transit_tracker
                .update()
                .await
                .map_err(|e| anyhow!("Failed to update transit tracker {e}"))
                .unwrap();

            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });

    #[cfg(feature = "http_server")]
    http_server::build_rocket(tx_bus, rx_bus_reader)
        .ignite()
        .await?
        .launch()
        .await?;

    Ok(())
}
