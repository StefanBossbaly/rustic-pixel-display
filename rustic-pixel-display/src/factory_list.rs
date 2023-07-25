use crate::render::Render;
use crate::render::RenderFactory;
use anyhow::Result;
use embedded_graphics::{pixelcolor::Rgb888, prelude::DrawTarget};
use serde::de::DeserializeOwned;
use std::convert::Infallible;

pub struct Node<T, P> {
    pub value: T,
    pub parent: P,
}

impl<T, P> Node<T, P> {
    pub const fn append<V>(self, value: V) -> Node<V, Self> {
        Node {
            value,
            parent: self,
        }
    }
}

impl<T, C, D, P> RenderFactory<D> for Node<T, P>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    C: DeserializeOwned,
    T: RenderFactory<D, Config = C>,
{
    type Config = C;

    fn render_name(&self) -> &'static str {
        self.value.render_name()
    }

    fn render_description(&self) -> &'static str {
        self.value.render_description()
    }

    fn load_from_config(&self, config: Self::Config) -> Result<Box<dyn Render<D>>> {
        self.value.load_from_config(config)
    }
}

pub struct List<T> {
    pub value: T,
}

impl<T> List<T> {
    pub const fn new(value: T) -> List<T> {
        Self { value }
    }

    pub const fn append<V>(self, value: V) -> Node<V, Self> {
        Node {
            value,
            parent: self,
        }
    }
}

impl<T, C, D> RenderFactory<D> for List<T>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    C: DeserializeOwned,
    T: RenderFactory<D, Config = C>,
{
    type Config = C;

    fn render_name(&self) -> &'static str {
        self.value.render_name()
    }

    fn render_description(&self) -> &'static str {
        self.value.render_description()
    }

    fn load_from_config(&self, config: Self::Config) -> Result<Box<dyn Render<D>>> {
        self.value.load_from_config(config)
    }
}
