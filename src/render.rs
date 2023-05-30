use crate::http_server::Font;
use anyhow::Result;
use embedded_graphics::{
    image::{Image, ImageRaw, ImageRawBE},
    mono_font::{self, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::*,
    text::{Alignment, Text},
    Drawable,
};
use rpi_led_panel::Canvas;

pub(crate) trait Render: Send + Sync {
    fn render(&self, canvas: &mut Canvas) -> Result<()>;
}

#[derive(Default)]
pub(crate) struct DebugTextRender {
    config: Option<DebugTextConfig>,
}

pub(crate) struct DebugTextConfig {
    pub(crate) text: String,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) font: Font,
}

impl DebugTextRender {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

impl Render for DebugTextRender {
    fn render(&self, canvas: &mut rpi_led_panel::Canvas) -> Result<()> {
        if let Some(config) = &self.config {
            let font: mono_font::MonoFont<'static> = config.font.into();

            let text = Text::with_alignment(
                &config.text,
                Point::new(config.x, config.y),
                MonoTextStyle::new(&font, Rgb888::GREEN),
                Alignment::Left,
            );

            text.draw(canvas)?;
        }

        Ok(())
    }
}

const IMAGE_DATA: &[u8] = include_bytes!("../assets/ferris_test_card.rgb");
const IMAGE_SIZE: usize = 64;

pub(crate) struct ImageRender<'a> {
    image_raw: ImageRaw<'a, Rgb888>,
}

impl<'a> ImageRender<'a> {
    pub(crate) fn new() -> Self {
        let image_data = ImageRawBE::<Rgb888>::new(IMAGE_DATA, IMAGE_SIZE as u32);

        Self {
            image_raw: image_data,
        }
    }
}

impl<'a> Render for ImageRender<'a> {
    fn render(&self, canvas: &mut rpi_led_panel::Canvas) -> Result<()> {
        let image = Image::new(
            &self.image_raw,
            Point::new(
                (128 / 2 - IMAGE_SIZE / 2) as i32,
                (64 / 2 - IMAGE_SIZE / 2) as i32,
            ),
        );

        image.draw(canvas)?;

        Ok(())
    }
}
