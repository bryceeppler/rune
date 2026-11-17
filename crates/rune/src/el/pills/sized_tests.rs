//! A sized pill in a field: a photo between lines, beside a checkbox chip in the same field.
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use accesskit::Role;

use crate::render::{KIND_IMAGE, KIND_RECT};
use crate::{Caret, Color, Cx, El, Env, Icon, Key, KeyEvent, Mark, MarkRange, Mods, PillStyle, Pixels, Runtime, Selection, TextEdit, TextStyle, col, el, text_input};

const PHOTO: &str = "![](photo:6a09e667)";
const TASK: &str = "- [ ] ";
const LINE: f32 = 22.0;
const TINT: Color = Color { r: 0.0, g: 0.0, b: 1.0, a: 0.5 };
const CARET: Color = Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };
const RING: Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
const HAIRLINE: Color = Color { r: 1.0, g: 1.0, b: 0.0, a: 1.0 };
const MUTED: Color = Color { r: 0.0, g: 1.0, b: 1.0, a: 1.0 };
static WAITING: Icon = Icon { name: "waiting", view: 24.0, body: "<rect width=\"24\" height=\"24\"/>" };

struct S {
    edit: TextEdit,
    photo: Pixels,
    width: f32,
    ringed: bool,
    margin: [f32; 2],
    pressed: Vec<String>,
    said: &'static str,
}

/// The note's field, and a 90 by 34 point bar inside the photo's top-right corner, 10 points in,
/// placed from where the field says the photo draws.
fn view(s: &S, cx: &mut Cx<S>) -> El {
    let (photo, ringed, [above, below], said) = (s.photo.clone(), s.ringed, s.margin, s.said);
    let field = text_input(&s.edit, TextStyle::new(15.0, 400.0).lh(LINE))
        .key("note")
        .w(s.width)
        .caret(CARET, TINT)
        .pill_style(move |id| match id {
            "photo" => {
                let style = PillStyle::new().sized([640.0, 400.0]).image(&photo).description(said).radius(8.0).border(HAIRLINE, 1.0)
                    .fg(MUTED).icon(&WAITING).caption("Downloading photo…", TextStyle::new(13.0, 500.0).lh(20.0)).margin(above, below);
                if ringed { style.selection_ring(RING, 3.0, 2.0) } else { style }
            }
            _ => PillStyle::new(),
        })
        .on_edit(|s: &mut S| &mut s.edit)
        .on_submit(|s: &mut S, _| s.edit.insert("\n"))
        .on_pill_press(|s: &mut S, id, _| s.pressed.push(id.to_owned()))
        .on_input_copy(|s: &S, copy| {
            let (a, b) = s.edit.range();
            match s.edit.pills().any(|(r, id)| id == "photo" && a <= r.start && r.end <= b) {
                true => copy.with_image(png()),
                false => copy,
            }
        });
    let bar = cx.pill_rect("note", "photo").map(|r| el().key("bar").size(90.0, 34.0).pin(Some(r[0] + r[2] - 100.0), Some(r[1] + 10.0), None, None));
    col().child(el().child(field).children(bar))
}

/// "- [ ] Pack\n" with a checkbox pill, then the photo on a line of its own, then "After".
fn note() -> (TextEdit, usize, usize) {
    let text = format!("{TASK}Pack\n{PHOTO}\nAfter");
    let at = text.find(PHOTO).unwrap();
    let marks = vec![MarkRange::new(0..TASK.len(), Mark::Pill("task".into())), MarkRange::new(at..at + PHOTO.len(), Mark::Pill("photo".into()))];
    let mut edit = TextEdit::from_parts(text, marks, Vec::new());
    edit.focused = true;
    edit.set_caret(Caret::at(at - 1));
    (edit, at, at + PHOTO.len())
}

/// A PNG's header for a 64 by 40 image, which is all the clipboard checks.
fn png() -> crate::ClipboardImage {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    bytes.extend(64u32.to_be_bytes().into_iter().chain(40u32.to_be_bytes()).chain([8, 6, 0, 0, 0]));
    crate::ClipboardImage::from_png(bytes).unwrap()
}

