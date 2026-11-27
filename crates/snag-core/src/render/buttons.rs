use crate::{
    annotation::Tool,
    overlay::Overlay,
    toolbar::{Action, Button, Toolbar},
};
use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Rect, Stroke, Transform,
};

pub(super) fn draw(frame: &mut PixmapMut<'_>, bar: Toolbar, overlay: &Overlay) {
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
        let mut active = Paint::default();
        active.set_color_rgba8(28, 104, 172, 255);
        frame.fill_path(
            &background,
            if overlay.active(action) {
                &active
            } else {
                &dark
            },
            FillRule::Winding,
            transform,
            None,
        );
        let mut icon = PathBuilder::new();
        let polyline = |p: &mut PathBuilder, coords: &[(f32, f32)]| {
            p.move_to(coords[0].0, coords[0].1);
            for &(x, y) in &coords[1..] {
                p.line_to(x, y);
            }
        };
        match action {
            Button::Tool(Tool::Arrow) => {
                polyline(&mut icon, &[(7., 24.), (24., 7.), (21., 17.)]);
                polyline(&mut icon, &[(24., 7.), (14., 10.)]);
            }
            Button::Tool(Tool::Line) => polyline(&mut icon, &[(7., 24.), (24., 7.)]),
            Button::Tool(Tool::Rectangle) => {
                icon.push_rect(Rect::from_xywh(6., 9., 20., 14.).unwrap())
            }
            Button::Tool(Tool::Ellipse) => {
                icon.push_oval(Rect::from_xywh(6., 8., 20., 16.).unwrap())
            }
            Button::Tool(Tool::Pen) => {
                icon.move_to(5., 23.);
                icon.cubic_to(13., -1., 12., 28., 18., 18.);
                icon.quad_to(22., 10., 26., 10.);
            }
            Button::Tool(Tool::Highlighter) => {
                polyline(
                    &mut icon,
                    &[
                        (8., 18.),
                        (20., 6.),
                        (26., 12.),
                        (14., 24.),
                        (8., 18.),
                        (6., 26.),
                        (14., 24.),
                    ],
                );
            }
            Button::Tool(Tool::Pixelate) => {
                for (x, y) in [(7., 7.), (16., 7.), (7., 16.), (16., 16.)] {
                    icon.push_rect(Rect::from_xywh(x, y, 6., 6.).unwrap());
                }
            }
            Button::Tool(Tool::Counter) => {
                icon.push_circle(16., 16., 10.);
                polyline(&mut icon, &[(13., 14.), (16., 11.), (16., 21.)]);
            }
            Button::Undo | Button::Redo => {
                let x = |n| if action == Button::Redo { 32. - n } else { n };
                icon.move_to(x(25.), 23.);
                icon.quad_to(x(25.), 12., x(16.), 12.);
                icon.line_to(x(7.), 12.);
                polyline(&mut icon, &[(x(13.), 6.), (x(7.), 12.), (x(13.), 18.)]);
            }
            Button::Colour(c) => {
                let [r, g, b] = c.rgb();
                let mut paint = Paint::default();
                paint.set_color_rgba8(r, g, b, 255);
                icon.push_circle(16., 16., 9.);
                frame.fill_path(
                    &icon.finish().unwrap(),
                    &paint,
                    FillRule::Winding,
                    transform,
                    None,
                );
                continue;
            }
            Button::Width(w) => {
                icon.push_circle(16., 16., w.logical() as f32);
                frame.fill_path(
                    &icon.finish().unwrap(),
                    &white,
                    FillRule::Winding,
                    transform,
                    None,
                );
                continue;
            }
            Button::Close(Action::Copy) => {
                icon.move_to(10.0, 20.0);
                icon.line_to(8.0, 20.0);
                icon.line_to(8.0, 8.0);
                icon.line_to(19.0, 8.0);
                icon.line_to(19.0, 10.0);
                icon.push_rect(Rect::from_xywh(12.0, 12.0, 11.0, 13.0).unwrap());
            }
            Button::Close(Action::Save) => {
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
            Button::Close(Action::Cancel) => {
                icon.move_to(10.0, 10.0);
                icon.line_to(22.0, 22.0);
                icon.move_to(22.0, 10.0);
                icon.line_to(10.0, 22.0);
            }
        }
        frame.stroke_path(&icon.finish().unwrap(), &white, &stroke, transform, None);
    }
}
