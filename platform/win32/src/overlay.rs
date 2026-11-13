//! The overlay: a see-through, always-on-top window that shows outlined text, moves when dragged anywhere, and never
//! takes focus.
//!
//! It is a layered window: Direct2D draws into a 32-bit DIB with premultiplied alpha, and `UpdateLayeredWindow` hands
//! the bits to the compositor. Everything about that presentation stays in this file. ADR-0016 records why it is
//! layered and what would reopen the choice.

use std::cell::RefCell;
use std::ptr;
use std::rc::Rc;

use windows::Win32::Foundation::{
    COLORREF, E_INVALIDARG, E_UNEXPECTED, ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT,
    SIZE, WPARAM,
};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::{
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, ID2D1DCRenderTarget, ID2D1Factory,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, GetMonitorInfoW, HBITMAP, HDC, HGDIOBJ,
    HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint, MonitorFromWindow,
    SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_EXSTYLE, GWLP_USERDATA, GetCursorPos, GetWindowLongPtrW,
    GetWindowRect, HTCAPTION, HWND_TOPMOST, IsWindowVisible, MA_NOACTIVATE, PostQuitMessage, RegisterClassExW, SW_HIDE,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, ULW_ALPHA, USER_DEFAULT_SCREEN_DPI, UpdateLayeredWindow, WM_CLOSE, WM_DPICHANGED, WM_EXITSIZEMOVE,
    WM_MOUSEACTIVATE, WM_MOVING, WM_NCHITTEST, WM_NCRBUTTONDOWN, WM_NCRBUTTONUP, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use crate::Win32Error;
use crate::dpi::{dips_from_points, pixels_from_dips};
use crate::placement::{MonitorPlacement, monitors, named_monitor, resolve_origin};
use crate::snap::{Edges, Rect, keep_snapped, snap_while_moving};
use crate::text::{TextPainter, TextStyle};
use crate::ui::{EventSink, UiEvent};

const CLASS: PCWSTR = w!("OverlayMonitorOverlay");
/// Clear space around the outlined text, in pixels.
const MARGIN_PX: f32 = 4.0;
/// How near an edge a dragged overlay snaps to it, in pixels at 96 DPI.
const SNAP_DISTANCE_96_DPI: u64 = 12;

/// The overlay window. It belongs to the thread that created it, which must pump its messages.
pub struct OverlayWindow {
    window: HWND,
    /// Read by the window procedure through the window's user data, so it lives until the window is destroyed.
    shared: Rc<Shared>,
}

/// What the window procedure reaches: it may only ever take shared references, so the canvas sits in a `RefCell`.
struct Shared {
    sink: EventSink,
    canvas: RefCell<Canvas>,
}

/// Draws the text into a bitmap and makes the bitmap the window's content.
struct Canvas {
    window: HWND,
    text: TextPainter,
    target: ID2D1DCRenderTarget,
    /// Recreated whenever the window's size changes.
    surface: Option<Surface>,
    /// What was drawn last, to draw again at a new DPI.
    last: Option<(Vec<String>, TextStyle)>,
    /// The monitor edges the window is flush with, which a resize keeps it flush with.
    edges: Edges,
}

impl OverlayWindow {
    /// Creates the overlay, hidden, at the top-left of the primary monitor's work area. A right-click on it is sent
    /// to `sink`.
    ///
    /// # Errors
    /// The window class, the window, or its Direct2D render target could not be created.
    pub fn create(sink: EventSink) -> Result<Self, Win32Error> {
        let instance = register_class()?;
        let text = TextPainter::new()?;
        let target = dc_render_target(text.factory())?;
        let origin = primary_work_area_origin();
        // SAFETY: the class is registered; there is no parent, menu, or creation data.
        let window = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                CLASS,
                w!("Overlay Monitor"),
                WS_POPUP,
                origin.x,
                origin.y,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            )
        }?;
        let canvas = Canvas {
            window,
            text,
            target,
            surface: None,
            last: None,
            edges: Edges::default(),
        };
        let shared = Rc::new(Shared {
            sink,
            canvas: RefCell::new(canvas),
        });
        // SAFETY: the window is ours. The pointer stays valid while the window exists: `Drop` destroys the window
        // before `shared` is freed. Until it is stored, the procedure finds 0 and leaves the state alone.
        unsafe {
            SetWindowLongPtrW(
                window,
                GWLP_USERDATA,
                Rc::as_ptr(&shared).expose_provenance().cast_signed(),
            )
        };
        Ok(Self { window, shared })
    }

    /// Draws `lines` in `style` at the DPI of the window's monitor, and sizes the window to them: the text plus
    /// `2 * outline_px + 4` px on every side. It also puts the window back on top of other topmost windows, without
    /// moving, showing, or activating it.
    ///
    /// # Errors
    /// `DWRITE_E_NOFONT` when the font family is not installed; otherwise a drawing or presenting failure.
    pub fn draw(&mut self, lines: &[String], style: &TextStyle) -> Result<(), Win32Error> {
        let dpi = window_dpi(self.window);
        let Ok(mut canvas) = self.shared.canvas.try_borrow_mut() else {
            // Only a redraw for a DPI change holds it, and that never calls back here.
            return Err(E_UNEXPECTED.into());
        };
        canvas.last = Some((lines.to_vec(), style.clone()));
        canvas.render(lines, style, dpi)
    }

    pub fn show(&self) {
        // SAFETY: the window is alive. The result says whether it was visible before, which does not matter here.
        unsafe { ShowWindow(self.window, SW_SHOWNOACTIVATE) }.as_bool();
    }

    pub fn hide(&self) {
        // SAFETY: as in `show`.
        unsafe { ShowWindow(self.window, SW_HIDE) }.as_bool();
    }

    /// Makes mouse clicks pass through the window to what is behind it (`on`), or lets it catch them again.
    pub fn set_click_through(&self, on: bool) {
        // SAFETY: reading the window's extended style has no preconditions.
        let style = unsafe { GetWindowLongPtrW(self.window, GWL_EXSTYLE) }.cast_unsigned();
        let flag = WS_EX_TRANSPARENT.0 as usize;
        let style = if on { style | flag } else { style & !flag };
        // SAFETY: the window is alive and ours; only the transparent bit changes. The style takes effect at once.
        unsafe { SetWindowLongPtrW(self.window, GWL_EXSTYLE, style.cast_signed()) };
    }

    /// Puts the window back above other topmost windows, without moving, resizing, showing, or activating it.
    pub fn keep_on_top(&self) {
        // SAFETY: the window is alive. A failure leaves it where it was, and the next frame asks again.
        unsafe {
            SetWindowPos(
                self.window,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            )
        }
        .ok();
    }

    /// The monitor the window is on and its offset from that monitor's top-left (full bounds, so it matches the snap),
    /// or `None` when Windows cannot say.
    pub fn placement(&self) -> Option<MonitorPlacement> {
        let mut rect = RECT::default();
        // SAFETY: the window is alive and `rect` is a live local.
        unsafe { GetWindowRect(self.window, &raw mut rect) }.ok()?;
        // SAFETY: a handle of a live window; the nearest monitor is always found.
        let monitor = named_monitor(unsafe { MonitorFromWindow(self.window, MONITOR_DEFAULTTONEAREST) })?;
        Some(MonitorPlacement {
            monitor: monitor.name,
            x: rect.left.saturating_sub(monitor.bounds.left),
            y: rect.top.saturating_sub(monitor.bounds.top),
        })
    }

    /// Moves the window to `saved`, or to the top-left of the primary work area when nothing is saved or its monitor
    /// is gone. The offset is held inside the monitor, and the next draw keeps the whole window inside it too. It
    /// keeps its size, visibility, and focus.
    pub fn move_to(&self, saved: Option<&MonitorPlacement>) {
        let fallback = primary_work_area_origin();
        let (x, y) = resolve_origin(saved, &monitors(), (fallback.x, fallback.y));
        // SAFETY: the window is alive. A failure leaves it where it was, which is still on a monitor.
        unsafe {
            SetWindowPos(
                self.window,
                None,
                x,
                y,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
            )
        }
        .ok();
        // A place that was chosen, not dragged, is flush with nothing in particular.
        if let Ok(mut canvas) = self.shared.canvas.try_borrow_mut() {
            canvas.edges = Edges::default();
        }
    }

    /// The DPI of the monitor the window is on now, which changes when it is dragged to another one.
    pub fn dpi(&self) -> u32 {
        window_dpi(self.window)
    }

    pub fn is_visible(&self) -> bool {
        // SAFETY: the window is alive.
        unsafe { IsWindowVisible(self.window) }.as_bool()
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        // SAFETY: the window was created on this thread, which owns it, and is destroyed once, here, before `shared`
        // is freed. A failed destroy leaves nothing to undo: the window goes when its thread ends.
        unsafe { DestroyWindow(self.window) }.ok();
    }
}

