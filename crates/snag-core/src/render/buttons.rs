use crate::toolbar::{Action, Toolbar};
use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Rect, Stroke, Transform,
};

pub(super) fn draw(frame: &mut PixmapMut<'_>, bar: Toolbar) {
    // All icons use the same 32-unit square, scaled with their hit boxes.
    let mut background = PathBuilder::new();
    background.move_to(5.0, 0.0);
    background.line_to(27.0, 0.0);
    background.quad_to(32.0, 0.0, 32.0, 5.0);
    background.line_to(32.0, 27.0);
    background.quad_to(32.0, 32.0, 27.0, 32.0);
    background.line_to(5.0, 32.0);
    background.quad_to(0.0, 32.0, 0.0, 27.0);
    background.line_to(0.0, 5.0);
    background.quad_to(0.0, 0.0, 5.0, 0.0);
    background.close();
    let background = background.finish().unwrap();
    let mut dark = Paint::default();
    dark.set_color_rgba8(33, 37, 43, 255);
    let mut white = Paint::default();
    white.set_color_rgba8(255, 255, 255, 255);
    let stroke = Stroke {
        width: 2.0,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    for (action, rect) in bar.buttons() {
        let scale = rect.width() as f32 / 32.0;
        let transform = Transform::from_row(
            scale,
            0.0,
            0.0,
            scale,
            rect.left() as f32,
            rect.top() as f32,
        );
        frame.fill_path(&background, &dark, FillRule::Winding, transform, None);
        let mut icon = PathBuilder::new();
        match action {
            Action::Copy => {
                icon.move_to(10.0, 20.0);
                icon.line_to(8.0, 20.0);
                icon.line_to(8.0, 8.0);
                icon.line_to(19.0, 8.0);
                icon.line_to(19.0, 10.0);
                icon.push_rect(Rect::from_xywh(12.0, 12.0, 11.0, 13.0).unwrap());
            }
            Action::Save => {
                icon.move_to(16.0, 7.0);
                icon.line_to(16.0, 19.0);
                icon.move_to(11.0, 14.0);
                icon.line_to(16.0, 19.0);
                icon.line_to(21.0, 14.0);
                icon.move_to(8.0, 20.0);
                icon.line_to(8.0, 24.0);
                icon.line_to(24.0, 24.0);
                icon.line_to(24.0, 20.0);
            }
            Action::Cancel => {
                icon.move_to(10.0, 10.0);
                icon.line_to(22.0, 22.0);
                icon.move_to(22.0, 10.0);
                icon.line_to(10.0, 22.0);
            }
        }
        frame.stroke_path(&icon.finish().unwrap(), &white, &stroke, transform, None);
    }
}
