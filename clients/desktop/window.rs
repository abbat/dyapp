//! The main window's normal size, position and maximized state across launches
//! (v0.0.1-client-screens.md, Desktop window and tray). Minimized and fullscreen are never saved.

#![deny(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::cast_possible_truncation
)]

#[path = "window/geometry.rs"]
mod geometry;

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

/// A rectangle in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct Saved {
    pub normal: Rect,
    pub maximized: bool,
}

/// The last state seen by window events; written once, when the app exits.
#[derive(Default)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct WindowState(Mutex<Option<Saved>>);

const FILE: &str = "window.json";
/// `window` as saved when at least a quarter of it and part of its title bar are on a display;
/// otherwise centred on `primary`, shrunk to fit it.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn place(window: Rect, displays: &[Rect], primary: Rect) -> Rect {
    geometry::place(window, displays, primary)
}

fn rect(position: PhysicalPosition<i32>, size: PhysicalSize<u32>) -> Rect {
    Rect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    }
}

/// Puts the window where `config/window.json` says, then shows it. A missing or unreadable file
/// keeps the configured default: 1024×720, centred.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn restore<R: Runtime>(window: &WebviewWindow<R>, config: Option<&Path>) {
    let saved = config
        .and_then(|dir| fs::read(dir.join(FILE)).ok())
        .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok());
    let displays: Vec<Rect> = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|monitor| rect(*monitor.position(), *monitor.size()))
        .collect();
    let primary = window.primary_monitor().ok().flatten();
    if let (Some(saved), Some(primary)) = (saved, primary) {
        let at = place(
            saved.normal,
            &displays,
            rect(*primary.position(), *primary.size()),
        );
        let _ = window.set_size(PhysicalSize::new(at.width, at.height));
        let _ = window.set_position(PhysicalPosition::new(at.x, at.y));
        if saved.maximized {
            let _ = window.maximize();
        }
    }
    let _ = window.show();
}

/// Records the window after a move or resize; keeps the last normal bounds while maximized.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn track<R: Runtime>(window: &tauri::Window<R>, state: &WindowState) {
    if window.is_minimized().unwrap_or(true) || window.is_fullscreen().unwrap_or(true) {
        return;
    }
    let mut saved = state
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if window.is_maximized().unwrap_or(false) {
        if let Some(saved) = saved.as_mut() {
            saved.maximized = true;
        }
        return;
    }
    if let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) {
        *saved = Some(Saved {
            normal: rect(position, size),
            maximized: false,
        });
    }
}

/// Native window events retain geometry and apply the platform's close behavior.
pub(crate) fn event(window: &tauri::Window, event: &tauri::WindowEvent) {
    match event {
        tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
            track(window, &window.state::<WindowState>());
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            #[cfg(target_os = "macos")]
            {
                api.prevent_close();
                let _ = crate::macos::hide(window);
            }
            #[cfg(not(target_os = "macos"))]
            if window.state::<crate::tray::Tray>().hides_on_close() {
                api.prevent_close();
                let _ = window.hide();
            }
        }
        _ => {}
    }
    #[cfg(feature = "ui-smoke")]
    if matches!(
        event,
        tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)
    ) {
        // Geometry readiness follows track(), so a fast Quit cannot save stale bounds.
        crate::ui_smoke::window::geometry(window);
    } else {
        crate::ui_smoke::window::publish(window);
    }
}

/// Writes the last recorded state to `config/window.json`.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn save(state: &WindowState, config: &Path) {
    let saved = *state
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(json) = saved.and_then(|saved| serde_json::to_vec(&saved).ok()) {
        let _ = fs::create_dir_all(config);
        let _ = fs::write(config.join(FILE), json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };

    #[test]
    fn a_window_on_a_display_keeps_its_place_and_one_off_screen_is_centred() {
        let window = Rect {
            x: 100,
            y: 50,
            width: 1024,
            height: 720,
        };
        assert_eq!(place(window, &[SCREEN], SCREEN), window);
        // Three quarters off the right edge, title bar still visible: kept.
        let edge = Rect {
            x: 1920 - 256,
            ..window
        };
        assert_eq!(place(edge, &[SCREEN], SCREEN), edge);
        // A display that went away.
        let gone = Rect { x: 2500, ..window };
        assert_eq!(
            place(gone, &[SCREEN], SCREEN),
            Rect {
                x: 448,
                y: 180,
                ..window
            }
        );
        // Title bar above the display, the body below it.
        let above = Rect { y: -40, ..window };
        assert_eq!(
            place(above, &[SCREEN], SCREEN),
            Rect {
                x: 448,
                y: 180,
                ..window
            }
        );
        // Larger than the primary display: shrunk to fit.
        let big = Rect {
            x: -5000,
            y: 0,
            width: 3000,
            height: 2000,
        };
        assert_eq!(place(big, &[SCREEN], SCREEN), SCREEN);
        // Spread over two displays, a quarter on each.
        let second = Rect { x: 1920, ..SCREEN };
        let across = Rect {
            x: 1920 - 512,
            ..window
        };
        assert_eq!(place(across, &[SCREEN, second], SCREEN), across);
    }
}
