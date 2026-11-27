use crate::{
    geometry::{PixelPoint, PixelRect, Point, ScaleFactor},
    selection::{Handle, Hit, Selection},
    toolbar::{Action, Toolbar},
    view::View,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Crosshair,
    Move,
    Resize(Handle),
    Arrow,
}

#[derive(Clone, Copy, Debug)]
enum Drag {
    New(Option<Selection>),
    Move(Selection),
    Resize(Selection, Handle),
}

pub struct Overlay {
    view: View,
    scale: ScaleFactor,
    selection: Option<Selection>,
    pointer: Option<Point>,
    drag: Option<(PixelPoint, Drag)>,
    pressed_button: Option<Action>,
}

impl Overlay {
    pub fn new(view: View, scale: ScaleFactor) -> Self {
        Self {
            view,
            scale,
            selection: None,
            pointer: None,
            drag: None,
            pressed_button: None,
        }
    }

    pub fn set_view(&mut self, view: View, scale: ScaleFactor) -> bool {
        if self.view == view && self.scale == scale {
            return false;
        }
        self.view = view;
        self.scale = scale;
        self.reset_pointer();
        true
    }

    pub fn view(&self) -> View {
        self.view
    }
    pub fn scale(&self) -> ScaleFactor {
        self.scale
    }
    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }
    pub fn crop(&self) -> Option<PixelRect> {
        self.selection.map(Selection::rect)
    }

    pub fn toolbar(&self) -> Option<Toolbar> {
        if self.drag.is_some() {
            return None;
        }
        Toolbar::place(
            self.view.window_rect(self.crop()?),
            self.view.window,
            self.scale,
        )
    }

    fn hit(&self, point: PixelPoint) -> Hit {
        let scale = ScaleFactor::new(self.scale.get() / self.view.scale).unwrap_or(self.scale);
        self.selection
            .map_or(Hit::Outside, |s| s.hit_test(point, scale))
    }

    fn button_at_pointer(&self) -> Option<Action> {
        self.toolbar()?.hit(self.pointer?)
    }

    pub fn cursor(&self) -> Cursor {
        if let Some((_, drag)) = self.drag {
            return match drag {
                Drag::New(_) => Cursor::Crosshair,
                Drag::Move(_) => Cursor::Move,
                Drag::Resize(_, handle) => Cursor::Resize(handle),
            };
        }
        if self.button_at_pointer().is_some() {
            return Cursor::Arrow;
        }
        match self
            .pointer
            .and_then(|p| self.view.capture_point(p))
            .map(|p| self.hit(p))
        {
            Some(Hit::Handle(h)) => Cursor::Resize(h),
            Some(Hit::Inside) => Cursor::Move,
            _ => Cursor::Crosshair,
        }
    }

    pub fn pointer_moved(&mut self, pointer: Point) -> bool {
        let Some(point) = self.view.capture_point(pointer) else {
            return false;
        };
        self.pointer = Some(pointer);
        let Some((start, drag)) = self.drag else {
            return false;
        };
        let (width, height) = self.view.capture;
        let dx = point.x.saturating_sub(start.x);
        let dy = point.y.saturating_sub(start.y);
        let next = match drag {
            Drag::New(old) => Selection::from_drag(start, point, width, height).or(old),
            Drag::Move(s) => Some(s.moved(dx, dy)),
            Drag::Resize(s, handle) => {
                let anchor = s.handle_position(handle);
                // Keep the grab offset until the pointer reaches the edge of the image.
                let edge = |pos: i64, origin: i64, delta: i64, limit: u32| {
                    if delta != 0 && (pos <= 0 || pos >= i64::from(limit)) {
                        pos
                    } else {
                        origin.saturating_add(delta)
                    }
                };
                s.resized(
                    handle,
                    PixelPoint {
                        x: edge(point.x, anchor.x, dx, width),
                        y: edge(point.y, anchor.y, dy, height),
                    },
                )
                .or(self.selection)
            }
        };
        let changed = self.selection != next;
        self.selection = next;
        changed
    }

    pub fn press(&mut self) -> bool {
        let Some(point) = self.pointer.and_then(|p| self.view.capture_point(p)) else {
            return false;
        };
        self.pressed_button = self.button_at_pointer();
        if self.pressed_button.is_some() || self.drag.is_some() {
            return false;
        }
        let drag = match (self.selection, self.hit(point)) {
            (Some(s), Hit::Handle(h)) => Drag::Resize(s, h),
            (Some(s), Hit::Inside) => Drag::Move(s),
            (old, _) => Drag::New(old),
        };
        self.drag = Some((point, drag));
        self.selection.is_some()
    }

    pub fn release(&mut self) -> (bool, Option<Action>) {
        let redraw = self.drag.take().is_some() && self.selection.is_some();
        let action = self
            .pressed_button
            .take()
            .filter(|a| Some(*a) == self.button_at_pointer());
        (redraw, action)
    }

    pub fn pointer_left(&mut self) {
        self.pointer = None;
    }

    pub fn reset_pointer(&mut self) -> bool {
        self.pointer_left();
        self.pressed_button = None;
        self.drag.take().is_some() && self.selection.is_some()
    }
}

