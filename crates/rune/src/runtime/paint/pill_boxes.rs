//! A sized pill's box over its chip: its icon and caption until its image is ready, the image,
//! its border, then the selection's tint, or the pill's ring outside the box, while the
//! selection covers it.
use super::*;
use crate::text::{PillChip, PillMetrics, TextSystem};

/// What the selection over a text's pill boxes looks like.
pub(super) struct Covered {
    /// The selected bytes of the drawn text.
    pub range: (usize, usize),
    pub tint: Color,
}

/// Where a text's pill boxes draw, and what they draw into.
pub(super) struct BoxPaint<'a> {
    pub instances: &'a mut Vec<Instance>,
    pub images: &'a mut crate::pixels::ImageAtlas,
    pub text: &'a mut TextSystem,
    pub glyphs: &'a mut Vec<GlyphQuad>,
    /// Moves a rect from where the chips were placed to where it draws, `k` times its size.
    pub place: &'a dyn Fn([f32; 4]) -> [f32; 4],
    pub k: f32,
    pub raster: f32,
    pub s: f32,
    pub o: f32,
    pub srgb: bool,
    pub clip: [f32; 4],
    pub fade: [f32; 4],
}

impl BoxPaint<'_> {
    /// Pushes the boxes of `pills` at `rects` (by pill index), each image filling its box and
    /// cropped to it. `fg` is the colour of a pill that sets none, `text_size` the field's.
    pub(super) fn paint(&mut self, rects: &[(usize, [f32; 4])], pills: &[PillChip], covered: Option<&Covered>, fg: Color, text_size: f32) {
        for &(i, at) in rects {
            let Some(pill) = pills.get(i) else { continue };
            let rect = (self.place)(at);
            let radius = pill.radius(at, self.s) * self.k;
            if !pill.image_ready() {
                self.placeholder(pill, at, pill.style.fg.unwrap_or(fg), text_size);
            }
            if let Some(pixels) = &pill.style.image {
                self.image(pixels, rect, radius);
            }
            if let Some((color, width)) = pill.style.border {
                self.ring(rect, radius, color, width);
            }
            let Some(covered) = covered.filter(|c| pill.within(c.range.0, c.range.1)) else { continue };
            match pill.style.selection_ring {
                Some(ring) => {
                    let out = (ring.width + ring.gap) * self.s * self.k;
                    let rect = [rect[0] - out, rect[1] - out, rect[2] + 2.0 * out, rect[3] + 2.0 * out];
                    self.ring(rect, radius + out, ring.color, ring.width);
                }
                None => self.instances.push(rect_inst(rect, premul(covered.tint, self.o, self.srgb), radius, self.clip, self.fade)),
            }
        }
    }

    fn image(&mut self, pixels: &crate::Pixels, rect: [f32; 4], radius: f32) {
        let (_, crop) = crate::pixels::fit_in(Some(crate::SurfaceFit::Fill), Default::default(), rect, pixels.size(), 0.0);
        let span = [crop[2] - crop[0], crop[3] - crop[1]];
        let Some(image) = self.images.place(pixels, [rect[2] / span[0], rect[3] / span[1]]) else { return };
        let [x, y, iw, ih] = image.rect.map(|v| v as f32);
        self.instances.push(Instance {
            rect,
            radii: [radius; 4],
            fill: [0.0, 0.0, 0.0, self.o],
            border: [0.0; 4],
            params: [image.page as f32, KIND_IMAGE, image.premultiplied as u8 as f32, 1.0],
            uv: [x + (crop[0] * iw).round(), y + (crop[1] * ih).round(), span[0] * iw, span[1] * ih],
            clip: self.clip,
            fade: crate::render::pack_fade(self.fade, self.clip, self.clip),
        });
    }

    /// A ring `width` points wide just inside `rect`.
    fn ring(&mut self, rect: [f32; 4], radius: f32, color: Color, width: f32) {
        let mut ring = rect_inst(rect, [0.0; 4], radius, self.clip, self.fade);
        ring.border = premul(color, self.o, self.srgb);
        ring.params[0] = width * self.s * self.k;
        self.instances.push(ring);
    }

    /// The icon and the caption under it, centred together in the box at `at` (where the chips
    /// were placed). The icon takes 90% of the caption's line height, or the field's icon size
    /// with no caption.
    fn placeholder(&mut self, pill: &PillChip, at: [f32; 4], fg: Color, text_size: f32) {
        let s = self.s;
        let caption = pill.style.caption.as_ref().map(|(text, style)| (self.text.layout_rich(text, &[], *style, s, None), style.line_px() * s));
        let icon_px = caption.as_ref().map_or(PillMetrics::new(text_size).icon * s, |(_, line)| line * 0.9);
        let icon = pill.style.icon.map(|icon| (icon, icon_px.round().max(1.0)));
        let gap = match (&icon, &caption) {
            (Some(_), Some((_, line))) => (line * 0.5).round(),
            _ => 0.0,
        };
        let height = icon.map_or(0.0, |(_, px)| px) + gap + caption.as_ref().map_or(0.0, |(_, line)| *line);
        let (cx, mut y) = (at[0] + at[2] * 0.5, (at[1] + (at[3] - height) * 0.5).round());
        let fill = premul(fg, self.o, self.srgb);
        if let Some((icon, px)) = icon {
            let pos = (self.place)([(cx - px * 0.5).round(), y, px, px]);
            let px_r = (px * self.raster).round().max(1.0) as u32;
            if let Some(q) = self.text.icon_quad(icon, px_r, [pos[0], pos[1]]) {
                self.instances.push(icon_inst(&q, [pos[0], pos[1]], px * self.k, 0.0, fill, self.clip, self.fade));
            }
            y += px + gap;
        }
        if let Some((layout, _)) = caption {
            let pos = (self.place)([(cx - layout.full_width() * 0.5).round(), y, 0.0, 0.0]);
            let origin = [pos[0], pos[1]];
            let r_origin = if self.k == 1.0 && self.raster == 1.0 { origin } else { [0.0, 0.0] };
            self.glyphs.clear();
            self.text.set_glyph_inks(fg, &[]);
            self.text.glyphs_for_layout_scaled(&layout, r_origin, self.raster, false, self.glyphs);
            for q in self.glyphs.iter() {
                self.instances.extend(glyph_or_image(q, self.text, self.images, fill, self.clip, self.fade)
                    .map(|g| zoom_glyph(g, origin, r_origin, self.k / self.raster)));
            }
        }
    }
}
