//! DirectWrite's font for a character: the system fallback's answer for the person's locale
//! (`IDWriteFontFallback::MapCharacters`), else, before Windows 8.1, the first installed family
//! that has it.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows::Win32::Globalization::GetUserDefaultLocaleName;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_READING_DIRECTION, DWRITE_READING_DIRECTION_LEFT_TO_RIGHT, DWriteCreateFactory, IDWriteFactory, IDWriteFactory2,
    IDWriteFont, IDWriteFontCollection, IDWriteLocalFontFileLoader, IDWriteNumberSubstitution, IDWriteTextAnalysisSource,
    IDWriteTextAnalysisSource_Impl,
};
use windows::core::{Interface, OutRef, PCWSTR, Result, implement};

/// The longest locale name Windows gives, its terminating zero included (`LOCALE_NAME_MAX_LENGTH`).
const LOCALE_NAME_MAX: usize = 85;

pub(super) fn system_font(ch: char) -> Option<PathBuf> {
    // SAFETY: DirectWrite calls on the shared factory, with out-pointers to live locals.
    unsafe {
        let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
        let mut collection = None;
        factory.GetSystemFontCollection(&mut collection, false).ok()?;
        let collection = collection?;
        let font = match factory.cast::<IDWriteFactory2>() {
            Ok(factory) => mapped(&factory, &collection, ch)?,
            Err(_) => first_covering(&collection, ch)?,
        };
        file_path(&font).ok().flatten()
    }
}

/// The font the system fallback maps `ch` to, for the person's locale.
unsafe fn mapped(factory: &IDWriteFactory2, collection: &IDWriteFontCollection, ch: char) -> Option<IDWriteFont> {
    let mut units = [0u16; 2];
    let text = ch.encode_utf16(&mut units);
    let mut name = [0u16; LOCALE_NAME_MAX];
    // SAFETY: the buffer holds the longest locale name; the result counts its terminating zero.
    let written = unsafe { GetUserDefaultLocaleName(&mut name) };
    let locale = &name[..usize::try_from(written).unwrap_or(0).max(1)];
    let source: IDWriteTextAnalysisSource = Source { text, locale }.into();
    let (mut length, mut font, mut scale) = (0, None, 1.0);
    // SAFETY: `source` and the out-pointers outlive the call.
    unsafe {
        factory.GetSystemFontFallback().ok()?.MapCharacters(
            &source,
            0,
            text.len() as u32,
            collection,
            PCWSTR::null(),
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            &mut length,
            &mut font,
            &mut scale,
        ).ok()?;
    }
    font
}

/// The first family of `collection` whose regular face has `ch`.
unsafe fn first_covering(collection: &IDWriteFontCollection, ch: char) -> Option<IDWriteFont> {
    // SAFETY: plain queries on a live collection.
    unsafe {
        (0..collection.GetFontFamilyCount()).find_map(|i| {
            let family = collection.GetFontFamily(i).ok()?;
            let font = family.GetFirstMatchingFont(DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL).ok()?;
            font.HasCharacter(ch as u32).ok()?.as_bool().then_some(font)
        })
    }
}

/// The file `font` is in, when it is a local file.
unsafe fn file_path(font: &IDWriteFont) -> Result<Option<PathBuf>> {
    // SAFETY: DirectWrite fills the counts and buffers it is given, sized as it reports.
    unsafe {
        let face = font.CreateFontFace()?;
        let mut count = 0;
        face.GetFiles(&mut count, None)?;
        let mut files = vec![None; count as usize];
        face.GetFiles(&mut count, Some(files.as_mut_ptr()))?;
        let Some(Some(file)) = files.into_iter().next() else { return Ok(None) };
        let (mut key, mut size) = (std::ptr::null_mut(), 0);
        file.GetReferenceKey(&mut key, &mut size)?;
        let loader: IDWriteLocalFontFileLoader = file.GetLoader()?.cast()?;
        let len = loader.GetFilePathLengthFromKey(key, size)?;
        let mut wide = vec![0u16; len as usize + 1];
        loader.GetFilePathFromKey(key, size, &mut wide)?;
        Ok(Some(OsString::from_wide(&wide[..len as usize]).into()))
    }
}

/// DirectWrite reads a null pointer, never an empty slice's, as "no text here".
fn chunk(text: &[u16]) -> *mut u16 {
    if text.is_empty() { std::ptr::null_mut() } else { text.as_ptr().cast_mut() }
}

/// The one character `MapCharacters` analyses, and the locale it reads it in.
#[implement(IDWriteTextAnalysisSource)]
struct Source<'a> {
    text: &'a [u16],
    /// Zero-terminated.
    locale: &'a [u16],
}

impl IDWriteTextAnalysisSource_Impl for Source_Impl<'_> {
    fn GetTextAtPosition(&self, position: u32, text: *mut *mut u16, length: *mut u32) -> Result<()> {
        let rest = self.text.get(position as usize..).unwrap_or_default();
        // SAFETY: DirectWrite passes valid out-pointers and only reads the text.
        unsafe {
            *text = chunk(rest);
            *length = rest.len() as u32;
        }
        Ok(())
    }

    fn GetTextBeforePosition(&self, position: u32, text: *mut *mut u16, length: *mut u32) -> Result<()> {
        let before = self.text.get(..position as usize).unwrap_or_default();
        // SAFETY: as in `GetTextAtPosition`.
        unsafe {
            *text = chunk(before);
            *length = before.len() as u32;
        }
        Ok(())
    }

    fn GetParagraphReadingDirection(&self) -> DWRITE_READING_DIRECTION {
        DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
    }

    fn GetLocaleName(&self, position: u32, length: *mut u32, locale: *mut *mut u16) -> Result<()> {
        // SAFETY: as in `GetTextAtPosition`.
        unsafe {
            *length = (self.text.len() as u32).saturating_sub(position);
            *locale = self.locale.as_ptr().cast_mut();
        }
        Ok(())
    }

    fn GetNumberSubstitution(&self, position: u32, length: *mut u32, substitution: OutRef<'_, IDWriteNumberSubstitution>) -> Result<()> {
        substitution.write(None)?;
        // SAFETY: as in `GetTextAtPosition`.
        unsafe { *length = (self.text.len() as u32).saturating_sub(position) };
        Ok(())
    }
}
