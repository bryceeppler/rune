//! Free paint helpers for rich-text block decorations, field carets, glyphs and rects.
use super::*;
use crate::edit::Block;

/// A list item's marker: where its text starts (device x), on the right when it reads right to
/// left, and its baseline, block and number.
struct Marker {
    text_x: f32,
    rtl: bool,
    baseline: f32,
    block: Block,
    number: u32,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_block_decorations(
    instances: &mut Vec<Instance>,
    text_sys: &mut TextSystem,
    images: &mut crate::pixels::ImageAtlas,
    glyph_scratch: &mut Vec<crate::text::GlyphQuad>,
    spec: &TextSpec,
    layout: &TextLayout,
    origin: [f32; 2],
    (x0, x1): (f32, f32),
    s: f32,
    tk: f32,
    raster: f32,
    scale_motion: ScaleMotion,
    glide: [f32; 2],
    text_color: Color,
    o: f32,
    srgb: bool,
    clip: [f32; 4],
    fade: [f32; 4],
) {
    use crate::edit::MAX_LIST_LEVEL;
    let line_h_default = spec.style.line_px() * s * tk;
    let box_w = (x1 - x0).max(layout.full_width() * tk).max(40.0 * s * tk);
    let mut para_idx = 0usize;
    let mut first_line_of_para = true;
    let mut list_counts = [0u32; (MAX_LIST_LEVEL as usize) + 1];
    let mut code_run: Option<(f32, f32)> = None;
    let mut quote_run: Option<(f32, f32)> = None;
    let mut markers: Vec<Marker> = Vec::new();

    let flush_code = |instances: &mut Vec<Instance>, run: &mut Option<(f32, f32)>| {
        if let Some((top, bot)) = run.take() {
            let pad_y = if tk == 1.0 { (2.0 * s).round() } else { 2.0 * s * tk };
            instances.push(rect_inst(
                [x0 + glide[0], top - pad_y + glide[1], box_w, (bot - top) + 2.0 * pad_y],
                premul(spec.rich_style.code_block_bg, o, srgb),
                6.0 * s * tk,
                clip,
                fade,
            ));
        }
    };
    let flush_quote = |instances: &mut Vec<Instance>, run: &mut Option<(f32, f32)>| {
        if let Some((top, bot)) = run.take() {
            let bar_w = if tk == 1.0 { (3.0 * s).round().max(2.0) } else { (3.0 * s * tk).max(2.0 * tk) };
            let bar_x = if tk == 1.0 { (origin[0] + 2.0 * s).round() } else { origin[0] + 2.0 * s * tk } + glide[0];
            let inset_y = if tk == 1.0 { (2.0 * s).round() } else { 2.0 * s * tk };
            instances.push(rect_inst(
                [bar_x, top + inset_y + glide[1], bar_w, (bot - top - 2.0 * inset_y).max(4.0 * s * tk)],
                premul(spec.rich_style.quote_bar, o, srgb),
                bar_w * 0.5,
                clip,
                fade,
            ));
        }
    };

    let mut had_lines = false;
    for line in layout.lines() {
        had_lines = true;
        let m = line.metrics();
        let blk = spec.blocks.get(para_idx).copied().unwrap_or(Block::Paragraph);
        let is_soft = spec.soft_breaks.binary_search(&(para_idx as u32)).is_ok();
        let top = origin[1] + m.block_min_coord * tk;
        let bot = if m.block_max_coord > m.block_min_coord {
            origin[1] + m.block_max_coord * tk
        } else {
            top + line_h_default
        };
        let baseline = origin[1] + m.baseline * tk;
        let marker = |block: Block, number: u32| {
            let rtl = line.is_rtl();
            let text_x = if rtl { m.inline_max_coord * tk } else { (block.indent_pt(spec.style.size) * s).round() * tk };
            Marker { text_x: origin[0] + text_x, rtl, baseline, block, number }
        };

        if blk == Block::Code {
            flush_quote(instances, &mut quote_run);
            match &mut code_run {
                Some((_, b)) => *b = bot,
                None => code_run = Some((top, bot)),
            }
        } else if blk.quoted() {
            flush_code(instances, &mut code_run);
            match &mut quote_run {
                Some((_, b)) => *b = bot,
                None => quote_run = Some((top, bot)),
            }
        } else {
            flush_code(instances, &mut code_run);
            flush_quote(instances, &mut quote_run);
        }

        if first_line_of_para {
            match blk {
                Block::Bullet { level: lvl, .. } => {
                    let l = (lvl.min(MAX_LIST_LEVEL)) as usize;
                    for c in &mut list_counts[l + 1..] {
                        *c = 0;
                    }
                    list_counts[l] = 0;
                    if !is_soft {
                        markers.push(marker(blk, 0));
                    }
                }
                Block::Numbered(lvl) => {
                    let l = (lvl.min(MAX_LIST_LEVEL)) as usize;
                    for c in &mut list_counts[l + 1..] {
                        *c = 0;
                    }
                    if !is_soft {
                        list_counts[l] += 1;
                        markers.push(marker(blk, list_counts[l]));
                    }
                }
                Block::Paragraph | Block::Hanging { .. } | Block::Quote | Block::Code => {
                    if !is_soft {
                        list_counts = [0; (MAX_LIST_LEVEL as usize) + 1];
                    }
                }
            }
        }

        if line.break_reason() == parley::BreakReason::Explicit {
            para_idx += 1;
            first_line_of_para = true;
        } else {
            first_line_of_para = false;
        }
    }

    if !had_lines
        && let Some(&blk) = spec.blocks.first()
    {
        let top = origin[1];
        let bot = top + line_h_default;
        let baseline = top + spec.style.size * 0.8 * s * tk;
        if blk.quoted() {
            quote_run = Some((top, bot));
        }
        match blk {
            Block::Code => code_run = Some((top, bot)),
            Block::Bullet { .. } | Block::Numbered(_) => {
                markers.push(Marker {
                    text_x: origin[0] + (blk.indent_pt(spec.style.size) * s).round() * tk,
                    rtl: false,
                    baseline,
                    block: blk,
                    number: u32::from(matches!(blk, Block::Numbered(_))),
                });
            }
            Block::Paragraph | Block::Hanging { .. } | Block::Quote => {}
        }
    }

    flush_code(instances, &mut code_run);
    flush_quote(instances, &mut quote_run);

    if !markers.is_empty() {
        let marker_color = spec.rich_style.marker.unwrap_or_else(|| text_color.alpha(0.68));
        let mc = premul(marker_color, o, srgb);
        for Marker { text_x, rtl, baseline: baseline_y, block, number: num } in markers {
            match block {
                Block::Bullet { level: lvl, .. } => {
                    let d = if tk == 1.0 { (4.5 * s).round().max(3.0) } else { (4.5 * s * tk).max(3.0 * tk) };
                    let gap = (7.0 * s).round() * tk;
                    let cx = if rtl { text_x + gap } else { text_x - gap - d };
                    let cy = baseline_y - 0.34 * spec.style.size * s * tk - d * 0.5;
                    let (cx, cy) = if tk == 1.0 { (cx.round(), cy.round()) } else { (cx, cy) };
                    let radius = if lvl >= 2 { 1.0 * s * tk } else { d * 0.5 };
                    let c = if lvl == 1 { premul(marker_color.alpha(marker_color.a * 0.65), o, srgb) } else { mc };
                    instances.push(rect_inst([cx + glide[0], cy + glide[1], d, d], c, radius, clip, fade));
                }
                Block::Numbered(_) => {
                    // Right to left, the bidirectional algorithm draws `1.` as `.1`.
                    let label = if rtl { format!(".{num}") } else { format!("{num}.") };
                    let ml = text_sys.layout_rich(&label, &[], spec.style, s, None);
                    let mw = ml.full_width();
                    let m_base = ml.lines().next().map_or(spec.style.size * 0.8 * s, |l| l.metrics().baseline);
                    let gap = (5.0 * s).round() * tk;
                    let mx = if rtl { text_x + gap } else { text_x - gap - mw * tk };
                    let my = baseline_y - m_base * tk;
                    let m_origin = if tk == 1.0 { [mx.round(), my.round()] } else { [mx, my] };
                    let m_r_origin = if scale_motion == ScaleMotion::Still { m_origin } else { [0.0, 0.0] };
                    glyph_scratch.clear();
                    text_sys.set_glyph_inks(marker_color, &[]);
                    text_sys.set_glide(glide);
                    text_sys.glyphs_for_layout_scaled(&ml, m_r_origin, raster, false, glyph_scratch);
                    text_sys.set_glide([0.0; 2]);
                    for q in &*glyph_scratch {
                        instances.extend(
                            glyph_or_image(q, text_sys, images, mc, clip, fade)
                                .map(|g| zoom_glyph(g, m_origin, m_r_origin, tk / raster)),
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

/// Pixel caret and logical-point IME area use the same shaped cursor geometry.
pub(super) fn input_caret(n: &Node, r: [f32; 4], scale: f32, s: f32, gliding: bool, scrolls: &mut HashMap<String, f32>) -> Option<([f32; 4], [f32; 4], [f32; 2])> {
    let (spec, layout, input) = (n.text.as_ref()?, n.text_layout.as_ref()?, n.input.as_ref()?);
    let e = &input.edit;
    let ins = n.inset.map(|v| v * scale * s);
    let (x0, x1) = (r[0] + ins[0], r[2] - ins[2]);
    // The text scales with its box, so the caret does too.
    let mut x = text_x(spec, x0, x1, (layout.full_width() * scale).ceil());
    if input.single_line { x -= single_line_scroll(scrolls, &n.path, input, &spec.content, layout, s, (x1 - x0) / scale.max(1e-4)) * scale; }
    let bb = layout.caret_box(caret_index(e, &spec.content), e.caret_affinity(), 1.5 * s);
    let (cx, cy) = (x + bb.x0 as f32 * scale, r[1] + ins[1] + bb.y0 as f32 * scale);
    let h = (if bb.y1 > bb.y0 { (bb.y1 - bb.y0) as f32 } else { spec.style.line_px() * s }) * scale;
    let (caret_x, caret_w) = if (scale - 1.0).abs() <= 1e-5 {
        let gx = if gliding { pixel_snap::text_glide(r[0]) } else { 0.0 };
        ((cx - gx).round() + gx, (1.5 * s).round().max(1.0))
    } else {
        (cx, (1.5 * s * scale).max(1.0))
    };
    Some(([caret_x, cy + h * 0.1, caret_w, h * 0.8], [cx / s, cy / s, 2.0 * scale, h / s], [x, r[1] + ins[1]]))
}

/// Whether `looping`'s sampled value multiplies the element's opacity.
pub(super) fn fades(looping: Option<rune_motion::LoopSpec>) -> bool {
    matches!(looping.map(|l| l.kind), Some(rune_motion::LoopKind::Opacity { .. } | rune_motion::LoopKind::OpacityCycle(_) | rune_motion::LoopKind::Sequence(_)))
}

/// `rect` (x, y, w, h) magnified `k` times from `r_origin` to `origin`.
pub(super) fn scale_about(rect: [f32; 4], origin: [f32; 2], r_origin: [f32; 2], k: f32) -> [f32; 4] {
    [origin[0] + (rect[0] - r_origin[0]) * k, origin[1] + (rect[1] - r_origin[1]) * k, rect[2] * k, rect[3] * k]
}

/// A glyph instance drawn `k` times its rasterized size about `origin`. An alpha glyph keeps
/// its atlas footprint in `uv.zw`, so the shader samples it filtered.
pub(super) fn zoom_glyph(mut g: Instance, origin: [f32; 2], r_origin: [f32; 2], k: f32) -> Instance {
    if k == 1.0 && origin == r_origin { return g; }
    if g.params[1] == KIND_GLYPH && g.uv[2] == 0.0 { (g.uv[2], g.uv[3]) = (g.rect[2], g.rect[3]); }
    g.rect = [
        origin[0] + (g.rect[0] - r_origin[0]) * k,
        origin[1] + (g.rect[1] - r_origin[1]) * k,
        g.rect[2] * k,
        g.rect[3] * k,
    ];
    g
}

/// A text glyph's instance: alpha-atlas coverage tinted by `fill`, or a colour glyph's image
/// at `fill`'s alpha. None when the image atlas can't place it.
pub(super) fn glyph_or_image(q: &GlyphQuad, text: &TextSystem, images: &mut crate::pixels::ImageAtlas, fill: [f32; 4], clip: [f32; 4], fade: [f32; 4]) -> Option<Instance> {
    if q.color == 0 {
        return Some(glyph(q, fill, clip, fade));
    }
    let image = images.place(text.color_glyph(q.color), [q.rect[2], q.rect[3]])?;
    Some(Instance {
        rect: q.rect,
        radii: [0.0; 4],
        fill: [0.0, 0.0, 0.0, fill[3]],
        border: [0.0; 4],
        params: [image.page as f32, KIND_IMAGE, image.premultiplied as u8 as f32, 1.0],
        uv: image.rect.map(|v| v as f32),
        clip,
        fade: crate::render::pack_fade(fade, clip, clip),
    })
}

pub(super) fn rect_inst(rect: [f32; 4], fill: [f32; 4], radius: f32, clip: [f32; 4], fade: [f32; 4]) -> Instance {
    Instance {
        rect,
        radii: [radius; 4],
        fill,
        border: [0.0; 4],
        params: [0.0, KIND_RECT, 0.0, 1.0],
        uv: [0.0; 4],
        clip,
        fade: crate::render::pack_fade(fade, clip, clip),
    }
}
