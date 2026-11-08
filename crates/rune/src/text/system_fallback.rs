//! The system's own font for a character no loaded font draws: fontconfig's on Linux and the
//! BSDs, Core Text's on Apple platforms, DirectWrite's on Windows. A character is asked about
//! once, after a shaping draws it as `.notdef`; every later shaping routes it to that font.

use std::ops::Range;
use std::rc::Rc;

use std::hash::{Hash, Hasher};

use parley::FontContext;
use parley::fontique::FamilyId;
use unicode_segmentation::GraphemeCursor;

use super::system_fonts::{LoadedFiles, load_files};
use super::hash::KeyHasher;
use super::{FxHashMap, FxHashSet, Ink, Layout};

/// Characters remembered at most; past it they are forgotten and asked about again.
const MAX_CHARS: usize = 4096;
/// System font files loaded for characters at most.
const MAX_FILES: usize = 64;
/// Runs remembered as drawn whole at most; past it they are forgotten and checked again.
const MAX_RUNS: usize = 16384;

#[derive(Default)]
pub(super) struct SystemFallback {
    /// The family each character asked about draws in, `None` when no system font has it or a
    /// loaded font already does.
    found: FxHashMap<char, Option<Rc<str>>>,
    /// Whether any character in `found` has a family, so text needs routing.
    any: bool,
    /// The families the system's answers loaded, which later characters try first.
    families: Vec<FamilyId>,
    files: usize,
    /// Runs whose font has every drawable character of their text, by `run_key`, so a shaping
    /// of the same runs skips them.
    covered: FxHashSet<u64>,
}

impl SystemFallback {
    /// The graphemes of `text` holding a character a system font draws, merged by family.
    pub(super) fn routes(&self, text: &str) -> Vec<(Range<usize>, Rc<str>)> {
        let mut out: Vec<(Range<usize>, Rc<str>)> = Vec::new();
        if !self.any || text.is_ascii() {
            return out;
        }
        for (i, ch) in text.char_indices().filter(|&(_, ch)| !ch.is_ascii()) {
            let Some(Some(family)) = self.found.get(&ch) else { continue };
            let grapheme = grapheme_around(text, i);
            match out.last_mut() {
                Some((range, last)) if range.end >= grapheme.start && last == family => range.end = range.end.max(grapheme.end),
                _ => out.push((grapheme, family.clone())),
            }
        }
        out
    }

    /// Asks the system for each character `layout` draws as `.notdef` that no loaded font has
    /// and that hasn't been asked about, loading the files it names. Whether one of them now
    /// has a family. A character a loaded font has drew a box only for the font its cluster
    /// took, which no other font for the character fixes.
    pub(super) fn resolve(&mut self, fcx: &mut FontContext, loaded: &mut LoadedFiles, layout: &Layout<Ink>, text: &str) -> bool {
        let mut missing: Vec<char> = Vec::new();
        for line in layout.lines() {
            for run in line.runs() {
                let font = run.font();
                let content = text.get(run.text_range()).unwrap_or_default();
                let key = run_key(font.data.id(), font.index, content);
                if self.covered.contains(&key) {
                    continue;
                }
                let Some(face) = swash::FontRef::from_index(font.data.as_ref(), font.index as usize) else { continue };
                if content.chars().all(|ch| !drawable(ch) || face.charmap().map(ch) != 0) {
                    if self.covered.len() >= MAX_RUNS {
                        self.covered.clear();
                    }
                    self.covered.insert(key);
                    continue;
                }
                for cluster in run.clusters().filter(|c| c.glyphs().any(|g| g.id == 0)) {
                    for ch in text.get(cluster.text_range()).unwrap_or_default().chars() {
                        if drawable(ch) && face.charmap().map(ch) == 0 && !self.found.contains_key(&ch) && !missing.contains(&ch) {
                            missing.push(ch);
                        }
                    }
                }
            }
        }
        let mut drawn = false;
        for ch in missing {
            if self.found.len() >= MAX_CHARS {
                self.found.clear();
                self.any = false;
            }
            let loaded_family = self.families.iter().copied().find(|&id| covers(fcx, id, ch));
            let family = loaded_family.or_else(|| {
                let families: Vec<FamilyId> = fcx.collection.family_ids().collect();
                if self.files >= MAX_FILES || families.into_iter().any(|id| covers(fcx, id, ch)) { return None; }
                let path = system_font(ch)?;
                self.files += !loaded.contains_key(&path) as usize;
                let id = load_files(&mut fcx.collection, loaded, [path]).into_iter().find(|&id| covers(fcx, id, ch))?;
                self.families.push(id);
                Some(id)
            });
            let name = family.and_then(|id| fcx.collection.family_name(id)).map(Rc::from);
            drawn |= name.is_some();
            self.found.insert(ch, name);
        }
        self.any |= drawn;
        drawn
    }
}

