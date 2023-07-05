use anyhow::Result;
use embedded_graphics::{pixelcolor::Rgb888, prelude::DrawTarget};
use std::convert::Infallible;

pub trait Render<D: DrawTarget<Color = Rgb888, Error = Infallible>> {
    fn render(&self, canvas: &mut D) -> Result<()>;
}
