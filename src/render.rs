use anyhow::Result;
use rpi_led_panel::Canvas;

pub(crate) trait Render: Send + Sync {
    fn render(&self, canvas: &mut Canvas) -> Result<()>;
}
