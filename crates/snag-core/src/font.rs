use fontdb::{Database, Family, ID, Query};
use std::collections::HashMap;
use tiny_skia::{Path, PathBuilder, Transform};
use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// Geometry relative to the left edge and vertical centre of the line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Line {
    pub outline: Option<Path>,
    pub carets: Vec<f32>,
    pub half_height: f32,
}

pub(crate) struct Fonts {
    db: Database,
    default: ID,
    ascent: f32,
    descent: f32,
    missing: Glyph,
    glyphs: HashMap<char, Glyph>,
}

impl Fonts {
    pub fn system() -> Option<Self> {
        let mut db = Database::new();
        db.load_system_fonts();
        Self::from_database(db)
    }

    fn from_database(db: Database) -> Option<Self> {
        let preferred = db.query(&Query {
            families: &[Family::SansSerif],
            ..Query::default()
        });
        let (default, ascent, descent, missing) = preferred
            .into_iter()
            .chain(db.faces().map(|info| info.id))
            .find_map(|id| {
                db.with_face_data(id, |bytes, index| {
                    let face = Face::parse(bytes, index).ok()?;
                    let tables = face.tables();
                    if tables.glyf.is_none() && tables.cff.is_none() && tables.cff2.is_none() {
                        return None;
                    }
                    let em = f32::from(face.units_per_em());
                    Some((
                        id,
                        f32::from(face.ascender()) / em,
                        f32::from(face.descender()) / em,
                        Glyph::read(&face, GlyphId(0)),
                    ))
                })?
            })?;
        Some(Self {
            db,
            default,
            ascent,
            descent,
            missing,
            glyphs: HashMap::new(),
        })
    }

    fn glyph(&mut self, ch: char) -> &Glyph {
        self.glyphs.entry(ch).or_insert_with(|| {
            std::iter::once(self.default)
                .chain(
                    self.db
                        .faces()
                        .map(|info| info.id)
                        .filter(|id| *id != self.default),
                )
                .find_map(|id| {
                    self.db.with_face_data(id, |bytes, index| {
                        let face = Face::parse(bytes, index).ok()?;
                        let glyph = Glyph::read(&face, face.glyph_index(ch)?);
                        (glyph.path.is_some() || ch.is_whitespace()).then_some(glyph)
                    })?
                })
                .unwrap_or_else(|| self.missing.clone())
        })
    }

    pub fn layout(&mut self, text: &str, size: f32) -> Line {
        let half_height = (self.ascent - self.descent) * size / 2.;
        let baseline = self.ascent * size - half_height;
        let mut path = PathBuilder::new();
        let mut carets = vec![0.];
        let mut x = 0.;
        for ch in text.chars() {
            let glyph = self.glyph(ch);
            let scale = size / glyph.em;
            if let Some(outline) = &glyph.path {
                let transform = Transform::from_row(scale, 0., 0., -scale, x, baseline);
                if let Some(placed) = outline.clone().transform(transform) {
                    path.push_path(&placed);
                }
            }
            x += glyph.advance * scale;
            carets.push(x);
        }
        Line {
            outline: path.finish(),
            carets,
            half_height,
        }
    }
}

#[derive(Clone)]
struct Glyph {
    path: Option<Path>,
    advance: f32,
    em: f32,
}

impl Glyph {
    fn read(face: &Face<'_>, id: GlyphId) -> Self {
        let mut builder = Outline(PathBuilder::new());
        let path = face
            .outline_glyph(id, &mut builder)
            .and_then(|_| builder.0.finish());
        Self {
            path,
            advance: f32::from(face.glyph_hor_advance(id).unwrap_or(0)),
            em: f32::from(face.units_per_em()),
        }
    }
}

struct Outline(PathBuilder);

impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

#[cfg(test)]
pub(crate) fn fixture_fonts() -> Fonts {
    let mut db = Database::new();
    db.load_font_data(include_bytes!("../tests/fixtures/Caladea-Regular.ttf").to_vec());
    db.load_font_data(include_bytes!("../tests/fixtures/NotoMono-Regular.ttf").to_vec());
    Fonts::from_database(db).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_discovery_falls_back_to_first_face() {
        let mut db = Database::new();
        db.load_fonts_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));
        db.set_sans_serif_family("absent family");
        assert_eq!(db.faces().count(), 2);
        let first = db.faces().next().unwrap().id;
        assert_eq!(Fonts::from_database(db).unwrap().default, first);
        assert!(Fonts::from_database(Database::new()).is_none());
    }

    #[test]
    fn accents_spaces_and_fallback_follow_character_advances() {
        let mut fonts = fixture_fonts();
        let line = fonts.layout("Țară é", 24.);
        assert_eq!(line.carets.len(), 7);
        assert!(line.carets.windows(2).all(|p| p[1] > p[0]));
        let space = fonts.layout(" ", 24.);
        assert!(space.outline.is_none() && space.carets[1] > 0.);
        assert_ne!(fonts.glyph('Ț').path.clone(), fonts.missing.path);
        assert_ne!(fonts.glyph('Ț').path.clone(), fonts.glyph('T').path);
        assert_eq!(fonts.glyph('\u{10ffff}').path.clone(), fonts.missing.path);
        let mut db = Database::new();
        db.load_font_data(include_bytes!("../tests/fixtures/NotoMono-Regular.ttf").to_vec());
        let mut fallback = Fonts::from_database(db).unwrap();
        assert_eq!(fonts.glyph('Я').path, fallback.glyph('Я').path);
        assert_ne!(fonts.glyph('Я').path.clone(), fonts.missing.path);
    }
}
