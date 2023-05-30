use crate::{config, render::Render};
use anyhow::{anyhow, Context, Result};
use bus::{Bus, BusReader};
use log::{debug, trace, warn};
use rpi_led_panel::{Canvas, RGBMatrix};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::RecvTimeoutError,
        Arc,
    },
    time::Duration,
};
use std::{io::BufReader, thread};

#[derive(Debug, Clone)]
pub(crate) enum RxEvent {
    UpdateMatrixConfig(config::HardwareConfig),
}

unsafe impl std::marker::Send for RxEvent {}

#[derive(Debug, Clone)]
pub(crate) enum TxEvent {
    UpdateConfig(config::HardwareConfig),
}

pub(crate) struct LedDriver {
    /// Flag used to gracefully terminate the render and driver threads
    alive: Arc<AtomicBool>,
    /// Handle to the render thread
    render_thread_handle: Option<thread::JoinHandle<Result<()>>>,
    /// Handle to the driver thread
    driver_thread_handle: Option<thread::JoinHandle<Result<()>>>,
}

impl LedDriver {
    const CONFIG_FILE: &'static str = "led-statusboard.yaml";

    pub(crate) fn new(render: Box<dyn Render>) -> Result<(Self, Bus<RxEvent>, BusReader<TxEvent>)> {
        let alive = Arc::new(AtomicBool::new(true));

        // Bus used to receive events. We will transfer ownership of this bus since we are the sink and the caller will be the source.
        let mut event_bus_rx: Bus<RxEvent> = Bus::new(10);
        let mut event_bus_rx_render_reader = event_bus_rx.add_rx();
        let mut event_bus_rx_driver_reader = event_bus_rx.add_rx();

        // Bus used to send events. We will retain ownership of this bus since we are the source and the caller will be the sink.
        let mut event_bus_tx: Bus<TxEvent> = Bus::new(10);
        let event_bus_tx_reader = event_bus_tx.add_rx();

        // Read the configuration from the saved file
        let config = Self::read_config()?;
        debug!("Loaded config from file: {:#?}", config);

        // Clone variable that will be moved into the thread
        let alive_render = alive.clone();
        let alive_driver = alive.clone();

        // Channels used to send the canvas between the render and driver threads
        let (driver_to_render_sender, driver_to_render_receiver) =
            std::sync::mpsc::channel::<Box<Canvas>>();
        let (render_to_driver_sender, render_to_driver_receiver) =
            std::sync::mpsc::channel::<Box<Canvas>>();

        // Create the render thread
        let render_thread_handle = thread::spawn(move || -> Result<()> {
            while alive_render.load(Ordering::SeqCst) {
                match event_bus_rx_render_reader.try_recv() {
                    Ok(event) => match event {
                        _ => {}
                    },
                    Err(x) => match x {
                        std::sync::mpsc::TryRecvError::Disconnected => {
                            return Err(anyhow!("Disconnected from event bus"));
                        }
                        std::sync::mpsc::TryRecvError::Empty => {}
                    },
                }

                let mut canvas = driver_to_render_receiver.recv()?;

                render.render(canvas.as_mut())?;

                render_to_driver_sender.send(canvas)?;
            }

            Ok(())
        });

        // Create the driver thread
        let driver_thread_handle = thread::spawn(move || -> Result<()> {
            let mut matrix: Option<RGBMatrix> = None;
            let mut step: u64 = 0;

            while alive_driver.load(Ordering::SeqCst) {
                match event_bus_rx_driver_reader.try_recv() {
                    Ok(rx_event) => match rx_event {
                        RxEvent::UpdateMatrixConfig(rx_config) => {
                            debug!("Updating config: {:#?}", rx_config);
                            let config_event = TxEvent::UpdateConfig(rx_config.clone());

                            // Convert into RGBMatrixConfig
                            let hardware_config = rx_config
                                .try_into()
                                .map_err(|e| anyhow!("Can't convert to RGBMatrixConfig {:?}", e))?;

                            let result = RGBMatrix::new(hardware_config, 0)
                                .context("Invalid configuration provided")?;

                            matrix = Some(result.0);
                            driver_to_render_sender.send(result.1)?;

                            event_bus_tx.broadcast(config_event);
                        }
                        _ => {}
                    },
                    Err(x) => match x {
                        std::sync::mpsc::TryRecvError::Disconnected => {
                            return Err(anyhow!("Disconnected from event bus"));
                        }
                        std::sync::mpsc::TryRecvError::Empty => {}
                    },
                }

                if let Some(matrix) = &mut matrix {
                    // Figure our the current framerate so we know how long to wait to receive a frame from the render
                    let framerate = matrix.get_framerate();
                    let timeout = Duration::from_millis((1000.0 / framerate as f64) as u64);

                    match render_to_driver_receiver.recv_timeout(timeout) {
                        Ok(canvas) => {
                            let canvas_new = matrix.update_on_vsync(canvas);
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

                    if step % 120 == 0 {
                        trace!("\r{:>100}\rFramerate: {}", "", matrix.get_framerate());
                        std::io::stdout().flush().unwrap();
                    }
                    step += 1;
                }
            }

            Ok(())
        });

        Ok((
            Self {
                alive,
                render_thread_handle: Some(render_thread_handle),
                driver_thread_handle: Some(driver_thread_handle),
            },
            event_bus_rx,
            event_bus_tx_reader,
        ))
    }

    /// Load the configuration file from the user's home directory
    fn get_config_file() -> Result<File> {
        let home_dir = std::env::var("HOME").context("Can not load HOME environment variable")?;
        let mut file_path = PathBuf::from(home_dir);
        file_path.push(Self::CONFIG_FILE);
        File::open(file_path).with_context(|| format!("Failed to open file {}", Self::CONFIG_FILE))
    }

    fn read_config() -> Result<config::HardwareConfig> {
        let file_reader = BufReader::new(Self::get_config_file()?);
        serde_yaml::from_reader(file_reader).context("Unable to parse YAML file")
    }

    fn write_config(config: &config::HardwareConfig) -> Result<()> {
        let file_writer = BufWriter::new(Self::get_config_file()?);
        serde_yaml::to_writer(file_writer, config).context("Could not write to YAML file")?;
        Ok(())
    }
}

impl Drop for LedDriver {
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
