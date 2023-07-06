use anyhow::Result;
use embedded_graphics::{pixelcolor::Rgb888, prelude::DrawTarget};
use serde::de::DeserializeOwned;
use std::convert::Infallible;

mod sub_canvas;

pub use sub_canvas::SubCanvas;

pub trait Render<D: DrawTarget<Color = Rgb888, Error = Infallible>> {
    fn render(&self, canvas: &mut D) -> Result<()>;
}

pub trait Configurable: Sized {
    type Config: DeserializeOwned;

    fn config_name() -> &'static str;

    fn load_from_config(config: Self::Config) -> Result<Self>;
}
