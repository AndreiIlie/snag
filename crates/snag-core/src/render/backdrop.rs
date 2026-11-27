use super::RenderError;
use crate::{image::RgbaImage, view::View};
use tiny_skia::{Color, ColorU8, Pixmap, PixmapMut};

/// Holds at most one resampled capture for the lifetime of the frozen image.
pub struct Backdrop<'a> {
    pub(super) image: &'a RgbaImage,
    pub(super) mosaics: super::mosaic::MosaicCache,
    scaled: Option<Pixmap>,
}

impl<'a> Backdrop<'a> {
    pub fn new(image: &'a RgbaImage) -> Self {
        Self {
            image,
            scaled: None,
            mosaics: Default::default(),
        }
    }

    pub(super) fn draw(
        &mut self,
        view: View,
        frame: &mut PixmapMut<'_>,
    ) -> Result<(), RenderError> {
        let b = view.bounds;
        let size = (b.width(), b.height());
        if view.capture != (self.image.width(), self.image.height()) {
            return Err(RenderError::InvalidBuffer);
        }
        if size == view.capture {
            self.scaled = None;
        } else if self.scaled.as_ref().map(|p| (p.width(), p.height())) != Some(size) {
            self.scaled = Some(resample(self.image, size)?);
        }

        if size != view.window {
            frame.fill(Color::BLACK);
        }
        let width = frame.width() as usize;
        for (y, row) in frame
            .pixels_mut()
            .chunks_exact_mut(width)
            .skip(b.top() as usize)
            .take(b.height() as usize)
            .enumerate()
        {
            let dest = &mut row[b.left() as usize..b.right() as usize];
            let start = y * size.0 as usize;
            let end = start + size.0 as usize;
            if let Some(scaled) = &self.scaled {
                dest.copy_from_slice(&scaled.pixels()[start..end]);
            } else {
                let src = &self.image.pixels().as_chunks::<4>().0[start..end];
                for (pixel, &[r, g, b, a]) in dest.iter_mut().zip(src) {
                    *pixel = ColorU8::from_rgba(r, g, b, a).premultiply();
                }
            }
        }
        Ok(())
    }
}

/// Interpolate premultiplied channels so transparent source colors cannot bleed.
fn resample(image: &RgbaImage, (width, height): (u32, u32)) -> Result<Pixmap, RenderError> {
    fn sample_axis(i: u32, src: u32, dst: u32) -> (usize, usize, f64) {
        let p = ((f64::from(i) + 0.5) * f64::from(src) / f64::from(dst) - 0.5)
            .clamp(0.0, f64::from(src - 1));
        (
            p as usize,
            (p as usize + 1).min(src as usize - 1),
            p.fract(),
        )
    }
    let mut out = Pixmap::new(width, height).ok_or(RenderError::InvalidBuffer)?;
    let source = image.pixels().as_chunks::<4>().0;
    let read = |x: usize, y: usize| {
        let [r, g, b, a] = source[y * image.width() as usize + x];
        let p = ColorU8::from_rgba(r, g, b, a).premultiply();
        [p.red(), p.green(), p.blue(), p.alpha()]
    };
    for (y, row) in out
        .data_mut()
        .chunks_exact_mut(width as usize * 4)
        .enumerate()
    {
        let (y0, y1, fy) = sample_axis(y as u32, image.height(), height);
        for (x, pixel) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let (x0, x1, fx) = sample_axis(x as u32, image.width(), width);
            let (tl, tr, bl, br) = (read(x0, y0), read(x1, y0), read(x0, y1), read(x1, y1));
            for c in 0..4 {
                let top = (1.0 - fx) * f64::from(tl[c]) + fx * f64::from(tr[c]);
                let bottom = (1.0 - fx) * f64::from(bl[c]) + fx * f64::from(br[c]);
                pixel[c] = ((1.0 - fy) * top + fy * bottom).round() as u8;
            }
        }
    }
    Ok(out)
}
