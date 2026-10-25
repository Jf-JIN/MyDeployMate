//! Initial size and position of the main window, fitted to the work area of the monitor it
//! opens on so the taskbar never covers it.

use tauri::{Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

/// Preferred and minimum inner size in logical pixels; the minimum is relaxed on small screens.
const PREFERRED: (f64, f64) = (1440.0, 900.0);
const MINIMUM: (f64, f64) = (1280.0, 720.0);
/// Share of the work area the whole window (frame included) may take at most.
const MAX_SHARE: f64 = 0.92;

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Inner size, minimum inner size and outer position of the window, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub size: (u32, u32),
    pub min_size: (u32, u32),
    pub position: (i32, i32),
}

/// Fits the window into `work`. `frame` is what the title bar and borders add to the inner size.
pub fn fit(work: Rect, scale: f64, frame: (u32, u32)) -> Layout {
    let axis = |preferred: f64, minimum: f64, available: u32, frame: u32| {
        let max = ((f64::from(available) * MAX_SHARE) as u32)
            .saturating_sub(frame)
            .max(1);
        let size = ((preferred * scale) as u32).min(max);
        let min = ((minimum * scale) as u32).min(size);
        (size, min)
    };
    let (width, min_width) = axis(PREFERRED.0, MINIMUM.0, work.width, frame.0);
    let (height, min_height) = axis(PREFERRED.1, MINIMUM.1, work.height, frame.1);
    let center = |start: i32, available: u32, outer: u32| {
        start + ((i64::from(available) - i64::from(outer)) / 2).max(0) as i32
    };
    Layout {
        size: (width, height),
        min_size: (min_width, min_height),
        position: (
            center(work.x, work.width, width + frame.0),
            center(work.y, work.height, height + frame.1),
        ),
    }
}

/// Sizes the hidden main window for its monitor, then shows it.
pub fn place_main(app: &tauri::App) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = apply(&window);
    let _ = window.show();
}

fn apply(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return Ok(());
    };
    let area = monitor.work_area();
    let work = Rect {
        x: area.position.x,
        y: area.position.y,
        width: area.size.width,
        height: area.size.height,
    };
    let (outer, inner) = (window.outer_size()?, window.inner_size()?);
    let frame = (
        outer.width.saturating_sub(inner.width),
        outer.height.saturating_sub(inner.height),
    );
    let layout = fit(work, monitor.scale_factor(), frame);
    window.set_min_size(Some(PhysicalSize::new(
        layout.min_size.0,
        layout.min_size.1,
    )))?;
    window.set_size(PhysicalSize::new(layout.size.0, layout.size.1))?;
    window.set_position(PhysicalPosition::new(layout.position.0, layout.position.1))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_screen_keeps_preferred_size_centered() {
        let work = Rect {
            x: 0,
            y: 0,
            width: 2560,
            height: 1392,
        };
        let layout = fit(work, 1.0, (16, 39));
        assert_eq!(layout.size, (1440, 900));
        assert_eq!(layout.min_size, (1280, 720));
        assert_eq!(layout.position, ((2560 - 1456) / 2, (1392 - 939) / 2));
    }

    #[test]
    fn scaled_1080p_fits_above_taskbar() {
        // 1920x1080 at 125%, taskbar 60px physical.
        let work = Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1020,
        };
        let layout = fit(work, 1.25, (20, 49));
        let outer_height = layout.size.1 + 49;
        assert!(outer_height < 1020);
        assert_eq!(layout.size.0, 1766 - 20);
        assert!(layout.min_size.1 <= layout.size.1);
        assert!(layout.position.1 >= 0);
        assert!(layout.position.1 as u32 + outer_height <= 1020);
    }

    #[test]
    fn small_screen_relaxes_minimum() {
        let work = Rect {
            x: 0,
            y: 0,
            width: 1366,
            height: 728,
        };
        let layout = fit(work, 1.0, (16, 39));
        assert!(layout.min_size.0 <= layout.size.0 && layout.min_size.1 <= layout.size.1);
        assert!(layout.size.1 + 39 <= 728);
    }

    #[test]
    fn secondary_monitor_offset_is_kept() {
        let work = Rect {
            x: -1920,
            y: 200,
            width: 1920,
            height: 1040,
        };
        let layout = fit(work, 1.0, (16, 39));
        assert!(layout.position.0 >= -1920 && layout.position.0 < 0);
        assert!(layout.position.1 >= 200);
    }
}
