mod text;

use crate::{
    annotation::{Annotation, Document, Edit, Style, Tool},
    font::Fonts,
    geometry::{PixelPoint, PixelRect, Point, ScaleFactor},
    history::History,
    selection::{Handle, Hit, Selection},
    toolbar::{Action, Button, Toolbar},
    view::View,
};
use std::cell::LazyCell;

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
    Draw,
    Annotation { index: usize, origin: PixelPoint },
    Resize(Selection, Handle),
}

pub struct Overlay {
    view: View,
    scale: ScaleFactor,
    selection: Option<Selection>,
    pointer: Option<Point>,
    drag: Option<(PixelPoint, Drag)>,
    pressed_button: Option<Button>,
    tool: Option<Tool>,
    style: Style,
    document: Document,
    history: History<Document>,
    draft: Option<Annotation>,
    fonts: LazyCell<Option<Fonts>>,
    // Byte offset at a character boundary in the text draft.
    caret: usize,
    selected: Option<usize>,
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
            tool: None,
            style: Style::default(),
            document: Document::default(),
            history: History::default(),
            draft: None,
            fonts: LazyCell::new(Fonts::system),
            caret: 0,
            selected: None,
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

    pub fn into_document(self) -> Document {
        self.document
    }

    pub(crate) fn layers(&self) -> impl Iterator<Item = &Annotation> + Clone {
        self.document
            .layers
            .iter()
            .chain(self.draft.iter().filter(|a| a.drawable()))
    }
    pub(crate) fn selected(&self) -> Option<&Annotation> {
        self.selected.map(|i| &self.document.layers[i])
    }
    pub(crate) fn active(&self, button: Button) -> bool {
        match button {
            Button::Tool(tool) => self.tool == Some(tool),
            Button::Colour(colour) => self.style.colour == colour,
            Button::Width(width) => self.style.width == width,
            _ => false,
        }
    }
    fn capture_pixels(&self, logical: f64) -> f64 {
        logical * self.scale.get() / self.view.scale
    }
    fn annotation_at(&self, point: PixelPoint) -> Option<usize> {
        if !self.on_capture() {
            return None;
        }
        self.document.hit(point, self.capture_pixels(3.))
    }
    fn on_capture(&self) -> bool {
        let b = self.view.bounds;
        self.pointer.is_some_and(|p| {
            (f64::from(b.left())..f64::from(b.right())).contains(&p.x)
                && (f64::from(b.top())..f64::from(b.bottom())).contains(&p.y)
        })
    }
    fn hit(&self, point: PixelPoint) -> Hit {
        let scale = ScaleFactor::new(self.capture_pixels(1.)).unwrap_or(self.scale);
        self.selection
            .map_or(Hit::Outside, |s| s.hit_test(point, scale))
    }

    fn button_at_pointer(&self) -> Option<Button> {
        self.toolbar()?.hit(self.pointer?)
    }

    pub fn cursor(&self) -> Cursor {
        if let Some((_, drag)) = self.drag {
            return match drag {
                Drag::New(_) | Drag::Draw => Cursor::Crosshair,
                Drag::Move(_) | Drag::Annotation { .. } => Cursor::Move,
                Drag::Resize(_, handle) => Cursor::Resize(handle),
            };
        }
        if self.button_at_pointer().is_some() {
            return Cursor::Arrow;
        }
        let Some(point) = self.pointer.and_then(|p| self.view.capture_point(p)) else {
            return Cursor::Crosshair;
        };
        match self.hit(point) {
            Hit::Handle(h) => Cursor::Resize(h),
            _ if self.tool.is_some() => Cursor::Crosshair,
            Hit::Inside => Cursor::Move,
            _ if self.annotation_at(point).is_some() => Cursor::Move,
            _ => Cursor::Crosshair,
        }
    }

