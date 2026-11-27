mod backdrop;
mod buttons;

use crate::{
    geometry::ScaleFactor,
    image::RgbaImage,
    overlay::Overlay,
    selection::{Handle, Selection, handle_size},
    toolbar::Toolbar,
    view::View,
};
use backdrop::Backdrop;
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
    image: &RgbaImage,
    selection: Option<Selection>,
    scale: ScaleFactor,
    buffer: &mut [u8],
) -> Result<(), RenderError> {
    let size = (image.width(), image.height());
    let view = View::fit(size, size).ok_or(RenderError::InvalidBuffer)?;
    if selection.is_some_and(|s| s.rect().right() > size.0 || s.rect().bottom() > size.1) {
        return Err(RenderError::SelectionOutsideImage);
    }
    FrameRenderer::new(image).draw(view, selection, scale, None, buffer)
}

pub struct FrameRenderer<'a> {
    backdrop: Backdrop<'a>,
}

impl<'a> FrameRenderer<'a> {
    pub fn new(image: &'a RgbaImage) -> Self {
        Self {
            backdrop: Backdrop::new(image),
        }
    }

    pub fn render(&mut self, overlay: &Overlay, buffer: &mut [u8]) -> Result<(), RenderError> {
        self.draw(
            overlay.view(),
            overlay.selection(),
            overlay.scale(),
            overlay.toolbar(),
            buffer,
        )
    }

    fn draw(
        &mut self,
        view: View,
        selection: Option<Selection>,
        scale: ScaleFactor,
        toolbar: Option<Toolbar>,
        buffer: &mut [u8],
    ) -> Result<(), RenderError> {
        let (width, height) = view.window;
        if buffer.len() as u64 / 4 != u64::from(width) * u64::from(height)
            || !buffer.len().is_multiple_of(4)
        {
            return Err(RenderError::InvalidBuffer);
        }
        let mut frame =
            PixmapMut::from_bytes(buffer, width, height).ok_or(RenderError::InvalidBuffer)?;
        self.backdrop.draw(view, &mut frame)?;
        draw_selection(&mut frame, view, selection, scale);
        if let Some(bar) = toolbar {
            buttons::draw(&mut frame, bar);
        }
        Ok(())
    }
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
    use crate::geometry::PixelPoint;

    #[test]
    fn fitted_frame_draws_letterbox_crop_and_toolbar_and_packs_in_place() {
        use crate::geometry::Point;
        let image = RgbaImage::new(400, 200, [200, 60, 10, 255].repeat(400 * 200)).unwrap();
        let view = View::fit((400, 200), (200, 200)).unwrap();
        let mut overlay = Overlay::new(view, ScaleFactor::ONE);
        overlay.pointer_moved(Point { x: 20.0, y: 70.0 });
        overlay.press();
        overlay.pointer_moved(Point { x: 130.0, y: 120.0 });
        overlay.release();
        let mut renderer = FrameRenderer::new(&image);
        let mut words = vec![0; 200 * 200];
        renderer
            .render(&overlay, bytemuck::cast_slice_mut(&mut words))
            .unwrap();
        pack_frame(&mut words);
        assert_eq!(words[0], 0);
        assert_eq!(words[60 * 200 + 5], 0x641e05);
        assert_eq!(words[90 * 200 + 60], 0xc83c0a);
        for (_, r) in overlay.toolbar().unwrap().buttons() {
            let index = ((r.top() + 4) * 200 + r.left() + 8) as usize;
            assert_eq!(words[index], 0x21252b);
        }
        let first = words.clone();
        renderer
            .render(&overlay, bytemuck::cast_slice_mut(&mut words))
            .unwrap();
        pack_frame(&mut words);
        assert_eq!(words, first);
    }

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
        assert!(
            frame
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [64, 0, 0, 192])
        );
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