fn gradient(w: u32, h: u32) -> Vec<u8> {
    (0..w * h).flat_map(|i| [(i % w * 255 / w) as u8, (i / w * 255 / h) as u8, 128, 255]).collect()
}

fn runtime(width: f32, decoded: bool) -> (Runtime<S>, usize, usize) {
    let (edit, at, end) = note();
    let photo = Pixels::new();
    if decoded {
        photo.set(64, 40, gradient(64, 40));
    }
    let mut rt = Runtime::new(S { edit, photo, width, ringed: false, margin: [0.0; 2], pressed: Vec::new(), said: "Harbour at dusk" }, view, (600.0, 600.0), Env::new(true, 2.0));
    settle(&mut rt);
    (rt, at, end)
}

fn settle(rt: &mut Runtime<S>) {
    for _ in 0..30 {
        if !rt.frame(1.0 / 60.0) {
            break;
        }
    }
}

fn key(rt: &mut Runtime<S>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    settle(rt);
}

fn shift() -> Mods {
    Mods { shift: true, ..Mods::default() }
}

fn caret(rt: &Runtime<S>) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some("note")).unwrap().caret.unwrap()
}

fn field(rt: &Runtime<S>) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some("note")).unwrap().rect
}

/// The photo's drawn rect in points.
fn image(rt: &Runtime<S>) -> Option<[f32; 4]> {
    rt.instances.iter().find(|i| i.params[1] == KIND_IMAGE).map(|i| i.rect.map(|v| v / 2.0))
}

fn filled(rt: &Runtime<S>, c: Color) -> Vec<[f32; 4]> {
    let want = c.premul_srgb(1.0);
    rt.instances
        .iter()
        .filter(|i| i.params[1] == KIND_RECT && i.fill.iter().zip(want).all(|(a, b)| (a - b).abs() < 0.02))
        .map(|i| i.rect.map(|v| v / 2.0))
        .collect()
}

#[test]
fn the_caret_beside_a_photo_is_as_tall_as_the_text_and_steps_over_the_photo() {
    let (mut rt, at, end) = runtime(500.0, true);
    let text_caret = caret(&rt);
    assert!((text_caret[3] - LINE).abs() < 1.0, "a text line's caret is the line's height: {text_caret:?}");
    let photo = image(&rt).expect("the photo draws");
    let mut stops = Vec::new();
    for _ in 0..3 {
        key(&mut rt, Key::Right, Mods::default());
        stops.push((rt.state.edit.selection().focus.byte, caret(&rt)));
    }
    assert_eq!(stops.iter().map(|s| s.0).collect::<Vec<_>>(), [at, end, end + 1], "Right stops before the photo, after it, then on the next line");
    for (byte, c) in &stops[..2] {
        assert!((c[3] - LINE).abs() < 1.0, "the caret at {byte} beside the photo is text height: {c:?}");
        assert!((c[1] + c[3] - (photo[1] + photo[3])).abs() < LINE * 0.5, "it sits at the photo's foot: {c:?} {photo:?}");
    }
    assert!((stops[0].1[0] - photo[0]).abs() < 1.5, "before the photo, at its left edge");
    assert!((stops[1].1[0] - (photo[0] + photo[2])).abs() < 1.5, "after the photo, at its right edge");
    key(&mut rt, Key::Left, Mods::default());
    key(&mut rt, Key::Left, Mods::default());
    assert_eq!(rt.state.edit.selection().focus.byte, at, "Left steps back over the photo");
    key(&mut rt, Key::Up, Mods::default());
    assert!(rt.state.edit.selection().focus.byte < at - 1, "Up from the photo's line goes to the line above");
    key(&mut rt, Key::Down, Mods::default());
    key(&mut rt, Key::Down, Mods::default());
    assert!(rt.state.edit.selection().focus.byte > end, "Down passes the photo's line to the one below");
}

