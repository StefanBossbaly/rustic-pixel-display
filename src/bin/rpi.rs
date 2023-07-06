use anyhow::Result;
use rustic_pixel_display::{renders::upcoming_arrivals::UpcomingArrivals, rpi};

#[cfg(feature = "http_server")]
use rustic_pixel_display::http_server;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let transit_render = Box::new(UpcomingArrivals::new(
        septa_api::types::RegionalRailStop::SuburbanStation,
        20,
    ));

    #[cfg(feature = "http_server")]
    {
        let (http_to_driver_sender, http_to_driver_receiver) = std::sync::mpsc::channel();
        let (driver_to_http_sender, driver_to_http_receiver) = std::sync::mpsc::channel();

        let _led_driver = rpi::LedDriver::new(
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
        let _led_driver = rpi::LedDriver::new(transit_render, None)?;

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("Ctrl+C received!");
            }
        }
    }

    Ok(())
}
