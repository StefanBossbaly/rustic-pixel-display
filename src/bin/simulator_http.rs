use anyhow::Result;
use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Point, RgbColor, Size},
    primitives::Rectangle,
};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
};
use parking_lot::Mutex;
use rustic_pixel_display::{
    factory_registry::FactoryRegistry, http_server::build_api_server, render::Render,
};
use rustic_pixel_display_macros::RenderFactories;
use rustic_pixel_examples::renders::{
    person_tracker::TransitTrackerFactory, upcoming_arrivals::UpcomingArrivalsFactory,
};
use std::{
    convert::Infallible,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    vec,
};
use tokio::{runtime::Handle, task};

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
    // Get the handle to the created Tokio Runtime
    let handle = Handle::current();

    let factory_registry = {
        let factory_registry: FactoryRegistry<RenderFactoryEntries<SimulatorDisplay<_>>, _> =
            FactoryRegistry::new(RenderFactoryEntries::factories());
        Arc::new(Mutex::new(factory_registry))
    };

    let http_registry = factory_registry.clone();
    let render_registry = factory_registry;

    let alive = Arc::new(AtomicBool::new(true));
    let http_alive = alive.clone();
    let render_alive = alive;

    let http_task = task::spawn(async move {
        let server = build_api_server("localhost:8080", handle, http_registry);

        while http_alive.load(Ordering::SeqCst) {
            server.poll();
        }
    });

    let render_task = task::spawn(async move {
        let output_settings = OutputSettingsBuilder::new().scale(4).max_fps(60).build();
        let mut window = Window::new("Simulator", &output_settings);
        let mut canvas: SimulatorDisplay<Rgb888> = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE);

        while render_alive.load(Ordering::SeqCst) {
            canvas
                .fill_solid(&Rectangle::new(Point::zero(), DISPLAY_SIZE), Rgb888::BLACK)
                .unwrap();

            render_registry.lock().render(&mut canvas).unwrap();
            window.update(&canvas);

            for event in window.events() {
                if event == SimulatorEvent::Quit {
                    render_alive.store(false, Ordering::SeqCst);
                }
            }
        }
    });

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Ctrl+C received!");
        }
    }

    //
    http_task.abort();
    render_task.abort();

    Ok(())
}
