/// A finite, positive ratio of image pixels to logical units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleFactor(f64);

impl ScaleFactor {
    pub const ONE: Self = Self(1.0);

    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value > 0.0).then_some(Self(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelPoint {
    pub x: i64,
    pub y: i64,
}

impl Point {
    /// Snap a logical pointer position to image pixels, including positions outside the image.
    pub fn to_pixels(self, scale: ScaleFactor) -> Option<PixelPoint> {
        (self.x.is_finite() && self.y.is_finite()).then(|| PixelPoint {
            x: (self.x * scale.get()).round() as i64,
            y: (self.y * scale.get()).round() as i64,
        })
    }
}

/// Opposite logical corners, in either drag direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub start: Point,
    pub end: Point,
}

impl Rect {
    /// Snap shared edges identically and intersect with the image.
    pub fn to_pixels(self, scale: ScaleFactor, width: u32, height: u32) -> Option<PixelRect> {
        PixelRect::from_drag(
            self.start.to_pixels(scale)?,
            self.end.to_pixels(scale)?,
            width,
            height,
        )
    }
}

/// A nonempty rectangle with exclusive right and bottom edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl PixelRect {
    pub(crate) fn from_drag(
        start: PixelPoint,
        end: PixelPoint,
        width: u32,
        height: u32,
    ) -> Option<Self> {
        let clamp_x = |x: i64| x.clamp(0, i64::from(width)) as u32;
        let clamp_y = |y: i64| y.clamp(0, i64::from(height)) as u32;
        Self::new(
            clamp_x(start.x.min(end.x)),
            clamp_y(start.y.min(end.y)),
            clamp_x(start.x.max(end.x)),
            clamp_y(start.y.max(end.y)),
        )
    }

    pub fn new(left: u32, top: u32, right: u32, bottom: u32) -> Option<Self> {
        (left < right && top < bottom).then_some(Self {
            left,
            top,
            right,
            bottom,
        })
    }

    pub fn left(self) -> u32 {
        self.left
    }
    pub fn top(self) -> u32 {
        self.top
    }
    pub fn right(self) -> u32 {
        self.right
    }
    pub fn bottom(self) -> u32 {
        self.bottom
    }
    pub fn width(self) -> u32 {
        self.right - self.left
    }
    pub fn height(self) -> u32 {
        self.bottom - self.top
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_rects_share_the_rounded_edge_at_each_scale() {
        for (factor, edge) in [(1.0, 3), (1.5, 5), (2.0, 6)] {
            let scale = ScaleFactor::new(factor).unwrap();
            let start = Point { x: -2.0, y: -1.0 };
            let middle = Point { x: 3.0, y: 3.0 };
            let end = Point { x: 20.0, y: 30.0 };
            assert_eq!(
                Rect { start, end: middle }.to_pixels(scale, 10, 11),
                PixelRect::new(0, 0, edge, edge)
            );
            assert_eq!(
                Rect {
                    start: end,
                    end: middle
                }
                .to_pixels(scale, 10, 11),
                PixelRect::new(edge, edge, 10, 11)
            );
        }
    }

    #[test]
    fn invalid_scale_and_nonfinite_points_are_rejected() {
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(ScaleFactor::new(value).is_none());
        }
        let rect = Rect {
            start: Point {
                x: f64::NAN,
                y: 0.0,
            },
            end: Point { x: 2.0, y: 2.0 },
        };
        assert!(rect.to_pixels(ScaleFactor::ONE, 4, 4).is_none());
    }
}
