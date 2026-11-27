use super::{Key, Overlay};
use crate::{
    annotation::{Annotation, Edit, Text, Tool, Width},
    geometry::PixelPoint,
};

impl Overlay {
    pub fn is_typing(&self) -> bool {
        self.draft.as_ref().is_some_and(|a| a.tool == Tool::Text)
    }

    pub(super) fn start_text(&mut self, anchor: PixelPoint) {
        let units = self.scale.get() / self.view.scale;
        let size = self.style.width.text_size() * units as f32;
        let fonts = self.fonts.as_mut().unwrap();
        let mut draft = Annotation::start(Tool::Text, self.style, units, anchor);
        draft.text = Some(Box::new(Text {
            content: String::new(),
            size,
            line: fonts.layout("", size),
        }));
        self.draft = Some(draft);
        self.caret = 0;
    }

    pub(super) fn finish_text(&mut self) {
        if let Some(draft) = self.draft.take().filter(Annotation::drawable) {
            self.history
                .execute(Edit::Add(Some(draft)), &mut self.document);
        }
    }

    pub(super) fn resize_text(&mut self, width: Width) {
        let draft = self.draft.as_mut().unwrap();
        draft.style.width = width;
        let text = draft.text.as_mut().unwrap();
        text.size = width.text_size() * draft.units as f32;
        let fonts = self.fonts.as_mut().unwrap();
        text.line = fonts.layout(&text.content, text.size);
    }

    /// Return None to dispatch a shortcut after committing the text.
    pub(super) fn typing_key(&mut self, key: Key) -> Option<bool> {
        let text = self.draft.as_mut()?.text.as_mut()?;
        let caret = &mut self.caret;
        let previous = || {
            text.content[..*caret]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i)
        };
        match key {
            Key::Home => {
                *caret = 0;
                return Some(true);
            }
            Key::End => {
                *caret = text.content.len();
                return Some(true);
            }
            Key::Left => {
                *caret = previous();
                return Some(true);
            }
            Key::Right => {
                if let Some(ch) = text.content[*caret..].chars().next() {
                    *caret += ch.len_utf8();
                }
                return Some(true);
            }
            Key::Character(ch) if !ch.is_control() => {
                text.content.insert(*caret, ch);
                *caret += ch.len_utf8();
            }
            Key::Backspace if *caret > 0 => {
                let start = previous();
                text.content.drain(start..*caret);
                *caret = start;
            }
            Key::Delete if *caret < text.content.len() => {
                text.content.remove(*caret);
            }
            Key::Character(_) | Key::Backspace | Key::Delete => return Some(false),
            Key::Escape => {
                self.draft = None;
                return Some(true);
            }
            _ => {
                self.finish_text();
                return (key == Key::Enter).then_some(true);
            }
        }
        let fonts = self.fonts.as_mut().unwrap();
        text.line = fonts.layout(&text.content, text.size);
        Some(true)
    }

    /// Caret x, top, and bottom in capture pixels.
    pub(crate) fn text_caret(&self) -> Option<[f32; 3]> {
        let draft = self.draft.as_ref()?;
        let text = draft.text.as_ref()?;
        let anchor = draft.ends().0;
        let index = text.content[..self.caret].chars().count();
        Some([
            anchor.x as f32 + text.line.carets[index],
            anchor.y as f32 - text.line.half_height,
            anchor.y as f32 + text.line.half_height,
        ])
    }
}

#[cfg(test)]
impl Overlay {
    pub(crate) fn use_fixture_fonts(&mut self) {
        self.fonts = std::cell::LazyCell::new(|| Some(crate::font::fixture_fonts()));
    }
}
