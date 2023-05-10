use crate::config;
use crate::render::{DebugTextRender, Render};
use anyhow::{anyhow, Context, Ok, Result};
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
    thread_handle: Option<thread::JoinHandle<Result<()>>>,
    alive: Arc<AtomicBool>,
    configs: Arc<Mutex<ConfigHolder>>,
    render: Arc<Mutex<Box<DebugTextRender>>>,
}

impl LedDriver {
    const CONFIG_FILE: &'static str = "led-statusboard.yaml";

    // TODO: Replace DebugTextRender with a Render trait object
    pub(crate) fn new(render: Arc<Mutex<Box<DebugTextRender>>>) -> Self {
        LedDriver {
            thread_handle: None,
            alive: Arc::new(AtomicBool::new(false)),
            configs: Arc::new(Mutex::new(ConfigHolder {
                current_config: None,
                pending_config: None,
            })),
            render,
        }
    }

    pub(crate) fn update_config(&mut self, config: config::HardwareConfig) -> Result<()> {
        self.write_config(&config)?;

        let mut configs = self
            .configs
            .lock()
            .map_err(|e| anyhow!("Poisoned mutex {:?}", e))?;
        configs.pending_config = Some(config);

        Ok(())
    }

    pub(crate) fn get_config(&self) -> Result<Option<config::HardwareConfig>> {
        let configs = self
            .configs
            .lock()
            .map_err(|e| anyhow!("Poisoned mutex {:?}", e))?;
        Ok(configs.current_config.clone())
    }

    fn get_config_file(&self) -> Result<File> {
        let home_dir = std::env::var("HOME").context("Can not load HOME environment variable")?;
        let mut file_path = PathBuf::from(home_dir);
        file_path.push(Self::CONFIG_FILE);
        Ok(File::open(file_path)
            .with_context(|| format!("Failed to open file {}", Self::CONFIG_FILE))?)
    }

    fn read_config(&self) -> Result<config::HardwareConfig> {
        let file_reader = BufReader::new(self.get_config_file()?);
        Ok(serde_yaml::from_reader(file_reader).context("Unable to parse YAML file")?)
    }

    fn write_config(&self, config: &config::HardwareConfig) -> Result<()> {
        let file_writer = BufWriter::new(self.get_config_file()?);
        serde_yaml::to_writer(file_writer, config).context("Could not write to YAML file")?;
        Ok(())
    }

    pub fn start(&mut self) -> Result<()> {
        // Clone variable that will be moved into the thread
        let alive = self.alive.clone();
        let configs = self.configs.clone();
        let render = self.render.clone();

        // Attempt to read the configuration
        let file_config = self.read_config()?;
        println!("Loaded config from file: {:#?}", file_config);

        // Populate the configuration
        {
            let mut configs_unlock = configs
                .lock()
                .map_err(|e| anyhow!("Poisoned mutex {:?}", e))?;
            configs_unlock.pending_config = Some(file_config);
        } // drop(configs_unlock)

        // Mark the thread as active
        self.alive.store(true, Ordering::SeqCst);

        self.thread_handle = Some(thread::spawn(move || -> Result<()> {
            let mut matrix: Option<RGBMatrix> = None;
            let mut canvas: Option<Box<Canvas>> = None;

            while alive.load(Ordering::SeqCst) {
                // Update the configuration, if necessary
                let mut configs_unlock = configs
                    .lock()
                    .map_err(|e| anyhow!("Poisoned mutex {:?}", e))?;

                if let Some(new_config) = configs_unlock.pending_config.take() {
                    println!("Updating config: {:#?}", new_config);

                    // Update the current config
                    configs_unlock.current_config = Some(new_config.clone());

                    // Convert into RGBMatrixConfig
                    let hardware_config = new_config
                        .try_into()
                        .map_err(|e| anyhow!("Can't convert to RGBMatrixConfig {:?}", e))?;

                    let result = RGBMatrix::new(hardware_config, 0)
                        .context("Invalid configuration provided")?;
                    (matrix, canvas) = (Some(result.0), Some(result.1));
                }

                if let (Some(matrix), Some(mut canvas_ref)) = (&mut matrix, canvas) {
                    canvas_ref.fill(0, 0, 0);

                    let render_unlock = render
                        .lock()
                        .map_err(|e| anyhow!("Poisoned mutex {:?}", e))?;

                    render_unlock.render(canvas_ref.as_mut())?;

                    canvas = Some(matrix.update_on_vsync(canvas_ref));
                } else {
                    matrix = None;
                    canvas = None;
                }
            }

            Ok(())
        }));

        Ok(())
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
            thread_handle
                .join()
                .expect("Failed to join thread")
                .expect("Thread encountered an error");
        }
    }
}
