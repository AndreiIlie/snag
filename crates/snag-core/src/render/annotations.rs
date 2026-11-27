use super::mosaic::Mosaic;
use crate::{
    annotation::{Annotation, Tool},
    geometry::PixelRect,
};
use tiny_skia::{Color, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Rect, Stroke, Transform};

pub(crate) fn draw_layer(
    frame: &mut PixmapMut<'_>,
    ts: Transform,
    shape: &Annotation,
    count: &mut usize,
    mosaic: Option<&Mosaic>,
) {
    if shape.tool == Tool::Pixelate {
        if let Some(blocks) = mosaic {
            blocks.draw(frame, ts);
        }
        return;
    }
    let [r, g, b] = shape.style.colour.rgb();
    let colour = Color::from_rgba8(
        r,
        g,
        b,
        if shape.tool == Tool::Highlighter {
            102
        } else {
            255
        },
    );
    let stroke = Stroke {
        width: shape.stroke() as f32,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    let mut path = PathBuilder::new();
    let (a, b) = shape.ends();
    match shape.tool {
        Tool::Rectangle | Tool::Ellipse => {
            let Some(bounds) = Rect::from_ltrb(
                a.x.min(b.x) as f32,
                a.y.min(b.y) as f32,
                a.x.max(b.x) as f32,
                a.y.max(b.y) as f32,
            ) else {
                return;
            };
            if shape.tool == Tool::Rectangle {
                path.push_rect(bounds);
            } else {
                path.push_oval(bounds);
            }
        }
        Tool::Arrow => {
            let [tip, left, right] = shape.arrow_head();
            let base = ((left.x + right.x) / 2., (left.y + right.y) / 2.);
            let mut head = PathBuilder::new();
            head.move_to(tip.x as f32, tip.y as f32);
            head.line_to(left.x as f32, left.y as f32);
            head.line_to(right.x as f32, right.y as f32);
            head.close();
            if let Some(p) = head.finish() {
                super::path::draw(frame, ts, p, colour, None);
            }
            path.move_to(a.x as f32, a.y as f32);
            path.line_to(base.0 as f32, base.1 as f32);
        }
        Tool::Counter => {
            *count += 1;
            let radius = shape.radius() as f32;
            if let Some(circle) = PathBuilder::from_circle(a.x as f32, a.y as f32, radius) {
                super::path::draw(frame, ts, circle, colour, None);
            }
            let [r, g, b] = shape.style.colour.rgb().map(u32::from);
            let c = if 299 * r + 587 * g + 114 * b > 128000 {
                0
            } else {
                255
            };
            let ink = Color::from_rgba8(c, c, c, 255);
            digits(frame, ts, (a.x as f32, a.y as f32), radius, *count, ink);
            return;
        }
        Tool::Pen | Tool::Highlighter | Tool::Line => {
            path.move_to(a.x as f32, a.y as f32);
            for p in shape.points().skip(1) {
                path.line_to(p.x as f32, p.y as f32);
            }
        }
        Tool::Pixelate => unreachable!(),
    }
    if let Some(path) = path.finish() {
        super::path::draw(frame, ts, path, colour, Some(&stroke));
    }
}

// Seven strokes in a 1-by-2 cell, clockwise from the top, then the middle bar.
fn digits(
    frame: &mut PixmapMut<'_>,
    ts: Transform,
    (cx, cy): (f32, f32),
    radius: f32,
    number: usize,
    colour: Color,
) {
    const DIGITS: [u8; 10] = [0x3f, 0x06, 0x5b, 0x4f, 0x66, 0x6d, 0x7d, 0x07, 0x7f, 0x6f];
    const SEGMENTS: [(f32, f32, f32, f32); 7] = [
        (0., 0., 1., 0.),
        (1., 0., 1., 1.),
        (1., 1., 1., 2.),
        (1., 2., 0., 2.),
        (0., 2., 0., 1.),
        (0., 1., 0., 0.),
        (0., 1., 1., 1.),
    ];
    let text = number.to_string();
    let width = text.len() as f32 * 1.6 - 0.6;
    let unit = (radius * 0.58).min(radius * 1.5 / width);
    let left = cx - unit * width / 2.;
    let top = cy - unit;
    let mut path = PathBuilder::new();
    for (i, digit) in text.bytes().enumerate() {
        let x = left + i as f32 * 1.6 * unit;
        for (j, (x1, y1, x2, y2)) in SEGMENTS.into_iter().enumerate() {
            if DIGITS[(digit - b'0') as usize] & (1 << j) != 0 {
                path.move_to(x + x1 * unit, top + y1 * unit);
                path.line_to(x + x2 * unit, top + y2 * unit);
            }
        }
    }
    let stroke = Stroke {
        width: 0.23 * unit,
        line_cap: LineCap::Round,
        ..Stroke::default()
    };
    if let Some(path) = path.finish() {
        super::path::draw(frame, ts, path, colour, Some(&stroke));
    }
}

pub(super) fn marker(
    frame: &mut PixmapMut<'_>,
    shape: &Annotation,
    ts: Transform,
    clip: PixelRect,
    width: f32,
) {
    let [l, t, r, b] = shape.bounds();
    let (l, t, r, b) = (
        l as f32 * ts.sx + ts.tx,
        t as f32 * ts.sy + ts.ty,
        r as f32 * ts.sx + ts.tx,
        b as f32 * ts.sy + ts.ty,
    );
    let mut paint = Paint {
        anti_alias: false,
        ..Paint::default()
    };
    let [red, green, blue, alpha] = super::BORDER;
    paint.set_color_rgba8(red, green, blue, alpha);
    for (x, y, end, bottom) in [
        (l - width, t - width, r + width, t),
        (l - width, b, r + width, b + width),
        (l - width, t, l, b),
        (r, t, r + width, b),
    ] {
        if let Some(rect) = Rect::from_ltrb(
            x.max(clip.left() as f32),
            y.max(clip.top() as f32),
            end.min(clip.right() as f32),
            bottom.min(clip.bottom() as f32),
        ) {
            frame.fill_rect(rect, &paint, Transform::identity(), None);
        }
    }
}
