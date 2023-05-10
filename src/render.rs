use crate::http_server::Font;

use anyhow::Result;
use embedded_graphics::{
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
    pub(crate) fn update_config(&mut self, config: DebugTextConfig) {
        self.config = Some(config);
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
