use anyhow::Result;
use embedded_graphics::{
    prelude::{DrawTarget, OriginDimensions, PixelColor, Point, Size},
    primitives::Rectangle,
    transform::Transform,
    Pixel,
};

pub struct SubCanvas<'a, D> {
    offset: Point,
    size: Size,
    canvas: &'a mut D,
}

impl<'a, D> SubCanvas<'a, D> {
    pub fn new(offset: Point, size: Size, canvas: &'a mut D) -> Self {
        SubCanvas {
            offset,
            size,
            canvas,
        }
    }
}

impl<D> OriginDimensions for SubCanvas<'_, D> {
    fn size(&self) -> Size {
        self.size
    }
}

impl<D, C> DrawTarget for SubCanvas<'_, D>
where
    C: PixelColor,
    D: DrawTarget<Color = C, Error = core::convert::Infallible>,
{
    type Color = C;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let translated_pixels = pixels.into_iter().map(|pixel| {
            let point = pixel.0;
            let translated_point = self.offset + point;
            Pixel(translated_point, pixel.1)
        });

        self.canvas.draw_iter(translated_pixels)
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        self.canvas
            .fill_contiguous(&area.translate(self.offset), colors)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.canvas.fill_solid(&area.translate(self.offset), color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let translated_bounds = Rectangle {
            top_left: self.offset,
            size: self.size,
        };

        self.canvas.fill_solid(&translated_bounds, color)
    }
}