#[test]
fn shift_arrows_select_a_photo_whole_and_backspace_delete_and_undo_treat_it_as_one() {
    let (mut rt, at, end) = runtime(500.0, true);
    let text = rt.state.edit.text.clone();
    key(&mut rt, Key::Right, shift());
    key(&mut rt, Key::Right, shift());
    assert_eq!(rt.state.edit.range(), (at - 1, end), "Shift+Right takes the photo whole");
    rt.update(|s, _| s.edit.set_caret(Caret::at(end)));
    key(&mut rt, Key::Backspace, Mods::default());
    assert_eq!(rt.state.edit.text, text.replace(PHOTO, ""), "Backspace after the photo deletes all of it");
    key(&mut rt, Key::Command('z'), Mods { cmd: true, ..Mods::default() });
    assert_eq!(rt.state.edit.text, text, "one undo brings it back");
    assert!(image(&rt).is_some(), "and it draws again");
    rt.update(|s, _| s.edit.set_caret(Caret::at(at)));
    key(&mut rt, Key::Delete, Mods::default());
    assert_eq!(rt.state.edit.text, text.replace(PHOTO, ""), "Delete before the photo deletes all of it");
}

#[test]
fn a_press_on_a_photo_fires_on_pill_press() {
    let (mut rt, _, _) = runtime(500.0, true);
    let r = image(&rt).unwrap();
    let (x, y) = (r[0] + r[2] * 0.7, r[1] + r[3] * 0.5);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.pressed, ["photo"]);
}

#[test]
fn the_photo_line_grows_to_the_box_and_the_box_scales_down_to_a_narrow_field() {
    let (wide, _, _) = runtime(500.0, true);
    let (narrow, _, _) = runtime(200.0, true);
    for (rt, width) in [(&wide, 500.0), (&narrow, 200.0)] {
        let (photo, f) = (image(rt).unwrap(), field(rt));
        let want_w = 640.0f32.min(width - 1.0);
        assert!(photo[2] <= width && photo[2] >= want_w - 1.0, "the box fills the field's width at most: {photo:?} in {width}");
        assert!((photo[3] / photo[2] - 400.0 / 640.0).abs() < 0.01, "and keeps its shape: {photo:?}");
        assert!(f[3] >= photo[3] + 2.0 * LINE, "the field holds the photo's height and two text lines: {f:?}");
    }
    let (empty, _, _) = {
        let (mut rt, at, end) = runtime(500.0, true);
        rt.update(|s, _| s.edit.set_selection(Selection::bytes(at, end)));
        rt.update(|s, _| s.edit.insert(""));
        settle(&mut rt);
        (rt, at, end)
    };
    assert!(field(&empty)[3] < 4.0 * LINE, "without the photo the field is three lines: {:?}", field(&empty));
}

#[test]
fn a_selected_photo_draws_its_tint_over_the_image_with_no_caret_or_strip() {
    let (mut rt, at, end) = runtime(500.0, true);
    assert_eq!(filled(&rt, CARET).len(), 1, "a caret draws before the photo is selected");
    rt.update(|s, _| s.edit.set_selection(Selection::bytes(at, end)));
    settle(&mut rt);
    let photo = image(&rt).unwrap();
    let image_at = rt.instances.iter().position(|i| i.params[1] == KIND_IMAGE).unwrap();
    let tints: Vec<usize> = rt.instances.iter().enumerate().filter(|(_, i)| i.params[1] == KIND_RECT && i.fill[2] > 0.1 && i.fill[0] < 0.01 && i.fill[1] < 0.01).map(|(k, _)| k).collect();
    assert_eq!(tints.len(), 1, "one tint, and no selection strip beside or under the photo");
    assert!(tints[0] > image_at, "the tint draws over the image");
    let tint = rt.instances[tints[0]].rect.map(|v| v / 2.0);
    assert!(tint.iter().zip(photo).all(|(a, b)| (a - b).abs() < 0.5), "it covers the photo exactly: {tint:?} {photo:?}");
    assert!(filled(&rt, CARET).is_empty(), "no caret while the photo is selected");
    rt.update(move |s, _| s.edit.set_selection(Selection::bytes(at - 2, end)));
    settle(&mut rt);
    assert_eq!(filled(&rt, TINT).len(), 2, "selected with a character before it: the character's strip and the photo's tint");
}

