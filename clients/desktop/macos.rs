//! Native macOS application chrome around the shared desktop interface.

use tauri::{menu::Menu, AppHandle, Manager};

pub(crate) fn setup(app: &AppHandle) -> tauri::Result<()> {
    // Tauri's native menu provides Edit selectors and the standard app-menu Cmd+Q.
    app.set_menu(Menu::default(app)?).map(drop)
}

pub(crate) fn hide(window: &tauri::Window) -> tauri::Result<()> {
    window.hide()?;
    let close_to_tray = window.state::<crate::tray::Tray>().hides_on_close();
    // With close-to-tray off, the Dock must remain a way to reopen the hidden window.
    window.app_handle().set_dock_visibility(!close_to_tray)
}

pub(crate) fn restore_dock(app: &AppHandle) {
    let _ = app.set_dock_visibility(true);
}
