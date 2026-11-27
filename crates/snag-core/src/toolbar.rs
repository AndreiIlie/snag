use crate::annotation::{Colour, Tool, Width};
use crate::geometry::{PixelRect, Point, ScaleFactor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Copy,
    Save,
    Cancel,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Tool(Tool),
    Undo,
    Redo,
    Colour(Colour),
    Width(Width),
    Close(Action),
}

fn groups() -> impl Iterator<Item = (usize, Button)> {
    Tool::ALL
        .into_iter()
        .map(|x| (0, Button::Tool(x)))
        .chain([(1, Button::Undo), (1, Button::Redo)])
        .chain(Colour::ALL.into_iter().map(|x| (2, Button::Colour(x))))
        .chain(Width::ALL.into_iter().map(|x| (3, Button::Width(x))))
        .chain([Action::Copy, Action::Save, Action::Cancel].map(|x| (4, Button::Close(x))))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Toolbar {
    left: u32,
    top: u32,
    side: u32,
    gap: u32,
}

impl Toolbar {
    pub fn place(region: PixelRect, window: (u32, u32), scale: ScaleFactor) -> Option<Self> {
        let (width, height) = window;
        let count = groups().count() as u32;
        if width < count || height == 0 {
            return None;
        }
        let px = |logical: f64| (logical * scale.get()).round().max(1.0) as u32;
        let spaces = count - 1 + 4;
        let gap = px(4.0).min(width / (8 * count + spaces));
        let side = px(32.0).min((width - gap * spaces) / count).min(height);
        let left = region.right().saturating_sub(count * side + spaces * gap);
        let below = region.bottom().saturating_add(gap);
        let top = if below <= height - side {
            below
        } else if region.top() >= side.saturating_add(gap) {
            region.top() - side - gap
        } else {
            region.bottom().saturating_sub(side.saturating_add(gap))
        };
        Some(Self {
            left,
            top,
            side,
            gap,
        })
    }

    pub fn buttons(self) -> impl Iterator<Item = (Button, PixelRect)> {
        groups().enumerate().map(move |(i, (group, action))| {
            let left = self.left + i as u32 * (self.side + self.gap) + group as u32 * self.gap;
            let rect =
                PixelRect::new(left, self.top, left + self.side, self.top + self.side).unwrap();
            (action, rect)
        })
    }

    pub fn hit(self, point: Point) -> Option<Button> {
        self.buttons().find_map(|(action, rect)| {
            ((f64::from(rect.left())..f64::from(rect.right())).contains(&point.x)
                && (f64::from(rect.top())..f64::from(rect.bottom())).contains(&point.y))
            .then_some(action)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_size_and_hit_zones_follow_the_window() {
        for factor in [1.0, 2.0] {
            let scale = ScaleFactor::new(factor).unwrap();
            let side = (32.0 * factor) as u32;
            let gap = (4.0 * factor) as u32;
            for (top, bottom, expected) in [
                (20, 80, 80 + gap),
                (200, 250, 200 - side - gap),
                (0, 260, 260 - side - gap),
            ] {
                let rect = PixelRect::new(1, top, 1900, bottom).unwrap();
                let bar = Toolbar::place(rect, (1920, 260), scale).unwrap();
                assert_eq!(bar.top, expected);
                for (action, button) in bar.buttons() {
                    assert_eq!((button.width(), button.height()), (side, side));
                    assert_eq!(
                        bar.hit(Point {
                            x: button.left().into(),
                            y: expected.into()
                        }),
                        Some(action)
                    );
                    assert_eq!(
                        bar.hit(Point {
                            x: button.right().into(),
                            y: expected.into()
                        }),
                        None
                    );
                }
                assert_eq!(bar.buttons().last().unwrap().1.right(), 1900);
            }
        }
        let bar = Toolbar::place(
            PixelRect::new(0, 0, 1, 1).unwrap(),
            (48, 3),
            ScaleFactor::ONE,
        )
        .unwrap();
        assert_eq!(bar.buttons().count(), 23);
        assert_eq!(bar.buttons().nth(8).unwrap().0, Button::Tool(Tool::Text));
        let expected = Tool::ALL
            .map(Button::Tool)
            .into_iter()
            .chain([Button::Undo, Button::Redo])
            .chain(Colour::ALL.map(Button::Colour))
            .chain(Width::ALL.map(Button::Width))
            .chain([Action::Copy, Action::Save, Action::Cancel].map(Button::Close));
        assert!(bar.buttons().map(|(button, _)| button).eq(expected));
        assert_eq!(bar.left, 0);
        assert!(
            bar.buttons()
                .all(|(_, r)| r.right() <= 48 && r.bottom() <= 3)
        );
    }
}
