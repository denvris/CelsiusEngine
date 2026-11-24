use std::error::Error;

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::{
            Direct3D11::{ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D},
            Dxgi::{Common::*, *},
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
    core::{Interface, PCWSTR, w},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const CLASS: PCWSTR = w!("CelsiusEngineDesktopMirror");

unsafe extern "system" fn mirror_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

struct MirrorWindow {
    hwnd: HWND,
    module: HINSTANCE,
}

impl MirrorWindow {
    fn new(width: u32, height: u32) -> Result<Self> {
        let module = HINSTANCE(unsafe { GetModuleHandleW(None)? }.0);
        let class = WNDCLASSW {
            lpfnWndProc: Some(mirror_window_proc),
            hInstance: module,
            lpszClassName: CLASS,
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
            ..Default::default()
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            return Err(windows::core::Error::from_win32().into());
        }
        let mut window = Self {
            hwnd: HWND::default(),
            module,
        };
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut work_area = RECT::default();
        unsafe {
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(std::ptr::from_mut(&mut work_area).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )?;
        }
        let available_width = (work_area.right - work_area.left - 40).max(320);
        let available_height = (work_area.bottom - work_area.top - 80).max(240);
        let scale = (f64::from(available_width) / f64::from(width))
            .min(f64::from(available_height) / f64::from(height))
            .min(1.0);
        let mut rect = RECT {
            right: (f64::from(width) * scale).round() as i32,
            bottom: (f64::from(height) * scale).round() as i32,
            ..Default::default()
        };
        unsafe {
            AdjustWindowRect(&mut rect, style, false)?;
            window.hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                CLASS,
                w!("Celsius Engine - VR Gameplay"),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                rect.right - rect.left,
                rect.bottom - rect.top,
                HWND::default(),
                HMENU::default(),
                module,
                None,
            )?;
        }
        Ok(window)
    }

    fn is_open(&self) -> bool {
        unsafe { IsWindow(self.hwnd).as_bool() }
    }
}

impl Drop for MirrorWindow {
    fn drop(&mut self) {
        unsafe {
            if self.is_open() {
                let _ = DestroyWindow(self.hwnd);
            }
            let _ = UnregisterClassW(CLASS, self.module);
        }
    }
}

/// A desktop copy of the already-rendered left eye. It owns no VR session resources.
pub(crate) struct DesktopMirror {
    back_buffer: ID3D11Texture2D,
    chain: IDXGISwapChain1,
    context: ID3D11DeviceContext,
    window: MirrorWindow,
    pending: bool,
}

impl DesktopMirror {
    pub fn new(
        device: &ID3D11Device,
        width: u32,
        height: u32,
        eye_format: DXGI_FORMAT,
    ) -> Result<Self> {
        let format = match eye_format {
            DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => DXGI_FORMAT_R8G8B8A8_UNORM,
            DXGI_FORMAT_B8G8R8A8_UNORM_SRGB => DXGI_FORMAT_B8G8R8A8_UNORM,
            _ => {
                return Err(
                    format!("Unsupported desktop mirror eye format: {}", eye_format.0).into(),
                );
            }
        };
        let window = MirrorWindow::new(width, height)?;
        let dxgi: IDXGIDevice = device.cast()?;
        let adapter = unsafe { dxgi.GetAdapter()? };
        let factory: IDXGIFactory2 = unsafe { adapter.GetParent()? };
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            ..Default::default()
        };
        let chain = unsafe {
            factory.CreateSwapChainForHwnd(
                device,
                window.hwnd,
                &desc,
                None,
                None::<&IDXGIOutput>,
            )?
        };
        let back_buffer = unsafe { chain.GetBuffer(0)? };
        let context = unsafe { device.GetImmediateContext()? };
        Ok(Self {
            back_buffer,
            chain,
            context,
            window,
            pending: false,
        })
    }

    pub fn show(&self) {
        unsafe {
            let _ = ShowWindow(self.window.hwnd, SW_SHOWNOACTIVATE);
        }
    }

    pub fn pump(&self) -> bool {
        let mut message = MSG::default();
        unsafe {
            while self.window.is_open()
                && PeekMessageW(&mut message, self.window.hwnd, 0, 0, PM_REMOVE).as_bool()
            {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        self.window.is_open()
    }

    pub fn copy_eye(&mut self, source: &ID3D11Texture2D) {
        if self.window.is_open() && !unsafe { IsIconic(self.window.hwnd).as_bool() } {
            unsafe {
                self.context.CopyResource(&self.back_buffer, source);
                self.context.Flush();
            }
            self.pending = true;
        }
    }

    pub fn present(&mut self) -> Result<()> {
        if std::mem::take(&mut self.pending) {
            let result = unsafe { self.chain.Present(0, DXGI_PRESENT_DO_NOT_WAIT) };
            if result != DXGI_ERROR_WAS_STILL_DRAWING {
                result.ok()?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Direct3D11::*;

    #[test]
    fn mirror_copies_srgb_eye_bytes_to_desktop_buffer() -> Result<()> {
        let device = crate::graphics::d3d11::create_device(None)?;
        let mut mirror = DesktopMirror::new(&device, 32, 32, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB)?;
        let pixel = [32_u8, 125, 214, 255];
        let pixels = [pixel; 32 * 32];
        let source_desc = D3D11_TEXTURE2D_DESC {
            Width: 32,
            Height: 32,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_TYPELESS,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            ..Default::default()
        };
        let source_data = D3D11_SUBRESOURCE_DATA {
            pSysMem: pixels.as_ptr().cast(),
            SysMemPitch: 32 * 4,
            ..Default::default()
        };
        let readback_desc = D3D11_TEXTURE2D_DESC {
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..source_desc
        };
        let mut source = None;
        let mut readback = None;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        let copied;
        unsafe {
            device.CreateTexture2D(&source_desc, Some(&source_data), Some(&mut source))?;
            device.CreateTexture2D(&readback_desc, None, Some(&mut readback))?;
            let readback = readback.unwrap();
            mirror.copy_eye(&source.unwrap());
            mirror.context.CopyResource(&readback, &mirror.back_buffer);
            mirror
                .context
                .Map(&readback, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            copied = mapped.pData.cast::<[u8; 4]>().read_unaligned();
            mirror.context.Unmap(&readback, 0);
        }
        assert_eq!(copied, pixel);
        mirror.present()?;
        Ok(())
    }
}
