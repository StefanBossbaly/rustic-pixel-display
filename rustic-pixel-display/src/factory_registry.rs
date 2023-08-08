use crate::render::{Render, RenderFactory};
use anyhow::Result;
use embedded_graphics::{pixelcolor::Rgb888, prelude::DrawTarget};
use serde::Serialize;
use std::{collections::HashMap, convert::Infallible, io::Read, marker::PhantomData};

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
    _phantom: PhantomData<D>,
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
            _phantom: PhantomData,
        }
    }

    pub fn load<R: Read>(&mut self, name: &str, reader: R) -> Result<bool> {
        match self.records_map.get_mut(name) {
            Some(record) => match record.state {
                State::Unloaded => {
                    let render = record.factory.load_from_config(reader)?;
                    record.state = State::Loaded(render);
                    Ok(true)
                }
                State::Loaded(_) => Ok(false),
            },
            None => Ok(false),
        }
    }

    pub fn unload(&mut self, name: &str) -> bool {
        match self.records_map.get_mut(name) {
            Some(record) => match record.state {
                State::Unloaded => false,
                State::Loaded(_) => {
                    // Check to see if the current render being unloaded was selected
                    if let Some(selected) = &self.selected {
                        if name == selected {
                            self.selected = None;
                        }
                    }

                    record.state = State::Unloaded;
                    true
                }
            },
            None => false,
        }
    }

    pub fn select(&mut self, name: &str) -> bool {
        match self.records_map.get(name) {
            Some(record) => match record.state {
                State::Unloaded => false,
                State::Loaded(_) => {
                    self.selected = Some(name.to_owned());
                    true
                }
            },
            None => false,
        }
    }

    pub fn render_current(&self, canvas: &mut D) -> Result<(), D::Error> {
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

    pub fn iter(&self) -> impl Iterator<Item = &F> {
        self.records_map.iter().map(|(_, record)| &record.factory)
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
