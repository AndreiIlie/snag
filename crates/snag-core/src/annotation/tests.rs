use super::*;
use crate::{
    export::{decode_png, encode_annotated_png, encode_png},
    geometry::{PixelRect, ScaleFactor},
    history::History,
    image::RgbaImage,
    overlay::{Key, Overlay},
    render::{Backdrop, render_overlay},
    toolbar::{Action, Button},
    view::View,
};

fn p(x: i64, y: i64) -> PixelPoint {
    PixelPoint { x, y }
}
fn shape(tool: Tool, start: PixelPoint, end: PixelPoint) -> Annotation {
    let mut a = Annotation::start(tool, Style::default(), 1., start);
    a.extend(end);
    a
}
fn rgba(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let index = (y as usize * image.width() as usize + x as usize) * 4;
    image.pixels()[index..index + 4].try_into().unwrap()
}

#[test]
fn history_restores_layers_and_moves_in_drawing_order() {
    let mut doc = Document::default();
    let mut history = History::default();
    let red = shape(Tool::Counter, p(10, 10), p(10, 10));
    let line = shape(Tool::Line, p(10, 10), p(50, 10));
    for a in [&red, &line] {
        history.execute(Edit::Add(Some(a.clone())), &mut doc);
    }
    assert_eq!(doc.hit(p(20, 10), 1.), Some(1));
    history.execute(
        Edit::Move {
            index: 0,
            from: PixelPoint::ZERO,
            to: p(80, 20),
        },
        &mut doc,
    );
    history.undo(&mut doc);
    assert_eq!(doc.layers[0], red);
    history.redo(&mut doc);
    let moved = doc.layers[0].clone();
    history.execute(
        Edit::Delete {
            index: 0,
            removed: None,
        },
        &mut doc,
    );
    assert_eq!(doc.layers.as_slice(), std::slice::from_ref(&line));
    history.undo(&mut doc);
    assert_eq!(doc.layers, [moved, line.clone()]);
    history.redo(&mut doc);
    history.undo(&mut doc);
    history.undo(&mut doc);
    history.undo(&mut doc);
    assert_eq!(doc.layers.as_slice(), std::slice::from_ref(&red));
    history.redo(&mut doc);
    assert_eq!(doc.layers, [red.clone(), line]);
    history.undo(&mut doc);
    history.execute(Edit::Add(Some(red)), &mut doc);
    assert!(!history.can_redo());
}

#[test]
fn hits_follow_the_visible_shape() {
    for (tool, hit, miss) in [
        (Tool::Line, p(40, 40), p(40, 70)),
        (Tool::Arrow, p(64, 73), p(40, 70)),
        (Tool::Rectangle, p(20, 40), p(45, 45)),
        (Tool::Ellipse, p(20, 50), p(50, 50)),
        (Tool::Pen, p(40, 40), p(40, 70)),
        (Tool::Highlighter, p(40, 52), p(30, 70)),
        (Tool::Pixelate, p(50, 50), p(90, 50)),
        (Tool::Counter, p(35, 20), p(40, 40)),
    ] {
        let a = shape(tool, p(20, 20), p(80, 80));
        assert!(a.hit(hit, 1.), "{tool:?}");
        assert!(!a.hit(miss, 1.), "{tool:?}");
    }
}

fn press(overlay: &mut Overlay, x: i64, y: i64) {
    overlay.pointer_moved(p(x, y).into());
    overlay.press();
}
fn drag(overlay: &mut Overlay, a: PixelPoint, b: PixelPoint) {
    press(overlay, a.x, a.y);
    overlay.pointer_moved(b.into());
    overlay.release();
}
fn button(overlay: &mut Overlay, button: Button) {
    let (_, r) = overlay
        .toolbar()
        .unwrap()
        .buttons()
        .find(|(b, _)| *b == button)
        .unwrap();
    press(overlay, i64::from(r.left()), i64::from(r.top()));
    overlay.release();
}
fn canvas() -> Overlay {
    let mut overlay = Overlay::new(View::fit((480, 480), (480, 480)).unwrap(), ScaleFactor::ONE);
    drag(&mut overlay, p(20, 20), p(450, 450));
    overlay
}

