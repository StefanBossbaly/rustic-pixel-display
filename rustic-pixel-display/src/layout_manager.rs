use crate::render::{Render, SubCanvas};
use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Point, Size},
};
use std::convert::Infallible;

type SubRender<D> = Box<dyn for<'a> Render<SubCanvas<'a, D>>>;

pub enum CommonLayout<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    Single(SubRender<D>),
    SplitWidth {
        left: SubRender<D>,
        right: SubRender<D>,
    },
    SplitHeight {
        top: SubRender<D>,
        bottom: SubRender<D>,
    },
    Split4 {
        top_left: SubRender<D>,
        top_right: SubRender<D>,
        bottom_left: SubRender<D>,
        bottom_right: SubRender<D>,
    },
}

struct Layout<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    size: Size,
    offset: Point,
    render: SubRender<D>,
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

    pub fn from_common_layout(
        common_layout: CommonLayout<D>,
        canvas_size: Size,
    ) -> LayoutManager<D> {
        let layouts = match common_layout {
            CommonLayout::Single(render) => {
                vec![Layout {
                    size: canvas_size,
                    offset: Point::zero(),
                    render,
                }]
            }
            CommonLayout::SplitWidth { left, right } => {
                let split_width = canvas_size.width / 2;

                vec![
                    Layout {
                        size: Size {
                            width: split_width,
                            ..canvas_size
                        },
                        offset: Point::zero(),
                        render: left,
                    },
                    Layout {
                        size: Size {
                            width: split_width,
                            ..canvas_size
                        },
                        offset: Point {
                            x: split_width as i32,
                            y: 0,
                        },
                        render: right,
                    },
                ]
            }
            CommonLayout::SplitHeight { top, bottom } => {
                let split_height = canvas_size.height / 2;

                vec![
                    Layout {
                        size: Size {
                            height: split_height,
                            ..canvas_size
                        },
                        offset: Point::zero(),
                        render: top,
                    },
                    Layout {
                        size: Size {
                            height: split_height,
                            ..canvas_size
                        },
                        offset: Point {
                            x: 0,
                            y: split_height as i32,
                        },
                        render: bottom,
                    },
                ]
            }
            CommonLayout::Split4 {
                top_left,
                top_right,
                bottom_left,
                bottom_right,
            } => {
                let split_width = canvas_size.width / 2;
                let split_height = canvas_size.height / 2;

                vec![
                    Layout {
                        size: Size {
                            width: split_width,
                            height: split_height,
                        },
                        offset: Point::zero(),
                        render: top_left,
                    },
                    Layout {
                        size: Size {
                            width: split_width,
                            height: split_height,
                        },
                        offset: Point {
                            x: 0,
                            y: split_width as i32,
                        },
                        render: top_right,
                    },
                    Layout {
                        size: Size {
                            width: split_width,
                            height: split_height,
                        },
                        offset: Point {
                            x: split_height as i32,
                            y: 0,
                        },
                        render: bottom_left,
                    },
                    Layout {
                        size: Size {
                            width: split_width,
                            height: split_height,
                        },
                        offset: Point {
                            x: split_height as i32,
                            y: split_width as i32,
                        },
                        render: bottom_right,
                    },
                ]
            }
        };

        Self { layouts }
    }

    pub fn add_render(&mut self, canvas_size: Size, canvas_offset: Point, render: SubRender<D>) {
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
