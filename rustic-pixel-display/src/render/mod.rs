use anyhow::Result;
use embedded_graphics::{pixelcolor::Rgb888, prelude::DrawTarget};
use std::{convert::Infallible, io::Read};

mod sub_canvas;

pub use sub_canvas::SubCanvas;

/// Performs drawing operations on a embedded-graphics target
///
/// Encapsulates drawing operations into a
pub trait Render<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn render(&self, canvas: &mut D) -> Result<(), D::Error>;
}

/// Constructs a [`Render`] from a configuration.
///
/// The `RenderFactory` trait is responsible for advertising the name and a
/// short description about the render it constructs. This allows the caller to
/// inquire about the different [`Render`]s it **can** construct without
/// actually having to construct them.
pub trait RenderFactory<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    /// Returns a unique name of the Render this factory will construct
    ///
    /// This method servers as a unique identifier that can be used as a key to
    /// identify this render. It must also be human readable, so that it can be
    /// used as an UI element a human would understand what it means.
    fn render_name(&self) -> &'static str;

    /// Returns a short description about what the render does.
    fn render_description(&self) -> &'static str;

    /// Attempts to construct a render based on the provided configuration.
    fn load_from_config<R: Read>(&self, reader: R) -> Result<Box<dyn Render<D>>>;
}
