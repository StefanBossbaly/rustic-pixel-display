use anyhow::Result;
use embedded_graphics::{
    prelude::{DrawTarget, OriginDimensions, PixelColor, Point, Size},
    primitives::Rectangle,
    Pixel,
};
use std::cell::RefCell;

pub struct SubCanvas<D> {
    offset: Point,
    size: Size,
    canvas: RefCell<D>,
}

impl<D> SubCanvas<D> {
    pub fn new(offset: Point, size: Size, canvas: RefCell<D>) -> Self {
        SubCanvas {
            offset,
            size,
            canvas,
        }
    }
}

impl<D> OriginDimensions for SubCanvas<D> {
    fn size(&self) -> embedded_graphics::prelude::Size {
        self.size
    }
}

impl<D, C> DrawTarget for SubCanvas<D>
where
    C: PixelColor,
    D: DrawTarget<Color = C, Error = core::convert::Infallible>,
{
    type Color = C;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        let mut canvas = self.canvas.borrow_mut();

        let translated_pixels = pixels.into_iter().map(|pixel| {
            let point = pixel.0;
            let translated_point = self.offset + point;
            Pixel(translated_point, pixel.1)
        });

        canvas.draw_iter(translated_pixels)
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let mut canvas = self.canvas.borrow_mut();
        let translated_area = Rectangle {
            top_left: area.top_left + self.offset,
            size: self.size,
        };

        canvas.fill_contiguous(&translated_area, colors)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let mut canvas = self.canvas.borrow_mut();
        let translated_area = Rectangle {
            top_left: area.top_left + self.offset,
            size: self.size,
        };

        canvas.fill_solid(&translated_area, color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let mut canvas = self.canvas.borrow_mut();
        let translated_bounds = Rectangle {
            top_left: self.offset,
            size: self.size,
        };

        canvas.fill_solid(&translated_bounds, color)
    }
}
