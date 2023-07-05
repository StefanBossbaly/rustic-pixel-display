use anyhow::Result;
use rustic_pixel_display::{simulator, transit};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let transit_render = Box::new(transit::UpcomingTrainsRender::new(
        septa_api::types::RegionalRailStop::SuburbanStation,
    ));

    let _led_driver = simulator::SimulatorDriver::new(transit_render)?;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Ctrl+C received!");
        }
    }

    Ok(())
}
