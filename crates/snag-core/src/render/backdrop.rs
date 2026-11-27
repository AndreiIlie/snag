use super::RenderError;
use crate::{image::RgbaImage, view::View};
use tiny_skia::{ColorU8, Pixmap, PixmapMut};

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

fn premultiply([r, g, b, a]: [u8; 4]) -> [u8; 4] {
    let p = ColorU8::from_rgba(r, g, b, a).premultiply();
    [p.red(), p.green(), p.blue(), p.alpha()]
}

fn average(tl: [u8; 4], tr: [u8; 4], bl: [u8; 4], br: [u8; 4]) -> [u8; 4] {
    std::array::from_fn(|ch| {
        let sum = u16::from(tl[ch]) + u16::from(tr[ch]) + u16::from(bl[ch]) + u16::from(br[ch]);
        ((sum + 2) / 4) as u8
    })
}

fn halve_row(top: &[[u8; 4]], bottom: &[[u8; 4]], row: &mut [[u8; 4]]) {
    // Test opacity by row so the common screenshot case can average whole rows
    // without a premultiplication branch at each source pixel.
    let opaque = |pixels: &[[u8; 4]]| pixels.iter().fold(255, |alpha, p| alpha & p[3]) == 255;
    let blocks = row
        .iter_mut()
        .zip(top.as_chunks::<2>().0)
        .zip(bottom.as_chunks::<2>().0);
    if opaque(top) && opaque(bottom) {
        for ((pixel, &[tl, tr]), &[bl, br]) in blocks {
            *pixel = average(tl, tr, bl, br);
        }
    } else {
        for ((pixel, &[tl, tr]), &[bl, br]) in blocks {
            *pixel = average(
                premultiply(tl),
                premultiply(tr),
                premultiply(bl),
                premultiply(br),
            );
        }
    }
}

// Ten fractional bits keep coordinate error at or below 1/2048 per axis, and the
// largest bilinear sum (255 * WEIGHT_SCALE squared) still fits in u32.
const WEIGHT_SCALE: u32 = 1024;

fn sample_axis(i: u32, src: u32, dst: u32) -> (usize, usize, u32) {
    // Map the destination pixel center to source pixels, clamping at both ends.
    let denominator = 2 * u128::from(dst);
    let numerator = ((2 * u128::from(i) + 1) * u128::from(src)).saturating_sub(u128::from(dst));
    let position = (numerator * u128::from(WEIGHT_SCALE) + denominator / 2) / denominator;
    let position = position.min(u128::from(src - 1) * u128::from(WEIGHT_SCALE)) as u64;
    let first = (position / u64::from(WEIGHT_SCALE)) as usize;
    (
        first,
        (first + 1).min(src as usize - 1),
        (position % u64::from(WEIGHT_SCALE)) as u32,
    )
}

