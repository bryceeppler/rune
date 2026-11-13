// winit 0.30's view reports no selection and no text to an input method and ignores
// replacementRange, so its content view gets a subclass that answers for the focused field.
use std::cell::Cell;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{ClassType, msg_send, sel};
use objc2_foundation::{NSAttributedString, NSNotFound, NSPoint, NSRange, NSRect, NSSize, NSString};

use super::{Document, Registration, Replacement, Utf16Range, with_client};
use crate::platform::macos::content_view;

const PREFIX: &str = "RuneTextInput_";

thread_local! {
    static IN_KEY_DOWN: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn install(window: &winit::window::Window) -> Option<Registration> {
    let view = content_view(window)?;
    // SAFETY: main thread (windows are created there), on winit's live content view. Each method
    // added has its selector's exact signature.
    unsafe {
        let base = view.class();
        if base.name().contains(PREFIX) { return None; }
        let name = format!("{PREFIX}{}", base.name());
        let class = match ClassBuilder::new(&name, base) {
            Some(mut b) => {
                b.add_method(sel!(keyDown:), key_down as unsafe extern "C" fn(_, _, _));
                b.add_method(sel!(selectedRange), selected_range as unsafe extern "C" fn(_, _) -> _);
                b.add_method(sel!(markedRange), marked_range as unsafe extern "C" fn(_, _) -> _);
                b.add_method(sel!(attributedSubstringForProposedRange:actualRange:), substring as unsafe extern "C" fn(_, _, _, _) -> _);
                b.add_method(sel!(firstRectForCharacterRange:actualRange:), first_rect as unsafe extern "C" fn(_, _, _, _) -> _);
                b.add_method(sel!(setMarkedText:selectedRange:replacementRange:), set_marked_text as unsafe extern "C" fn(_, _, _, _, _));
                b.add_method(sel!(insertText:replacementRange:), insert_text as unsafe extern "C" fn(_, _, _, _));
                b.add_method(sel!(unmarkText), unmark_text as unsafe extern "C" fn(_, _));
                b.register()
            }
            None => AnyClass::get(&name)?,
        };
        // Only this instance's class changes, to a subclass without ivars.
        AnyObject::set_class(&view, class);
    }
    Some(Registration::new(key(&view)))
}

pub(crate) fn publish<S: 'static>(window: &winit::window::Window, rt: &crate::runtime::Runtime<S>) {
    let Some(view) = content_view(window) else { return };
    with_client(key(&view), |c| rt.sync_text_input(c));
}

pub(crate) fn take_replacement(window: &winit::window::Window) -> Option<Replacement> {
    let view = content_view(window)?;
    with_client(key(&view), |c| c.replacing.take()).flatten()
}

fn key(view: &AnyObject) -> usize {
    view as *const AnyObject as usize
}


fn winit_class(view: &AnyObject) -> &'static AnyClass {
    let mut class = view.class();
    while !class.name().starts_with(PREFIX) {
        class = class.superclass().expect("a Rune text input view");
    }
    class.superclass().expect("winit's view class")
}

fn units(r: NSRange) -> Option<Utf16Range> {
    (r.location != NSNotFound as usize).then(|| r.location..r.location.saturating_add(r.length))
}

fn ns(r: Utf16Range) -> NSRange {
    NSRange::new(r.start, r.end - r.start)
}

unsafe fn plain(string: &AnyObject) -> String {
    let attributed: bool = unsafe { msg_send![string, isKindOfClass: NSAttributedString::class()] };
    let text: *const NSString = if attributed { unsafe { msg_send![string, string] } } else { (string as *const AnyObject).cast() };
    unsafe { text.as_ref() }.map(|s| s.to_string()).unwrap_or_default()
}

unsafe extern "C" fn key_down(view: &AnyObject, _: Sel, event: &AnyObject) {
    let outer = IN_KEY_DOWN.replace(true);
    // SAFETY: winit's keyDown: with the event AppKit passed.
    unsafe { let _: () = msg_send![super(view, winit_class(view)), keyDown: event]; }
    IN_KEY_DOWN.set(outer);
}

unsafe extern "C" fn selected_range(view: &AnyObject, _: Sel) -> NSRange {
    match with_client(key(view), |c| c.document.as_ref().and_then(Document::selected_range)).flatten() {
        Some(r) => ns(r),
        // SAFETY: winit's selectedRange.
        None => unsafe { msg_send![super(view, winit_class(view)), selectedRange] },
    }
}

unsafe extern "C" fn marked_range(view: &AnyObject, _: Sel) -> NSRange {
    match with_client(key(view), |c| c.document.as_ref().map(Document::marked_range)).flatten() {
        Some(r) => r.map_or(NSRange::new(NSNotFound as usize, 0), ns),
        // SAFETY: winit's markedRange.
        None => unsafe { msg_send![super(view, winit_class(view)), markedRange] },
    }
}

