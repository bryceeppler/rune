//! A window's title bar and taskbar button show the icon the window gives Windows, not the
//! icon in its exe, and winit gives none, so each window takes the exe's first icon group, the
//! one Explorer and the Start menu show.

use windows_sys::Win32::Foundation::{BOOL, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{EnumResourceNamesW, GetModuleHandleW};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, RT_GROUP_ICON, SM_CXICON, SM_CXSMICON};
use windows_sys::core::PCWSTR;
use winit::dpi::PhysicalSize;
use winit::platform::windows::{IconExtWindows, WindowAttributesExtWindows};
use winit::window::{Icon, WindowAttributes};

unsafe extern "system" fn first(_: HMODULE, _: PCWSTR, name: PCWSTR, found: isize) -> BOOL {
    if let Ok(ordinal) = u16::try_from(name as usize) {
        // SAFETY: `found` is the address of the `Option<u16>` that `group` keeps alive for the whole enumeration.
        unsafe { *(found as *mut Option<u16>) = Some(ordinal) };
    }
    0
}

fn group() -> Option<u16> {
    let mut found = None::<u16>;
    // SAFETY: `first` has the callback's signature and writes only `found`, which outlives the call.
    unsafe { EnumResourceNamesW(GetModuleHandleW(std::ptr::null()), RT_GROUP_ICON, Some(first), (&raw mut found) as isize) };
    found
}

pub(crate) fn with_exe_icon(attrs: WindowAttributes) -> WindowAttributes {
    let Some(group) = group() else { return attrs };
    // SAFETY: GetSystemMetrics takes no pointers and has no preconditions.
    let size = |metric| { let side = unsafe { GetSystemMetrics(metric) }.max(16) as u32; Some(PhysicalSize::new(side, side)) };
    attrs
        .with_window_icon(Icon::from_resource(group, size(SM_CXSMICON)).ok())
        .with_taskbar_icon(Icon::from_resource(group, size(SM_CXICON)).ok())
}
