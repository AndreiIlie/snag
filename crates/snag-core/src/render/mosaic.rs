use super::{RenderError, annotations::draw_layer};
use crate::{
    annotation::{Annotation, Tool},
    geometry::PixelRect,
    image::RgbaImage,
};
use tiny_skia::{ColorU8, Pixmap, PixmapMut, PremultipliedColorU8, Transform};

#[derive(Clone, Copy)]
pub(crate) struct Grid {
    pub(crate) rect: PixelRect,
    origin: (i64, i64),
    step: u32,
}

impl Grid {
    pub(crate) fn new(shape: &Annotation, image: &RgbaImage) -> Option<Self> {
        if shape.tool != Tool::Pixelate {
            return None;
        }
        let (a, b) = shape.ends();
        Some(Self {
            rect: PixelRect::from_drag(a, b, image.width(), image.height())?,
            origin: (a.x.min(b.x), a.y.min(b.y)),
            step: (shape.width() * 3.).round().max(1.) as u32,
        })
    }
    fn phase(self, position: u32, origin: i64) -> u32 {
        (i128::from(position) - i128::from(origin)).rem_euclid(i128::from(self.step)) as u32
    }
    pub(crate) fn reaching(self, area: PixelRect) -> Option<Self> {
        let part = self.rect.intersection(area)?;
        let span = |lo: u32, hi: u32, origin| {
            let end_phase = self.phase(hi, origin);
            (
                lo.saturating_sub(self.phase(lo, origin)),
                hi.saturating_add((self.step - end_phase) % self.step),
            )
        };
        let (left, right) = span(part.left(), part.right(), self.origin.0);
        let (top, bottom) = span(part.top(), part.bottom(), self.origin.1);
        Some(Self {
            rect: self
                .rect
                .intersection(PixelRect::new(left, top, right, bottom)?)?,
            ..self
        })
    }
    fn spans(self, mut start: u32, end: u32, origin: i64) -> impl Iterator<Item = (u32, u32)> {
        std::iter::from_fn(move || {
            if start >= end {
                return None;
            }
            let next = start
                .saturating_add(self.step - self.phase(start, origin))
                .min(end);
            Some((std::mem::replace(&mut start, next), next))
        })
    }
    fn blocks(self) -> impl Iterator<Item = PixelRect> {
        self.spans(self.rect.top(), self.rect.bottom(), self.origin.1)
            .flat_map(move |(top, bottom)| {
                self.spans(self.rect.left(), self.rect.right(), self.origin.0)
                    .map(move |(left, right)| PixelRect::new(left, top, right, bottom).unwrap())
            })
    }
}

pub(crate) struct Mosaic {
    grid: Grid,
    colours: Vec<PremultipliedColorU8>,
}

impl Mosaic {
    pub(crate) fn sample(grid: Grid, pixel: impl Fn(u32, u32) -> PremultipliedColorU8) -> Self {
        let colours = grid
            .blocks()
            .map(|rect| {
                let mut sums = [0u64; 4];
                for y in rect.top()..rect.bottom() {
                    for x in rect.left()..rect.right() {
                        let p = pixel(x, y);
                        for (sum, c) in
                            sums.iter_mut()
                                .zip([p.red(), p.green(), p.blue(), p.alpha()])
                        {
                            *sum += u64::from(c);
                        }
                    }
                }
                let count = u64::from(rect.width()) * u64::from(rect.height());
                let [r, g, b, a] = sums.map(|s| ((s + count / 2) / count) as u8);
                PremultipliedColorU8::from_rgba(r, g, b, a).unwrap()
            })
            .collect();
        Self { grid, colours }
    }
    pub(crate) fn draw(&self, frame: &mut PixmapMut<'_>, ts: Transform) {
        let width = frame.width() as usize;
        let height = frame.height() as usize;
        let x = |v: u32| (((v as f32) * ts.sx + ts.tx).round() as usize).min(width);
        let y = |v: u32| (((v as f32) * ts.sy + ts.ty).round() as usize).min(height);
        let dest = frame.pixels_mut();
        for (block, &colour) in self.grid.blocks().zip(&self.colours) {
            let (left, right) = (x(block.left()), x(block.right()));
            for row in y(block.top())..y(block.bottom()) {
                dest[row * width + left..row * width + right].fill(colour);
            }
        }
    }
}

pub(crate) fn premultiply(bytes: &[u8]) -> PremultipliedColorU8 {
    ColorU8::from_rgba(bytes[0], bytes[1], bytes[2], bytes[3]).premultiply()
}

pub(crate) fn capture_area(image: &RgbaImage, area: PixelRect) -> Option<Pixmap> {
    let mut out = Pixmap::new(area.width(), area.height())?;
    let src = image
        .rows(area)
        .ok()?
        .flat_map(|row| row.as_chunks::<4>().0.iter());
    for (dst, source) in out.pixels_mut().iter_mut().zip(src) {
        *dst = premultiply(source);
    }
    Some(out)
}

pub(crate) fn local_transform(area: PixelRect) -> Transform {
    Transform::from_translate(-(area.left() as f32), -(area.top() as f32))
}

struct Layer {
    shape: Annotation,
    mosaic: Option<Mosaic>,
}
#[derive(Default)]
pub(super) struct MosaicCache {
    layers: Vec<Layer>,
}

impl MosaicCache {
    pub(super) fn update<'a>(
        &mut self,
        image: &RgbaImage,
        shapes: impl Iterator<Item = &'a Annotation> + Clone,
    ) -> Result<(), RenderError> {
        let needed = shapes
            .clone()
            .enumerate()
            .filter(|(_, s)| s.tool == Tool::Pixelate)
            .map(|(i, _)| i + 1)
            .last()
            .unwrap_or(0);
        let prefix = self
            .layers
            .iter()
            .zip(shapes.clone().take(needed))
            .take_while(|(old, new)| old.shape == **new)
            .count();
        self.layers.truncate(prefix);
        for shape in shapes.take(needed).skip(prefix) {
            let mosaic = Grid::new(shape, image)
                .map(|grid| self.compose(image, grid))
                .transpose()?;
            self.layers.push(Layer {
                shape: shape.clone(),
                mosaic,
            });
        }
        Ok(())
    }
    pub(super) fn get(&self, index: usize) -> Option<&Mosaic> {
        self.layers.get(index)?.mosaic.as_ref()
    }
    fn compose(&self, image: &RgbaImage, grid: Grid) -> Result<Mosaic, RenderError> {
        let area = grid.rect;
        let overlaps = |layer: &Layer| {
            let [l, t, r, b] = layer.shape.bounds();
            l < f64::from(area.right())
                && r > f64::from(area.left())
                && t < f64::from(area.bottom())
                && b > f64::from(area.top())
        };
        if !self.layers.iter().any(overlaps) {
            let width = image.width() as usize;
            return Ok(Mosaic::sample(grid, |x, y| {
                premultiply(&image.pixels()[(y as usize * width + x as usize) * 4..])
            }));
        }
        let mut composed = capture_area(image, area).ok_or(RenderError::InvalidBuffer)?;
        let mut count = 0;
        for layer in &self.layers {
            draw_layer(
                &mut composed.as_mut(),
                local_transform(area),
                &layer.shape,
                &mut count,
                layer.mosaic.as_ref(),
            );
        }
        Ok(Mosaic::sample(grid, |x, y| {
            composed.pixel(x - area.left(), y - area.top()).unwrap()
        }))
    }
}