impl Canvas {
    fn render(&mut self, lines: &[String], style: &TextStyle, dpi: u32) -> Result<(), Win32Error> {
        let em_px = pixels_from_dips(dips_from_points(style.size_pt), dpi);
        let pad = padding(style.outline_px);
        let text = self.text.outline(lines, style, em_px, (pad, pad))?;
        let (width, height) = window_size(text.width, text.height, style.outline_px);
        let surface = sized_surface(&mut self.surface, width, height)?;
        surface.fill(premultiplied_bgra(style.background_rgb, style.background_alpha));

        let area = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        // SAFETY: the DC holds the surface's bitmap, and `area` is a live local that covers it.
        unsafe { self.target.BindDC(surface.dc, &raw const area) }?;
        // SAFETY: only this canvas draws to the target, on this thread, and the draw is ended below.
        unsafe { self.target.BeginDraw() };
        let painted = self.text.paint(&self.target, &text, style);
        // SAFETY: ends the draw begun above; no tags are wanted.
        let ended = unsafe { self.target.EndDraw(None, None) };
        painted.and(ended)?;
        present(self.window, surface, self.edges)
    }

    /// Draws the last lines again at `dpi`, after the window moved to a monitor with another scale.
    fn redraw(&mut self, dpi: u32) -> Result<(), Win32Error> {
        match self.last.clone() {
            Some((lines, style)) => self.render(&lines, &style, dpi),
            None => Ok(()),
        }
    }
}

