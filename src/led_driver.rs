use crate::config;
use embedded_graphics::{
    mono_font::{ascii::FONT_7X13_BOLD, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::*,
    text::{Alignment, Text},
    Drawable,
};
use rpi_led_panel::{Canvas, RGBMatrix};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    fs::File,
    io::BufWriter,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use std::{io::BufReader, thread};
// This will be the shared matrix config between the LedDriver thread and other signaling thread(s)
// Arc allows it to be shared between threads in a thread-safe manner, Mutex allows the inner data type
// to be thread safe. Option means that there is no pending configuration update, and Some means that there
// is a pending configuration update.
struct ConfigHolder {
    current_config: Option<config::HardwareConfig>,
    pending_config: Option<config::HardwareConfig>,
}

pub struct LedDriver {
    thread_handle: Option<thread::JoinHandle<()>>,
    alive: Arc<AtomicBool>,
    configs: Arc<Mutex<ConfigHolder>>,
}

impl LedDriver {
    pub fn new() -> Self {
        LedDriver {
            thread_handle: None,
            alive: Arc::new(AtomicBool::new(false)),
            configs: Arc::new(Mutex::new(ConfigHolder {
                current_config: None,
                pending_config: None,
            })),
        }
    }

    pub(crate) fn update_config(&mut self, config: config::HardwareConfig) {
        self.write_config(&config);

        let mut configs = self.configs.lock().expect("Mutex failed");
        configs.pending_config = Some(config);
    }

    pub(crate) fn get_config(&self) -> Option<config::HardwareConfig> {
        let configs = self.configs.lock().expect("Mutex failed");
        configs.current_config.clone()
    }

    fn read_config(&self) -> Result<config::HardwareConfig, Box<dyn std::error::Error>> {
        let home_dir = std::env::var("HOME").expect("Failed to find the HOME environment variable");
        let mut file_path = PathBuf::from(home_dir);
        file_path.push("led-statusboard.yaml");
        let file = File::open(file_path)?;
        let file_reader = BufReader::new(file);
        Ok(serde_yaml::from_reader(file_reader)?)
    }

    fn write_config(&self, config: &config::HardwareConfig) {
        let home_dir = std::env::var("HOME").expect("Failed to find the HOME environment variable");
        let mut file_path = PathBuf::from(home_dir);
        file_path.push("led-statusboard.yaml");
        let file = File::create(file_path).expect("Unable to create file");
        let file_writer = BufWriter::new(file);
        serde_yaml::to_writer(file_writer, config).expect("Could not write to YAML file");
    }

    pub fn start(&mut self) {
        // Attempt to load the configuration
        self.alive.store(true, Ordering::SeqCst);

        // Clone variable that will be moved into the thread
        let alive = self.alive.clone();
        let configs = self.configs.clone();

        // Attempt to read the configuration
        match self.read_config() {
            Ok(config) => {
                println!("Loaded config: {:#?}", config);
                let mut configs = configs.lock().expect("Mutex failed");
                configs.pending_config = Some(config);
            }
            Err(e) => {
                println!("Failed to load config: {:#?}", e);
            }
        }

        self.thread_handle = Some(thread::spawn(move || {
            let mut matrix: Option<RGBMatrix> = None;
            let mut canvas: Option<Box<Canvas>> = None;

            while alive.load(Ordering::SeqCst) {
                // Update the configuration, if necessary
                let mut configs = configs.lock().expect("Mutex failed");
                if let Some(new_config) = configs.pending_config.take() {
                    println!("Updating config: {:#?}", new_config);

                    // Update the current config
                    configs.current_config = Some(new_config.clone());

                    // Convert into RGBMatrixConfig
                    let hardware_config = new_config.try_into().expect("Invalid config");

                    let result =
                        RGBMatrix::new(hardware_config, 0).expect("Matrix initialization failed");
                    (matrix, canvas) = (Some(result.0), Some(result.1));
                }

                if let (Some(matrix), Some(mut canvas_ref)) = (&mut matrix, canvas) {
                    canvas_ref.fill(0, 0, 0);

                    let text = Text::with_alignment(
                        "Hello\nWorld",
                        Point::new(64, 32),
                        MonoTextStyle::new(&FONT_7X13_BOLD, Rgb888::GREEN),
                        Alignment::Center,
                    );

                    text.draw(canvas_ref.as_mut()).unwrap();
                    canvas = Some(matrix.update_on_vsync(canvas_ref.clone()));
                    println!("Updated canvas")
                } else {
                    canvas = None;
                }
            }
        }));
    }
}

impl Drop for LedDriver {
    fn drop(&mut self) {
        let Self {
            thread_handle,
            alive,
            ..
        } = self;

        if let Some(thread_handle) = thread_handle.take() {
            alive.store(false, Ordering::SeqCst);
            thread_handle.join().expect("Failed to join thread");
        }
    }
}
