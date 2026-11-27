pub(crate) mod annotations;
mod backdrop;
mod buttons;
pub(crate) mod mosaic;
mod path;

use crate::{
    geometry::ScaleFactor,
    overlay::Overlay,
    selection::{Handle, Selection, handle_size},
    view::View,
};
pub use backdrop::Backdrop;
use std::{error::Error, fmt};
use tiny_skia::{Paint, PixmapMut, Rect, Transform};

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
    backdrop: &mut Backdrop<'_>,
    overlay: &Overlay,
    buffer: &mut [u8],
) -> Result<(), RenderError> {
    let view = overlay.view();
    let (width, height) = view.window;
    if buffer.len() as u64 / 4 != u64::from(width) * u64::from(height)
        || !buffer.len().is_multiple_of(4)
    {
        return Err(RenderError::InvalidBuffer);
    }
    if overlay
        .crop()
        .is_some_and(|r| r.right() > view.capture.0 || r.bottom() > view.capture.1)
    {
        return Err(RenderError::SelectionOutsideImage);
    }
    let mut frame =
        PixmapMut::from_bytes(buffer, width, height).ok_or(RenderError::InvalidBuffer)?;
    backdrop.draw(view, &mut frame)?;
    let bounds = view.bounds;
    let ts = Transform::from_row(
        view.scale as f32,
        0.,
        0.,
        view.scale as f32,
        bounds.left() as f32,
        bounds.top() as f32,
    );
    backdrop.mosaics.update(backdrop.image, overlay.layers())?;
    let mut count = 0;
    for (i, shape) in overlay.layers().enumerate() {
        annotations::draw_layer(&mut frame, ts, shape, &mut count, backdrop.mosaics.get(i));
    }
    if overlay.layers().next().is_some() && (bounds.width(), bounds.height()) != view.window {
        // Restore the margins after painting strokes that cross the capture edges.
        let black = tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 0, 255).unwrap();
        for (y, row) in frame
            .pixels_mut()
            .chunks_exact_mut(width as usize)
            .enumerate()
        {
            if (bounds.top() as usize..bounds.bottom() as usize).contains(&y) {
                row[..bounds.left() as usize].fill(black);
                row[bounds.right() as usize..].fill(black);
            } else {
                row.fill(black);
            }
        }
    }
    draw_selection(&mut frame, view, overlay.selection(), overlay.scale());
    let border_width = overlay.scale().get().round().max(1.) as f32;
    if let Some([x, top, bottom]) = overlay.text_caret() {
        let x = (x * ts.sx + ts.tx).round();
        annotations::fill_border(
            &mut frame,
            bounds,
            [
                x,
                top * ts.sy + ts.ty,
                x + border_width,
                bottom * ts.sy + ts.ty,
            ],
        );
    }
    if let Some(shape) = overlay.selected() {
        annotations::marker(&mut frame, shape, ts, bounds, border_width);
    }
    if let Some(bar) = overlay.toolbar() {
        buttons::draw(&mut frame, bar, overlay);
    }
    Ok(())
}

/// Convert premultiplied RGBA over black to native presentation words in place.
pub fn pack_frame(words: &mut [u32]) {
    for word in words {
        let [r, g, b, _] = word.to_ne_bytes();
        *word = u32::from_be_bytes([0, r, g, b]);
    }
}

