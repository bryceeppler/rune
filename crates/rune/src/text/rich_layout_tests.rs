use super::*;

const TOKEN: &str = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
const WORDS: &str = "a plain sentence that wraps at spaces across a few short lines";

fn lines(l: &Layout<Ink>) -> Vec<std::ops::Range<usize>> {
    l.lines().map(|line| line.text_range()).collect()
}

#[test]
fn wrap_anywhere_breaks_an_unbroken_word_inside_the_width() {
    let mut ts = TextSystem::default();
    let style = TextStyle::new(14.0, 400.0);
    let (scale, max_w) = (2.0, 160.0 * 2.0);
    let clipped = ts.layout_rich(TOKEN, &[], style, scale, Some(max_w));
    assert_eq!(clipped.len(), 1, "without wrap-anywhere the token stays on one line");
    assert!(clipped.full_width() > max_w);

    let spans = [Span::new(0, 6).bold()];
    let opts = RichLayoutOptions::new().wrap_anywhere();
    let wrapped = ts.layout_rich_with(TOKEN, &spans, style, scale, Some(max_w), opts);
    assert!(wrapped.len() >= 3, "the token wraps over several lines (got {})", wrapped.len());
    for line in wrapped.lines() {
        assert!(line.metrics().advance <= max_w + 0.5, "line {:?} fits {max_w}", line.text_range());
    }
    let r = lines(&wrapped);
    assert_eq!((r[0].start, r.last().unwrap().end), (0, TOKEN.len()));
    assert!(r.windows(2).all(|w| w[0].end == w[1].start), "lines cover the token without gaps");
}

#[test]
fn wrap_anywhere_keeps_word_breaks_and_defaults_match_layout_rich() {
    let mut ts = TextSystem::default();
    let style = TextStyle::new(14.0, 400.0);
    let plain = ts.layout_rich(WORDS, &[], style, 2.0, Some(240.0));
    let anywhere = ts.layout_rich_with(WORDS, &[], style, 2.0, Some(240.0), RichLayoutOptions::new().wrap_anywhere());
    assert!(plain.len() > 1);
    assert_eq!(lines(&plain), lines(&anywhere), "words that fit break at spaces either way");

    let default = ts.layout_rich_with(WORDS, &[], style, 2.0, Some(240.0), RichLayoutOptions::default());
    assert!(Rc::ptr_eq(&plain, &default), "default options share the layout_rich cache entry");
    let aligned = ts.layout_aligned(WORDS, &[], style, 2.0, Some(240.0), Alignment::Center);
    let with = ts.layout_rich_with(WORDS, &[], style, 2.0, Some(240.0), RichLayoutOptions::new().align(Alignment::Center));
    assert!(Rc::ptr_eq(&aligned, &with), "align matches layout_aligned");
}

#[test]
fn a_hanging_paragraph_wraps_under_the_text_after_its_marker() {
    use crate::edit::Block;
    let mut ts = TextSystem::default();
    let style = TextStyle::new(14.0, 400.0);
    let item = format!("- [ ] {WORDS}");
    let text = format!("Title\n{item}");
    let marker = text.find("a plain").unwrap();
    let blocks = [Block::Paragraph, Block::Hanging(6)];
    let plain = ts.layout_blocks(&text, &[], &[Block::Paragraph; 2], &[], style, 2.0, Some(240.0), Alignment::Start);
    let marker_x = parley::Cluster::from_byte_index(&plain, marker).and_then(|c| c.visual_offset()).unwrap();
    let hung = ts.layout_blocks(&text, &[], &blocks, &[], style, 2.0, Some(240.0), Alignment::Start);
    let item_lines: Vec<_> = hung.lines().filter(|l| l.text_range().start > 5).collect();
    assert!(item_lines.len() > 2, "the item wraps");
    assert_eq!(item_lines[0].metrics().inline_min_coord, 0.0, "the marker starts the first line");
    for line in &item_lines[1..] {
        let m = line.metrics();
        assert_eq!(m.inline_min_coord, marker_x, "line {:?} starts under the text", line.text_range());
        assert!(m.inline_min_coord + m.advance - m.hanging_advance <= 240.5, "line {:?} fits", line.text_range());
    }

    let word = format!("- [ ] {}", TOKEN.repeat(2));
    let alone = ts.layout_blocks(&word, &[], &[Block::Hanging(6)], &[], style, 2.0, Some(240.0), Alignment::Start);
    assert_eq!(alone.get(0).unwrap().text_range(), 0..6, "a word wider than the line leaves the marker alone on the first line");
    let short = ts.layout_blocks("- [ ] x", &[], &[Block::Paragraph], &[], style, 2.0, Some(240.0), Alignment::Start);
    let x = parley::Cluster::from_byte_index(&short, 6).and_then(|c| c.visual_offset()).unwrap();
    let starts: Vec<f32> = alone.lines().skip(1).map(|l| l.metrics().inline_min_coord).collect();
    assert!(starts.iter().all(|&s| s == x), "the word starts under where text follows the marker, at {x}: {starts:?}");

    let rtl = "- [ ] שלום עולם ".repeat(8);
    let wide = format!("{}x", "-".repeat(80));
    for (text, marker) in [(rtl.as_str(), 6), (wide.as_str(), 81)] {
        let layout = ts.layout_blocks(text, &[], &[Block::Hanging(marker)], &[], style, 2.0, Some(240.0), Alignment::Start);
        assert!(layout.len() > 1, "{text:?} wraps");
        assert!(layout.lines().all(|l| l.metrics().inline_min_coord == 0.0), "{text:?} wraps at the margin");
    }
}

#[test]
fn a_hanging_task_line_measured_at_no_width_lays_out_every_character() {
    use crate::edit::Block;
    let mut ts = TextSystem::default();
    let text = "- [ ] Oat milk\n";
    let pills = [PillChip { start: 0, end: 6, icon: None, bg: None, fg: None }];
    let style = TextStyle::new(15.5, 400.0);
    for scale in [1.0, 2.0] {
        let layout = ts.layout_blocks_inner(text, &[], &[Block::Hanging(6)], &pills, style, scale, Some(0.0), Alignment::Start, Wrapping::Anywhere, false);
        let laid: usize = layout.lines().map(|line| line.text_range()).filter(|r| r.start <= r.end).map(|r| r.len()).sum();
        assert_eq!(laid, text.len(), "every character is on a line at scale {scale}");
    }
}