/// Logical shortcuts after the shell has translated the native keyboard event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Enter,
    Escape,
    Copy,
    Save,
}

impl Key {
    pub fn action(self) -> Action {
        match self {
            Self::Enter | Self::Copy => Action::Copy,
            Self::Save => Action::Save,
            Self::Escape => Action::Cancel,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32) -> Point {
        Point {
            x: x.into(),
            y: y.into(),
        }
    }
    fn canvas() -> Overlay {
        Overlay::new(
            View::fit((1920, 1080), (1920, 1080)).unwrap(),
            ScaleFactor::ONE,
        )
    }
    fn drag(overlay: &mut Overlay, start: Point, end: Point) {
        overlay.pointer_moved(start);
        overlay.press();
        overlay.pointer_moved(end);
        assert!(overlay.toolbar().is_none());
        assert_eq!(overlay.release().1, None);
    }

    #[test]
    fn drawing_moving_and_resizing_reach_the_last_pixel() {
        let mut overlay = canvas();
        assert_eq!(overlay.crop(), None);
        assert!(overlay.toolbar().is_none());
        assert!(!overlay.pointer_moved(at(10, 20)));
        assert_eq!(overlay.cursor(), Cursor::Crosshair);
        drag(&mut overlay, at(10, 20), at(1919, 1079));
        assert_eq!(overlay.crop(), PixelRect::new(10, 20, 1920, 1080));
        drag(&mut overlay, at(30, 40), at(0, 0));
        assert_eq!(overlay.crop(), PixelRect::new(0, 0, 1910, 1060));
        drag(&mut overlay, at(1910, 1060), at(1919, 1079));
        assert_eq!(overlay.crop(), PixelRect::new(0, 0, 1920, 1080));
    }

    #[test]
    fn pressing_a_handle_near_the_screen_edge_keeps_its_grab_offset() {
        let mut overlay = canvas();
        drag(&mut overlay, at(3, 4), at(220, 180));
        drag(&mut overlay, at(0, 0), at(0, 0));
        assert_eq!(overlay.crop(), PixelRect::new(3, 4, 220, 180));
    }

    #[test]
    fn each_handle_resizes_from_its_original_selection() {
        for handle in Handle::ALL {
            let mut overlay = canvas();
            drag(&mut overlay, at(30, 30), at(220, 180));
            let selection = overlay.selection.unwrap();
            let start = selection.handle_position(handle);
            overlay.pointer_moved(at(start.x as i32, start.y as i32));
            assert_eq!(overlay.cursor(), Cursor::Resize(handle));
            assert!(overlay.press());
            assert!(overlay.pointer_moved(at(0, 0)));
            let to = PixelPoint { x: 90, y: 60 };
            assert!(overlay.pointer_moved(at(90, 60)));
            assert!(!overlay.pointer_moved(at(90, 60)));
            overlay.release();
            assert_eq!(overlay.selection, selection.resized(handle, to));
        }
    }

    #[test]
    fn fitted_gestures_use_capture_pixels_and_clicks_preserve_the_region() {
        let mut overlay =
            Overlay::new(View::fit((800, 400), (400, 400)).unwrap(), ScaleFactor::ONE);
        drag(&mut overlay, at(30, 130), at(120, 190));
        let expected = PixelRect::new(60, 60, 240, 180);
        assert_eq!(overlay.crop(), expected);
        overlay.pointer_moved(at(70, 150));
        assert_eq!(overlay.cursor(), Cursor::Move);
        for p in [at(2, 3), at(70, 150), at(30, 130)] {
            drag(&mut overlay, p, p);
            assert_eq!(overlay.crop(), expected);
        }
        drag(&mut overlay, at(70, 150), at(80, 160));
        assert_eq!(overlay.crop(), PixelRect::new(80, 80, 260, 200));
    }

    #[test]
    fn buttons_and_shortcuts_choose_actions() {
        let mut overlay = canvas();
        drag(&mut overlay, at(40, 40), at(300, 200));
        for (action, rect) in overlay.toolbar().unwrap().buttons() {
            let point = at(rect.left() as i32 + 1, rect.top() as i32 + 1);
            overlay.pointer_moved(point);
            assert_eq!(overlay.cursor(), Cursor::Arrow);
            assert!(!overlay.press());
            overlay.pointer_moved(at(2, 1));
            assert_eq!(overlay.release(), (false, None));
            overlay.pointer_moved(point);
            overlay.press();
            assert_eq!(overlay.release(), (false, Some(action)));
        }
        assert_eq!(
            [Key::Enter, Key::Copy, Key::Save, Key::Escape].map(Key::action),
            [Action::Copy, Action::Copy, Action::Save, Action::Cancel]
        );
    }
}
