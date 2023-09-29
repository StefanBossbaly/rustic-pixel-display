use std::convert::Infallible;

use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{DrawTarget, OriginDimensions, Point, RgbColor, Size},
    Pixel,
};

use super::Render;

#[derive(Debug)]
pub struct CachedCanvas {
    size: Size,
    pixels: Box<[Rgb888]>,
}

impl CachedCanvas {
    pub fn new(size: Size) -> CachedCanvas {
        let num_of_pixels = size.width as usize * size.height as usize;

        Self {
            size,
            pixels: vec![Rgb888::BLACK; num_of_pixels].into_boxed_slice(),
        }
    }

    fn convert_point_to_offset(&self, point: Point) -> Option<usize> {
        let (x, y) = (point.x as u32, point.y as u32);
        if x < self.size.width && y < self.size.height {
            Some((x + y * self.size.width) as usize)
        } else {
            None
        }
    }

    fn convert_offset_to_point(&self, offset: usize) -> Option<Point> {
        let x = offset as u32 % self.size.width;
        let y = offset as u32 / self.size.width;

        if x < self.size.width && y < self.size.height {
            Some(Point {
                x: x as i32,
                y: y as i32,
            })
        } else {
            None
        }
    }
}

impl OriginDimensions for CachedCanvas {
    fn size(&self) -> Size {
        self.size
    }
}

impl DrawTarget for CachedCanvas {
    type Color = Rgb888;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels.into_iter() {
            if let Some(index) = self.convert_point_to_offset(point) {
                self.pixels[index] = color;
            }
        }

        Ok(())
    }
}

impl<D> Render<D> for CachedCanvas
where
    D: DrawTarget<Color = Rgb888, Error = Infallible>,
{
    fn render(&self, canvas: &mut D) -> Result<(), D::Error> {
        let pixels = self
            .pixels
            .clone()
            .iter()
            .enumerate()
            .map(|(offset, color)| Pixel(self.convert_offset_to_point(offset).unwrap(), *color))
            .collect::<Vec<_>>();

        canvas.draw_iter(pixels)?;

        Ok(())
    }
}
