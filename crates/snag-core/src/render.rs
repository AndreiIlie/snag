use crate::{
    geometry::ScaleFactor,
    image::RgbaImage,
    selection::{Handle, Selection, handle_size},
};
use std::{error::Error, fmt};
use tiny_skia::{ColorU8, Paint, PixmapMut, Rect, Transform};

const DIM: [u8; 4] = [0, 0, 0, 128];
const BORDER: [u8; 4] = [60, 155, 255, 255];
const HANDLE: [u8; 4] = [255, 255, 255, 255];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderError {
    InvalidBuffer,
    SelectionOutsideImage,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidBuffer => "invalid overlay buffer size",
            Self::SelectionOutsideImage => "selection extends outside the screenshot",
        })
    }
}
impl Error for RenderError {}

/// Write one tightly packed premultiplied RGBA8 frame into the caller's buffer.
pub fn render_overlay(
    image: &RgbaImage,
    selection: Option<Selection>,
    scale: ScaleFactor,
    buffer: &mut [u8],
) -> Result<(), RenderError> {
    if buffer.len() != image.pixels().len() {
        return Err(RenderError::InvalidBuffer);
    }
    if selection
        .is_some_and(|s| s.rect().right() > image.width() || s.rect().bottom() > image.height())
    {
        return Err(RenderError::SelectionOutsideImage);
    }
    let mut frame = PixmapMut::from_bytes(buffer, image.width(), image.height())
        .ok_or(RenderError::InvalidBuffer)?;
    for (dest, &[r, g, b, a]) in frame
        .pixels_mut()
        .iter_mut()
        .zip(image.pixels().as_chunks::<4>().0)
    {
        *dest = ColorU8::from_rgba(r, g, b, a).premultiply();
    }

    let width = f64::from(image.width());
    let height = f64::from(image.height());
    let mut fill = |left: f64, top: f64, right: f64, bottom: f64, [r, g, b, a]: [u8; 4]| {
        if let Some(rect) = Rect::from_ltrb(
            left.max(0.0) as f32,
            top.max(0.0) as f32,
            right.min(width) as f32,
            bottom.min(height) as f32,
        ) {
            let mut paint = Paint {
                anti_alias: false,
                ..Paint::default()
            };
            paint.set_color_rgba8(r, g, b, a);
            frame.fill_rect(rect, &paint, Transform::identity(), None);
        }
    };
    let Some(selection) = selection else {
        fill(0.0, 0.0, width, height, DIM);
        return Ok(());
    };
    let rect = selection.rect();
    let left = f64::from(rect.left());
    let top = f64::from(rect.top());
    let right = f64::from(rect.right());
    let bottom = f64::from(rect.bottom());
    fill(0.0, 0.0, width, top, DIM);
    fill(0.0, bottom, width, height, DIM);
    fill(0.0, top, left, bottom, DIM);
    fill(right, top, width, bottom, DIM);

    let stroke = scale.get().round().max(1.0);
    fill(left, top, right, (top + stroke).min(bottom), BORDER);
    fill(left, (bottom - stroke).max(top), right, bottom, BORDER);
    fill(left, top, (left + stroke).min(right), bottom, BORDER);
    fill((right - stroke).max(left), top, right, bottom, BORDER);

    let side = handle_size(scale).min(2.0 * width.max(height) + 2.0);
    for handle in Handle::ALL {
        let center = selection.handle_position(handle);
        let x = center.x as f64 - (side / 2.0).floor();
        let y = center.y as f64 - (side / 2.0).floor();
        fill(x, y, x + side, y + side, HANDLE);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selection::PixelPoint;

    #[test]
    fn renders_dim_selection_and_scaled_controls_into_reused_buffer() {
        let image = RgbaImage::new(80, 80, [255, 0, 0, 128].repeat(80 * 80)).unwrap();
        let selection = Selection::from_drag(
            PixelPoint { x: 12, y: 12 },
            PixelPoint { x: 68, y: 68 },
            80,
            80,
        );
        let mut frame = vec![19; image.pixels().len()];
        fn pixel(frame: &[u8], x: usize, y: usize) -> &[u8] {
            let start = (y * 80 + x) * 4;
            &frame[start..start + 4]
        }
        render_overlay(&image, None, ScaleFactor::ONE, &mut frame).unwrap();
        assert!(frame.chunks_exact(4).all(|p| p == [64, 0, 0, 192]));
        render_overlay(&image, selection, ScaleFactor::ONE, &mut frame).unwrap();
        assert_eq!(pixel(&frame, 0, 0), [64, 0, 0, 192]);
        assert_eq!(pixel(&frame, 40, 40), [128, 0, 0, 128]);
        assert_eq!(pixel(&frame, 22, 12), BORDER);
        assert_eq!(pixel(&frame, 22, 13), [128, 0, 0, 128]);
        assert_eq!(pixel(&frame, 42, 12), HANDLE);
        assert_eq!(pixel(&frame, 43, 12), BORDER);
        let first = frame.clone();
        render_overlay(&image, selection, ScaleFactor::ONE, &mut frame).unwrap();
        assert_eq!(frame, first);
        render_overlay(
            &image,
            selection,
            ScaleFactor::new(2.0).unwrap(),
            &mut frame,
        )
        .unwrap();
        assert_eq!(pixel(&frame, 22, 13), BORDER);
        assert_eq!(pixel(&frame, 22, 14), [128, 0, 0, 128]);
        assert_eq!(pixel(&frame, 45, 12), HANDLE);
        assert_eq!(pixel(&frame, 46, 12), BORDER);
    }
}
