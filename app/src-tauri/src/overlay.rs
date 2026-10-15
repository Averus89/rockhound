//! The overlay: a small click-through window over the game showing the
//! current value. It is the same web app as the main window; the frontend
//! renders the compact view when its window label is [`LABEL`].

use tauri::{AppHandle, Manager, PhysicalPosition};

pub const LABEL: &str = "overlay";

/// Overlay top edge as a fraction of the monitor height: above the
/// crosshair, below the top HUD row.
const TOP_FRACTION: f64 = 0.08;

/// Top-centre placement of a window of `size` on a monitor.
fn position(monitor_pos: (i32, i32), monitor_size: (u32, u32), size: (u32, u32)) -> (i32, i32) {
    let x = monitor_pos.0 + (i64::from(monitor_size.0) - i64::from(size.0)) as i32 / 2;
    let y = monitor_pos.1 + (f64::from(monitor_size.1) * TOP_FRACTION) as i32;
    (x, y)
}

/// Show or hide the overlay.
pub fn set_visible(app: &AppHandle, visible: bool) -> Result<(), String> {
    let window = app
        .get_webview_window(LABEL)
        .ok_or("overlay window is missing")?;
    if !visible {
        return window.hide().map_err(|e| e.to_string());
    }
    // Shown before placing: until then Linux reports a zero size, and the
    // input region can only be cleared on a window that is on screen.
    window.show().map_err(|e| e.to_string())?;
    let monitor = window
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("no monitor found")?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let (x, y) = position(
        (monitor.position().x, monitor.position().y),
        (monitor.size().width, monitor.size().height),
        (size.width, size.height),
    );
    window
        .set_position(PhysicalPosition::new(x, y))
        .and_then(|()| window.set_ignore_cursor_events(true))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centred_near_the_top() {
        assert_eq!(position((0, 0), (5120, 1440), (340, 96)), (2390, 115));
        // Second monitor to the left of the primary one.
        assert_eq!(position((-1920, 0), (1920, 1080), (320, 100)), (-1120, 86));
    }
}
