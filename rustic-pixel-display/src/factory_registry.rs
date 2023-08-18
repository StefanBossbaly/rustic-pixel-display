use crate::render::{Render, RenderFactory};
use anyhow::Result;
use embedded_graphics::{pixelcolor::Rgb888, prelude::DrawTarget};
use serde::Serialize;
use std::{collections::HashMap, convert::Infallible, error::Error, io::Read};

enum State<D: DrawTarget<Color = Rgb888, Error = Infallible>> {
    Unloaded,
    Loaded(Box<dyn Render<D>>),
}

struct FactoryRecord<F: RenderFactory<D>, D: DrawTarget<Color = Rgb888, Error = Infallible>> {
    factory: F,
    state: State<D>,
}

pub struct FactoryRegistry<F, D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    F: RenderFactory<D>,
{
    records_map: HashMap<String, FactoryRecord<F, D>>,
    selected: Option<String>,
}

unsafe impl<F, D> Send for FactoryRegistry<F, D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    F: RenderFactory<D>,
{
}

#[derive(Debug)]
pub enum FactoryRegistryError {
    FactoryNotFound(String),
    RenderNotLoaded,
    RenderNotUnload,
    FileIoError,
}

impl Error for FactoryRegistryError {}

impl std::fmt::Display for FactoryRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FactoryNotFound(name) => write!(f, "Factory \"{}\" was not found", name),
            Self::RenderNotLoaded => write!(f, "Render was not loaded"),
            Self::RenderNotUnload => write!(f, "Render was not unloaded"),
            Self::FileIoError => write!(f, "File IO error"),
        }
    }
}

impl<F, D> FactoryRegistry<F, D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    F: RenderFactory<D>,
{
    pub fn new(factories: Vec<F>) -> Self {
        Self {
            records_map: factories
                .into_iter()
                .map(|factory| {
                    (
                        factory.render_name().to_owned(),
                        FactoryRecord {
                            factory,
                            state: State::Unloaded,
                        },
                    )
                })
                .collect::<HashMap<_, _>>(),
            selected: None,
        }
    }

    pub fn load<R: Read>(&mut self, name: &str, reader: R) -> Result<(), FactoryRegistryError> {
        match self.records_map.get_mut(name) {
            Some(record) => match record.state {
                State::Unloaded => match record.factory.load_from_config(reader) {
                    Ok(render) => {
                        record.state = State::Loaded(render);
                        Ok(())
                    }
                    Err(_) => Err(FactoryRegistryError::FileIoError),
                },
                State::Loaded(_) => Err(FactoryRegistryError::RenderNotUnload),
            },
            None => Err(FactoryRegistryError::FactoryNotFound(name.to_owned())),
        }
    }

    pub fn unload(&mut self, name: &str) -> Result<(), FactoryRegistryError> {
        match self.records_map.get_mut(name) {
            Some(record) => match record.state {
                State::Unloaded => Err(FactoryRegistryError::RenderNotLoaded),
                State::Loaded(_) => {
                    // Check to see if the current render being unloaded was selected
                    if let Some(selected) = &self.selected {
                        if name == selected {
                            self.selected = None;
                        }
                    }

                    record.state = State::Unloaded;
                    Ok(())
                }
            },
            None => Err(FactoryRegistryError::FactoryNotFound(name.to_owned())),
        }
    }

    pub fn select(&mut self, name: &str) -> Result<(), FactoryRegistryError> {
        match self.records_map.get(name) {
            Some(record) => match record.state {
                State::Unloaded => Err(FactoryRegistryError::RenderNotLoaded),
                State::Loaded(_) => {
                    self.selected = Some(name.to_owned());
                    Ok(())
                }
            },
            None => Err(FactoryRegistryError::FactoryNotFound(name.to_owned())),
        }
    }

    pub fn clear(&mut self) -> Option<String> {
        self.selected.take()
    }

    pub fn iter(&self) -> impl Iterator<Item = &F> {
        self.records_map.values().map(|record| &record.factory)
    }
}

impl<F, D> Render<D> for FactoryRegistry<F, D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    F: RenderFactory<D>,
{
    fn render(&self, canvas: &mut D) -> Result<(), D::Error> {
        if let Some(selected) = &self.selected {
            match self.records_map.get(selected) {
                Some(record) => match &record.state {
                    State::Unloaded => Ok(()),
                    State::Loaded(render) => render.render(canvas),
                },
                None => Ok(()),
            }
        } else {
            Ok(())
        }
    }
}

#[derive(Serialize)]
pub struct FactoryEntry {
    pub name: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct FactoryEntries(Vec<FactoryEntry>);

impl<F, D> From<&FactoryRegistry<F, D>> for FactoryEntries
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    F: RenderFactory<D>,
{
    fn from(factory_registry: &FactoryRegistry<F, D>) -> Self {
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
