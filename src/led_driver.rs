use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::*,
    text::{Alignment, Text},
    Drawable,
};
use rpi_led_panel::{RGBMatrix, RGBMatrixConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

// This will be the shared matrix config between the LedDriver thread and other signaling thread(s)
// Arc allows it to be shared between threads in a thread-safe manner, Mutex allows the inner data type
// to be thread safe. Option means that there is no pending configuration update, and Some means that there
// is a pending configuration update.
type SharedMatrixConfig = Arc<Mutex<Option<RGBMatrixConfig>>>;

pub struct LedDriver {
    thread_handle: Option<thread::JoinHandle<()>>,
    alive: Arc<AtomicBool>,
    pending_matrix_config: SharedMatrixConfig,
}

impl LedDriver {
    pub fn new() -> Self {
        LedDriver {
            thread_handle: None,
            alive: Arc::new(AtomicBool::new(false)),
            pending_matrix_config: Arc::new(Mutex::new(None)),
        }
    }

    pub fn update_config(&mut self, config: RGBMatrixConfig) {
        let mut pending_config = self.pending_matrix_config.lock().expect("Mutex failed");
        *pending_config = Some(config);
    }

    pub fn start(&mut self, config: RGBMatrixConfig) {
        self.alive.store(true, Ordering::SeqCst);

        let (mut matrix, mut canvas) =
            RGBMatrix::new(config, 0).expect("Matrix initialization failed");

        // Clone variable that will be moved into the thread
        let alive = self.alive.clone();
        let pending_matrix_config = self.pending_matrix_config.clone();

        self.thread_handle = Some(thread::spawn(move || {
            while alive.load(Ordering::SeqCst) {
                // Update the configuration, if necessary
                {
                    let mut config = pending_matrix_config.lock().expect("Mutex failed");
                    if let Some(new_config) = config.take() {
                        println!("Updating config: {:#?}", new_config);

                        (matrix, canvas) =
                            RGBMatrix::new(new_config, 0).expect("Matrix initialization failed");
                    }
                }

                canvas.fill(0, 0, 0);

                let text = Text::with_alignment(
                    "Hello\nWorld",
                    Point::new(0, 0),
                    MonoTextStyle::new(&FONT_6X10, Rgb888::WHITE),
                    Alignment::Center,
                );
                text.draw(canvas.as_mut()).unwrap();
                canvas = matrix.update_on_vsync(canvas);
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
