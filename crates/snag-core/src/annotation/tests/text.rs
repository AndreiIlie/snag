use super::*;

fn editor() -> Overlay {
    let mut o = canvas();
    o.use_fixture_fonts();
    o.key(Key::Tool(Tool::Text));
    click(&mut o, 100, 100);
    assert!(o.is_typing());
    o
}

fn click(o: &mut Overlay, x: i64, y: i64) {
    press(o, x, y);
    o.release();
}

fn type_text(o: &mut Overlay, text: &str) {
    for ch in text.chars() {
        o.key(Key::Character(ch));
    }
}

fn text(o: &Overlay) -> &Text {
    o.layers().last().unwrap().text.as_ref().unwrap()
}

#[test]
fn edits_whole_characters_and_commits_one_history_step() {
    let mut o = editor();
    type_text(&mut o, "é字Ț");
    o.key(Key::Left);
    o.key(Key::Backspace);
    assert_eq!(text(&o).content, "éȚ");
    o.key(Key::Home);
    o.key(Key::Delete);
    type_text(&mut o, "Ăα");
    o.key(Key::Left);
    o.key(Key::Right);
    type_text(&mut o, "β");
    o.key(Key::End);
    type_text(&mut o, "!");
    assert_eq!(o.key(Key::Enter), (true, None));
    assert!(!o.is_typing());
    assert_eq!(text(&o).content, "ĂαβȚ!");
    o.key(Key::Undo);
    assert_eq!(o.layers().count(), 0);
    o.key(Key::Redo);
    assert_eq!(text(&o).content, "ĂαβȚ!");
    assert_eq!(o.key(Key::Enter).1, Some(Action::Copy));
}

#[test]
fn empty_spaces_and_discarded_drafts_preserve_redo() {
    let mut o = editor();
    type_text(&mut o, "A");
    o.key(Key::Undo);
    for (input, finish) in [
        ("", Key::Enter),
        ("   ", Key::Enter),
        ("discard", Key::Escape),
    ] {
        click(&mut o, 140, 150);
        type_text(&mut o, input);
        assert_eq!(o.key(finish).1, None);
        assert!(!o.is_typing());
        assert_eq!(o.layers().count(), 0);
    }
    o.key(Key::Redo);
    assert_eq!(text(&o).content, "A");
}

#[test]
fn restyles_in_place_and_moves_and_deletes_the_finished_line() {
    let mut o = editor();
    type_text(&mut o, "Țară");
    let medium = text(&o).line.carets[4];
    assert_eq!(text(&o).size, 24.);
    button(&mut o, Button::Width(Width::Thin));
    assert_eq!(text(&o).size, 16.);
    assert!(text(&o).line.carets[4] < medium);
    button(&mut o, Button::Width(Width::Thick));
    assert_eq!(text(&o).size, 40.);
    assert!(text(&o).line.carets[4] > medium);
    button(&mut o, Button::Colour(Colour::Green));
    assert_eq!(o.layers().last().unwrap().style.colour, Colour::Green);
    assert!(o.is_typing());

    click(&mut o, 210, 200);
    assert!(o.is_typing());
    type_text(&mut o, "B");
    assert_eq!(o.key(Key::Save).1, Some(Action::Save));
    assert_eq!(o.layers().count(), 2);
    assert!(!o.is_typing());
    o.key(Key::Tool(Tool::Text));
    let before = o.layers().next().unwrap().bounds();
    drag(&mut o, p(105, 100), p(155, 120));
    let moved = o.selected().unwrap().bounds();
    assert_eq!(
        moved,
        [
            before[0] + 50.,
            before[1] + 20.,
            before[2] + 50.,
            before[3] + 20.
        ]
    );
    o.key(Key::Delete);
    assert_eq!(o.layers().count(), 1);
    o.key(Key::Undo);
    assert_eq!(o.layers().next().unwrap().bounds(), moved);
    o.key(Key::Undo);
    assert_eq!(o.layers().next().unwrap().bounds(), before);
}

#[test]
fn crop_through_accents_matches_frame_and_excludes_caret() {
    let mut o = editor();
    type_text(&mut o, "Țară é");
    let image = RgbaImage::new(480, 480, [80, 70, 60, 255].repeat(480 * 480)).unwrap();
    let mut backdrop = Backdrop::new(&image);
    let mut frame = vec![0; 480 * 480 * 4];
    render_overlay(&mut backdrop, &o, &mut frame).unwrap();
    let [caret_x, _, _] = o.text_caret().unwrap();
    let caret = (100 * 480 + caret_x.round() as usize) * 4;
    let editing = frame.clone();
    o.key(Key::Copy);
    render_overlay(&mut backdrop, &o, &mut frame).unwrap();
    assert_ne!(&editing[caret..caret + 4], &frame[caret..caret + 4]);
    let crop = PixelRect::new(103, 86, caret_x.ceil() as u32 + 3, 109).unwrap();
    let out =
        decode_png(&encode_annotated_png(&image, &o.into_document(), Some(crop)).unwrap()).unwrap();
    assert!(out.pixels().as_chunks::<4>().0.iter().any(|v| v[0] > 80));
    for (y, row) in out
        .pixels()
        .chunks_exact(crop.width() as usize * 4)
        .enumerate()
    {
        let i = ((y + crop.top() as usize) * 480 + crop.left() as usize) * 4;
        assert_eq!(row, &frame[i..i + row.len()]);
    }
}

#[test]
fn fitted_view_preserves_draft_and_maps_ink_and_caret() {
    let mut o = editor();
    type_text(&mut o, "É");
    let caret = o.text_caret();
    o.reset_pointer();
    let view = View::fit((480, 480), (240, 280)).unwrap();
    o.set_view(view, ScaleFactor::ONE);
    assert_eq!(o.text_caret(), caret);
    let image = RgbaImage::new(480, 480, [30, 40, 50, 255].repeat(480 * 480)).unwrap();
    let mut frame = vec![0; 240 * 280 * 4];
    render_overlay(&mut Backdrop::new(&image), &o, &mut frame).unwrap();
    let pixels = frame.as_chunks::<4>().0;
    let ink = |x: usize, y: usize| pixels[y * 240 + x][0] > 100;
    assert!((60..79).any(|y| (50..61).any(|x| ink(x, y))));
    assert!(!(87..113).any(|y| (100..125).any(|x| ink(x, y))));
    let x = (caret.unwrap()[0] * 0.5).round() as usize;
    assert_eq!(pixels[70 * 240 + x], [60, 155, 255, 255]);
}