/// Interpolate premultiplied channels so transparent source colors cannot bleed.
fn resample(image: &RgbaImage, (width, height): (u32, u32)) -> Result<Pixmap, RenderError> {
    let mut out = Pixmap::new(width, height).ok_or(RenderError::InvalidBuffer)?;
    let source = image.pixels().as_chunks::<4>().0;
    let stride = image.width() as usize;
    let rows = out
        .data_mut()
        .as_chunks_mut::<4>()
        .0
        .chunks_exact_mut(width as usize);

    if width.checked_mul(2) == Some(image.width()) && height.checked_mul(2) == Some(image.height())
    {
        for (row, pair) in rows.zip(source.chunks_exact(stride * 2)) {
            let (top, bottom) = pair.split_at(stride);
            halve_row(top, bottom, row);
        }
        return Ok(out);
    }

    let columns: Vec<_> = (0..width)
        .map(|x| sample_axis(x, image.width(), width))
        .collect();
    for (y, row) in rows.enumerate() {
        let (y0, y1, fy) = sample_axis(y as u32, image.height(), height);
        let top = &source[y0 * stride..(y0 + 1) * stride];
        let bottom = &source[y1 * stride..(y1 + 1) * stride];
        for (pixel, &(x0, x1, fx)) in row.iter_mut().zip(&columns) {
            let [tl, tr, bl, br] = [top[x0], top[x1], bottom[x0], bottom[x1]].map(premultiply);
            for ch in 0..4 {
                let upper = (WEIGHT_SCALE - fx) * u32::from(tl[ch]) + fx * u32::from(tr[ch]);
                let lower = (WEIGHT_SCALE - fx) * u32::from(bl[ch]) + fx * u32::from(br[ch]);
                let sum = (WEIGHT_SCALE - fy) * upper + fy * lower;
                pixel[ch] =
                    ((sum + WEIGHT_SCALE * WEIGHT_SCALE / 2) / (WEIGHT_SCALE * WEIGHT_SCALE)) as u8;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_pixel(image: &RgbaImage, size: (u32, u32), x: u32, y: u32) -> [u8; 4] {
        let axis = |i: u32, src: u32, dst: u32| {
            let p = ((f64::from(i) + 0.5) * f64::from(src) / f64::from(dst) - 0.5)
                .clamp(0.0, f64::from(src - 1));
            (p as u32, (p as u32 + 1).min(src - 1), p.fract())
        };
        let (x0, x1, fx) = axis(x, image.width(), size.0);
        let (y0, y1, fy) = axis(y, image.height(), size.1);
        let read = |x, y| {
            let [r, g, b, a] = image.pixels().as_chunks::<4>().0[(y * image.width() + x) as usize];
            let p = ColorU8::from_rgba(r, g, b, a).premultiply();
            [p.red(), p.green(), p.blue(), p.alpha()]
        };
        let (tl, tr, bl, br) = (read(x0, y0), read(x1, y0), read(x0, y1), read(x1, y1));
        std::array::from_fn(|ch| {
            let upper = (1.0 - fx) * f64::from(tl[ch]) + fx * f64::from(tr[ch]);
            let lower = (1.0 - fx) * f64::from(bl[ch]) + fx * f64::from(br[ch]);
            ((1.0 - fy) * upper + fy * lower).round() as u8
        })
    }

    #[test]
    fn scaled_pixels_match_bilinear_reference() {
        for (src, size) in [
            ((8, 6), (4, 3)),
            ((17, 13), (7, 5)),
            ((3, 5), (11, 19)),
            ((1, 7), (3, 1)),
            ((9, 1), (1, 5)),
        ] {
            let mut pixels = Vec::new();
            for y in 0..src.1 {
                for x in 0..src.0 {
                    let alpha = if y < 2 {
                        255
                    } else {
                        [0, 16, 127, 255][(x + y) as usize % 4]
                    };
                    pixels.extend_from_slice(&[
                        (x * 117 + y * 37) as u8,
                        (x * 29 + y * 151) as u8,
                        (x ^ (37 * y)) as u8,
                        alpha,
                    ]);
                }
            }
            let image = RgbaImage::new(src.0, src.1, pixels).unwrap();
            let out = resample(&image, size).unwrap();
            assert_eq!((out.width(), out.height()), size);
            assert_eq!(out.data(), resample(&image, size).unwrap().data());
            let tolerance = if src == (size.0 * 2, size.1 * 2) {
                0
            } else {
                1
            };
            for (i, pixel) in out.data().as_chunks::<4>().0.iter().enumerate() {
                let x = i as u32 % size.0;
                let y = i as u32 / size.0;
                let reference = reference_pixel(&image, size, x, y);
                for (&got, want) in pixel.iter().zip(reference) {
                    assert!(
                        got.abs_diff(want) <= tolerance,
                        "{src:?} to {size:?} at ({x}, {y}): {pixel:?} vs {reference:?}"
                    );
                }
            }
        }
    }
}
