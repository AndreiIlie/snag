use super::{Annotation, Tool};
use crate::geometry::{PixelPoint, Point};
use std::f64::consts::FRAC_PI_2;

fn segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx * dx + dy * dy;
    let t = if len == 0. {
        0.
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len).clamp(0., 1.)
    };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}

impl Annotation {
    /// Tip and base corners of the filled arrow head, in capture coordinates.
    pub(crate) fn arrow_head(&self) -> [Point; 3] {
        let (start, end) = self.ends();
        let (a, b) = (Point::from(start), Point::from(end));
        let len = (b.x - a.x).hypot(b.y - a.y);
        let head = (4. * self.width()).min(len);
        let (dx, dy) = ((b.x - a.x) / len.max(1.), (b.y - a.y) / len.max(1.));
        let (x, y) = (b.x - dx * head, b.y - dy * head);
        [
            b,
            Point {
                x: x - dy * head * 0.5,
                y: y + dx * head * 0.5,
            },
            Point {
                x: x + dy * head * 0.5,
                y: y - dx * head * 0.5,
            },
        ]
    }
    pub(crate) fn bounds(&self) -> [f64; 4] {
        let (a, b) = self.ends();
        let margin = match self.tool {
            Tool::Counter => self.radius(),
            Tool::Pixelate | Tool::Text => 0.,
            _ => self.stroke() / 2.,
        };
        let mut rect = [
            a.x.min(b.x) as f64,
            a.y.min(b.y) as f64,
            a.x.max(b.x) as f64,
            a.y.max(b.y) as f64,
        ];
        let mut include = |p: Point| {
            rect[0] = rect[0].min(p.x);
            rect[1] = rect[1].min(p.y);
            rect[2] = rect[2].max(p.x);
            rect[3] = rect[3].max(p.y);
        };
        match self.tool {
            Tool::Pen | Tool::Highlighter => self.points().for_each(|p| include(p.into())),
            Tool::Arrow => self.arrow_head().into_iter().for_each(include),
            Tool::Text if let Some(text) = &self.text => {
                let line = &text.line;
                let mut include_offset = |x: f32, y: f32| {
                    include(Point {
                        x: a.x as f64 + f64::from(x),
                        y: a.y as f64 + f64::from(y),
                    })
                };
                include_offset(0., -line.half_height);
                include_offset(*line.carets.last().unwrap(), line.half_height);
                if let Some(path) = &line.outline {
                    let ink = path.bounds();
                    include_offset(ink.left(), ink.top());
                    include_offset(ink.right(), ink.bottom());
                }
            }
            _ => {}
        }
        [
            rect[0] - margin,
            rect[1] - margin,
            rect[2] + margin,
            rect[3] + margin,
        ]
    }
    pub(super) fn hit(&self, pixel: PixelPoint, tolerance: f64) -> bool {
        let p = Point::from(pixel);
        let [l, t, r, b] = self.bounds();
        if p.x < l - tolerance || p.x > r + tolerance || p.y < t - tolerance || p.y > b + tolerance
        {
            return false;
        }
        let (start, end) = self.ends();
        let (a, b) = (Point::from(start), Point::from(end));
        let d = tolerance + self.stroke() / 2.;
        match self.tool {
            Tool::Counter => (p.x - a.x).hypot(p.y - a.y) <= self.radius() + tolerance,
            Tool::Pixelate | Tool::Text => true,
            Tool::Line => segment(p, a, b) <= d,
            Tool::Arrow => {
                let [tip, left, right] = self.arrow_head();
                let cross =
                    |a: Point, b: Point| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
                let sides = [cross(tip, left), cross(left, right), cross(right, tip)];
                segment(p, a, b) <= d
                    || sides.iter().all(|s| *s >= 0.)
                    || sides.iter().all(|s| *s <= 0.)
                    || [
                        segment(p, tip, left),
                        segment(p, left, right),
                        segment(p, right, tip),
                    ]
                    .into_iter()
                    .any(|v| v <= tolerance)
            }
            Tool::Rectangle => {
                let c = Point { x: a.x, y: b.y };
                let e = Point { x: b.x, y: a.y };
                [
                    segment(p, a, c),
                    segment(p, c, b),
                    segment(p, b, e),
                    segment(p, e, a),
                ]
                .into_iter()
                .any(|v| v <= d)
            }
            Tool::Pen | Tool::Highlighter => self
                .points()
                .zip(self.points().skip(1))
                .any(|(a, b)| segment(p, a.into(), b.into()) <= d),
            Tool::Ellipse => {
                let (rx, ry) = ((b.x - a.x).abs() / 2., (b.y - a.y).abs() / 2.);
                let (x, y) = (
                    (p.x - (a.x + b.x) / 2.).abs(),
                    (p.y - (a.y + b.y) / 2.).abs(),
                );
                // In the point's quadrant distance to the ellipse has one minimum.
                let distance = |angle: f64| (rx * angle.cos() - x).hypot(ry * angle.sin() - y);
                let (mut lo, mut hi) = (0., FRAC_PI_2);
                for _ in 0..40 {
                    let step = (hi - lo) / 3.;
                    if distance(lo + step) < distance(hi - step) {
                        hi -= step;
                    } else {
                        lo += step;
                    }
                }
                distance((lo + hi) / 2.)
                    .min(distance(0.))
                    .min(distance(FRAC_PI_2))
                    <= d
            }
        }
    }
}
