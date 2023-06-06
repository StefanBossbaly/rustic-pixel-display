use anyhow::Result;
use rpi_led_panel::Canvas;

pub(crate) trait Render {
    fn render(&self, canvas: &mut Canvas) -> Result<()>;
}
