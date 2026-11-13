//! Icons made from the owner's `.ico` file, for the tray, and the exe's own icon for its windows.

use std::ptr;

use crate::Win32Error;
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, DestroyIcon, HICON, ICON_BIG, ICON_SMALL, IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTCOLOR,
    LR_SHARED, LoadIconW, LoadImageW, SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON, SendMessageW, WM_SETICON,
};
use windows::core::PCWSTR;

/// The version number `CreateIconFromResourceEx` wants for an icon image.
const ICON_IMAGE_VERSION: u32 = 0x0003_0000;
const HEADER_LEN: usize = 6;
const ENTRY_LEN: usize = 16;
/// The `ICONDIR` type of an icon file; a cursor file is 2.
const TYPE_ICON: u16 = 1;

/// One `ICONDIRENTRY`, as far as the picker cares.
struct Entry<'a> {
    width: u32,
    image: &'a [u8],
}

fn le_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at.checked_add(2)?)?.try_into().ok()?))
}

fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

fn entry<'a>(ico: &'a [u8], raw: &[u8]) -> Option<Entry<'a>> {
    // A width byte of 0 means 256.
    let width = match *raw.first()? {
        0 => 256,
        w => u32::from(w),
    };
    let len = usize::try_from(le_u32(raw, 8)?).ok()?;
    let start = usize::try_from(le_u32(raw, 12)?).ok()?;
    let image = ico.get(start..start.checked_add(len)?)?;
    Some(Entry { width, image })
}

/// The image of the smallest entry at least `wanted_px` wide, or of the largest when none is. `None` for a malformed
/// file: a bad header, or any entry that points outside the file.
pub fn pick_ico_image(ico: &[u8], wanted_px: u32) -> Option<&[u8]> {
    if le_u16(ico, 0)? != 0 || le_u16(ico, 2)? != TYPE_ICON {
        return None;
    }
    let count = usize::from(le_u16(ico, 4)?);
    let table = ico.get(HEADER_LEN..HEADER_LEN.checked_add(count.checked_mul(ENTRY_LEN)?)?)?;
    let (raws, _) = table.as_chunks::<ENTRY_LEN>();
    let entries = raws.iter().map(|raw| entry(ico, raw)).collect::<Option<Vec<_>>>()?;
    let at_least = entries.iter().filter(|e| e.width >= wanted_px).min_by_key(|e| e.width);
    at_least
        .or_else(|| entries.iter().max_by_key(|e| e.width))
        .map(|e| e.image)
}

/// An icon handle; dropping it frees the icon unless it is a stock one the system owns.
pub struct Icon {
    handle: HICON,
    owned: bool,
}

impl Icon {
    /// Makes an icon of about `wanted_px` from the image the file holds closest to that size.
    ///
    /// # Errors
    /// The file is malformed or the system refused the image.
    pub fn from_ico(ico: &[u8], wanted_px: u32) -> Result<Self, Win32Error> {
        let image = pick_ico_image(ico, wanted_px).ok_or_else(|| Win32Error::from_hresult(E_INVALID_ICO))?;
        let size = i32::try_from(wanted_px).map_err(|error| Win32Error::new(E_INVALID_ICO, error.to_string()))?;
        // SAFETY: `image` is a live slice holding one icon image; the call copies what it needs.
        let handle = unsafe { CreateIconFromResourceEx(image, true, ICON_IMAGE_VERSION, size, size, LR_DEFAULTCOLOR) }?;
        Ok(Self { handle, owned: true })
    }

    /// The stock application icon, for when the owner's cannot be made.
    ///
    /// # Errors
    /// The system has no such icon.
    pub fn stock() -> Result<Self, Win32Error> {
        // SAFETY: no module asks for a stock icon, which the system owns and never frees; `owned` keeps `Drop` from
        // destroying it.
        let handle = unsafe { LoadIconW(None, IDI_APPLICATION) }?;
        Ok(Self { handle, owned: false })
    }

    pub(crate) fn handle(&self) -> HICON {
        self.handle
    }
}

