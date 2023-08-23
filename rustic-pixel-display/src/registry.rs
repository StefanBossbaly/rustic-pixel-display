use crate::render::{Render, RenderFactory};
use anyhow::Result;
use embedded_graphics::{pixelcolor::Rgb888, prelude::DrawTarget};
use std::{collections::HashMap, convert::Infallible, error::Error, io::Read};
use uuid::Uuid;

pub struct RenderEntry<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    pub render: Box<dyn Render<D>>,
    pub factory_name: String,
}

pub struct Registry<F, D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    F: RenderFactory<D>,
{
    factory_entries: HashMap<String, F>,
    render_entries: HashMap<Uuid, RenderEntry<D>>,
    selected: Option<Uuid>,
}

unsafe impl<F, D> Send for Registry<F, D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    F: RenderFactory<D>,
{
}

#[derive(Debug)]
pub enum RegistryError {
    FactoryNotFound(String),
    RenderNotFound(Uuid),
    RenderNotLoaded,
    RenderNotUnload,
    FileIoError,
}

impl Error for RegistryError {}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FactoryNotFound(name) => write!(f, "Factory \"{}\" was not found", name),
            Self::RenderNotFound(uuid) => write!(f, "Render \"{}\" was not found", uuid),
            Self::RenderNotLoaded => write!(f, "Render was not loaded"),
            Self::RenderNotUnload => write!(f, "Render was not unloaded"),
            Self::FileIoError => write!(f, "File IO error"),
        }
    }
}

impl<F, D> Registry<F, D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    F: RenderFactory<D>,
{
    pub fn new(factories: Vec<F>) -> Self {
        Self {
            factory_entries: factories
                .into_iter()
                .map(|factory| (factory.render_name().to_owned(), factory))
                .collect::<HashMap<_, _>>(),
            render_entries: HashMap::new(),
            selected: None,
        }
    }

    pub fn load<R: Read>(&mut self, factory_name: &str, reader: R) -> Result<Uuid, RegistryError> {
        let Self {
            factory_entries,
            render_entries,
            ..
        } = self;

        let render = match factory_entries.get(factory_name) {
            Some(factory) => match factory.load_from_config(reader) {
                Ok(render) => render,
                Err(_) => return Err(RegistryError::FileIoError),
            },
            None => return Err(RegistryError::FactoryNotFound(factory_name.to_owned())),
        };

        let uuid = Uuid::new_v4();
        render_entries.insert(
            uuid.clone(),
            RenderEntry {
                render,
                factory_name: factory_name.to_owned(),
            },
        );

        Ok(uuid)
    }

    pub fn unload(&mut self, uuid: &Uuid) -> Result<(), RegistryError> {
        let Self {
            render_entries,
            selected,
            ..
        } = self;

        if let Some(selected_uuid) = selected {
            if selected_uuid == uuid {
                *selected = None;
            }
        }

        match render_entries.remove(uuid) {
            Some(_) => Ok(()),
            None => Err(RegistryError::RenderNotFound(uuid.clone())),
        }
    }

    pub fn select(&mut self, uuid: &Uuid) -> Result<(), RegistryError> {
        let Self {
            render_entries,
            selected,
            ..
        } = self;

        if !render_entries.contains_key(&uuid) {
            Err(RegistryError::RenderNotFound(uuid.clone()))
        } else {
            *selected = Some(uuid.clone());
            Ok(())
        }
    }

    pub fn factory_iter(&self) -> impl Iterator<Item = (&String, &F)> {
        let Self {
            factory_entries, ..
        } = self;

        factory_entries.iter()
    }

    pub fn render_iter(&self) -> impl Iterator<Item = (&Uuid, &RenderEntry<D>)> {
        let Self { render_entries, .. } = self;

        render_entries.iter()
    }
}

impl<F, D> Render<D> for Registry<F, D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    F: RenderFactory<D>,
{
    fn render(&self, canvas: &mut D) -> Result<(), <D as DrawTarget>::Error> {
        let Self {
            render_entries,
            selected,
            ..
        } = self;

        if let Some(selected) = selected {
            if let Some(render_entry) = render_entries.get(selected) {
                render_entry.render.render(canvas)?;
            }
        }

        Ok(())
    }
}