/// Makes `surface` the bitmap's content and size. The window stays flush with the monitor edges it was snapped to,
/// inside its monitor, and goes back on top of windows that became topmost since, without being activated.
fn present(window: HWND, surface: &Surface, edges: Edges) -> Result<(), Win32Error> {
    let source = POINT::default();
    let blend = per_pixel_alpha();
    // SAFETY: the window and the surface's DC are alive, and every pointer is to a live value.
    unsafe {
        UpdateLayeredWindow(
            window,
            None,
            None,
            Some(&raw const surface.size),
            Some(surface.dc),
            Some(&raw const source),
            COLORREF(0),
            Some(&raw const blend),
            ULW_ALPHA,
        )
    }?;
    let SIZE { cx, cy } = surface.size;
    let mut current = RECT::default();
    // SAFETY: the window is alive and `current` is a live local.
    let placed = unsafe { GetWindowRect(window, &raw mut current) }
        .ok()
        .and_then(|()| {
            // SAFETY: a handle of a live window; the nearest monitor is always found.
            let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
            monitor_bounds(monitor)
        })
        .map(|bounds| keep_snapped((cx, cy), from_win32(current), bounds, edges));
    // SAFETY: the window is alive; it is not activated. Without a monitor to place it on it keeps its position.
    unsafe {
        match placed {
            Some(at) => SetWindowPos(window, Some(HWND_TOPMOST), at.left, at.top, cx, cy, SWP_NOACTIVATE),
            None => SetWindowPos(window, Some(HWND_TOPMOST), 0, 0, cx, cy, SWP_NOACTIVATE | SWP_NOMOVE),
        }
    }
}

/// Blends with each pixel's own alpha, which is premultiplied, at full strength.
fn per_pixel_alpha() -> BLENDFUNCTION {
    // The flags are bytes (0 and 1) that the bindings declare as u32.
    let byte = |flag: u32| u8::try_from(flag).unwrap_or_default();
    BLENDFUNCTION {
        BlendOp: byte(AC_SRC_OVER),
        BlendFlags: 0,
        SourceConstantAlpha: u8::MAX,
        AlphaFormat: byte(AC_SRC_ALPHA),
    }
}

/// A 32-bit top-down DIB selected into its own memory DC: what Direct2D draws into and the window shows.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    /// The DC's original bitmap, put back before the DC is deleted.
    original: HGDIOBJ,
    pixels: *mut u32,
    len: usize,
    size: SIZE,
}

