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
use rouille::{input::json::JsonError, router, try_or_400, Response};
use rustic_pixel_display::{
    factory_registry::{FactoryEntries, FactoryRegistry},
    render::Render,
};
use rustic_pixel_display_macros::RenderFactories;
use rustic_pixel_examples::renders::{
    person_tracker::TransitTrackerFactory, upcoming_arrivals::UpcomingArrivalsFactory,
};
use serde_json::json;
use std::{
    convert::Infallible,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    vec,
};
use tokio::{runtime::Runtime, task};

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

    let factory_registry = {
        let factory_registry: FactoryRegistry<RenderFactoryEntries<SimulatorDisplay<_>>, _> =
            FactoryRegistry::new(RenderFactoryEntries::factories());
        Arc::new(Mutex::new(factory_registry))
    };

    let rt = Runtime::new().unwrap();
    let http_registry = factory_registry.clone();
    let render_registry = factory_registry;

    let server = rouille::Server::new("localhost:8080", move |request| {
        let mut factory_registry_unlock = http_registry.lock();

        // This request will be processed in rouille's executor. Because of this, we need to ensure that
        // any async task that are launched are tied to our tokio runtime. The enter() ensures that if a task
        // is spawned, it will be spawned on this runtime.
        let _guard = rt.enter();

        router!(request,
            (GET) (/) => {
                // For the sake of the example we just put a dummy route for `/` so that you see
                // something if you connect to the server with a browser.
                Response::text("Hello! Unfortunately there is nothing to see here.")
            },
            (GET) (/factory/discovery) => {
                let entries: FactoryEntries = (&*factory_registry_unlock).into();
                Response::json(&entries)
            },
            (GET) (/factory/load/{render_name: String}) => {
                let json_reader = try_or_400!(if let Some(header) = request.header("Content-Type") {
                    if !header.starts_with("application/json") {
                        Err(JsonError::WrongContentType)
                    } else if let Some(b) = request.data() {
                        Ok(b)
                    } else {
                        Err(JsonError::BodyAlreadyExtracted)
                    }
                } else {
                    Err(JsonError::WrongContentType)
                });

                Response::json(
                    &match factory_registry_unlock.load(&render_name, json_reader) {
                        Ok(_) => {
                            json!({
                                "success": true,
                            })
                        }
                        Err(e) => {
                            json!({
                                "success": false,
                                "error": format!("{}", e)
                            })
                        }
                    }
                )
            },
            (GET) (/factory/unload/{render_name: String}) => {
                Response::json(&match factory_registry_unlock.unload(&render_name) {
                    Ok(_) => json!({"success" : true}),
                    Err(e) => json!({"success" : false, "error": format!("{}", e)})
                })
            },
            (GET) (/factory/select/{render_name: String}) => {
                Response::json(&match factory_registry_unlock.select(&render_name) {
                    Ok(_) => json!({"success" : true}),
                    Err(e) => json!({"success" : false, "error": format!("{}", e)})
                })
            },
            (GET) (/factory/clear) => {
                Response::json(&match factory_registry_unlock.clear() {
                    Some(_) => json!({"success" : true}),
                    None => json!({"success" : false})
                })
            },
            // If none of the other blocks matches the request, return a 404 response.
            _ => Response::empty_404()
        )
    })
    .unwrap();

    let alive = Arc::new(AtomicBool::new(true));
    let http_alive = alive.clone();
    let render_alive = alive;

    let http_task = task::spawn(async move {
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

    http_task.abort();
    render_task.abort();

    Ok(())
}
