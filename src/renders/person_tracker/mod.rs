use anyhow::Result;
use embedded_graphics::{
    mono_font::{self, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Point, RgbColor, Size},
    text::Text,
    Drawable,
};
use embedded_layout::{
    layout::linear::{spacing, LinearLayout},
    prelude::{vertical, Chain},
    View,
};
use log::warn;
use rustic_pixel_display::render::{Render, SubCanvas};
use std::{collections::HashMap, convert::Infallible};

mod home_assistant_tracker;
mod septa_tracker;

pub use home_assistant_tracker::{HomeAssistantTracker, HomeTrackerConfig};
pub use septa_tracker::{TransitTracker, TransitTrackerConfig};

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

pub trait SubRender<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn sub_render(&self, canvas: &mut SubCanvas<D>) -> Result<()>;
}

pub trait State<D>: Usefulness + SubRender<D>
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

// Create a blanket impl for State<D> if struct implements both Usefulness + SubRender<D>
impl<D, T> State<D> for T
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
    T: Usefulness + SubRender<D>,
{
}

pub struct PersonTracker<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    person_to_trackers: HashMap<String, Vec<Box<dyn StateProvider<D>>>>,
}

impl<D> PersonTracker<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    pub const fn new(person_to_trackers: HashMap<String, Vec<Box<dyn StateProvider<D>>>>) -> Self {
        Self { person_to_trackers }
    }
}

impl<D> Render<D> for PersonTracker<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn render(&self, canvas: &mut D) -> Result<()> {
        let mut offset = Point::zero();
        let canvas_bounds = canvas.bounding_box();

        for (person_name, trackers) in &self.person_to_trackers {
            let render_states = trackers.iter().map(|tracker| tracker.provide_state());

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
                Some(most_useful) => {
                    let person_layout = LinearLayout::horizontal(Chain::new(Text::new(
                        person_name,
                        Point::zero(),
                        MonoTextStyle::new(&mono_font::ascii::FONT_6X10, Rgb888::WHITE),
                    )))
                    .with_alignment(vertical::Center)
                    .with_spacing(spacing::FixedMargin(6))
                    .arrange()
                    .translate(offset);

                    person_layout.draw(canvas).unwrap();

                    let person_size = person_layout.bounds().size();

                    let sub_canvas_offset = Point {
                        x: offset.x,
                        y: offset.y + person_size.height as i32,
                    };

                    let sub_canvas_size = Size {
                        width: canvas_bounds.size.width,
                        height: 50,
                    };

                    most_useful
                        .sub_render(&mut SubCanvas::new(
                            sub_canvas_offset,
                            sub_canvas_size,
                            canvas,
                        ))
                        .unwrap();

                    offset = sub_canvas_offset
                        + Size {
                            width: 0,
                            height: sub_canvas_size.height,
                        };
                }
                None => warn!("No renders"),
            }
        }

        Ok(())
    }
}
