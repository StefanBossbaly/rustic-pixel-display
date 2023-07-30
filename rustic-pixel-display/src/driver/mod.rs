use crate::{
    config::{HardwareConfig, RxEvent, TxEvent},
    render::Render,
};
use anyhow::{anyhow, Result};
use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{DrawTarget, RgbColor},
};
use log::{debug, info, warn};
use std::{
    convert::Infallible,
    marker::PhantomData,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::RecvTimeoutError,
        Arc,
    },
    thread,
    time::Duration,
};

mod rust_driver;

pub use rust_driver::RustHardwareDriver;

pub trait HardwareDriver: Sized {
    type Config: TryFrom<HardwareConfig>;
    type Canvas: DrawTarget<Color = Rgb888, Error = Infallible> + Send + Sync + 'static;

    fn new(config: Self::Config) -> Result<Self>;

    fn create_canvas(&mut self) -> Box<Self::Canvas>;

    fn display_canvas(&mut self, canvas: Box<Self::Canvas>) -> Box<Self::Canvas>;
}

pub struct MatrixDriver<D: HardwareDriver> {
    /// Flag used to gracefully terminate the render and driver threads
    alive: Arc<AtomicBool>,

    /// Handle to the render thread
    render_thread_handle: Option<thread::JoinHandle<Result<()>>>,

    /// Handle to the driver thread
    driver_thread_handle: Option<thread::JoinHandle<Result<()>>>,

    /// Even though we have D::Canvas as part of the parameter list, Rust still complains
    _driver: PhantomData<D>,
}

impl<D: HardwareDriver> MatrixDriver<D> {
    pub fn new(
        render: Box<dyn Render<D::Canvas> + Send + Sync>,
        config: HardwareConfig,
        event_sender_receiver: Option<(
            std::sync::mpsc::Sender<TxEvent>,
            std::sync::mpsc::Receiver<RxEvent>,
        )>,
    ) -> Result<Self> {
        let alive = Arc::new(AtomicBool::new(true));

        // Clone variable that will be moved into the thread
        let alive_render = alive.clone();
        let alive_driver = alive.clone();

        // Channels used to send the canvas between the render and driver threads
        let (driver_to_render_sender, driver_to_render_receiver) =
            std::sync::mpsc::channel::<Box<D::Canvas>>();
        let (render_to_driver_sender, render_to_driver_receiver) =
            std::sync::mpsc::channel::<Box<D::Canvas>>();

        // Create the render thread
        let render_thread_handle = thread::spawn(move || -> Result<()> {
            debug!("Started render thread");
            while alive_render.load(Ordering::SeqCst) {
                match driver_to_render_receiver.recv() {
                    Ok(mut canvas) => {
                        canvas.clear(Rgb888::BLACK)?;
                        render.render(canvas.as_mut())?;
                        render_to_driver_sender.send(canvas)?;
                    }
                    Err(_) => {
                        break;
                    }
                }
            }

            Ok(())
        });

        // Create the driver thread
        let driver_thread_handle = thread::spawn(move || -> Result<()> {
            debug!("Started LED Matrix driver thread");

            let (event_sender, event_receiver) = match event_sender_receiver {
                Some((sender, receiver)) => (Some(sender), Some(receiver)),
                None => (None, None),
            };

            // Convert into RGBMatrixConfig
            let hardware_config_clone = config.clone();
            let hardware_config = config
                .try_into()
                .map_err(|_e| anyhow!("Can't convert to RGBMatrixConfig"))?;

            let mut hardware_driver = D::new(hardware_config)?;
            let canvas = hardware_driver.create_canvas();
            driver_to_render_sender.send(canvas)?;

            // Send the new configuration
            if let Some(sender) = &event_sender {
                sender.send(TxEvent::UpdateMatrixConfig(hardware_config_clone))?;
            }

            while alive_driver.load(Ordering::SeqCst) {
                // Only process events if provided with a receiver by the caller
                if let Some(event_receiver) = &event_receiver {
                    match event_receiver.try_recv() {
                        Ok(rx_event) => match rx_event {
                            RxEvent::UpdateMatrixConfig(rx_config) => {
                                info!("Updating config: {:#?}", rx_config);

                                let rx_config_clone = rx_config.clone();

                                let driver_config = rx_config
                                    .try_into()
                                    .map_err(|_e| anyhow!("Can't convert to RGBMatrixConfig"))?;

                                hardware_driver = D::new(driver_config)?;

                                driver_to_render_sender.send(hardware_driver.create_canvas())?;

                                // Send the new configuration
                                if let Some(sender) = &event_sender {
                                    sender.send(TxEvent::UpdateMatrixConfig(rx_config_clone))?;
                                }
                            }
                        },
                        Err(error) => match error {
                            std::sync::mpsc::TryRecvError::Disconnected => {
                                return Err(anyhow!("Disconnected from event bus"));
                            }
                            std::sync::mpsc::TryRecvError::Empty => {}
                        },
                    }
                }

                //let timeout = Duration::from_millis((1000.0 / framerate as f64) as u64);
                let timeout = Duration::from_millis(30);

                match render_to_driver_receiver.recv_timeout(timeout) {
                    Ok(canvas) => {
                        let canvas_new = hardware_driver.display_canvas(canvas);
                        driver_to_render_sender.send(canvas_new)?;
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        break;
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        warn!("Timeout waiting for frame from render");
                        continue;
                    }
                }
            }

            Ok(())
        });

        Ok(Self {
            alive,
            render_thread_handle: Some(render_thread_handle),
            driver_thread_handle: Some(driver_thread_handle),
            _driver: PhantomData,
        })
    }
}

impl<D: HardwareDriver> Drop for MatrixDriver<D> {
    fn drop(&mut self) {
        let Self {
            alive,
            render_thread_handle,
            driver_thread_handle,
            ..
        } = self;

        // Stop the threads
        alive.store(false, Ordering::SeqCst);

        if let Some(render_handle) = render_thread_handle.take() {
            render_handle
                .join()
                .expect("Failed to join the render thread")
                .expect("Render thread encountered an error");
        }

        if let Some(driver_handle) = driver_thread_handle.take() {
            driver_handle
                .join()
                .expect("Failed to join the driver thread")
                .expect("Driver thread encountered an error");
        }
    }
}
