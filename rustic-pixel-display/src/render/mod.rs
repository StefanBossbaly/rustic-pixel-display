use anyhow::Result;
use embedded_graphics::{pixelcolor::Rgb888, prelude::DrawTarget};
use std::{convert::Infallible, io::Read};

mod sub_canvas;

pub use sub_canvas::SubCanvas;

pub trait Render<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn render(&self, canvas: &mut D) -> Result<(), D::Error>;
}

pub trait RenderFactory<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn render_name(&self) -> &'static str;

    fn render_description(&self) -> &'static str;

    fn load_from_config<R: Read>(&self, reader: R) -> Result<Box<dyn Render<D>>>;
}
