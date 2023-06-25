// TODO: Remove once code is more stable
#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

use anyhow::Result;

#[cfg(feature = "http_server")]
mod http_server;

#[cfg(feature = "http_server")]
#[macro_use]
extern crate rocket;

#[cfg(feature = "simulator")]
mod simulator_driver;

mod config;
mod led_driver;
mod render;
mod transit;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let transit_render = Box::new(transit::UpcomingTrainsRender::new(
        septa_api::types::RegionalRailStop::Downingtown,
    ));

    #[cfg(feature = "http_server")]
    {
        let (http_to_driver_sender, http_to_driver_receiver) = std::sync::mpsc::channel();
        let (driver_to_http_sender, driver_to_http_receiver) = std::sync::mpsc::channel();

        let _led_driver = led_driver::LedDriver::new(
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
        #[cfg(feature = "simulator")]
        let _led_driver = simulator_driver::SimulatorDriver::new(transit_render)?;

        #[cfg(not(feature = "simulator"))]
        let _led_driver = led_driver::LedDriver::new(transit_render, None)?;

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("Ctrl+C received!");
            }
        }
    }

    Ok(())
}