#[test]
fn tool_gestures_and_styles_use_capture_coordinates() {
    for tool in Tool::ALL {
        let view = View::fit((800, 400), (400, 400)).unwrap();
        let mut overlay = Overlay::new(view, ScaleFactor::new(1.5).unwrap());
        assert_eq!(overlay.key(Key::Tool(tool)), (false, None));
        drag(&mut overlay, p(30, 115), p(365, 270));
        button(&mut overlay, Button::Tool(tool));
        assert!(overlay.active(Button::Tool(tool)));
        button(&mut overlay, Button::Colour(Colour::Green));
        button(&mut overlay, Button::Width(Width::Thin));
        // Letterbox presses, including counters, leave the document empty.
        press(&mut overlay, 80, 10);
        overlay.release();
        press(&mut overlay, 80, 150);
        overlay.release();
        if tool != Tool::Counter {
            assert_eq!(overlay.layers().count(), 0);
        }
        drag(&mut overlay, p(90, 160), p(130, 174));
        let a = overlay.layers().last().unwrap();
        assert_eq!(a.tool, tool);
        assert_eq!(a.ends().0, p(180, 120));
        let end = if tool == Tool::Counter {
            p(180, 120)
        } else {
            p(260, 148)
        };
        assert_eq!(a.ends().1, end);
        assert_eq!(a.style.colour, Colour::Green);
        assert_eq!(a.width(), 6.);
        assert_eq!(
            overlay.layers().count(),
            if tool == Tool::Counter { 2 } else { 1 }
        );
        drag(&mut overlay, p(30, 115), p(20, 110));
        assert_eq!(overlay.crop().unwrap().left(), 40);
        overlay.key(Key::Tool(tool));
        assert!(!overlay.active(Button::Tool(tool)));
    }
}

#[test]
fn selection_moves_deletes_and_replays_one_edit_per_drag() {
    let mut o = canvas();
    o.key(Key::Tool(Tool::Rectangle));
    drag(&mut o, p(60, 60), p(130, 120));
    button(&mut o, Button::Tool(Tool::Rectangle));
    drag(&mut o, p(60, 88), p(80, 100));
    let moved = o.selected().unwrap().clone();
    assert_eq!(moved.ends().0, p(80, 72));
    o.key(Key::Delete);
    assert_eq!(o.layers().count(), 0);
    button(&mut o, Button::Undo);
    assert_eq!(o.layers().next(), Some(&moved));
    o.key(Key::Undo);
    assert_eq!(o.layers().next().unwrap().ends().0, p(60, 60));
    button(&mut o, Button::Redo);
    assert_eq!(o.layers().next(), Some(&moved));
    o.key(Key::Redo);
    assert_eq!(o.layers().count(), 0);
    o.key(Key::Undo);
    press(&mut o, 80, 95);
    o.release();
    assert!(o.selected().is_some());
    press(&mut o, 200, 200);
    o.release();
    assert!(o.selected().is_none());
}

#[test]
fn exported_geometry_and_blending_match_each_tool() {
    let image = RgbaImage::new(120, 120, [10, 20, 30, 255].repeat(120 * 120)).unwrap();
    for (tool, x, y, expected) in [
        (Tool::Arrow, 76, 76, [255, 0, 0, 255]),
        (Tool::Line, 55, 55, [255, 0, 0, 255]),
        (Tool::Rectangle, 30, 55, [255, 0, 0, 255]),
        (Tool::Ellipse, 30, 55, [255, 0, 0, 255]),
        (Tool::Pen, 55, 55, [255, 0, 0, 255]),
        (Tool::Highlighter, 55, 55, [108, 12, 18, 255]),
        (Tool::Counter, 24, 30, [255, 0, 0, 255]),
    ] {
        let a = shape(tool, p(30, 30), p(80, 80));
        let doc = Document { layers: vec![a] };
        let out = decode_png(&encode_annotated_png(&image, &doc, None).unwrap()).unwrap();
        assert_eq!(rgba(&out, x, y), expected, "{tool:?}");
        assert_eq!(rgba(&out, 7, 7), rgba(&image, 7, 7));
        if matches!(tool, Tool::Rectangle | Tool::Ellipse) {
            assert_eq!(rgba(&out, 55, 55), rgba(&image, 55, 55));
        }
        if tool == Tool::Counter {
            assert_eq!(rgba(&out, 34, 30), [255; 4]);
        }
    }
}

