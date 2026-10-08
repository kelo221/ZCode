//! GPUI Window state persistence (bounds, position, maximized).
//! Stores to ~/.zcode/v2/gpui_window_state.json (own file, separate from desktop).

#![allow(dead_code)]

use gpui::{App, Bounds, Pixels, Point, Size, WindowBounds, point, px, size};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            width: 1280.0,
            height: 820.0,
            maximized: false,
        }
    }
}

impl WindowState {
    pub fn to_window_bounds(&self, cx: &App) -> WindowBounds {
        let w = self.width.max(400.0);
        let h = self.height.max(300.0);
        let window_size: Size<Pixels> = size(px(w), px(h));

        let bounds = if let (Some(x), Some(y)) = (self.x, self.y) {
            let origin: Point<Pixels> = point(px(x.max(0.0)), px(y.max(0.0)));
            Bounds::new(origin, window_size)
        } else {
            Bounds::centered(None, window_size, cx)
        };

        if self.maximized {
            WindowBounds::Maximized(bounds)
        } else {
            WindowBounds::Windowed(bounds)
        }
    }

    pub fn from_bounds(bounds: Bounds<Pixels>, maximized: bool) -> Self {
        Self {
            x: Some(bounds.origin.x.into()),
            y: Some(bounds.origin.y.into()),
            width: bounds.size.width.into(),
            height: bounds.size.height.into(),
            maximized,
        }
    }
}

pub fn window_state_path() -> PathBuf {
    crate::shared::settings::resolve_user_home_dir()
        .join(".zcode")
        .join("v2")
        .join("gpui_window_state.json")
}

pub fn load_window_state() -> WindowState {
    load_window_state_from_path(&window_state_path())
}

pub fn load_window_state_from_path(path: &Path) -> WindowState {
    let Ok(content) = std::fs::read_to_string(path) else {
        return WindowState::default();
    };
    serde_json::from_str(&content).unwrap_or_default()
}

pub fn save_window_state(state: &WindowState) -> Result<(), std::io::Error> {
    save_window_state_to_path(state, &window_state_path())
}

pub fn save_window_state_to_path(state: &WindowState, path: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json_text = serde_json::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp_path, json_text)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_state_roundtrip() {
        let temp_dir = std::env::temp_dir().join(format!("zcode_wnd_test_{}", std::process::id()));
        let file = temp_dir.join("gpui_window_state.json");

        let state = WindowState {
            x: Some(150.0),
            y: Some(200.0),
            width: 1440.0,
            height: 900.0,
            maximized: true,
        };

        save_window_state_to_path(&state, &file).unwrap();
        let loaded = load_window_state_from_path(&file);
        assert_eq!(loaded, state);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_window_state_default_fallback() {
        let nonexistent = Path::new("nonexistent_window_state_12345.json");
        let def = load_window_state_from_path(nonexistent);
        assert_eq!(def.width, 1280.0);
        assert_eq!(def.height, 820.0);
        assert!(!def.maximized);
    }
}
