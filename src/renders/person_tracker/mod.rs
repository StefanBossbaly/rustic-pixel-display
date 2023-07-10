use crate::render::Render;
use embedded_graphics::{pixelcolor::Rgb888, prelude::DrawTarget};
use log::warn;
use std::convert::Infallible;
mod home_assistant_tracker;
mod septa_tracker;
use anyhow::Result;

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub enum UsefulnessVal {
    NotUseful,
    BarelyUseful,
    SomewhatUseful,
    Useful,
    VeryUseful,
    Essential,
}

pub trait Usefulness {
    fn usefulness(&self) -> UsefulnessVal;
}

pub trait State<D>: Usefulness + Render<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
}

pub trait StateProvider<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn provide_state(&self) -> Box<dyn State<D>>;
}

// Create a blanket impl for State<D> if struct implements both Usefulness + Render<D>
impl<D, T> State<D> for T
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    T: Usefulness + Render<D>,
{
}

struct PersonTracker<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    trackers: Vec<Box<dyn StateProvider<D>>>,
}

impl<D> PersonTracker<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn new(trackers: Vec<Box<dyn StateProvider<D>>>) -> Self {
        Self { trackers }
    }
}

impl<D> Render<D> for PersonTracker<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn render(&self, canvas: &mut D) -> Result<()> {
        let render_states = self.trackers.iter().map(|tracker| tracker.provide_state());

        let mut most_useful_render: Option<Box<dyn State<D>>> = None;

        for render_state in render_states {
            match &most_useful_render {
                Some(most_useful) => {
                    if most_useful.usefulness() < render_state.usefulness() {
                        most_useful_render = Some(render_state);
                    }
                }
                None => {
                    most_useful_render = Some(render_state);
                }
            }
        }

        match most_useful_render {
            Some(most_useful) => most_useful.render(canvas).unwrap(),
            None => warn!("No renders"),
        }

        Ok(())
    }
}
