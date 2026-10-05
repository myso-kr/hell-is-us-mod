//! A window's pixels shown through DirectComposition: a flip-model swap chain with premultiplied
//! alpha, the window's one visual. What `layered.rs` uses for its large windows (the big map, the
//! game view's layer) in place of UpdateLayeredWindow, which copies the whole bitmap through the
//! compositor on every call (4.6 ms a frame for the big map, measured). Here a frame is one upload
//! of the pixels into the swap chain's buffer and a present.
//!
//! The window is made with `WS_EX_NOREDIRECTIONBITMAP` (it has no bitmap of its own; the
//! composition is its content), kept layered and transparent so clicks go through to the game.

use windows::core::Interface;
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice3, IDCompositionDesktopDevice, IDCompositionTarget, IDCompositionVisual2,
    IDCompositionVisual3,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGIDevice, IDXGIFactory2, IDXGISwapChain1, DXGI_PRESENT, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
    DXGI_USAGE_RENDER_TARGET_OUTPUT,
};

pub struct Composed {
    context: ID3D11DeviceContext,
    swap: IDXGISwapChain1,
    device: IDCompositionDesktopDevice,
    visual: IDCompositionVisual2,
    // Kept alive with the window: dropping it takes the content off the screen.
    _target: IDCompositionTarget,
    _d3d: ID3D11Device,
    w: u32,
    opacity: u8,
}

impl Composed {
    /// The composition of the window `hwnd`, `w` × `h` pixels.
    pub fn new(hwnd: windows_sys::Win32::Foundation::HWND, w: i32, h: i32) -> windows::core::Result<Composed> {
        // SAFETY: COM calls on objects made here; `hwnd` is a live window of this thread.
        unsafe {
            let mut d3d = None;
            let mut context = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                Some(&mut context),
            )?;
            let (d3d, context): (ID3D11Device, ID3D11DeviceContext) =
                (d3d.ok_or(windows::core::Error::empty())?, context.ok_or(windows::core::Error::empty())?);
            let dxgi: IDXGIDevice = d3d.cast()?;
            let factory: IDXGIFactory2 = dxgi.GetAdapter()?.GetParent()?;
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: w as u32,
                Height: h as u32,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                ..Default::default()
            };
            let swap = factory.CreateSwapChainForComposition(&d3d, &desc, None)?;
            let device: IDCompositionDesktopDevice = DCompositionCreateDevice3(&dxgi)?;
            let target = device.CreateTargetForHwnd(HWND(hwnd), true)?;
            let visual = device.CreateVisual()?;
            visual.SetContent(&swap)?;
            target.SetRoot(&visual)?;
            device.Commit()?;
            Ok(Composed { context, swap, device, visual, _target: target, _d3d: d3d, w: w as u32, opacity: 255 })
        }
    }

    /// Show `px` (premultiplied 0xAARRGGBB, the window's size), the whole of it faded to `alpha`.
    pub fn present(&mut self, px: &[u32], alpha: u8) -> windows::core::Result<()> {
        // SAFETY: as above; `px` holds w × h pixels, rows `w * 4` bytes apart.
        unsafe {
            let buffer: ID3D11Texture2D = self.swap.GetBuffer(0)?;
            self.context.UpdateSubresource(&buffer, 0, None, px.as_ptr().cast(), self.w * 4, 0);
            self.swap.Present(0, DXGI_PRESENT(0)).ok()?;
            if alpha != self.opacity {
                // Faded as a whole by the compositor, as UpdateLayeredWindow's constant alpha.
                let v: IDCompositionVisual3 = self.visual.cast()?;
                v.SetOpacity2(alpha as f32 / 255.0)?;
                self.device.Commit()?;
                self.opacity = alpha;
            }
        }
        Ok(())
    }
}