#[test]
fn capture_space_mosaics_survive_fit_cropping_and_mid_drag_copy() {
    // Fitting to one third samples only white columns in this pattern.
    let pixels = (0..360 * 180)
        .flat_map(|i| {
            let v = if i % 3 == 1 { 255 } else { 0 };
            [v, v, v, 255]
        })
        .collect();
    let image = RgbaImage::new(360, 180, pixels).unwrap();
    let mut o = Overlay::new(View::fit((360, 180), (120, 100)).unwrap(), ScaleFactor::ONE);
    drag(&mut o, p(4, 23), p(108, 76));
    o.key(Key::Tool(Tool::Pixelate));
    press(&mut o, 20, 35);
    o.pointer_moved(p(80, 60).into());
    let mut frame = vec![0; 120 * 100 * 4];
    render_overlay(&mut Backdrop::new(&image), &o, &mut frame).unwrap();
    let i = (46 * 120 + 46) * 4;
    assert_eq!(&frame[i..i + 4], &[85, 85, 85, 255]);
    assert_eq!(o.key(Key::Copy).1, Some(Action::Copy));
    o.key(Key::Tool(Tool::Pixelate));
    press(&mut o, 46, 46);
    o.pointer_moved(p(50, 50).into());
    assert_eq!(o.key(Key::Save).1, Some(Action::Save));
    assert_eq!(o.layers().next().unwrap().ends().0, p(72, 57));
    let crop = PixelRect::new(77, 69, 92, 79);
    let result =
        decode_png(&encode_annotated_png(&image, &o.into_document(), crop).unwrap()).unwrap();
    assert!(
        result
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|px| *px == [85, 85, 85, 255])
    );
}

#[test]
fn marker_and_dim_stay_in_preview_and_tools_clip_to_the_capture() {
    let image = RgbaImage::new(200, 100, [100, 100, 100, 255].repeat(200 * 100)).unwrap();
    let mut o = Overlay::new(View::fit((200, 100), (200, 180)).unwrap(), ScaleFactor::ONE);
    drag(&mut o, p(20, 60), p(180, 117));
    o.key(Key::Tool(Tool::Line));
    drag(&mut o, p(70, 80), p(70, 4));
    o.key(Key::Tool(Tool::Line));
    press(&mut o, 70, 70);
    o.release();
    let mut frame = vec![0; 200 * 180 * 4];
    let mut backdrop = Backdrop::new(&image);
    render_overlay(&mut backdrop, &o, &mut frame).unwrap();
    let px = |x: usize, y: usize| &frame[(y * 200 + x) * 4..(y * 200 + x + 1) * 4];
    assert_eq!(px(70, 30), [0, 0, 0, 255]);
    assert_eq!(px(70, 50), [127, 0, 0, 255]);
    assert_eq!(px(67, 70), [60, 155, 255, 255]);
    assert_eq!(px(70, 70), [255, 0, 0, 255]);
    let mut again = vec![0; frame.len()];
    render_overlay(&mut backdrop, &o, &mut again).unwrap();
    assert_eq!(frame, again);
    let crop = o.crop();
    let out = decode_png(&encode_annotated_png(&image, &o.into_document(), crop).unwrap()).unwrap();
    assert_eq!(rgba(&out, 47, 10), [100, 100, 100, 255]);
    assert_eq!(rgba(&out, 50, 10), px(70, 70));
}

