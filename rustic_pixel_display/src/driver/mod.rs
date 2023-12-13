use crate::{config::HardwareConfig, render::Render};
use anyhow::{anyhow, Result};
use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{DrawTarget, RgbColor},
};
use log::{debug, warn};
use std::{
    convert::Infallible,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::RecvTimeoutError,
        Arc,
    },
    thread,
    time::Duration,
};

#[cfg(feature = "http_server")]
use crate::{http_server::build_api_server, registry::Registry, render::RenderFactory};

mod cpp_driver;
mod rust_driver;

pub use cpp_driver::CppHardwareDriver;
pub use rust_driver::RustHardwareDriver;

pub trait HardwareDriver: Sized {
    type Config: TryFrom<HardwareConfig>;
    type Canvas: DrawTarget<Color = Rgb888, Error = Infallible> + Send + Sync + 'static;

    fn new(config: Self::Config) -> Result<Self>;

    fn create_canvas(&mut self) -> Box<Self::Canvas>;

    fn display_canvas(&mut self, canvas: Box<Self::Canvas>) -> Box<Self::Canvas>;
}

pub struct MatrixDriver {
    /// Flag used to gracefully terminate the render and driver threads
    alive: Arc<AtomicBool>,

    /// Handle to the render thread
    render_thread_handle: Option<thread::JoinHandle<Result<()>>>,

    /// Handle to the driver thread
    driver_thread_handle: Option<thread::JoinHandle<Result<()>>>,

    /// Handle to the HTTP thread (if any)
    http_thread_handle: Option<thread::JoinHandle<Result<()>>>,
}

impl MatrixDriver {
    pub fn with_single_render<H, R>(render: R, config: HardwareConfig) -> Result<Self>
    where
        H: HardwareDriver,
        R: Render<H::Canvas> + Sync + Send + 'static,
    {
        let alive = Arc::new(AtomicBool::new(true));

        // Clone variable that will be moved into the thread
        let alive_render = alive.clone();
        let alive_driver = alive.clone();

        // Channels used to send the canvas between the render and driver threads
        let (driver_to_render_sender, driver_to_render_receiver) =
            std::sync::mpsc::channel::<Box<H::Canvas>>();
        let (render_to_driver_sender, render_to_driver_receiver) =
            std::sync::mpsc::channel::<Box<H::Canvas>>();

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

            // Convert into RGBMatrixConfig
            let hardware_config = config
                .try_into()
                .map_err(|_e| anyhow!("Can't convert to RGBMatrixConfig"))?;

            let mut hardware_driver = H::new(hardware_config)?;
            let canvas = hardware_driver.create_canvas();
            driver_to_render_sender.send(canvas)?;

            while alive_driver.load(Ordering::SeqCst) {
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
            http_thread_handle: None,
        })
    }

    #[cfg(feature = "http_server")]
    pub fn with_register<H, A, F>(
        http_addr: A,
        registry: Arc<parking_lot::Mutex<Registry<F, H::Canvas>>>,
        config: HardwareConfig,
    ) -> Result<Self>
    where
        A: std::net::ToSocketAddrs + Send + 'static,
        H: HardwareDriver,
        F: RenderFactory<H::Canvas> + Send + Sync + 'static,
    {
        let alive = Arc::new(AtomicBool::new(true));

        // Clone variable that will be moved into the thread
        let alive_render = alive.clone();
        let alive_driver = alive.clone();
        let alive_http = alive.clone();

        // Clone variable will be move onto the respective threads
        let render_registry = registry.clone();
        let http_registry = registry;

        // Channels used to send the canvas between the render and driver threads
        let (driver_to_render_sender, driver_to_render_receiver) =
            std::sync::mpsc::channel::<Box<H::Canvas>>();
        let (render_to_driver_sender, render_to_driver_receiver) =
            std::sync::mpsc::channel::<Box<H::Canvas>>();

        // Create the render thread
        let render_thread_handle = thread::spawn(move || -> Result<()> {
            debug!("Started render thread");
            while alive_render.load(Ordering::SeqCst) {
                match driver_to_render_receiver.recv() {
                    Ok(mut canvas) => {
                        canvas.clear(Rgb888::BLACK)?;
                        render_registry.lock().render(canvas.as_mut())?;
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

            // Convert into RGBMatrixConfig
            let hardware_config = config
                .try_into()
                .map_err(|_e| anyhow!("Can't convert to RGBMatrixConfig"))?;

            let mut hardware_driver = H::new(hardware_config)?;
            let canvas = hardware_driver.create_canvas();
            driver_to_render_sender.send(canvas)?;

            while alive_driver.load(Ordering::SeqCst) {
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

        // Get the handle to the created Tokio Runtime
        let handle = tokio::runtime::Handle::current();

        let http_thread_handle = thread::spawn(move || -> Result<()> {
            let server = build_api_server(http_addr, handle, http_registry);

            while alive_http.load(Ordering::SeqCst) {
                server.poll();
            }

            Ok(())
        });

        Ok(Self {
            alive,
            render_thread_handle: Some(render_thread_handle),
            driver_thread_handle: Some(driver_thread_handle),
            http_thread_handle: Some(http_thread_handle),
        })
    }
}

impl Drop for MatrixDriver {
    fn drop(&mut self) {
        let Self {
            alive,
            render_thread_handle,
            driver_thread_handle,
            http_thread_handle,
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

        if let Some(http_handle) = http_thread_handle.take() {
            http_handle
                .join()
                .expect("Failed to join the http thread")
                .expect("HTTP thread encountered an error");
        }
    }
}
