use anyhow::Result;
use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Point, RgbColor, Size},
    primitives::Rectangle,
};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
};
use lazy_static::lazy_static;
use rustic_pixel_display::render::Render;
use rustic_pixel_examples::renders::person_tracker::{
    HomeAssistantTracker, HomeTrackerConfig, PersonTracker, StateProvider, TransitTracker,
    TransitTrackerConfig,
};
use std::{collections::HashMap, env::var, vec};

const DISPLAY_SIZE: Size = Size {
    width: 128,
    height: 128,
};

lazy_static! {
    static ref HASS_URL: String =
        var("HASS_URL").expect("Pleases set HASS_URL to the url of the home assistant instance");
    static ref BEARER_TOKEN: String = var("BEARER_TOKEN")
        .expect("Please set BEARER_TOKEN environment variable to a long lived access token");
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let output_settings = OutputSettingsBuilder::new().scale(10).max_fps(60).build();
    let mut window = Window::new("Simulator", &output_settings);
    let mut canvas = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE);

    let mut person_map: HashMap<String, Vec<Box<dyn StateProvider<_>>>> = HashMap::new();

    person_map.insert(
        "Stefan".to_owned(),
        vec![
            Box::new(TransitTracker::new(TransitTrackerConfig {
                home_assistant_url: HASS_URL.clone(),
                home_assistant_bearer_token: BEARER_TOKEN.clone(),
                person_entity_id: "person.stefan".to_string(),
            })?),
            Box::new(HomeAssistantTracker::new(HomeTrackerConfig {
                home_assistant_url: HASS_URL.clone(),
                home_assistant_bearer_token: BEARER_TOKEN.clone(),
                person_entity_id: "person.stefan".to_string(),
            })?),
        ],
    );

    person_map.insert(
        "Abby".to_owned(),
        vec![
            Box::new(TransitTracker::new(TransitTrackerConfig {
                home_assistant_url: HASS_URL.clone(),
                home_assistant_bearer_token: BEARER_TOKEN.clone(),
                person_entity_id: "person.abby".to_string(),
            })?),
            Box::new(HomeAssistantTracker::new(HomeTrackerConfig {
                home_assistant_url: HASS_URL.clone(),
                home_assistant_bearer_token: BEARER_TOKEN.clone(),
                person_entity_id: "person.abby".to_string(),
            })?),
        ],
    );

    let person_tracker = PersonTracker::new(person_map);

    'render_loop: loop {
        canvas
            .fill_solid(&Rectangle::new(Point::zero(), DISPLAY_SIZE), Rgb888::BLACK)
            .unwrap();

        person_tracker.render(&mut canvas).unwrap();
        window.update(&canvas);

        for event in window.events() {
            if event == SimulatorEvent::Quit {
                break 'render_loop;
            }
        }
    }

    Ok(())
}