    pub fn pointer_moved(&mut self, pointer: Point) -> bool {
        let Some(point) = self.view.capture_point(pointer) else {
            return false;
        };
        if self.pointer.replace(pointer) == Some(pointer) {
            return false;
        }
        let Some((start, drag)) = self.drag else {
            return false;
        };
        let (width, height) = self.view.capture;
        let dx = point.x.saturating_sub(start.x);
        let dy = point.y.saturating_sub(start.y);
        let next = match drag {
            Drag::Draw => return self.draft.as_mut().unwrap().extend(point),
            Drag::Annotation { index, origin } => {
                let offset = origin.plus(PixelPoint { x: dx, y: dy });
                let shape = &mut self.document.layers[index];
                return std::mem::replace(&mut shape.offset, offset) != offset;
            }
            Drag::New(old) => Selection::from_drag(start, point, width, height).or(old),
            Drag::Move(s) => Some(s.moved(dx, dy)),
            Drag::Resize(s, handle) => {
                let anchor = s.handle_position(handle);
                // Keep the grab offset until the pointer reaches the edge of the image.
                let edge = |pos: i64, origin: i64, delta: i64, limit: u32| {
                    if pos <= 0 || pos >= i64::from(limit) {
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
        let finished = self.is_typing()
            && !matches!(
                self.pressed_button,
                Some(Button::Width(_) | Button::Colour(_))
            );
        if finished {
            self.commit_draft();
        }
        if self.pressed_button.is_some() || self.drag.is_some() {
            return finished;
        }
        let drag = match (self.selection, self.hit(point)) {
            (Some(s), Hit::Handle(h)) => Drag::Resize(s, h),
            _ if self.tool.is_some() => {
                if !self.on_capture() {
                    return finished;
                }
                if self.tool == Some(Tool::Text) {
                    self.start_text(point);
                    self.selected = None;
                    return true;
                }
                self.draft = Some(Annotation::start(
                    self.tool.unwrap(),
                    self.style,
                    self.capture_pixels(1.),
                    point,
                ));
                Drag::Draw
            }
            _ if let Some(index) = self.annotation_at(point) => {
                self.selected = Some(index);
                Drag::Annotation {
                    index,
                    origin: self.document.layers[index].offset,
                }
            }
            (Some(s), Hit::Inside) => Drag::Move(s),
            (old, _) => Drag::New(old),
        };
        if !matches!(drag, Drag::Annotation { .. }) {
            self.selected = None;
        }
        self.drag = Some((point, drag));
        self.selection.is_some()
    }

    fn commit_draft(&mut self) {
        if let Some(draft) = self.draft.take().filter(Annotation::drawable) {
            self.history
                .execute(Edit::Add(Some(draft)), &mut self.document);
        }
    }

    pub fn release(&mut self) -> (bool, Option<Action>) {
        let redraw = self.drag.is_some() && self.selection.is_some();
        match self.drag.take().map(|(_, d)| d) {
            Some(Drag::Draw) => self.commit_draft(),
            Some(Drag::Annotation { index, origin }) => {
                let to = self.document.layers[index].offset;
                if origin != to {
                    self.history.execute(
                        Edit::Move {
                            index,
                            from: origin,
                            to,
                        },
                        &mut self.document,
                    );
                }
            }
            _ => {}
        }
        if let Some(button) = self
            .pressed_button
            .take()
            .filter(|b| Some(*b) == self.button_at_pointer())
        {
            return self.activate(button);
        }
        (redraw, None)
    }
    fn activate(&mut self, button: Button) -> (bool, Option<Action>) {
        match button {
            Button::Close(action) => return (false, Some(action)),
            Button::Tool(tool) => {
                if self.selection.is_none() {
                    return (false, None);
                }
                if tool == Tool::Text && self.fonts.is_none() {
                    return (false, None);
                }
                self.tool = (self.tool != Some(tool)).then_some(tool);
                self.selected = None;
            }
            Button::Undo => {
                self.history.undo(&mut self.document);
                self.selected = None;
            }
            Button::Redo => {
                self.history.redo(&mut self.document);
                self.selected = None;
            }
            Button::Colour(c) => {
                self.style.colour = c;
                if self.is_typing() {
                    self.draft.as_mut().unwrap().style.colour = c;
                }
            }
            Button::Width(w) => {
                self.style.width = w;
                if self.is_typing() {
                    self.resize_text(w);
                }
            }
        }
        (true, None)
    }
    pub fn key(&mut self, key: Key) -> (bool, Option<Action>) {
        if let Some(changed) = self.typing_key(key) {
            return (changed, None);
        }
        match key {
            Key::Escape => (false, Some(Action::Cancel)),
            Key::Enter | Key::Copy | Key::Save => {
                if self.drag.is_some() {
                    self.release();
                }
                (
                    false,
                    Some(if key == Key::Save {
                        Action::Save
                    } else {
                        Action::Copy
                    }),
                )
            }
            _ if self.drag.is_some() => (false, None),
            Key::Tool(tool) => self.activate(Button::Tool(tool)),
            Key::Undo => self.activate(Button::Undo),
            Key::Redo => self.activate(Button::Redo),
            Key::Character(_) | Key::Backspace | Key::Left | Key::Right | Key::Home | Key::End => {
                (false, None)
            }
            Key::Delete => {
                let Some(index) = self.selected.take() else {
                    return (false, None);
                };
                self.history.execute(
                    Edit::Delete {
                        index,
                        removed: None,
                    },
                    &mut self.document,
                );
                (true, None)
            }
        }
    }

    pub fn pointer_left(&mut self) {
        self.pointer = None;
    }

    pub fn reset_pointer(&mut self) -> bool {
        self.pointer_left();
        self.pressed_button = None;
        if !self.is_typing() {
            self.draft = None;
        }
        if let Some((_, Drag::Annotation { index, origin })) = self.drag {
            self.document.layers[index].offset = origin;
        }
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
    Tool(Tool),
    Undo,
    Redo,
    Delete,
    Character(char),
    Backspace,
    Left,
    Right,
    Home,
    End,
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
        overlay.press();
        overlay.pointer_moved(at(30, 30));
        overlay.pointer_moved(at(0, 0));
        overlay.release();
        assert_eq!(overlay.crop(), PixelRect::new(0, 0, 220, 180));
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
    fn scaled_handle_hits_and_drags_use_capture_pixels() {
        let view = View::fit((800, 400), (400, 400)).unwrap();
        for handle in Handle::ALL {
            let mut overlay = Overlay::new(view, ScaleFactor::new(2.0).unwrap());
            drag(&mut overlay, at(30, 130), at(220, 250));
            let selection = overlay.selection().unwrap();
            let centre = view.window_point(selection.handle_position(handle));
            // Eight window pixels from the handle is sixteen capture pixels at this fit.
            let press = at(centre.x as i32 + 8, centre.y as i32);
            overlay.pointer_moved(press);
            assert_eq!(overlay.cursor(), Cursor::Resize(handle));
            drag(&mut overlay, press, at(98, 160));
            let target = PixelPoint { x: 180, y: 120 };
            assert_eq!(overlay.selection(), selection.resized(handle, target));
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
        for (button, rect) in overlay.toolbar().unwrap().buttons() {
            let Button::Close(action) = button else {
                continue;
            };
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
            [Key::Enter, Key::Copy, Key::Save, Key::Escape].map(|k| overlay.key(k).1.unwrap()),
            [Action::Copy, Action::Copy, Action::Save, Action::Cancel]
        );
    }
}
