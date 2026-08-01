// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Works around a real bug hit on a real AMD/Mesa desktop (RX 7700 XT,
    // reported as a blank white window): WebKitGTK's DMA-BUF-based
    // hardware compositing path fails to negotiate an EGL display inside
    // the AppImage's bundled webkit2gtk, aborting with
    // "Could not create default EGL display: EGL_BAD_PARAMETER" and
    // silently rendering nothing. Must be set before WebKitGTK
    // initializes, so this has to happen here at the very top of main(),
    // before tauri::Builder ever touches the webview.
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        // Belt-and-suspenders: this app's UI is a plain form/table, not
        // anything that benefits from GPU compositing, so it's safe to
        // also force the fully non-accelerated path if DMA-BUF disabling
        // alone doesn't clear a given GPU/driver combo's EGL failure.
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }

    modsync_lib::run()
}
