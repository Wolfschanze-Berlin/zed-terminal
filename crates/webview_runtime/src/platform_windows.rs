use std::num::NonZero;

use anyhow::{Context as _, Result};
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SWP_SHOWWINDOW,
    SW_HIDE, SW_SHOWNA, WS_CLIPCHILDREN, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::w;
use wry::WebViewBuilder;

use crate::ipc::{self, IpcReceiver};
use crate::{Webview, WebviewConfig, IPC_BRIDGE_SCRIPT};

pub struct WebviewHandle {
    inner: wry::WebView,
    /// The overlay popup HWND that hosts the webview. This is a separate
    /// top-level window positioned over the panel region, necessary because
    /// GPUI's DirectComposition visual is "topmost" and always covers child HWNDs.
    overlay_hwnd: HWND,
    parent_hwnd: HWND,
}

// wry's Windows WebView wraps WebView2 COM objects that are STA-bound.
// All creation and access happens on the GPUI foreground thread (which is an STA).
// The Send impl is required by the `Webview` trait bound so that `Box<dyn Webview>`
// can be stored in Entity fields; actual cross-thread use does not occur.
unsafe impl Send for WebviewHandle {}

/// Wrapper around an HWND that implements `HasWindowHandle`, needed by wry.
struct HwndWrapper(HWND);

impl HasWindowHandle for HwndWrapper {
    fn window_handle(
        &self,
    ) -> std::result::Result<WindowHandle<'_>, wry::raw_window_handle::HandleError> {
        let mut handle = Win32WindowHandle::new(
            NonZero::new(self.0 .0 as isize)
                .ok_or(wry::raw_window_handle::HandleError::Unavailable)?,
        );
        handle.hinstance = None;
        // SAFETY: The HWND is valid for the duration of this borrow.
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(handle)) })
    }
}

/// Create a popup overlay window owned by `parent_hwnd`. The overlay sits
/// above the DComp visual because it's a separate top-level window. It has
/// `WS_EX_NOACTIVATE` so clicking it doesn't steal focus from the main window,
/// and `WS_EX_TOOLWINDOW` so it doesn't appear in the taskbar.
fn create_overlay_hwnd(parent_hwnd: HWND) -> Result<HWND> {
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            w!("Static"),
            w!(""),
            WS_POPUP | WS_CLIPCHILDREN,
            0,
            0,
            1,
            1,
            Some(parent_hwnd),
            None,
            None,
            None,
        )
    }
    .context("Failed to create overlay HWND for webview")?;
    Ok(hwnd)
}

pub(crate) fn create(
    parent_hwnd: HWND,
    config: WebviewConfig,
) -> Result<(Box<dyn Webview>, IpcReceiver)> {
    let (ipc_sender, ipc_receiver) = ipc::create_channel();

    let allow_remote_urls = config.allow_remote_urls;
    let allowed_hosts = config.allowed_hosts.clone();

    // Create a popup overlay window that will sit above GPUI's DComp surface.
    let overlay_hwnd = create_overlay_hwnd(parent_hwnd)?;
    let overlay = HwndWrapper(overlay_hwnd);

    let mut builder = match config.content {
        crate::WebviewContent::Url(ref url) => WebViewBuilder::new().with_url(url),
        crate::WebviewContent::Html(ref html) => WebViewBuilder::new().with_html(html),
    };

    builder = builder
        .with_ipc_handler({
            let sender = ipc_sender.clone();
            move |request| {
                if let Err(err) = sender.send_blocking(request.body().to_string()) {
                    log::warn!("webview IPC channel closed: {err}");
                }
            }
        })
        .with_navigation_handler(move |url| {
            if url.starts_with("data:") || url.starts_with("about:") {
                return true;
            }
            if !allow_remote_urls {
                log::info!("webview navigation blocked (network not allowed): {url}");
                return false;
            }
            if allowed_hosts.is_empty() {
                return true;
            }
            let is_allowed = allowed_hosts.iter().any(|host| url.contains(host.as_str()));
            if !is_allowed {
                log::info!("webview navigation blocked (host not in allowlist): {url}");
            }
            is_allowed
        })
        .with_initialization_script(IPC_BRIDGE_SCRIPT);

    for script in &config.initialization_scripts {
        builder = builder.with_initialization_script(script);
    }

    // Build the webview as a child of the overlay popup (not the GPUI window).
    let webview = builder
        .build_as_child(&overlay)
        .context("Failed to create WebView2 instance")?;

    Ok((
        Box::new(WebviewHandle {
            inner: webview,
            overlay_hwnd,
            parent_hwnd,
        }),
        ipc_receiver,
    ))
}

impl Webview for WebviewHandle {
    fn set_bounds(&self, x: f32, y: f32, width: f32, height: f32) -> Result<()> {
        // Convert the panel-local logical coordinates to screen coordinates
        // by adding the parent window's screen position.
        let mut parent_rect = windows::Win32::Foundation::RECT::default();
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowRect(self.parent_hwnd, &mut parent_rect)?;
        };

        let dpi = unsafe {
            windows::Win32::UI::HiDpi::GetDpiForWindow(self.parent_hwnd)
        };
        let scale = dpi as f32 / 96.0;

        let screen_x = parent_rect.left + (x * scale) as i32;
        let screen_y = parent_rect.top + (y * scale) as i32;
        let pixel_width = (width * scale) as i32;
        let pixel_height = (height * scale) as i32;

        // Move the overlay popup to the correct screen position
        unsafe {
            SetWindowPos(
                self.overlay_hwnd,
                Some(HWND_TOP),
                screen_x,
                screen_y,
                pixel_width,
                pixel_height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )?;
        }

        // Resize the webview to fill the overlay
        self.inner
            .set_bounds(wry::Rect {
                position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                    pixel_width as u32,
                    pixel_height as u32,
                )),
            })
            .context("webview set_bounds failed")
    }

    fn set_visible(&self, visible: bool) -> Result<()> {
        unsafe {
            let _ = ShowWindow(self.overlay_hwnd, if visible { SW_SHOWNA } else { SW_HIDE });
        }
        self.inner
            .set_visible(visible)
            .context("webview set_visible failed")
    }

    fn evaluate_script(&self, script: &str) -> Result<()> {
        self.inner
            .evaluate_script(script)
            .context("webview evaluate_script failed")
    }
}
