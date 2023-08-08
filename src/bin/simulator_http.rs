use anyhow::Result;

use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Point, RgbColor, Size},
    primitives::Rectangle,
};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
};
use rustic_pixel_display::{
    factory_registry::FactoryRegistry, http_server::HttpServer, render::Render,
};
use rustic_pixel_display_macros::RenderFactories;
use rustic_pixel_examples::renders::{
    person_tracker::TransitTrackerFactory, upcoming_arrivals::UpcomingArrivalsFactory,
};
use std::{convert::Infallible, vec};

const DISPLAY_SIZE: Size = Size {
    width: 256,
    height: 256,
};

#[derive(RenderFactories)]
enum RenderFactoryEntries<D: DrawTarget<Color = Rgb888, Error = Infallible>> {
    TransitTracker(TransitTrackerFactory<D>),
    UpcomingArrivals(UpcomingArrivalsFactory<D>),
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let output_settings = OutputSettingsBuilder::new().scale(4).max_fps(60).build();
    let mut window = Window::new("Simulator", &output_settings);
    let mut canvas: SimulatorDisplay<Rgb888> = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE);

    let factory_registry: FactoryRegistry<RenderFactoryEntries<SimulatorDisplay<_>>, _> =
        FactoryRegistry::new(RenderFactoryEntries::factories());

    let (rx_event_sender, _rx_event_receiver) = tokio::sync::mpsc::channel(128);
    let (_tx_event_sender, tx_event_receiver) = tokio::sync::mpsc::channel(128);

    let _http_server = HttpServer::new(rx_event_sender, tx_event_receiver, factory_registry.into());

    'render_loop: loop {
        canvas
            .fill_solid(&Rectangle::new(Point::zero(), DISPLAY_SIZE), Rgb888::BLACK)
            .unwrap();

        window.update(&canvas);

        for event in window.events() {
            if event == SimulatorEvent::Quit {
                break 'render_loop;
            }
        }
    }

    Ok(())
}