/// The rect of the first instance with a border of `color`, in points, and its border width.
fn ring(rt: &Runtime<S>, color: Color) -> Option<([f32; 4], f32)> {
    let want = color.premul_srgb(1.0);
    rt.instances.iter().find(|i| i.border.iter().zip(want).all(|(a, b)| (a - b).abs() < 0.02)).map(|i| (i.rect.map(|v| v / 2.0), i.params[0] / 2.0))
}

#[test]
fn a_ringed_photo_keeps_room_above_for_its_ring_and_draws_it_outside_in_place_of_the_tint() {
    let (plain, _, _) = runtime(500.0, true);
    let (mut rt, at, end) = runtime(500.0, true);
    rt.update(|s, _| s.ringed = true);
    settle(&mut rt);
    let (photo, unringed) = (image(&rt).unwrap(), image(&plain).unwrap());
    assert!((photo[1] - unringed[1] - 5.0).abs() < 0.6, "the line keeps the ring's 5 points above the photo");
    assert!((photo[2] - unringed[2]).abs() < 0.5 && (photo[3] - unringed[3]).abs() < 0.5, "at the same size: {photo:?} {unringed:?}");
    assert!((field(&rt)[3] - field(&plain)[3] - 5.0).abs() < 0.6, "so the field grows by the ring's reach");
    let (hairline, width) = ring(&rt, HAIRLINE).expect("the hairline draws");
    assert!(hairline.iter().zip(photo).all(|(a, b)| (a - b).abs() < 0.5) && width == 1.0, "a 1 point hairline on the photo's edge: {hairline:?}");
    assert!(ring(&rt, RING).is_none(), "no ring until selected");
    rt.update(move |s, _| s.edit.set_selection(Selection::bytes(at, end)));
    settle(&mut rt);
    let (r, width) = ring(&rt, RING).expect("the ring draws");
    let want = [photo[0] - 5.0, photo[1] - 5.0, photo[2] + 10.0, photo[3] + 10.0];
    assert!(r.iter().zip(want).all(|(a, b)| (a - b).abs() < 0.5) && width == 3.0, "3 points wide, 2 points out from the photo: {r:?} {want:?}");
    assert!(filled(&rt, TINT).is_empty(), "no tint under the ring");
    let image_at = rt.instances.iter().position(|i| i.params[1] == KIND_IMAGE).unwrap();
    let ring_at = rt.instances.iter().position(|i| i.border[0] > 0.9 && i.border[1] < 0.1).unwrap();
    assert!(ring_at > image_at, "the ring draws over the image");
}

#[test]
fn a_photo_still_loading_shows_its_icon_and_caption_centred_in_its_box() {
    let (rt, _, _) = runtime(500.0, false);
    assert!(image(&rt).is_none());
    let field = field(&rt);
    let muted = MUTED.premul_srgb(1.0);
    let ink: Vec<[f32; 4]> = rt.instances.iter().filter(|i| i.params[1] != KIND_RECT && i.fill.iter().zip(muted).all(|(a, b)| (a - b).abs() < 0.02)).map(|i| i.rect.map(|v| v / 2.0)).collect();
    assert!(ink.len() >= 10, "the icon and the caption's glyphs draw in the muted colour: {}", ink.len());
    let union = ink.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |u, r| [u[0].min(r[0]), u[1].min(r[1]), u[2].max(r[0] + r[2]), u[3].max(r[1] + r[3])]);
    let photo_box = [field[0], field[1] + LINE, field[2].min(500.0) - 1.0, (field[2].min(500.0) - 1.0) * 400.0 / 640.0];
    let (cx, cy) = ((union[0] + union[2]) * 0.5, (union[1] + union[3]) * 0.5);
    assert!((cx - (photo_box[0] + photo_box[2] * 0.5)).abs() < 3.0, "centred across the box: {union:?} in {photo_box:?}");
    assert!((cy - (photo_box[1] + photo_box[3] * 0.5)).abs() < 6.0, "and down it: {union:?} in {photo_box:?}");
    let icon = ink.iter().copied().min_by(|a, b| a[1].total_cmp(&b[1])).unwrap();
    assert!(ink.iter().filter(|r| r[1] > icon[1] + icon[3]).count() >= 9, "the caption sits under the icon");
}

