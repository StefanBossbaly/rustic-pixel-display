use anyhow::Result;
use embedded_graphics::{pixelcolor::Rgb888, prelude::DrawTarget};
use serde::de::DeserializeOwned;
use std::convert::Infallible;

mod sub_canvas;

pub use sub_canvas::SubCanvas;

pub trait Render<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn render(&self, canvas: &mut D) -> Result<()>;
}

pub trait RenderFactory<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    type Config: DeserializeOwned;

    fn render_name(&self) -> &'static str;

    fn render_description(&self) -> &'static str;

    fn load_from_config(&self, config: Self::Config) -> Result<Box<dyn Render<D>>>;
}