/// A run's font, face and text, which decide whether the font has every character in it.
fn run_key(blob: u64, index: u32, text: &str) -> u64 {
    let mut h = KeyHasher::default();
    (blob, index, text).hash(&mut h);
    h.finish()
}

/// The grapheme of `text` holding byte `at`.
fn grapheme_around(text: &str, at: usize) -> Range<usize> {
    let mut cursor = GraphemeCursor::new(at, text.len(), true);
    let start = if cursor.is_boundary(text, 0).unwrap_or(true) { at } else { cursor.prev_boundary(text, 0).ok().flatten().unwrap_or(at) };
    let mut cursor = GraphemeCursor::new(at, text.len(), true);
    let end = cursor.next_boundary(text, 0).ok().flatten().unwrap_or(text.len());
    start..end
}

/// Whether a face of family `id` maps `ch` to a glyph.
fn covers(fcx: &mut FontContext, id: FamilyId, ch: char) -> bool {
    let Some(family) = fcx.collection.family(id) else { return false };
    family.fonts().iter().any(|font| {
        font.load(Some(&mut fcx.source_cache))
            .and_then(|blob| swash::FontRef::from_index(blob.as_ref(), font.index() as usize).map(|f| f.charmap().map(ch) != 0))
            .unwrap_or(false)
    })
}

/// Whether `ch` shows a glyph of its own: not a control, a space, a default-ignorable format
/// character or a private-use code point, whose glyph only the app's own font knows.
fn drawable(ch: char) -> bool {
    !ch.is_control()
        && !ch.is_whitespace()
        && !matches!(ch as u32,
            0x00AD | 0x034F | 0x061C | 0x115F..=0x1160 | 0x17B4..=0x17B5 | 0x180B..=0x180F | 0x200B..=0x200F
            | 0x202A..=0x202E | 0x2060..=0x206F | 0x3164 | 0xE000..=0xF8FF | 0xFE00..=0xFE0F | 0xFEFF | 0xFFA0
            | 0xFFF0..=0xFFF8 | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A | 0xE0000..=0xE0FFF | 0xF0000..)
}

#[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
fn system_font(ch: char) -> Option<std::path::PathBuf> {
    let out = std::process::Command::new("fc-match").arg("--format=%{file}").arg(format!(":charset={:x}", ch as u32)).output().ok()?;
    let path = String::from_utf8(out.stdout).ok()?;
    (!path.is_empty()).then(|| path.into())
}

#[cfg(target_vendor = "apple")]
fn system_font(ch: char) -> Option<std::path::PathBuf> {
    use std::ptr::NonNull;

    use objc2_core_foundation::{CFRange, CFRetained, CFString, CFType, CFURL};

    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" {
        static kCTFontURLAttribute: &'static CFString;
        fn CTFontCreateWithName(name: &CFString, size: f64, matrix: *const std::ffi::c_void) -> Option<NonNull<CFType>>;
        fn CTFontCreateForString(current: &CFType, string: &CFString, range: CFRange) -> Option<NonNull<CFType>>;
        fn CTFontCopyAttribute(font: &CFType, attribute: &CFString) -> Option<NonNull<CFType>>;
    }
    let text = CFString::from_str(ch.encode_utf8(&mut [0; 4]));
    let range = CFRange { location: 0, length: ch.len_utf16() as isize };
    // SAFETY: each Create or Copy call returns a +1 reference, which `from_raw` takes over.
    unsafe {
        let base = CFRetained::from_raw(CTFontCreateWithName(&CFString::from_str(".AppleSystemUIFont"), 13.0, std::ptr::null())?);
        let font = CFRetained::from_raw(CTFontCreateForString(&base, &text, range)?);
        let url = CFRetained::from_raw(CTFontCopyAttribute(&font, kCTFontURLAttribute)?);
        url.downcast::<CFURL>().ok()?.to_file_path()
    }
}

#[cfg(windows)]
mod dwrite;
#[cfg(windows)]
use dwrite::system_font;

#[cfg(not(any(unix, windows)))]
fn system_font(_: char) -> Option<std::path::PathBuf> { None }

#[cfg(target_os = "android")]
fn system_font(_: char) -> Option<std::path::PathBuf> { None }