unsafe extern "C" fn substring(view: &AnyObject, _: Sel, range: NSRange, actual: *mut NSRange) -> *mut NSAttributedString {
    let Some((text, covered)) = with_client(key(view), |c| c.document.as_ref().zip(units(range)).and_then(|(d, r)| d.substring(r))).flatten() else {
        return std::ptr::null_mut();
    };
    // SAFETY: AppKit passes null or a range to fill.
    if let Some(actual) = unsafe { actual.as_mut() } { *actual = ns(covered); }
    Retained::autorelease_return(NSAttributedString::from_nsstring(&NSString::from_str(&text)))
}

unsafe extern "C" fn first_rect(view: &AnyObject, _: Sel, range: NSRange, actual: *mut NSRange) -> NSRect {
    let measured = with_client(key(view), |c| {
        let (document, geometry) = (c.document.as_ref()?, c.geometry.as_ref()?);
        geometry.first_rect(document, units(range)?)
    }).flatten();
    let Some(([x, y, w, h], covered)) = measured else {
        // SAFETY: AppKit passes null or a range to fill.
        if let Some(actual) = unsafe { actual.as_mut() } { *actual = NSRange::new(range.location, 0); }
        // SAFETY: winit's answer, the caret area the runtime placed.
        return unsafe { msg_send![super(view, winit_class(view)), firstRectForCharacterRange: range, actualRange: std::ptr::null_mut::<NSRange>()] };
    };
    // SAFETY: AppKit passes null or a range to fill.
    if let Some(actual) = unsafe { actual.as_mut() } { *actual = ns(covered); }
    let local = NSRect::new(NSPoint::new(x as f64, y as f64), NSSize::new(w as f64, h as f64));
    // SAFETY: the view is winit's flipped content view, in its window; AppKit takes screen points.
    unsafe {
        let in_window: NSRect = msg_send![view, convertRect: local, toView: std::ptr::null::<AnyObject>()];
        let window: *mut AnyObject = msg_send![view, window];
        if window.is_null() { return in_window; }
        msg_send![window, convertRectToScreen: in_window]
    }
}

unsafe extern "C" fn set_marked_text(view: &AnyObject, _: Sel, string: &AnyObject, selected: NSRange, replacement: NSRange) {
    // SAFETY: AppKit's string argument.
    let text = unsafe { plain(string) };
    with_client(key(view), |c| {
        c.replacing = c.document.as_ref().zip(units(replacement)).and_then(|(d, r)| d.replacement(r));
        let selected = units(selected).unwrap_or(0..0);
        if let Some(d) = &mut c.document { d.mark(&text, selected, c.replacing.as_ref()); }
    });
    // SAFETY: winit's setMarkedText: with AppKit's arguments; it reports the composition.
    unsafe {
        let _: () = msg_send![super(view, winit_class(view)), setMarkedText: string, selectedRange: selected, replacementRange: replacement];
    }
}

unsafe extern "C" fn insert_text(view: &AnyObject, _: Sel, string: &AnyObject, replacement: NSRange) {
    let winit = winit_class(view);
    // SAFETY: AppKit's string argument; winit's hasMarkedText.
    let (text, marked) = unsafe { (plain(string), msg_send![super(view, winit), hasMarkedText]) };
    let marked: Bool = marked;
    let replacing = with_client(key(view), |c| {
        c.replacing = c.document.as_ref().zip(units(replacement)).and_then(|(d, r)| d.replacement(r));
        if marked.as_bool() || c.replacing.is_some() {
            if let Some(d) = &mut c.document { d.insert(&text, c.replacing.as_ref()); }
        }
        c.replacing.is_some()
    }).unwrap_or(false);
    let none = NSRange::new(NSNotFound as usize, 0);
    // SAFETY: winit's NSTextInputClient methods with AppKit's string. winit reports an insert
    // only as the end of a composition, so one at a range goes through a composition of its
    // text; after a key, winit's keyDown: returns to its ground state, else unmarkText does.
    unsafe {
        if marked.as_bool() || !replacing {
            let _: () = msg_send![super(view, winit), insertText: string, replacementRange: replacement];
            return;
        }
        let end = NSRange::new(text.encode_utf16().count(), 0);
        let _: () = msg_send![super(view, winit), setMarkedText: string, selectedRange: end, replacementRange: none];
        let _: () = msg_send![super(view, winit), insertText: string, replacementRange: none];
        if !IN_KEY_DOWN.get() { let _: () = msg_send![super(view, winit), unmarkText]; }
    }
}

unsafe extern "C" fn unmark_text(view: &AnyObject, _: Sel) {
    with_client(key(view), |c| if let Some(d) = &mut c.document { d.unmark() });
    // SAFETY: winit's unmarkText.
    unsafe { let _: () = msg_send![super(view, winit_class(view)), unmarkText]; }
}
