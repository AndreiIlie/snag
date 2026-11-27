mod hit;
use crate::{geometry::PixelPoint, history::Command};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Arrow,
    Rectangle,
    Ellipse,
    Line,
    Pen,
    Highlighter,
    Pixelate,
    Counter,
}

impl Tool {
    pub const ALL: [Self; 8] = [
        Self::Arrow,
        Self::Rectangle,
        Self::Ellipse,
        Self::Line,
        Self::Pen,
        Self::Highlighter,
        Self::Pixelate,
        Self::Counter,
    ];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Colour {
    #[default]
    Red,
    Yellow,
    Green,
    Blue,
    Black,
    White,
}

impl Colour {
    pub const ALL: [Self; 6] = [
        Self::Red,
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Black,
        Self::White,
    ];
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Self::Red => [255, 0, 0],
            Self::Yellow => [255, 230, 0],
            Self::Green => [0, 200, 70],
            Self::Blue => [0, 90, 255],
            Self::Black => [0; 3],
            Self::White => [255; 3],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Width {
    Thin,
    #[default]
    Medium,
    Thick,
}
impl Width {
    pub const ALL: [Self; 3] = [Self::Thin, Self::Medium, Self::Thick];
    pub fn logical(self) -> f64 {
        match self {
            Self::Thin => 2.,
            Self::Medium => 4.,
            Self::Thick => 8.,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub colour: Colour,
    pub width: Width,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub(crate) tool: Tool,
    pub(crate) style: Style,
    units: f64,
    points: Vec<PixelPoint>,
    pub(crate) offset: PixelPoint,
}

impl Annotation {
    pub(crate) fn start(tool: Tool, style: Style, units: f64, point: PixelPoint) -> Self {
        Self {
            tool,
            style,
            units,
            points: vec![point],
            offset: PixelPoint::ZERO,
        }
    }
    pub(crate) fn extend(&mut self, point: PixelPoint) -> bool {
        if self.tool == Tool::Counter || self.points.last() == Some(&point) {
            return false;
        }
        if self.points.len() == 1 || matches!(self.tool, Tool::Pen | Tool::Highlighter) {
            self.points.push(point);
        } else {
            self.points[1] = point;
        }
        true
    }
    pub(crate) fn drawable(&self) -> bool {
        let (a, b) = self.ends();
        match self.tool {
            Tool::Counter => true,
            Tool::Rectangle | Tool::Ellipse | Tool::Pixelate => a.x != b.x && a.y != b.y,
            Tool::Pen | Tool::Highlighter => self.points.len() > 1,
            _ => a != b,
        }
    }
    pub(crate) fn points(&self) -> impl DoubleEndedIterator<Item = PixelPoint> + '_ {
        self.points.iter().map(|p| p.plus(self.offset))
    }
    pub(crate) fn ends(&self) -> (PixelPoint, PixelPoint) {
        (
            self.points[0].plus(self.offset),
            self.points.last().unwrap().plus(self.offset),
        )
    }
    pub(crate) fn width(&self) -> f64 {
        self.style.width.logical() * self.units
    }
    pub(crate) fn stroke(&self) -> f64 {
        if self.tool == Tool::Highlighter {
            self.width() * 5.
        } else {
            self.width()
        }
    }
    pub(crate) fn radius(&self) -> f64 {
        8. * self.units + 2. * self.width()
    }
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Document {
    pub(crate) layers: Vec<Annotation>,
}

impl Document {
    pub(crate) fn hit(&self, point: PixelPoint, tolerance: f64) -> Option<usize> {
        self.layers.iter().rposition(|a| a.hit(point, tolerance))
    }
}

pub(crate) enum Edit {
    Add(Option<Annotation>),
    Delete {
        index: usize,
        removed: Option<Annotation>,
    },
    Move {
        index: usize,
        from: PixelPoint,
        to: PixelPoint,
    },
}

impl Command<Document> for Edit {
    fn apply(&mut self, doc: &mut Document) {
        match self {
            Self::Add(shape) => doc.layers.push(shape.take().unwrap()),
            Self::Delete { index, removed } => *removed = Some(doc.layers.remove(*index)),
            Self::Move { index, to, .. } => doc.layers[*index].offset = *to,
        }
    }
    fn undo(&mut self, doc: &mut Document) {
        match self {
            Self::Add(shape) => *shape = doc.layers.pop(),
            Self::Delete { index, removed } => doc.layers.insert(*index, removed.take().unwrap()),
            Self::Move { index, from, .. } => doc.layers[*index].offset = *from,
        }
    }
}

#[cfg(test)]
mod tests;
