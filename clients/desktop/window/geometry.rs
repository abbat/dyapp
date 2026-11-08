//! Window placement math, independent of Tauri and persistence.
use super::Rect;

const TITLE_BAR: u32 = 32;

fn axis_overlap(origin: i32, extent: u32, other_origin: i32, other_extent: u32) -> u64 {
    let start = i64::from(origin).max(i64::from(other_origin));
    let end = (i64::from(origin) + i64::from(extent))
        .min(i64::from(other_origin) + i64::from(other_extent));
    u64::try_from(end - start).unwrap_or(0)
}

fn overlap(a: Rect, b: Rect) -> u64 {
    axis_overlap(a.x, a.width, b.x, b.width) * axis_overlap(a.y, a.height, b.y, b.height)
}

fn center(origin: i32, outer: u32, inner: u32) -> i32 {
    let offset = outer.saturating_sub(inner) / 2;
    i32::try_from(i64::from(origin) + i64::from(offset)).unwrap_or(i32::MAX)
}

pub(super) fn place(window: Rect, displays: &[Rect], primary: Rect) -> Rect {
    let visible = displays
        .iter()
        .map(|display| overlap(window, *display))
        .fold(0, u64::saturating_add);
    let title = Rect {
        height: TITLE_BAR.min(window.height),
        ..window
    };
    let area = u64::from(window.width) * u64::from(window.height);
    if visible >= area.div_ceil(4) && displays.iter().any(|display| overlap(title, *display) > 0) {
        return window;
    }
    let (width, height) = (
        window.width.min(primary.width),
        window.height.min(primary.height),
    );
    Rect {
        x: center(primary.x, primary.width, width),
        y: center(primary.y, primary.height, height),
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_handles_signed_edges_and_full_unsigned_extents() {
        assert_eq!(axis_overlap(i32::MAX, 1, i32::MAX, 1), 1);
        assert_eq!(axis_overlap(i32::MIN, 1, i32::MAX, 1), 0);
        assert_eq!(axis_overlap(-10, 20, 0, 20), 10);
        assert_eq!(axis_overlap(5, 0, 5, u32::MAX), 0);
        let full = Rect {
            x: i32::MIN,
            y: i32::MIN,
            width: u32::MAX,
            height: u32::MAX,
        };
        assert_eq!(overlap(full, full), 18_446_744_065_119_617_025);
        assert_eq!(place(full, &[full, full], full), full);
    }

    #[test]
    fn centering_clamps_only_unrepresentable_positions() {
        assert_eq!(center(-1920, 1920, 800), -1360);
        assert_eq!(center(i32::MIN, u32::MAX, 0), -1);
        assert_eq!(center(i32::MAX, u32::MAX, 0), i32::MAX);
        assert_eq!(center(i32::MAX - 1, 4, 0), i32::MAX);
    }

    #[test]
    fn placement_preserves_quarter_visibility_and_title_bar_policy() {
        let window = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        let primary = Rect {
            x: 100,
            y: 100,
            width: 200,
            height: 200,
        };
        let centered = Rect {
            x: 150,
            y: 150,
            ..window
        };
        let quarter = Rect {
            width: 25,
            ..window
        };
        assert_eq!(place(window, &[quarter], primary), window);
        assert_eq!(
            place(
                window,
                &[Rect {
                    width: 24,
                    ..quarter
                }],
                primary
            ),
            centered
        );
        let body_only = Rect {
            y: 50,
            height: 50,
            ..window
        };
        assert_eq!(place(window, &[body_only], primary), centered);
    }

    #[test]
    fn offscreen_window_centers_without_signed_overflow() {
        let window = Rect {
            x: i32::MIN,
            y: i32::MIN,
            width: 1,
            height: 1,
        };
        let primary = Rect {
            x: i32::MAX - 1,
            y: i32::MAX - 1,
            width: u32::MAX,
            height: u32::MAX,
        };
        assert_eq!(
            place(window, &[primary], primary),
            Rect {
                x: i32::MAX,
                y: i32::MAX,
                ..window
            }
        );
    }
}
