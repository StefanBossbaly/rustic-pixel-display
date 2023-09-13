use anyhow::Result;
use embedded_graphics::{pixelcolor::Rgb888, prelude::DrawTarget};
use parking_lot::Mutex;
use rustic_pixel_display::{
    config::{HardwareConfig, HardwareMapping, LedSequence, RowAddressSetterType},
    driver::{self, HardwareDriver, RustHardwareDriver},
};
use rustic_pixel_display::{registry::Registry, render::Render};
use rustic_pixel_display_macros::RenderFactories;
use rustic_pixel_examples::renders::{
    person_tracker::TransitTrackerFactory, upcoming_arrivals::UpcomingArrivalsFactory,
    weather::WeatherFactory,
};
use std::{convert::Infallible, sync::Arc, vec};

#[derive(RenderFactories)]
enum RenderFactoryEntries<
    D: DrawTarget<Color = Rgb888, Error = Infallible> + Clone + Send + 'static,
> {
    TransitTracker(TransitTrackerFactory<D>),
    UpcomingArrivals(UpcomingArrivalsFactory<D>),
    Weather(WeatherFactory<D>),
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Use the Rust Driver
    type DriverType = RustHardwareDriver;
    type CanvasType = <RustHardwareDriver as HardwareDriver>::Canvas;

    // Create the factory registry. This will house all the registered RenderFactories that can
    // be used to construct renders.
    let factory_registry = {
        let factory_registry: Registry<RenderFactoryEntries<CanvasType>, _> =
            Registry::new(RenderFactoryEntries::factories());
        Arc::new(Mutex::new(factory_registry))
    };

    let _led_driver = driver::MatrixDriver::with_register::<DriverType, _, _>(
        "0.0.0.0:8080",
        factory_registry,
        HardwareConfig {
            hardware_mapping: HardwareMapping::Regular,
            rows: 64,
            cols: 128,
            refresh_rate: 120,
            pi_chip: None,
            pwm_bits: 4,
            pwm_lsb_nanoseconds: 130,
            slowdown: Some(2),
            interlaced: false,
            dither_bits: 0,
            chain_length: 2,
            parallel: 2,
            panel_type: None,
            multiplexing: None,
            row_setter: RowAddressSetterType::Direct,
            led_sequence: LedSequence::Bgr,
        },
    )?;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Ctrl+C received!");
        }
    }

    Ok(())
}
