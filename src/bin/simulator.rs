use anyhow::Result;
use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Point, RgbColor, Size},
    primitives::Rectangle,
};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
};
use rustic_pixel_display::{render::Render, transit};

const DISPLAY_SIZE: Size = Size {
    width: 256,
    height: 128,
};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let output_settings = OutputSettingsBuilder::new().scale(4).max_fps(60).build();
    let mut window = Window::new("Simulator", &output_settings);
    let mut canvas = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE);

    let transit_render = Box::new(transit::UpcomingTrainsRender::new(
        septa_api::types::RegionalRailStop::SuburbanStation,
    ));

    'render_loop: loop {
        canvas
            .fill_solid(&Rectangle::new(Point::zero(), DISPLAY_SIZE), Rgb888::BLACK)
            .unwrap();

        transit_render.render(&mut canvas).unwrap();
        window.update(&canvas);

        for event in window.events() {
            if event == SimulatorEvent::Quit {
                break 'render_loop;
            }
        }
    }

    Ok(())
}
