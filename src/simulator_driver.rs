use crate::render::Render;
use anyhow::Result;
use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Point, RgbColor, Size},
    primitives::Rectangle,
};
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay, Window};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};

pub(crate) struct SimulatorDriver {
    /// Flag used to gracefully terminate the render and driver threads
    alive: Arc<AtomicBool>,

    /// Handle to the driver thread
    driver_thread_handle: Option<thread::JoinHandle<Result<()>>>,
}

impl SimulatorDriver {
    pub(crate) fn new(
        render: Box<dyn Render<SimulatorDisplay<Rgb888>> + Send + Sync>,
    ) -> Result<Self> {
        let alive = Arc::new(AtomicBool::new(true));
        let alive_driver = alive.clone();

        let driver_thread_handle = thread::spawn(move || -> Result<()> {
            let output_settings = OutputSettingsBuilder::new().scale(8).max_fps(120).build();
            let mut window = Window::new("Simulator", &output_settings);

            let mut canvas = SimulatorDisplay::<Rgb888>::new(Size::new(128, 64));

            while alive_driver.load(Ordering::SeqCst) {
                canvas
                    .fill_solid(
                        &Rectangle::new(Point::zero(), Size::new(128, 64)),
                        Rgb888::BLACK,
                    )
                    .unwrap();
                render.render(&mut canvas).unwrap();
                window.update(&canvas);
            }

            Ok(())
        });

        Ok(Self {
            alive,
            driver_thread_handle: Some(driver_thread_handle),
        })
    }
}

impl Drop for SimulatorDriver {
    fn drop(&mut self) {
        let Self {
            alive,
            driver_thread_handle,
            ..
        } = self;

        // Stop the threads
        alive.store(false, Ordering::SeqCst);

        if let Some(driver_handle) = driver_thread_handle.take() {
            driver_handle
                .join()
                .expect("Failed to join the driver thread")
                .expect("Driver thread encountered an error");
        }
    }
}