impl Surface {
    fn new(width: i32, height: i32) -> Result<Self, Win32Error> {
        let len = pixel_count(width, height).ok_or_else(|| Win32Error::from(E_INVALIDARG))?;
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).unwrap_or(u32::MAX),
                biWidth: width,
                // Negative: the first row is the top one.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        let mut bits = ptr::null_mut();
        // SAFETY: `info` and `bits` are live locals; RGB colours need no DC, and no file mapping is passed.
        let bitmap = unsafe { CreateDIBSection(None, &raw const info, DIB_RGB_COLORS, &raw mut bits, None, 0) }?;
        // SAFETY: no reference DC asks for one compatible with the screen.
        let dc = unsafe { CreateCompatibleDC(None) };
        if dc.is_invalid() {
            let error = Win32Error::from_thread();
            // SAFETY: the bitmap was just created, is selected nowhere, and is deleted once.
            unsafe { DeleteObject(bitmap.into()) }.as_bool();
            return Err(error);
        }
        // SAFETY: both the DC and the bitmap were just created and are owned here.
        let original = unsafe { SelectObject(dc, bitmap.into()) };
        Ok(Self {
            dc,
            bitmap,
            original,
            pixels: bits.cast(),
            len,
            size: SIZE { cx: width, cy: height },
        })
    }

    /// Sets every pixel to `pixel`, given as the bytes of one DIB pixel.
    fn fill(&mut self, pixel: [u8; 4]) {
        // SAFETY: flushing GDI's batched calls has no preconditions; it must come before the bits are written.
        unsafe { GdiFlush() }.as_bool();
        // SAFETY: `pixels` points to the DIB's `len` pixels, which live as long as the bitmap this surface owns, and
        // nothing else reads or writes them during this call.
        let pixels = unsafe { std::slice::from_raw_parts_mut(self.pixels, self.len) };
        pixels.fill(u32::from_ne_bytes(pixel));
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: the DC and the bitmap are owned here and released once; the bitmap is deselected first, as GDI
        // cannot delete a selected bitmap.
        unsafe {
            SelectObject(self.dc, self.original);
            DeleteObject(self.bitmap.into()).as_bool();
            DeleteDC(self.dc).as_bool();
        }
    }
}

/// The surface, recreated only when the window's size changed.
fn sized_surface(surface: &mut Option<Surface>, width: i32, height: i32) -> Result<&mut Surface, Win32Error> {
    let wanted = SIZE { cx: width, cy: height };
    match surface.take() {
        Some(current) if current.size == wanted => Ok(surface.insert(current)),
        stale => {
            // The old bitmap goes first, so the two are never held at once.
            drop(stale);
            Ok(surface.insert(Surface::new(width, height)?))
        }
    }
}

fn pixel_count(width: i32, height: i32) -> Option<usize> {
    let width = usize::try_from(width).ok().filter(|&width| width > 0)?;
    let height = usize::try_from(height).ok().filter(|&height| height > 0)?;
    width.checked_mul(height)
}

/// A Direct2D target that draws into whatever DC it is bound to: on the CPU, so the overlay leaves the GPU to the
/// game, with premultiplied alpha, at 96 DPI because the text is already sized in the monitor's pixels.
fn dc_render_target(factory: &ID2D1Factory) -> Result<ID2D1DCRenderTarget, Win32Error> {
    let properties = D2D1_RENDER_TARGET_PROPERTIES {
        r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0,
        dpiY: 96.0,
        usage: D2D1_RENDER_TARGET_USAGE_NONE,
        minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
    };
    // SAFETY: `properties` is a live local.
    let target = unsafe { factory.CreateDCRenderTarget(&raw const properties) }?;
    // ClearType needs an opaque background; on a see-through window it fringes. The outlines are geometry, but any
    // text drawn as glyph runs must stay grayscale too.
    // SAFETY: the target was just created.
    unsafe { target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE) };
    Ok(target)
}

fn register_class() -> Result<HINSTANCE, Win32Error> {
    // SAFETY: no name asks for this executable's own module.
    let instance: HINSTANCE = unsafe { GetModuleHandleW(None) }?.into();
    let class = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
        lpfnWndProc: Some(window_procedure),
        hInstance: instance,
        lpszClassName: CLASS,
        ..WNDCLASSEXW::default()
    };
    // SAFETY: `class` is a live local, and its name is static.
    if unsafe { RegisterClassExW(&raw const class) } == 0 {
        let error = Win32Error::from_thread();
        // An overlay made earlier in this process registered it already.
        if error.code() != ERROR_CLASS_ALREADY_EXISTS.to_hresult() {
            return Err(error);
        }
    }
    Ok(instance)
}