impl Drop for Icon {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: the handle was made by `from_ico`, is destroyed once, and the tray icon that used it is gone
            // by contract. A failed destroy leaks one icon until the process ends.
            unsafe { DestroyIcon(self.handle) }.ok();
        }
    }
}

/// The id of the icon group the build embeds in the exe (`app.rc`), which Task Manager and the shell show too.
const EXE_ICON_ID: u16 = 1;

/// The big and the small icon of a window, shared with the system: a shared image is cached per size and never freed
/// by us, so these are plain handles that stay valid for the process's life.
#[derive(Clone, Copy, Debug)]
pub struct WindowIcons {
    big: HICON,
    small: HICON,
}

/// `(width, height)` of the big and the small window icon at `dpi`.
pub(crate) fn icon_sizes(dpi: u32) -> ((i32, i32), (i32, i32)) {
    // SAFETY: the call only reads system settings.
    let metric = |index| unsafe { GetSystemMetricsForDpi(index, dpi) };
    (
        (metric(SM_CXICON), metric(SM_CYICON)),
        (metric(SM_CXSMICON), metric(SM_CYSMICON)),
    )
}

fn load_shared(module: HMODULE, id: u16, (width, height): (i32, i32)) -> Option<HICON> {
    // `MAKEINTRESOURCE`: a resource id travels as a pointer value no larger than 16 bits.
    let name = PCWSTR(ptr::without_provenance(usize::from(id)));
    // SAFETY: `name` is an integer resource id, which the call accepts in place of a string; the module is loaded.
    // A shared image is owned by the system, which is why `LR_SHARED` and no destroy go together.
    let image = unsafe {
        LoadImageW(
            Some(module.into()),
            name,
            IMAGE_ICON,
            width,
            height,
            LR_DEFAULTCOLOR | LR_SHARED,
        )
    }
    .ok()?;
    Some(HICON(image.0))
}

impl WindowIcons {
    /// Loads icon group `id` of `module` at the two sizes `dpi` asks for; `None` when it has no such icon.
    pub fn from_resource(module: HMODULE, id: u16, dpi: u32) -> Option<Self> {
        let (big, small) = icon_sizes(dpi);
        Some(Self {
            big: load_shared(module, id, big)?,
            small: load_shared(module, id, small)?,
        })
    }

    /// The icon of this exe at `dpi`, or `None` when the exe has none (a test binary, say).
    pub fn of_this_exe(dpi: u32) -> Option<Self> {
        // SAFETY: no name asks for this executable's own module.
        let module = unsafe { GetModuleHandleW(None) }.ok()?;
        Self::from_resource(module, EXE_ICON_ID, dpi)
    }

    /// The exe's icon at the system's DPI, for a window class; absent, the system's default shows.
    pub(crate) fn of_this_exe_for_class() -> Self {
        // SAFETY: the call only reads a system setting.
        Self::of_this_exe(unsafe { GetDpiForSystem() }).unwrap_or_else(|| Self {
            big: HICON::default(),
            small: HICON::default(),
        })
    }

    pub(crate) fn big(&self) -> HICON {
        self.big
    }

    pub(crate) fn small(&self) -> HICON {
        self.small
    }

    /// Makes `window` show these icons in its title bar, the taskbar, and Alt+Tab.
    pub fn apply(&self, window: HWND) {
        for (kind, icon) in [(ICON_BIG, self.big), (ICON_SMALL, self.small)] {
            // SAFETY: the window is alive; the icon is a shared handle that outlives it. The answer is the previous
            // icon, which is not ours to free.
            unsafe {
                SendMessageW(
                    window,
                    WM_SETICON,
                    Some(WPARAM(kind as usize)),
                    Some(LPARAM(icon.0.expose_provenance().cast_signed())),
                )
            };
        }
    }
}

/// `HRESULT_FROM_WIN32(ERROR_INVALID_DATA)`: the file is not a usable icon file.
const E_INVALID_ICO: windows::core::HRESULT = windows::core::HRESULT(0x8007_000D_u32.cast_signed());

#[cfg(test)]
mod tests;