#[test]
fn the_view_places_a_bar_at_the_photos_corner_from_its_pill_rect() {
    let (mut rt, _, _) = runtime(500.0, true);
    let at = |rt: &Runtime<S>, key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.rect);
    let (photo, bar) = (image(&rt).unwrap(), at(&rt, "bar").expect("the bar shows"));
    assert!((bar[0] + bar[2] - (photo[0] + photo[2] - 10.0)).abs() < 0.5 && (bar[1] - (photo[1] + 10.0)).abs() < 0.5, "10 points inside the corner: {bar:?} {photo:?}");
    rt.update(|s, _| s.width = 300.0);
    settle(&mut rt);
    let (photo, bar) = (image(&rt).unwrap(), at(&rt, "bar").unwrap());
    assert!((bar[0] + bar[2] - (photo[0] + photo[2] - 10.0)).abs() < 0.5, "and follows the photo when the field narrows: {bar:?} {photo:?}");
    rt.update(|s, _| { s.edit.set_selection(Selection::bytes(0, s.edit.text.len())); s.edit.insert(""); });
    settle(&mut rt);
    assert!(at(&rt, "bar").is_none(), "a field without the pill has no rect for it");
}

#[test]
fn assistive_technology_hears_one_object_for_the_photo_and_an_image_with_its_description() {
    let (rt, _, _) = runtime(500.0, true);
    let tree = rt.accessibility_tree();
    let node = |role: Role| tree.nodes.iter().filter(move |(_, n)| n.role() == role).map(|(_, n)| n);
    let field = &tree.nodes.iter().find(|(_, n)| n.author_id() == Some("note")).unwrap().1;
    assert_eq!(field.value(), Some(format!("{TASK}Pack\n\u{fffc}\nAfter").as_str()));
    let runs: String = node(Role::TextRun).map(|n| n.value().unwrap_or_default()).collect();
    assert_eq!(runs, format!("{TASK}Pack\n\u{fffc}\nAfter"), "the lines read the same");
    let photo_run = node(Role::TextRun).find(|n| n.value().is_some_and(|v| v.starts_with('\u{fffc}'))).unwrap();
    assert_eq!(photo_run.character_lengths()[0], 3, "the object is one character of three bytes");
    let images: Vec<_> = node(Role::Image).collect();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].label(), Some("Harbour at dusk"));
    let b = images[0].bounds().unwrap();
    let photo = image(&rt).unwrap();
    assert!(((b.x1 - b.x0) as f32 - photo[2]).abs() < 1.0 && ((b.y1 - b.y0) as f32 - photo[3]).abs() < 1.0, "the image's bounds are the photo's: {b:?} {photo:?}");
}

#[test]
fn assistive_technology_hears_a_photos_new_description_though_its_box_keeps_its_size() {
    let (mut rt, _, _) = runtime(500.0, false);
    rt.update(|s, _| s.said = "Downloading photo…");
    settle(&mut rt);
    let said = |update: &accesskit::TreeUpdate| update.nodes.iter().find(|(_, n)| n.role() == Role::Image).and_then(|(_, n)| n.label().map(str::to_owned));
    rt.a11y_activate("Note");
    let whole = rt.a11y_update().expect("the whole tree");
    assert_eq!(said(&whole).as_deref(), Some("Downloading photo…"));
    rt.state.photo.set(64, 40, gradient(64, 40));
    rt.update(|s, _| s.said = "Harbour at dusk");
    settle(&mut rt);
    let update = rt.a11y_update().expect("the photo's description changed");
    assert_eq!(said(&update).as_deref(), Some("Harbour at dusk"), "the photo's image node carries its new description");
}

#[test]
fn a_photo_that_finishes_decoding_wakes_the_window_and_draws_without_a_new_layout() {
    let (mut rt, _, _) = runtime(500.0, false);
    let wakes = Arc::new(AtomicUsize::new(0));
    let count = wakes.clone();
    rt.set_live_waker(Arc::new(move || {
        count.fetch_add(1, Ordering::Relaxed);
    }));
    settle(&mut rt);
    assert!(image(&rt).is_none(), "nothing to draw yet");
    let before = field(&rt);
    rt.state.photo.set(64, 40, gradient(64, 40));
    assert_eq!(wakes.load(Ordering::Relaxed), 1, "the decoded photo wakes the window");
    rt.frame(1.0 / 60.0);
    assert!(image(&rt).is_some(), "and its next frame draws it");
    assert_eq!(rt.stats.shaped, 0, "without shaping the text again");
    assert_eq!(field(&rt), before, "or moving anything");
}

