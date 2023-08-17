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
use rouille::{router, Response};
use rustic_pixel_display::{
    factory_registry::{FactoryEntries, FactoryRegistry},
    http_server::HttpServer,
    render::Render,
};
use rustic_pixel_display_macros::RenderFactories;
use rustic_pixel_examples::renders::{
    person_tracker::TransitTrackerFactory, upcoming_arrivals::UpcomingArrivalsFactory,
};
use serde_json::json;
use std::{convert::Infallible, fs::File, io::BufReader, process::id, sync::Arc, vec};

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

    let factory_registry = {
        let factory_registry: FactoryRegistry<RenderFactoryEntries<SimulatorDisplay<_>>, _> =
            FactoryRegistry::new(RenderFactoryEntries::factories());
        Arc::new(Mutex::new(factory_registry))
    };

    rouille::Server::new("localhost:8080", move |request| {
        let mut factory_registry_unlock = factory_registry.lock();
        let entries: FactoryEntries = (&*factory_registry_unlock).into();

        router!(request,
            (GET) (/) => {
                // For the sake of the example we just put a dummy route for `/` so that you see
                // something if you connect to the server with a browser.
                Response::text("Hello! Unfortunately there is nothing to see here.")
            },
            (GET) (/factory/discovery) => {
                Response::json(&entries)
            },
            (GET) (/factory/load/{render_name: String}) => {
                Response::json(&match File::open(format!("~/{}", render_name)) {
                    Ok(file) => {
                        let reader = BufReader::new(file);
                        match factory_registry_unlock.load(&render_name, reader) {
                            Ok(result) => {
                                json!({
                                    "success": result,
                                })
                            }
                            Err(e) => {
                                json!({
                                    "success": false,
                                    "error": format!("{}", e)
                                })
                            }
                        }
                    }
                    Err(e) => {
                        json!({
                            "success": false,
                            "error": format!("{}", e)
                        })
                    }
                })
            },
            (GET) (/factory/unload/{render_name: String}) => {
                let unloaded = factory_registry_unlock.unload(&render_name);
                Response::json(&json!({"success": unloaded}))
            },
            (GET) (/factory/select/{render_name: String}) => {
                let selected = factory_registry_unlock.select(&render_name);
                Response::json(&json!({"success": selected}))

            },
            // If none of the other blocks matches the request, return a 404 response.
            _ => Response::empty_404()
        )
    })
    .unwrap()
    .run();

    Ok(())
}
