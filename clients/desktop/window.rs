//! The main window's normal size, position and maximized state across launches
//! (v0.0.1-client-screens.md, Desktop window and tray). Minimized and fullscreen are never saved.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

/// A rectangle in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub normal: Rect,
    pub maximized: bool,
}

/// The last state seen by window events; written once, when the app exits.
#[derive(Default)]
pub struct WindowState(Mutex<Option<Saved>>);

const FILE: &str = "window.json";
/// The strip at the top of the window that must stay on a display so it can be dragged back.
const TITLE_BAR: u32 = 32;

fn overlap(a: Rect, b: Rect) -> u64 {
    let width = (a.x + a.width as i32).min(b.x + b.width as i32) - a.x.max(b.x);
    let height = (a.y + a.height as i32).min(b.y + b.height as i32) - a.y.max(b.y);
    if width <= 0 || height <= 0 {
        return 0;
    }
    width as u64 * height as u64
}

/// `window` as saved when at least a quarter of it and part of its title bar are on a display;
/// otherwise centred on `primary`, shrunk to fit it.
pub fn place(window: Rect, displays: &[Rect], primary: Rect) -> Rect {
    let visible: u64 = displays.iter().map(|display| overlap(window, *display)).sum();
    let title = Rect { height: TITLE_BAR.min(window.height), ..window };
    let area = window.width as u64 * window.height as u64;
    if visible * 4 >= area && displays.iter().any(|display| overlap(title, *display) > 0) {
        return window;
    }
    let (width, height) = (window.width.min(primary.width), window.height.min(primary.height));
    Rect {
        x: primary.x + ((primary.width - width) / 2) as i32,
        y: primary.y + ((primary.height - height) / 2) as i32,
        width,
        height,
    }
}

fn rect(position: PhysicalPosition<i32>, size: PhysicalSize<u32>) -> Rect {
    Rect { x: position.x, y: position.y, width: size.width, height: size.height }
}

/// Puts the window where `config/window.json` says, then shows it. A missing or unreadable file
/// keeps the configured default: 1024×720, centred.
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
        let at = place(saved.normal, &displays, rect(*primary.position(), *primary.size()));
        let _ = window.set_size(PhysicalSize::new(at.width, at.height));
        let _ = window.set_position(PhysicalPosition::new(at.x, at.y));
        if saved.maximized {
            let _ = window.maximize();
        }
    }
    let _ = window.show();
}

/// Records the window after a move or resize; keeps the last normal bounds while maximized.
pub fn track<R: Runtime>(window: &tauri::Window<R>, state: &WindowState) {
    if window.is_minimized().unwrap_or(true) || window.is_fullscreen().unwrap_or(true) {
        return;
    }
    let mut saved = state.0.lock().unwrap_or_else(|e| e.into_inner());
    if window.is_maximized().unwrap_or(false) {
        if let Some(saved) = saved.as_mut() {
            saved.maximized = true;
        }
        return;
    }
    if let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) {
        *saved = Some(Saved { normal: rect(position, size), maximized: false });
    }
}

/// Writes the last recorded state to `config/window.json`.
pub fn save(state: &WindowState, config: &Path) {
    let saved = *state.0.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(json) = saved.and_then(|saved| serde_json::to_vec(&saved).ok()) {
        let _ = fs::create_dir_all(config);
        let _ = fs::write(config.join(FILE), json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { x: 0, y: 0, width: 1920, height: 1080 };

    #[test]
    fn a_window_on_a_display_keeps_its_place_and_one_off_screen_is_centred() {
        let window = Rect { x: 100, y: 50, width: 1024, height: 720 };
        assert_eq!(place(window, &[SCREEN], SCREEN), window);
        // Three quarters off the right edge, title bar still visible: kept.
        let edge = Rect { x: 1920 - 256, ..window };
        assert_eq!(place(edge, &[SCREEN], SCREEN), edge);
        // A display that went away.
        let gone = Rect { x: 2500, ..window };
        assert_eq!(place(gone, &[SCREEN], SCREEN), Rect { x: 448, y: 180, ..window });
        // Title bar above the display, the body below it.
        let above = Rect { y: -40, ..window };
        assert_eq!(place(above, &[SCREEN], SCREEN), Rect { x: 448, y: 180, ..window });
        // Larger than the primary display: shrunk to fit.
        let big = Rect { x: -5000, y: 0, width: 3000, height: 2000 };
        assert_eq!(place(big, &[SCREEN], SCREEN), SCREEN);
        // Spread over two displays, a quarter on each.
        let second = Rect { x: 1920, ..SCREEN };
        let across = Rect { x: 1920 - 512, ..window };
        assert_eq!(place(across, &[SCREEN, second], SCREEN), across);
    }
}
