use crate::render::{Render, RenderFactory};
use anyhow::Result;
use embedded_graphics::{pixelcolor::Rgb888, prelude::DrawTarget};
use serde::Serialize;
use std::{convert::Infallible, io::Read, marker::PhantomData};

pub struct FactoryRegistry<F, D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    F: RenderFactory<D>,
{
    factories: Vec<F>,
    _phantom: PhantomData<D>,
}

impl<F, D> FactoryRegistry<F, D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    F: RenderFactory<D>,
{
    pub fn new(factories: Vec<F>) -> Self {
        Self {
            factories,
            _phantom: PhantomData,
        }
    }

    pub fn construct_render<R: Read>(
        &self,
        name: &str,
        reader: R,
    ) -> Option<Result<Box<dyn Render<D>>>> {
        for factory in self.factories.iter() {
            if factory.render_name() == name {
                return Some(factory.load_from_config(reader));
            }
        }

        None
    }

    pub fn iter(&self) -> impl Iterator<Item = &F> {
        self.factories.iter()
    }
}

#[derive(Serialize)]
pub struct FactoryEntry {
    pub name: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct FactoryEntries(Vec<FactoryEntry>);

impl<F, D> From<FactoryRegistry<F, D>> for FactoryEntries
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    F: RenderFactory<D>,
{
    fn from(factory_registry: FactoryRegistry<F, D>) -> Self {
        let factories = factory_registry
            .iter()
            .map(|factory| FactoryEntry {
                name: factory.render_name().to_owned(),
                description: factory.render_description().to_owned(),
            })
            .collect();

        Self(factories)
    }
}
