//! Where a dragged or resized window lands against a monitor's edges. Plain numbers only, so it is testable without
//! a desktop; `overlay` converts to and from Win32 rectangles.

/// A rectangle in screen pixels, like Win32's `RECT`: `right` and `bottom` are exclusive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// The monitor edges a window is flush with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per side of a rectangle is the whole meaning; the interface is fixed by the brief"
)]
pub struct Edges {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

/// Moves `proposed` flush to each edge of `monitor` it crosses or comes within `distance` of, keeping its size, and
/// says which edges it is flush with. A window larger than the monitor on an axis goes flush to the left or top edge.
pub fn snap_while_moving(proposed: Rect, monitor: Rect, distance: i32) -> (Rect, Edges) {
    let (left, right, flush_left, flush_right) =
        snap_axis((proposed.left, proposed.right), (monitor.left, monitor.right), distance);
    let (top, bottom, flush_top, flush_bottom) =
        snap_axis((proposed.top, proposed.bottom), (monitor.top, monitor.bottom), distance);
    let snapped = Rect {
        left,
        top,
        right,
        bottom,
    };
    let edges = Edges {
        left: flush_left,
        top: flush_top,
        right: flush_right,
        bottom: flush_bottom,
    };
    (snapped, edges)
}

/// One axis of `snap_while_moving`: the new low and high coordinates, and whether the low or the high edge is flush.
/// The low edge wins when both are in reach, and always for a span longer than the monitor.
fn snap_axis(span: (i32, i32), monitor: (i32, i32), distance: i32) -> (i32, i32, bool, bool) {
    let (low, high) = span;
    let length = high.saturating_sub(low);
    if length > monitor.1.saturating_sub(monitor.0) || low <= monitor.0.saturating_add(distance) {
        (monitor.0, monitor.0.saturating_add(length), true, false)
    } else if high >= monitor.1.saturating_sub(distance) {
        (monitor.1.saturating_sub(length), monitor.1, false, true)
    } else {
        (low, high, false, false)
    }
}

/// Places a window of the new `size` so it stays flush with each marked edge (a right or bottom edge keeps its
/// coordinate, so the window grows leftward or upward), then clamps it inside `monitor`.
pub fn keep_snapped(size: (i32, i32), current: Rect, monitor: Rect, edges: Edges) -> Rect {
    let (left, right) = place_axis(
        size.0,
        (current.left, current.right),
        (monitor.left, monitor.right),
        (edges.left, edges.right),
    );
    let (top, bottom) = place_axis(
        size.1,
        (current.top, current.bottom),
        (monitor.top, monitor.bottom),
        (edges.top, edges.bottom),
    );
    Rect {
        left,
        top,
        right,
        bottom,
    }
}

/// One axis of `keep_snapped`. A span longer than the monitor is flush to its low edge.
fn place_axis(length: i32, current: (i32, i32), monitor: (i32, i32), flush: (bool, bool)) -> (i32, i32) {
    let latest_low = monitor.1.saturating_sub(length).max(monitor.0);
    let low = match flush {
        (true, _) => monitor.0,
        (false, true) => current.1.saturating_sub(length),
        (false, false) => current.0,
    };
    let low = low.clamp(monitor.0, latest_low);
    (low, low.saturating_add(length))
}

#[cfg(test)]
mod tests;