/// The top-left of the primary monitor's work area: the screen's top-left, unless a taskbar sits there.
fn primary_work_area_origin() -> POINT {
    // SAFETY: plain values; the screen origin is always on the primary monitor.
    let monitor = unsafe { MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY) };
    let mut info = MONITORINFO {
        cbSize: u32::try_from(size_of::<MONITORINFO>()).unwrap_or(u32::MAX),
        ..MONITORINFO::default()
    };
    // SAFETY: `info` is a live local with its size set.
    if unsafe { GetMonitorInfoW(monitor, &raw mut info) }.as_bool() {
        POINT {
            x: info.rcWork.left,
            y: info.rcWork.top,
        }
    } else {
        POINT::default()
    }
}

fn window_dpi(window: HWND) -> u32 {
    // SAFETY: any handle is accepted; an invalid one gives 0.
    match unsafe { GetDpiForWindow(window) } {
        0 => USER_DEFAULT_SCREEN_DPI,
        dpi => dpi,
    }
}

/// The space from the window's edge to the text: the outline, then as much again and a margin.
fn padding(outline_px: f32) -> f32 {
    2.0 * outline_px + MARGIN_PX
}

/// The window's size in pixels: the text plus room for the outline and a margin on every side, rounded up.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a float-to-int `as` saturates, and a window side is far below i32::MAX"
)]
fn window_size(text_w: f32, text_h: f32, outline_px: f32) -> (i32, i32) {
    let both_sides = 2.0 * padding(outline_px);
    ((text_w + both_sides).ceil() as i32, (text_h + both_sides).ceil() as i32)
}

/// One pixel of `rgb` at `alpha`, as a 32-bit DIB stores it for `UpdateLayeredWindow`: blue, green, red, alpha, each
/// colour premultiplied by the alpha.
fn premultiplied_bgra(rgb: u32, alpha: u8) -> [u8; 4] {
    let [_, red, green, blue] = rgb.to_be_bytes();
    let premultiply = |channel: u8| {
        // At most 255 * 255 + 127, which fits a u16; divided by 255 it fits a u8 again.
        u8::try_from((u16::from(channel) * u16::from(alpha) + 127) / 255).unwrap_or(u8::MAX)
    };
    [premultiply(blue), premultiply(green), premultiply(red), alpha]
}

/// The screen point a mouse message carries: x in the low word, y in the high word, each signed, since a monitor left
/// of or above the primary one has negative coordinates.
pub(crate) fn screen_point(lparam: LPARAM) -> (i32, i32) {
    let word = |shift: u32| i32::from(u16::try_from((lparam.0 >> shift) & 0xFFFF).unwrap_or(0).cast_signed());
    (word(0), word(16))
}

/// The `LRESULT` of a message whose answer is one of Windows' small `u32` codes.
fn code(value: u32) -> LRESULT {
    LRESULT(isize::try_from(value).unwrap_or(0))
}

