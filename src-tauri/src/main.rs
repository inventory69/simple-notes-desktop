// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Linux/Wayland WebKit compatibility fixes
    // These must be set BEFORE Tauri/WebKit initializes
    #[cfg(target_os = "linux")]
    {
        // SAFETY: These env vars must be set before any threads are spawned.
        // This runs at the very start of main(), before Tauri initializes.
        unsafe {
            // Fix for GBM/DMABUF errors on Wayland
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            // Fix for compositing issues on Wayland
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
            // Disable WebKit network/renderer sandbox — required on some immutable distros
            // (Fedora Silverblue, NixOS) where the AppImage sandbox conflicts with the
            // container runtime and causes EGL_BAD_PARAMETER at startup.
            std::env::set_var("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS", "1");
            // Prevent GStreamer VA-API probe from touching EGL before WebKit is ready
            std::env::set_var("GST_VAAPI_ALL_DRIVERS", "1");
        }
    }

    simple_notes_desktop_lib::run()
}
