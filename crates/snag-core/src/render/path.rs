use tiny_skia::{
    Color, ColorU8, FillRule, Paint, Path, PathSegment, PixmapMut, Point, Stroke, Transform,
};

pub(super) fn draw(
    target: &mut PixmapMut<'_>,
    transform: Transform,
    path: Path,
    colour: Color,
    stroke: Option<&Stroke>,
) {
    if transform.sx != 1.0 || transform.sy != 1.0 {
        let mut paint = Paint::default();
        paint.set_color(colour);
        if let Some(stroke) = stroke {
            target.stroke_path(&path, &paint, stroke, transform, None);
        } else {
            target.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }
        return;
    }
    let outline = if let Some(stroke) = stroke {
        let Some(outline) = path.stroke(stroke, 1.) else {
            return;
        };
        outline
    } else {
        path
    };
    let origin = (-(transform.tx as i64), -(transform.ty as i64));
    fill(target, origin, &outline, colour.to_color_u8());
}

// Scan the outline once on a quarter-pixel grid fixed to the capture. Clipping
// the resulting runs keeps curve coverage identical in the frame and cropped PNG.
const SAMPLES: i64 = 4;

struct Edge {
    first: i64,
    end: i64,
    start: Point,
    slope: f64,
    winding: i32,
}

fn sample_index(coordinate: f64) -> i64 {
    (coordinate * SAMPLES as f64 - 0.5).ceil() as i64
}

fn edges(path: &Path, top: i64, bottom: i64) -> Vec<Edge> {
    let mut edges = Vec::new();
    let mut line = |a: Point, b: Point| {
        let (start, finish, winding) = if a.y <= b.y { (a, b, 1) } else { (b, a, -1) };
        let first = sample_index(start.y.into()).max(top * SAMPLES);
        let end = sample_index(finish.y.into()).min(bottom * SAMPLES);
        if first < end {
            edges.push(Edge {
                first,
                end,
                start,
                slope: (f64::from(finish.x) - f64::from(start.x))
                    / (f64::from(finish.y) - f64::from(start.y)),
                winding,
            });
        }
    };
    let mut current = Point::default();
    let mut segments = path.segments();
    segments.set_auto_close(true);
    for segment in segments {
        match segment {
            PathSegment::MoveTo(p) => current = p,
            PathSegment::LineTo(p) => {
                line(current, p);
                current = p;
            }
            PathSegment::QuadTo(b, c) => {
                let a = current;
                let steps = subdivisions(a, b, c);
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let u = 1. - t;
                    let p = scaled(a, u * u) + scaled(b, 2. * u * t) + scaled(c, t * t);
                    line(current, p);
                    current = p;
                }
            }
            PathSegment::CubicTo(b, c, d) => {
                let a = current;
                let steps = subdivisions(a, b, c).max(subdivisions(b, c, d));
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let u = 1. - t;
                    let p = scaled(a, u * u * u)
                        + scaled(b, 3. * u * u * t)
                        + scaled(c, 3. * u * t * t)
                        + scaled(d, t * t * t);
                    line(current, p);
                    current = p;
                }
            }
            PathSegment::Close => {}
        }
    }
    edges.sort_unstable_by_key(|e| e.first);
    edges
}

fn scaled(mut p: Point, factor: f32) -> Point {
    p.scale(factor);
    p
}

// Bound curve-to-chord error to an eighth of a pixel, limiting work for
// extreme off-canvas control points.
fn subdivisions(a: Point, b: Point, c: Point) -> usize {
    let bend = a - scaled(b, 2.) + c;
    (6. * bend.x.abs().max(bend.y.abs()))
        .sqrt()
        .ceil()
        .clamp(1., 4096.) as usize
}

fn fill(target: &mut PixmapMut<'_>, (left, top): (i64, i64), path: &Path, colour: ColorU8) {
    let width = target.width() as usize;
    let bottom = top + i64::from(target.height());
    let bounds = path.bounds();
    if f64::from(bounds.right()) <= left as f64
        || f64::from(bounds.left()) >= (left + width as i64) as f64
    {
        return;
    }
    let edges = edges(path, top, bottom);
    let Some(first) = edges.first() else {
        return;
    };
    let end = edges.iter().map(|e| e.end).max().unwrap();
    let mut pending = edges.iter().peekable();
    let mut active: Vec<&Edge> = Vec::new();
    let mut crossings = Vec::new();
    let mut events = Vec::new();
    let data = target.data_mut();
    for y in first.first.div_euclid(SAMPLES)..=(end - 1).div_euclid(SAMPLES) {
        events.clear();
        for sub in y * SAMPLES..(y + 1) * SAMPLES {
            active.retain(|e| e.end > sub);
            while pending.peek().is_some_and(|e| e.first <= sub) {
                active.push(pending.next().unwrap());
            }
            crossings.clear();
            let sample_y = (sub as f64 + 0.5) / SAMPLES as f64;
            for edge in &active {
                let x = f64::from(edge.start.x) + (sample_y - f64::from(edge.start.y)) * edge.slope;
                let local = sample_index(x)
                    .saturating_sub(left * SAMPLES)
                    .clamp(0, width as i64 * SAMPLES);
                crossings.push((local as usize, edge.winding));
            }
            crossings.sort_unstable();
            let mut winding = 0;
            for &(x, step) in &crossings {
                let was_outside = winding == 0;
                winding += step;
                if was_outside != (winding == 0) {
                    // A subpixel endpoint changes the coverage of its own pixel
                    // by the remaining fraction, and of following pixels in full.
                    let sign = if was_outside { 1 } else { -1 };
                    let fraction = (x % SAMPLES as usize) as i32;
                    events.push((x / SAMPLES as usize, sign * (SAMPLES as i32 - fraction)));
                    if fraction != 0 {
                        events.push((x / SAMPLES as usize + 1, sign * fraction));
                    }
                }
            }
        }
        events.sort_unstable();
        let start = (y - top) as usize * width * 4;
        let row = &mut data[start..start + width * 4];
        let mut previous = 0;
        let mut coverage = 0;
        for &(x, change) in &events {
            if x > previous && coverage > 0 {
                blend(&mut row[previous * 4..x * 4], colour, coverage);
            }
            coverage += change;
            previous = x;
        }
    }
}

fn blend(run: &mut [u8], colour: ColorU8, coverage: i32) {
    let samples = (SAMPLES * SAMPLES) as u32;
    let alpha = (u32::from(colour.alpha()) * coverage as u32 + samples / 2) / samples;
    let rgba = [colour.red(), colour.green(), colour.blue(), 255];
    if alpha == 255 {
        run.as_chunks_mut::<4>().0.fill(rgba);
        return;
    }
    for pixel in run.as_chunks_mut::<4>().0 {
        for (dst, src) in pixel.iter_mut().zip(rgba) {
            *dst = ((u32::from(src) * alpha + u32::from(*dst) * (255 - alpha) + 127) / 255) as u8;
        }
    }
}
