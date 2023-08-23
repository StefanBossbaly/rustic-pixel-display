use crate::render::{Render, SubCanvas};
use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Point, RgbColor, Size},
};
use serde::Serialize;
use std::convert::Infallible;

type SubRender<D> = Box<dyn for<'a> Render<SubCanvas<'a, D>>>;

pub enum CommonLayout<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    Single(Option<SubRender<D>>),
    SplitWidth {
        left: Option<SubRender<D>>,
        right: Option<SubRender<D>>,
    },
    SplitHeight {
        top: Option<SubRender<D>>,
        bottom: Option<SubRender<D>>,
    },
    Split4 {
        top_left: Option<SubRender<D>>,
        top_right: Option<SubRender<D>>,
        bottom_left: Option<SubRender<D>>,
        bottom_right: Option<SubRender<D>>,
    },
}

#[derive(Clone, Copy, Serialize)]
pub enum LayoutType {
    Single,
    SplitWidth,
    SplitHeight,
    Split4,
}

impl<D> From<&CommonLayout<D>> for LayoutType
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn from(value: &CommonLayout<D>) -> Self {
        match value {
            CommonLayout::Single(_) => Self::Single,
            CommonLayout::SplitWidth { .. } => Self::SplitWidth,
            CommonLayout::SplitHeight { .. } => Self::SplitHeight,
            CommonLayout::Split4 { .. } => Self::Split4,
        }
    }
}

struct Layout<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    size: Size,
    offset: Point,
    render: Option<SubRender<D>>,
}

pub struct LayoutManager<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    layouts: Vec<Layout<D>>,
    layout_type: LayoutType,
}

impl<D> LayoutManager<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    pub fn from_common_layout(
        common_layout: CommonLayout<D>,
        canvas_size: Size,
    ) -> LayoutManager<D> {
        let layout_type = (&common_layout).into();
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

        Self {
            layouts,
            layout_type,
        }
    }

    pub fn len(&self) -> usize {
        self.layouts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn layout(&self) -> LayoutType {
        self.layout_type
    }
}

impl<D> Render<D> for LayoutManager<D>
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn render(&self, canvas: &mut D) -> Result<(), D::Error> {
        for layout in self.layouts.iter() {
            let Layout {
                size,
                offset,
                render,
            } = layout;

            let mut sub_canvas = SubCanvas::new(*offset, *size, canvas);

            if let Some(render) = render {
                render.render(&mut sub_canvas)?;
            } else {
                sub_canvas.clear(Rgb888::BLACK)?;
            }
        }

        Ok(())
    }
}