#[test]
fn mosaics_compose_in_order_and_preserve_only_untouched_straight_alpha() {
    let mut bytes = [20, 0, 0, 1].repeat(64 * 64);
    for pair in bytes.as_chunks_mut::<8>().0 {
        pair[4] = 80;
    }
    let image = RgbaImage::new(64, 64, bytes).unwrap();
    assert_eq!(
        encode_png(&image, None).unwrap(),
        encode_annotated_png(&image, &Document::default(), None).unwrap()
    );
    let mut coarse = shape(Tool::Pixelate, p(10, 10), p(58, 58));
    coarse.style.width = Width::Thick;
    let mut fine = coarse.clone();
    fine.style.width = Width::Thin;
    let mut doc = Document {
        layers: vec![
            shape(Tool::Arrow, p(20, 34), p(46, 34)),
            coarse.clone(),
            fine.clone(),
        ],
    };
    let out = decode_png(&encode_annotated_png(&image, &doc, None).unwrap()).unwrap();
    assert_eq!(rgba(&out, 10, 10), rgba(&out, 11, 10));
    assert_eq!(rgba(&out, 1, 1), rgba(&image, 1, 1));
    let before = rgba(&out, 20, 34);
    assert!(before[3] > 1 && before[3] < 255);
    assert_eq!(before, rgba(&out, 28, 43));
    doc.layers.push(shape(Tool::Line, p(10, 35), p(50, 35)));
    let out =
        decode_png(&encode_annotated_png(&image, &doc, PixelRect::new(19, 33, 29, 44)).unwrap())
            .unwrap();
    assert_eq!(rgba(&out, 1, 2), [255, 0, 0, 255]);
    assert_eq!(rgba(&out, 9, 10), before);
}

#[test]
fn deleting_a_counter_renumbers_later_digits_and_undo_restores_them() {
    let image = RgbaImage::new(480, 480, [25, 30, 35, 255].repeat(480 * 480)).unwrap();
    let mut o = canvas();
    let mut frame = vec![0; 480 * 480 * 4];
    let mut backdrop = Backdrop::new(&image);
    o.key(Key::Tool(Tool::Counter));
    for x in [80, 160] {
        press(&mut o, x, 80);
        o.release();
    }
    o.key(Key::Tool(Tool::Counter));
    render_overlay(&mut backdrop, &o, &mut frame).unwrap();
    let second = (80 * 480 + 160) * 4;
    // The middle bar is present in digit 2 and absent in digit 1.
    assert_eq!(&frame[second..second + 4], &[255; 4]);
    press(&mut o, 80, 80);
    o.release();
    o.key(Key::Delete);
    render_overlay(&mut backdrop, &o, &mut frame).unwrap();
    assert_eq!(&frame[second..second + 4], &[255, 0, 0, 255]);
    o.key(Key::Undo);
    render_overlay(&mut backdrop, &o, &mut frame).unwrap();
    assert_eq!(&frame[second..second + 4], &[255; 4]);
}

#[test]
fn the_identity_preview_and_export_share_layer_colours_after_edits() {
    let mut o = canvas();
    o.key(Key::Tool(Tool::Highlighter));
    press(&mut o, 100, 100);
    for point in [p(200, 200), p(100, 200), p(200, 100)] {
        o.pointer_moved(point.into());
    }
    o.release();
    let image = RgbaImage::new(480, 480, [10, 20, 30, 255].repeat(480 * 480)).unwrap();
    let mut backdrop = Backdrop::new(&image);
    let mut frame = vec![0; 480 * 480 * 4];
    let i = (150 * 480 + 150) * 4;
    render_overlay(&mut backdrop, &o, &mut frame).unwrap();
    assert_eq!(&frame[i..i + 4], &[108, 12, 18, 255]);
    o.key(Key::Tool(Tool::Pixelate));
    drag(&mut o, p(100, 125), p(202, 187));
    render_overlay(&mut backdrop, &o, &mut frame).unwrap();
    let before = frame.clone();
    o.key(Key::Undo);
    render_overlay(&mut backdrop, &o, &mut frame).unwrap();
    assert_ne!(before, frame);
    o.key(Key::Redo);
    render_overlay(&mut backdrop, &o, &mut frame).unwrap();
    assert_eq!(before, frame);
    let crop = PixelRect::new(112, 112, 214, 208).unwrap();
    let result =
        decode_png(&encode_annotated_png(&image, &o.into_document(), Some(crop)).unwrap()).unwrap();
    for y in crop.top()..crop.bottom() {
        let start = ((y * 480 + crop.left()) * 4) as usize;
        let end = start + crop.width() as usize * 4;
        let row = ((y - crop.top()) * crop.width() * 4) as usize;
        assert_eq!(
            &frame[start..end],
            &result.pixels()[row..row + (end - start)]
        );
    }
}
