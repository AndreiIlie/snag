use crate::geometry::{PixelPoint, PixelRect, Point};

/// The capture and its centred, aspect-preserving fit in physical window pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub(crate) capture: (u32, u32),
    pub(crate) window: (u32, u32),
    pub(crate) bounds: PixelRect,
    pub(crate) scale: f64,
}

impl View {
    pub fn fit(capture: (u32, u32), window: (u32, u32)) -> Option<Self> {
        let (cw, ch) = capture;
        let (ww, wh) = window;
        if [cw, ch, ww, wh].contains(&0) {
            return None;
        }
        let scale = (f64::from(ww) / f64::from(cw)).min(f64::from(wh) / f64::from(ch));
        let width = ((f64::from(cw) * scale).round() as u32).clamp(1, ww);
        let height = ((f64::from(ch) * scale).round() as u32).clamp(1, wh);
        let left = (ww - width) / 2;
        let top = (wh - height) / 2;
        Some(Self {
            capture,
            window,
            scale,
            bounds: PixelRect::new(left, top, left + width, top + height)?,
        })
    }

    pub fn capture_point(self, point: Point) -> Option<PixelPoint> {
        fn axis(p: f64, start: u32, end: u32, extent: u32, scale: f64) -> i64 {
            let pixel = p.floor();
            if pixel == f64::from(start) {
                0
            } else if pixel == f64::from(end - 1) {
                i64::from(extent)
            } else {
                ((p - f64::from(start)) / scale).round() as i64
            }
        }
        let b = self.bounds;
        (point.x.is_finite() && point.y.is_finite()).then(|| PixelPoint {
            x: axis(point.x, b.left(), b.right(), self.capture.0, self.scale),
            y: axis(point.y, b.top(), b.bottom(), self.capture.1, self.scale),
        })
    }

    pub fn window_point(self, point: PixelPoint) -> PixelPoint {
        let b = self.bounds;
        PixelPoint {
            x: (point.x as f64 * self.scale + f64::from(b.left())).round() as i64,
            y: (point.y as f64 * self.scale + f64::from(b.top())).round() as i64,
        }
    }

    pub fn window_rect(self, rect: PixelRect) -> PixelRect {
        let start = self.window_point(PixelPoint {
            x: rect.left().into(),
            y: rect.top().into(),
        });
        let end = self.window_point(PixelPoint {
            x: rect.right().into(),
            y: rect.bottom().into(),
        });
        let b = self.bounds;
        let left = (start.x as u32).clamp(b.left(), b.right() - 1);
        let top = (start.y as u32).clamp(b.top(), b.bottom() - 1);
        PixelRect::new(
            left,
            top,
            (end.x as u32).clamp(left + 1, b.right()),
            (end.y as u32).clamp(top + 1, b.bottom()),
        )
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_and_maps_capture_edges_and_letterbox() {
        for (window, first, last) in [
            ((200, 100), (0, 0), (199, 99)),
            ((100, 100), (0, 25), (99, 74)),
            ((600, 200), (100, 0), (499, 199)),
        ] {
            let view = View::fit((200, 100), window).unwrap();
            let p = |(x, y)| {
                view.capture_point(Point {
                    x: f64::from(x),
                    y: f64::from(y),
                })
                .unwrap()
            };
            assert_eq!(p(first), PixelPoint { x: 0, y: 0 });
            assert_eq!(p(last), PixelPoint { x: 200, y: 100 });
            assert!(p((first.0 - 5, first.1 - 5)).x < 0);
            let middle = PixelPoint { x: 100, y: 50 };
            let at = view.window_point(middle);
            assert_eq!(p((at.x as i32, at.y as i32)), middle);
            assert_eq!(
                view.window_rect(PixelRect::new(0, 0, 200, 100).unwrap()),
                view.bounds
            );
        }
    }
}
