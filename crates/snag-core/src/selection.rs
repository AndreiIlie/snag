use crate::geometry::{PixelPoint, PixelRect, ScaleFactor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl Handle {
    pub const ALL: [Self; 8] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Right,
        Self::BottomRight,
        Self::Bottom,
        Self::BottomLeft,
        Self::Left,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Handle(Handle),
    Inside,
    Outside,
}

pub(crate) fn handle_size(scale: ScaleFactor) -> f64 {
    (6.0 * scale.get()).round().max(1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    rect: PixelRect,
    image_width: u32,
    image_height: u32,
}

impl Selection {
    pub fn from_drag(
        start: PixelPoint,
        end: PixelPoint,
        image_width: u32,
        image_height: u32,
    ) -> Option<Self> {
        let rect = PixelRect::from_drag(start, end, image_width, image_height)?;
        Some(Self {
            rect,
            image_width,
            image_height,
        })
    }

    pub fn rect(self) -> PixelRect {
        self.rect
    }

    pub fn handle_position(self, handle: Handle) -> PixelPoint {
        use Handle::*;
        let r = self.rect;
        let x = match handle {
            TopLeft | Left | BottomLeft => r.left(),
            Top | Bottom => r.left() + r.width() / 2,
            TopRight | Right | BottomRight => r.right(),
        };
        let y = match handle {
            TopLeft | Top | TopRight => r.top(),
            Left | Right => r.top() + r.height() / 2,
            BottomLeft | Bottom | BottomRight => r.bottom(),
        };
        PixelPoint {
            x: x.into(),
            y: y.into(),
        }
    }

    pub fn hit_test(self, point: PixelPoint, scale: ScaleFactor) -> Hit {
        let radius = handle_size(scale) / 2.0 + 2.0 * scale.get();
        let mut nearest = None;
        let mut best = f64::INFINITY;
        for handle in Handle::ALL {
            let center = self.handle_position(handle);
            let dx = point.x.abs_diff(center.x) as f64;
            let dy = point.y.abs_diff(center.y) as f64;
            let distance = dx * dx + dy * dy;
            if dx.max(dy) <= radius && distance < best {
                nearest = Some(handle);
                best = distance;
            }
        }
        if let Some(handle) = nearest {
            return Hit::Handle(handle);
        }
        let r = self.rect;
        if (i64::from(r.left())..i64::from(r.right())).contains(&point.x)
            && (i64::from(r.top())..i64::from(r.bottom())).contains(&point.y)
        {
            Hit::Inside
        } else {
            Hit::Outside
        }
    }

    pub fn moved(self, dx: i64, dy: i64) -> Self {
        let r = self.rect;
        let dx = dx.clamp(
            -i64::from(r.left()),
            i64::from(self.image_width - r.right()),
        );
        let dy = dy.clamp(
            -i64::from(r.top()),
            i64::from(self.image_height - r.bottom()),
        );
        let shift_x = |x: u32| (i64::from(x) + dx) as u32;
        let shift_y = |y: u32| (i64::from(y) + dy) as u32;
        let rect = PixelRect::new(
            shift_x(r.left()),
            shift_y(r.top()),
            shift_x(r.right()),
            shift_y(r.bottom()),
        );
        Self {
            rect: rect.expect("translation preserves rectangle size"),
            ..self
        }
    }

    /// Apply each drag update to the starting selection so the opposite edge stays anchored.
    pub fn resized(self, handle: Handle, to: PixelPoint) -> Option<Self> {
        use Handle::*;
        let mut start = self.handle_position(TopLeft);
        let mut end = self.handle_position(BottomRight);
        match handle {
            TopLeft | Left | BottomLeft => start.x = to.x,
            TopRight | Right | BottomRight => end.x = to.x,
            Top | Bottom => {}
        }
        match handle {
            TopLeft | Top | TopRight => start.y = to.y,
            BottomLeft | Bottom | BottomRight => end.y = to.y,
            Left | Right => {}
        }
        Self::from_drag(start, end, self.image_width, self.image_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(x: i64, y: i64) -> PixelPoint {
        PixelPoint { x, y }
    }

    #[test]
    fn normalizes_drags_and_clips_to_image() {
        for (start, end) in [
            (p(-1, -3), p(120, 90)),
            (p(120, 90), p(-1, -3)),
            (p(-1, 90), p(120, -3)),
            (p(120, -3), p(-1, 90)),
        ] {
            assert_eq!(
                Selection::from_drag(start, end, 100, 80).map(Selection::rect),
                PixelRect::new(0, 0, 100, 80)
            );
        }
        assert!(Selection::from_drag(p(-2, 4), p(0, 8), 100, 80).is_none());
    }

    #[test]
    fn places_and_hits_all_handles_before_the_interior() {
        let selection = Selection::from_drag(p(20, 20), p(80, 60), 100, 80).unwrap();
        let positions = [
            p(20, 20),
            p(50, 20),
            p(80, 20),
            p(80, 40),
            p(80, 60),
            p(50, 60),
            p(20, 60),
            p(20, 40),
        ];
        for (handle, center) in Handle::ALL.into_iter().zip(positions) {
            assert_eq!(selection.handle_position(handle), center);
            assert_eq!(
                selection.hit_test(center, ScaleFactor::ONE),
                Hit::Handle(handle)
            );
        }
        assert_eq!(selection.hit_test(p(50, 40), ScaleFactor::ONE), Hit::Inside);
        assert_eq!(
            selection.hit_test(p(50, 13), ScaleFactor::ONE),
            Hit::Outside
        );
        assert_eq!(
            selection.hit_test(p(50, 13), ScaleFactor::new(2.0).unwrap()),
            Hit::Handle(Handle::Top)
        );
    }

    #[test]
    fn moves_without_shrinking_and_resizes_through_the_anchor() {
        let selection = Selection::from_drag(p(20, 10), p(60, 50), 100, 80).unwrap();
        assert_eq!(
            Some(selection.moved(i64::MIN, i64::MAX).rect()),
            PixelRect::new(0, 40, 40, 80)
        );
        assert_eq!(
            Some(selection.moved(i64::MAX, i64::MIN).rect()),
            PixelRect::new(60, 0, 100, 40)
        );
        assert_eq!(
            selection
                .resized(Handle::TopLeft, p(120, 60))
                .map(Selection::rect),
            PixelRect::new(60, 50, 100, 60)
        );
        assert!(selection.resized(Handle::Right, p(20, 70)).is_none());
        assert_eq!(
            selection
                .resized(Handle::Right, p(-5, 70))
                .map(Selection::rect),
            PixelRect::new(0, 10, 20, 50)
        );
    }
}
