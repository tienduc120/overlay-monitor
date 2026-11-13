//! A window that presents frames through Direct3D 11, so other crates' tests can watch them arrive.

use windows::Win32::Foundation::{E_POINTER, HMODULE, HWND, LPARAM, LRESULT, S_OK, WPARAM};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIFactory2,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassExW, SW_SHOWNOACTIVATE, ShowWindow, WNDCLASSEXW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use crate::Win32Error;

const CLASS: PCWSTR = w!("OverlayMonitorTestFrames");
/// Small enough to go unnoticed at the corner of the screen for the moment it is up.
const SIZE: i32 = 64;

/// Shows a small window at the top-left corner, presents `frames` frames from it one refresh apart, and destroys
/// it. Returns how many presents the system accepted as shown.
///
/// # Errors
/// The window, the Direct3D device, or its swap chain could not be created.
pub fn present_frames(frames: u32) -> Result<u32, Win32Error> {
    // Declared first, so it is destroyed after the swap chain that draws into it.
    let window = Window::open()?;
    let device = hardware_device()?;
    // SAFETY: the call takes no arguments the caller could get wrong.
    let factory: IDXGIFactory2 = unsafe { CreateDXGIFactory1() }?;
    let description = DXGI_SWAP_CHAIN_DESC1 {
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 2,
        Scaling: DXGI_SCALING_STRETCH,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
        ..DXGI_SWAP_CHAIN_DESC1::default()
    };
    // SAFETY: the window is open on this thread, and `description` is a live local.
    let swap_chain = unsafe { factory.CreateSwapChainForHwnd(&device, window.0, &raw const description, None, None) }?;
    let mut presented = 0;
    for _ in 0..frames {
        // SAFETY: the swap chain and its window are alive; a sync interval of 1 waits for the next refresh.
        if unsafe { swap_chain.Present(1, DXGI_PRESENT(0)) } == S_OK {
            presented += 1;
        }
    }
    Ok(presented)
}

fn hardware_device() -> Result<ID3D11Device, Win32Error> {
    let mut device = None;
    // SAFETY: `device` is a live local the call fills; no adapter, software module, or feature levels are passed.
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            None,
            D3D11_SDK_VERSION,
            Some(&raw mut device),
            None,
            None,
        )
    }?;
    device.ok_or_else(|| E_POINTER.into())
}

/// A shown, inactive popup window, destroyed on drop.
struct Window(HWND);

impl Window {
    fn open() -> Result<Self, Win32Error> {
        // SAFETY: no name asks for this executable's own module.
        let instance = unsafe { GetModuleHandleW(None) }?;
        let class = WNDCLASSEXW {
            cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
            lpfnWndProc: Some(window_procedure),
            hInstance: instance.into(),
            lpszClassName: CLASS,
            ..WNDCLASSEXW::default()
        };
        // SAFETY: `class` is a live local and its name is static. Registering again fails because the class already
        // exists, which is what is needed, so the result is not checked; creating the window checks it instead.
        unsafe { RegisterClassExW(&raw const class) };
        // SAFETY: the class is registered; there is no parent, menu, or creation data.
        let window = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                CLASS,
                w!("overlay-monitor test frames"),
                WS_POPUP,
                0,
                0,
                SIZE,
                SIZE,
                None,
                None,
                Some(instance.into()),
                None,
            )
        }?;
        let opened = Self(window);
        // SAFETY: the window was just created on this thread. The result is whether it was visible before: it was not.
        unsafe { ShowWindow(opened.0, SW_SHOWNOACTIVATE) }.as_bool();
        Ok(opened)
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: the window was created on this thread and is destroyed once, here. A failed destroy leaves nothing to
        // undo: the window goes when the thread ends.
        unsafe { DestroyWindow(self.0) }.ok();
    }
}

/// Hands every message to the default handling: the window only exists to be presented to.
///
/// # Safety
/// Only Windows calls this, with the arguments of a message for a window of this class.
unsafe extern "system" fn window_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: the arguments are the ones Windows passed.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
