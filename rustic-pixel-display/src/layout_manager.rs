use crate::render::{Render, SubCanvas};
use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Point, Size},
};
use std::convert::Infallible;

struct Layout<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    size: Size,
    offset: Point,
    render: Box<dyn for<'a> Render<SubCanvas<'a, D>>>,
}

pub struct LayoutManager<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    layouts: Vec<Layout<D>>,
}

impl<D> LayoutManager<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    pub fn new() -> LayoutManager<D> {
        Self {
            layouts: Vec::new(),
        }
    }

    pub fn add_render(
        &mut self,
        canvas_size: Size,
        canvas_offset: Point,
        render: Box<dyn for<'a> Render<SubCanvas<'a, D>>>,
    ) {
        self.layouts.push(Layout {
            size: canvas_size,
            offset: canvas_offset,
            render,
        })
    }
}

impl<D> Render<D> for LayoutManager<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn render(&self, canvas: &mut D) -> anyhow::Result<()> {
        for layout in self.layouts.iter() {
            let Layout {
                size,
                offset,
                render,
            } = layout;

            render
                .render(&mut SubCanvas::new(offset.clone(), size.clone(), canvas))
                .unwrap();
        }

        Ok(())
    }
}
