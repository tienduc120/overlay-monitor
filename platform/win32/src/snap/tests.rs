use super::{Edges, Rect, keep_snapped, snap_while_moving};

const MONITOR: Rect = Rect {
    left: 0,
    top: 0,
    right: 2560,
    bottom: 1440,
};
const DISTANCE: i32 = 15;

fn window(left: i32, top: i32) -> Rect {
    Rect {
        left,
        top,
        right: left + 200,
        bottom: top + 100,
    }
}

#[test]
fn a_window_near_the_left_edge_snaps_flush() {
    let (snapped, edges) = snap_while_moving(window(8, 500), MONITOR, DISTANCE);

    assert_eq!(snapped, window(0, 500), "it must move flush left and keep its size");
    assert!(edges.left, "the left edge must be marked: {edges:?}");
}

#[test]
fn a_window_past_the_right_edge_comes_back_flush() {
    let (snapped, edges) = snap_while_moving(window(2400, 500), MONITOR, DISTANCE);

    assert_eq!(snapped.right, 2560, "its right side must sit on the monitor's");
    assert_eq!(snapped.left, 2360, "its size must not change");
    assert!(edges.right, "the right edge must be marked: {edges:?}");
}

#[test]
fn a_window_far_from_every_edge_is_left_alone() {
    let (snapped, edges) = snap_while_moving(window(500, 500), MONITOR, DISTANCE);

    assert_eq!(snapped, window(500, 500));
    assert_eq!(edges, Edges::default(), "no edge may be marked");
}

#[test]
fn a_corner_snaps_both_edges() {
    let (snapped, edges) = snap_while_moving(window(-5, 1345), MONITOR, DISTANCE);

    assert_eq!((snapped.left, snapped.bottom), (0, 1440));
    assert!(edges.left && edges.bottom, "both edges must be marked: {edges:?}");
    assert!(!edges.right && !edges.top, "the other edges must not be: {edges:?}");
}

#[test]
fn the_second_monitor_has_its_own_edges() {
    let second = Rect {
        left: 2560,
        top: 0,
        right: 4480,
        bottom: 1080,
    };

    let (snapped, edges) = snap_while_moving(window(2565, 500), second, DISTANCE);

    assert_eq!(snapped.left, 2560);
    assert!(edges.left, "{edges:?}");
}

#[test]
fn a_window_wider_than_the_monitor_is_flush_to_the_left() {
    let narrow = Rect {
        left: 0,
        top: 0,
        right: 100,
        bottom: 1440,
    };

    let (snapped, edges) = snap_while_moving(window(30, 500), narrow, DISTANCE);

    assert_eq!(snapped.left, 0);
    assert!(edges.left, "{edges:?}");
}

#[test]
fn a_wider_window_stays_flush_with_its_right_edge() {
    let current = Rect {
        left: 2360,
        top: 500,
        right: 2560,
        bottom: 600,
    };
    let edges = Edges {
        right: true,
        ..Edges::default()
    };

    let placed = keep_snapped((260, 100), current, MONITOR, edges);

    assert_eq!((placed.left, placed.right), (2300, 2560));
}

#[test]
fn a_taller_window_stays_flush_with_its_bottom_edge() {
    let current = Rect {
        left: 500,
        top: 1340,
        right: 700,
        bottom: 1440,
    };
    let edges = Edges {
        bottom: true,
        ..Edges::default()
    };

    let placed = keep_snapped((200, 160), current, MONITOR, edges);

    assert_eq!((placed.top, placed.bottom), (1280, 1440));
}

#[test]
fn a_window_flush_left_keeps_its_left_when_it_grows() {
    let current = window(0, 500);
    let edges = Edges {
        left: true,
        ..Edges::default()
    };

    let placed = keep_snapped((260, 100), current, MONITOR, edges);

    assert_eq!((placed.left, placed.right), (0, 260));
}

#[test]
fn a_grown_window_never_leaves_the_monitor() {
    let current = Rect {
        left: 2300,
        top: 1300,
        right: 2500,
        bottom: 1400,
    };

    let placed = keep_snapped((300, 200), current, MONITOR, Edges::default());

    assert_eq!(
        placed,
        Rect {
            left: 2260,
            top: 1240,
            right: 2560,
            bottom: 1440
        },
        "it must be pushed back inside, size kept"
    );
}
