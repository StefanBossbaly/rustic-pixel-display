use anyhow::Result;
use clap::{Parser, Subcommand};
use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Point, RgbColor, Size},
    primitives::Rectangle,
};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
};
use rustic_pixel_display::render::Render;
use rustic_pixel_examples::renders::{
    person_tracker::{
        HomeAssistantTracker, HomeTrackerConfig, PersonTracker, StateProvider, TransitTracker,
        TransitTrackerConfig,
    },
    upcoming_arrivals::{UpcomingArrivals, UpcomingArrivalsConfig},
    weather::{Configuration, Weather},
};
use std::{collections::HashMap, env::var, vec};

const DISPLAY_SIZE: Size = Size {
    width: 256,
    height: 256,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Weather,
    UpcomingArrivals,
    PersonTracker,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let output_settings = OutputSettingsBuilder::new().scale(4).max_fps(60).build();
    let mut window = Window::new("Simulator", &output_settings);
    let mut canvas = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE);

    let args = Args::parse();

    let render: Box<dyn Render<_>> = match args.command {
        Commands::Weather => Box::new(Weather::new(Configuration {
            api_key: "API_KEY".to_owned(),
            location: rustic_pixel_examples::renders::weather::Location::City(
                "Philadelphia".to_owned(),
            ),
        })),
        Commands::UpcomingArrivals => Box::new(UpcomingArrivals::new(UpcomingArrivalsConfig {
            septa_station: Some(septa_api::types::RegionalRailStop::SuburbanStation),
            amtrak_station: None,
            results: Some(20),
        })?),
        Commands::PersonTracker => {
            let hass_url: String = var("HASS_URL")
                .expect("Pleases set HASS_URL to the url of the home assistant instance");
            let bearer_token: String = var("BEARER_TOKEN").expect(
                "Please set BEARER_TOKEN environment variable to a long lived access token",
            );

            let mut person_map: HashMap<String, Vec<Box<dyn StateProvider<_>>>> = HashMap::new();

            person_map.insert(
                "Stefan".to_owned(),
                vec![
                    Box::new(TransitTracker::new(TransitTrackerConfig {
                        home_assistant_url: hass_url.clone(),
                        home_assistant_bearer_token: bearer_token.clone(),
                        person_entity_id: "person.stefan".to_string(),
                    })?),
                    Box::new(HomeAssistantTracker::new(HomeTrackerConfig {
                        home_assistant_url: hass_url.clone(),
                        home_assistant_bearer_token: bearer_token.clone(),
                        person_entity_id: "person.stefan".to_string(),
                    })?),
                ],
            );

            person_map.insert(
                "Abby".to_owned(),
                vec![
                    Box::new(TransitTracker::new(TransitTrackerConfig {
                        home_assistant_url: hass_url.clone(),
                        home_assistant_bearer_token: bearer_token.clone(),
                        person_entity_id: "person.abby".to_string(),
                    })?),
                    Box::new(HomeAssistantTracker::new(HomeTrackerConfig {
                        home_assistant_url: hass_url.clone(),
                        home_assistant_bearer_token: bearer_token.clone(),
                        person_entity_id: "person.abby".to_string(),
                    })?),
                ],
            );

            Box::new(PersonTracker::new(person_map))
        }
    };

    'render_loop: loop {
        canvas
            .fill_solid(&Rectangle::new(Point::zero(), DISPLAY_SIZE), Rgb888::BLACK)
            .unwrap();

        render.render(&mut canvas).unwrap();
        window.update(&canvas);

        for event in window.events() {
            if event == SimulatorEvent::Quit {
                break 'render_loop;
            }
        }
    }

    Ok(())
}