/// Answers the overlay's messages: the whole window is a caption, so dragging anywhere moves it; a click never
/// activates it; a drag snaps it to monitor edges; a right-click asks for its menu; a new DPI resizes and redraws it.
///
/// # Safety
/// Only Windows calls this, with the arguments of a message for a window of this class.
unsafe extern "system" fn window_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_NCHITTEST => code(HTCAPTION),
        WM_MOUSEACTIVATE => code(MA_NOACTIVATE),
        // Answered here, so the default handling of a right-click on a caption (the window menu) never starts and the
        // button-up still arrives.
        WM_NCRBUTTONDOWN => LRESULT(0),
        WM_CLOSE => {
            // Ends the loop in order instead of destroying the window under the process.
            // SAFETY: posts WM_QUIT to this thread, which owns the window.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        WM_MOVING => {
            let mut cursor = POINT::default();
            // SAFETY: `cursor` is a live local. A failure leaves the proposed move as it is.
            if unsafe { GetCursorPos(&raw mut cursor) }.is_err() {
                return LRESULT(0);
            }
            // SAFETY: for WM_MOVING, `lparam` points to the proposed rectangle, which the procedure may rewrite and
            // which nothing else reads during the message.
            let proposed = unsafe { ptr::with_exposed_provenance_mut::<RECT>(lparam.0.cast_unsigned()).as_mut() };
            // SAFETY: called from this class's window procedure.
            let handled =
                proposed.is_some_and(|proposed| on_moving(window, unsafe { shared(window) }, proposed, cursor));
            // TRUE tells Windows the rectangle was changed.
            LRESULT(isize::from(handled))
        }
        WM_EXITSIZEMOVE => {
            // SAFETY: called from this class's window procedure.
            if let Some(shared) = unsafe { shared(window) } {
                shared.sink.send(UiEvent::OverlayMoved);
            }
            LRESULT(0)
        }
        WM_NCRBUTTONUP => {
            // SAFETY: called from this class's window procedure.
            if let Some(shared) = unsafe { shared(window) } {
                let (x, y) = screen_point(lparam);
                shared.sink.send(UiEvent::OverlayMenu { x, y });
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            // SAFETY: for WM_DPICHANGED, `lparam` points to the suggested window rectangle at the new DPI.
            let suggested = unsafe { *ptr::with_exposed_provenance::<RECT>(lparam.0.cast_unsigned()) };
            // The low word of `wparam` is the new horizontal DPI; it is the same as the vertical one.
            let dpi = u32::try_from(wparam.0 & 0xFFFF).unwrap_or(USER_DEFAULT_SCREEN_DPI);
            // SAFETY: called from this class's window procedure.
            on_dpi_changed(window, unsafe { shared(window) }, &suggested, dpi);
            LRESULT(0)
        }
        // SAFETY: the arguments are the ones Windows passed.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// Moves the window to `suggested`, then redraws the last lines at `dpi`, which resizes it to fit them.
fn on_dpi_changed(window: HWND, shared: Option<&Shared>, suggested: &RECT, dpi: u32) {
    let (width, height) = (suggested.right - suggested.left, suggested.bottom - suggested.top);
    // SAFETY: the window is alive, as it is receiving a message.
    unsafe {
        SetWindowPos(
            window,
            None,
            suggested.left,
            suggested.top,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
    .ok();
    // A failed redraw keeps the old drawing until the view draws again; it does so at once, as it sees the DPI differ
    // from the one it last drew at. So does a canvas in use:
    // the message then came from inside a draw, whose own resize put the window on another monitor.
    if let Some(shared) = shared
        && let Ok(mut canvas) = shared.canvas.try_borrow_mut()
    {
        canvas.redraw(dpi).ok();
    }
}

/// The snap distance in pixels at `dpi`: 12 at 96 DPI.
fn snap_distance(dpi: u32) -> i32 {
    let scaled = u64::from(dpi) * SNAP_DISTANCE_96_DPI / u64::from(USER_DEFAULT_SCREEN_DPI);
    i32::try_from(scaled).unwrap_or(i32::MAX)
}

/// Snaps the rectangle being dragged to the edges of the monitor under `cursor`, and remembers which edges it is
/// flush with. Returns whether it rewrote `proposed`.
fn on_moving(window: HWND, shared: Option<&Shared>, proposed: &mut RECT, cursor: POINT) -> bool {
    let Some(shared) = shared else { return false };
    // A canvas in use means this message came from inside a draw; leave that move alone.
    let Ok(mut canvas) = shared.canvas.try_borrow_mut() else {
        return false;
    };
    // SAFETY: a plain value; the nearest monitor is always found.
    let monitor = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };
    let Some(bounds) = monitor_bounds(monitor) else {
        return false;
    };
    let (snapped, edges) = snap_while_moving(from_win32(*proposed), bounds, snap_distance(window_dpi(window)));
    *proposed = to_win32(snapped);
    canvas.edges = edges;
    true
}

/// The full bounds of `monitor`, taskbar included: in a game the taskbar is hidden.
fn monitor_bounds(monitor: HMONITOR) -> Option<Rect> {
    let mut info = MONITORINFO {
        cbSize: u32::try_from(size_of::<MONITORINFO>()).unwrap_or(u32::MAX),
        ..MONITORINFO::default()
    };
    // SAFETY: `info` is a live local with its size set.
    unsafe { GetMonitorInfoW(monitor, &raw mut info) }
        .as_bool()
        .then(|| from_win32(info.rcMonitor))
}

fn from_win32(rect: RECT) -> Rect {
    Rect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

fn to_win32(rect: Rect) -> RECT {
    RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

/// The state `create` stored in the window's user data, or `None` while it is not stored yet.
///
/// # Safety
/// Call it only from this class's window procedure: then the state outlives the message being handled.
unsafe fn shared<'a>(window: HWND) -> Option<&'a Shared> {
    // SAFETY: reading a window's user data has no preconditions.
    let data = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) };
    let state = ptr::with_exposed_provenance::<Shared>(data.cast_unsigned());
    // SAFETY: non-zero data is the `Rc<Shared>` pointer `create` stored, which `OverlayWindow` keeps alive until it
    // has destroyed the window; only shared references are ever made from it.
    unsafe { state.as_ref() }
}

#[cfg(test)]
mod tests;