fn draw_selection(
    frame: &mut PixmapMut<'_>,
    view: View,
    selection: Option<Selection>,
    scale: ScaleFactor,
) {
    let (width, height) = (f64::from(frame.width()), f64::from(frame.height()));
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
        return;
    };
    let rect = view.window_rect(selection.rect());
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
        let center = view.window_point(selection.handle_position(handle));
        let x = center.x as f64 - (side / 2.0).floor();
        let y = center.y as f64 - (side / 2.0).floor();
        fill(x, y, x + side, y + side, HANDLE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{geometry::Point, image::RgbaImage};

    #[test]
    fn fitted_frame_draws_letterbox_crop_and_toolbar_and_packs_in_place() {
        let image = RgbaImage::new(400, 200, [200, 60, 10, 255].repeat(400 * 200)).unwrap();
        let view = View::fit((400, 200), (200, 200)).unwrap();
        let mut overlay = Overlay::new(view, ScaleFactor::ONE);
        overlay.pointer_moved(Point { x: 20.0, y: 70.0 });
        overlay.press();
        overlay.pointer_moved(Point { x: 130.0, y: 120.0 });
        overlay.release();
        let mut backdrop = Backdrop::new(&image);
        let mut words = vec![0; 200 * 200];
        render_overlay(
            &mut backdrop,
            &overlay,
            bytemuck::cast_slice_mut(&mut words),
        )
        .unwrap();
        pack_frame(&mut words);
        assert_eq!(words[0], 0);
        assert_eq!(words[60 * 200 + 5], 0x641e05);
        assert_eq!(words[90 * 200 + 60], 0xc83c0a);
        for (button, r) in overlay.toolbar().unwrap().buttons() {
            let background = if overlay.active(button) {
                0x1c68ac
            } else {
                0x21252b
            };
            // In compact buttons an antialiased icon can reach the top row.
            assert!(
                (r.top()..r.bottom()).any(|y| {
                    let row = (y * 200) as usize;
                    words[row + r.left() as usize..row + r.right() as usize].contains(&background)
                }),
                "{button:?}"
            );
        }
        let first = words.clone();
        render_overlay(
            &mut backdrop,
            &overlay,
            bytemuck::cast_slice_mut(&mut words),
        )
        .unwrap();
        pack_frame(&mut words);
        assert_eq!(words, first);
    }

    #[test]
    fn renders_dim_selection_and_scaled_controls_into_reused_buffer() {
        let image = RgbaImage::new(80, 80, [255, 0, 0, 128].repeat(80 * 80)).unwrap();
        let view = View::fit((80, 80), (80, 80)).unwrap();
        let mut overlay = Overlay::new(view, ScaleFactor::ONE);
        let mut backdrop = Backdrop::new(&image);
        let mut frame = vec![19; image.pixels().len()];
        fn pixel(frame: &[u8], x: usize, y: usize) -> &[u8] {
            let start = (y * 80 + x) * 4;
            &frame[start..start + 4]
        }
        render_overlay(&mut backdrop, &overlay, &mut frame).unwrap();
        assert!(
            frame
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [64, 0, 0, 192])
        );
        overlay.pointer_moved(Point { x: 12.0, y: 12.0 });
        overlay.press();
        overlay.pointer_moved(Point { x: 68.0, y: 68.0 });
        render_overlay(&mut backdrop, &overlay, &mut frame).unwrap();
        assert_eq!(pixel(&frame, 0, 0), [64, 0, 0, 192]);
        assert_eq!(pixel(&frame, 40, 40), [128, 0, 0, 128]);
        assert_eq!(pixel(&frame, 22, 12), BORDER);
        assert_eq!(pixel(&frame, 22, 13), [128, 0, 0, 128]);
        assert_eq!(pixel(&frame, 42, 12), HANDLE);
        assert_eq!(pixel(&frame, 43, 12), BORDER);
        let first = frame.clone();
        render_overlay(&mut backdrop, &overlay, &mut frame).unwrap();
        assert_eq!(frame, first);
        overlay.release();
        overlay.set_view(view, ScaleFactor::new(2.0).unwrap());
        render_overlay(&mut backdrop, &overlay, &mut frame).unwrap();
        assert_eq!(pixel(&frame, 22, 13), BORDER);
        assert_eq!(pixel(&frame, 22, 14), [128, 0, 0, 128]);
        assert_eq!(pixel(&frame, 45, 12), HANDLE);
        assert_eq!(pixel(&frame, 46, 12), BORDER);
    }
}
