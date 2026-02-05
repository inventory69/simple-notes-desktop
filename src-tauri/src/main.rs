// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Linux/Wayland WebKit compatibility fixes
    // These must be set BEFORE Tauri/WebKit initializes
    #[cfg(target_os = "linux")]
    {
        // Fix for GBM/DMABUF errors on Wayland
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        // Fix for compositing issues on Wayland
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }

    simple_notes_desktop_lib::run()
}
