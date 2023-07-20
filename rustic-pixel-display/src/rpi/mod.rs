use crate::{
    config::{self, HardwareConfig, RxEvent, TxEvent},
    render::Render,
};
use anyhow::{anyhow, Context, Result};
use log::{debug, info, trace, warn};
use rpi_led_panel::{Canvas, RGBMatrix, RGBMatrixConfig};
use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::PathBuf,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::RecvTimeoutError,
        Arc,
    },
    thread,
    time::Duration,
};

pub struct LedDriver {
    /// Flag used to gracefully terminate the render and driver threads
    alive: Arc<AtomicBool>,

    /// Handle to the render thread
    render_thread_handle: Option<thread::JoinHandle<Result<()>>>,

    /// Handle to the driver thread
    driver_thread_handle: Option<thread::JoinHandle<Result<()>>>,
}

impl LedDriver {
    const CONFIG_FILE: &'static str = "led-statusboard.yaml";

    pub fn new(
        render: Box<dyn Render<rpi_led_panel::Canvas> + Send + Sync>,
        event_sender_receiver: Option<(
            std::sync::mpsc::Sender<TxEvent>,
            std::sync::mpsc::Receiver<RxEvent>,
        )>,
    ) -> Result<Self> {
        let alive = Arc::new(AtomicBool::new(true));

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
            debug!("Started render thread");
            while alive_render.load(Ordering::SeqCst) {
                match driver_to_render_receiver.recv() {
                    Ok(mut canvas) => {
                        canvas.fill(0, 0, 0);
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

            let mut matrix;
            let mut step: u64 = 0;
            let (event_sender, event_receiver) = match event_sender_receiver {
                Some((sender, receiver)) => (Some(sender), Some(receiver)),
                None => (None, None),
            };

            // Convert into RGBMatrixConfig
            let hardware_config_clone = config.clone();
            let hardware_config = config
                .try_into()
                .map_err(|e| anyhow!("Can't convert to RGBMatrixConfig {:?}", e))?;

            let result =
                RGBMatrix::new(hardware_config, 0).context("Invalid configuration provided")?;

            matrix = Some(result.0);
            driver_to_render_sender.send(result.1)?;

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

                                // Convert into RGBMatrixConfig
                                let hardware_config = rx_config.try_into().map_err(|e| {
                                    anyhow!("Can't convert to RGBMatrixConfig {:?}", e)
                                })?;

                                let result = RGBMatrix::new(hardware_config, 0)
                                    .context("Invalid configuration provided")?;

                                // Update the new configuration
                                matrix = Some(result.0);
                                driver_to_render_sender.send(result.1)?;

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
                } else {
                    std::thread::yield_now();
                }
            }

            Ok(())
        });

        Ok(Self {
            alive,
            render_thread_handle: Some(render_thread_handle),
            driver_thread_handle: Some(driver_thread_handle),
        })
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

    #[allow(dead_code)]
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

impl TryFrom<HardwareConfig> for RGBMatrixConfig {
    type Error = Box<dyn std::error::Error>;

    fn try_from(config: HardwareConfig) -> Result<Self, Self::Error> {
        Ok(RGBMatrixConfig {
            hardware_mapping: rpi_led_panel::HardwareMapping::from_str(
                config.hardware_mapping.as_ref(),
            )?,
            rows: config.rows,
            cols: config.cols,
            refresh_rate: config.refresh_rate,
            pi_chip: match config.pi_chip {
                Some(pi_chip) => Some(rpi_led_panel::PiChip::from_str(pi_chip.as_ref())?),
                None => None,
            },
            pwm_bits: config.pwm_bits,
            pwm_lsb_nanoseconds: config.pwm_lsb_nanoseconds,
            slowdown: config.slowdown,
            interlaced: config.interlaced,
            dither_bits: config.dither_bits,
            chain_length: config.chain_length,
            parallel: config.parallel,
            panel_type: match config.panel_type {
                Some(panel_type) => Some(rpi_led_panel::PanelType::from_str(panel_type.as_ref())?),
                None => None,
            },
            multiplexing: match config.multiplexing {
                Some(multiplexing) => Some(rpi_led_panel::MultiplexMapperType::from_str(
                    multiplexing.as_ref(),
                )?),
                None => None,
            },
            row_setter: rpi_led_panel::RowAddressSetterType::from_str(config.row_setter.as_ref())?,
            led_sequence: rpi_led_panel::LedSequence::from_str(config.led_sequence.as_ref())?,
        })
    }
}