#[test]
fn a_photo_that_ends_the_note_keeps_a_text_height_caret_after_it() {
    let (mut rt, _, end) = runtime(500.0, true);
    rt.update(move |s, _| {
        s.edit.set_selection(Selection::bytes(end, s.edit.text.len()));
        s.edit.insert("");
    });
    settle(&mut rt);
    assert!(rt.state.edit.text.ends_with(PHOTO));
    let (c, photo) = (caret(&rt), image(&rt).unwrap());
    assert!((c[3] - LINE).abs() < 1.0, "{c:?}");
    assert!((c[1] + c[3] - (photo[1] + photo[3])).abs() < LINE * 0.5, "at the photo's foot: {c:?} {photo:?}");
}

#[test]
fn copy_and_cut_of_a_selected_photo_carry_the_image_the_field_adds() {
    let (mut rt, at, end) = runtime(500.0, true);
    let cmd = |rt: &mut Runtime<S>, c| key(rt, Key::Command(c), Mods { cmd: true, ..Mods::default() });
    let copied = |rt: &Runtime<S>| rt.copy_request.as_ref().map(|w| (w.content().text().to_owned(), w.content().image().map(|i| i.size()), w.content().html().is_some()));
    rt.update(move |s, _| s.edit.set_selection(Selection::bytes(at - 2, at - 1)));
    settle(&mut rt);
    cmd(&mut rt, 'c');
    assert_eq!(copied(&rt), Some(("k".into(), None, true)), "text alone copies as before, with its HTML");
    rt.update(move |s, _| s.edit.set_selection(Selection::bytes(at, end)));
    settle(&mut rt);
    cmd(&mut rt, 'c');
    assert_eq!(copied(&rt), Some((PHOTO.into(), Some((64, 40)), true)), "the photo's reference, its HTML and the image");
    cmd(&mut rt, 'x');
    assert_eq!(copied(&rt), Some((PHOTO.into(), Some((64, 40)), true)), "a cut sees the photo before it goes");
    assert!(!rt.state.edit.text.contains(PHOTO), "and removes it");
}

#[test]
fn a_photo_stands_its_margins_away_from_the_lines_above_and_below() {
    let (plain, _, _) = runtime(500.0, true);
    let (mut rt, at, end) = runtime(500.0, true);
    rt.update(|s, _| {
        s.margin = [10.0, 12.0];
        s.ringed = true;
    });
    settle(&mut rt);
    let (photo, f) = (image(&rt).unwrap(), field(&rt));
    assert!((photo[1] - (f[1] + LINE) - 10.0).abs() < 0.6, "10 points under the line above, past the ring's 5: {photo:?} in {f:?}");
    assert!((photo[2] - image(&plain).unwrap()[2]).abs() < 0.5 && (photo[3] - image(&plain).unwrap()[3]).abs() < 0.5, "at the photo's own size");
    rt.update(move |s, _| s.edit.set_caret(Caret::at(end + 1)));
    settle(&mut rt);
    let after = caret(&rt);
    assert!((after[1] - (photo[1] + photo[3]) - 12.0).abs() < 0.6, "the line below starts 12 points under the photo: {after:?} {photo:?}");
    assert!((after[3] - LINE).abs() < 1.0, "and keeps its own height: {after:?}");
    assert!((f[3] - (3.0 * LINE - LINE + 10.0 + photo[3] + 12.0)).abs() < 0.6, "the field holds both lines, the photo and its margins: {f:?}");
    rt.update(move |s, _| s.edit.set_caret(Caret::at(at)));
    settle(&mut rt);
    let beside = caret(&rt);
    assert!((beside[3] - LINE).abs() < 1.0 && (beside[1] + beside[3] - (photo[1] + photo[3])).abs() < LINE * 0.5, "the caret beside the photo is text height at its foot: {beside:?} {photo:?}");
}
