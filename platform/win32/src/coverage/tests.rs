use super::{Coverage, blend};

fn grid(width: usize, rows: &[&[u8]]) -> Vec<u8> {
    let values: Vec<u8> = rows.iter().flat_map(|row| row.iter().copied()).collect();
    assert_eq!(values.len(), width * rows.len(), "every row must be {width} wide");
    values
}

#[test]
fn a_texture_lands_where_its_top_left_says() {
    let mut coverage = Coverage::new(4, 3);

    coverage.add(&[1, 2, 3, 4], 2, (1, 1));

    assert_eq!(coverage.values, grid(4, &[&[0, 0, 0, 0], &[0, 1, 2, 0], &[0, 3, 4, 0]]));
}

#[test]
fn what_falls_outside_the_canvas_is_cut_off() {
    let mut coverage = Coverage::new(3, 2);

    coverage.add(&[1, 2, 3, 4], 2, (-1, -1));
    coverage.add(&[5, 6, 7, 8], 2, (2, 1));

    assert_eq!(coverage.values, grid(3, &[&[4, 0, 0], &[0, 0, 5]]));
}

#[test]
fn a_pixel_covered_twice_keeps_the_larger_value() {
    let mut coverage = Coverage::new(2, 1);

    coverage.add(&[100, 200], 2, (0, 0));
    coverage.add(&[50, 250], 2, (0, 0));

    assert_eq!(coverage.values, [100, 250]);
}

#[test]
fn growing_a_straight_edge_moves_it_out_by_the_radius_with_its_partial_pixel() {
    let mut coverage = Coverage::new(8, 1);
    // The edge lies a quarter of the way into the third pixel.
    coverage.add(&[255, 255, 64], 3, (0, 0));

    let grown = coverage.grown(2.0);

    assert_eq!(grown.values, [255, 255, 255, 255, 64, 0, 0, 0]);
}

#[test]
fn a_fractional_radius_grows_by_part_of_a_pixel() {
    let mut coverage = Coverage::new(4, 1);
    coverage.add(&[255], 1, (0, 0));

    let grown = coverage.grown(0.5);

    assert_eq!(grown.values, [255, 127, 0, 0], "half of the next pixel");
}

#[test]
fn growing_a_pixel_reaches_its_neighbours_and_rounds_the_corners() {
    let mut coverage = Coverage::new(5, 5);
    coverage.add(&[255], 1, (2, 2));

    let grown = coverage.grown(1.0);

    let expected = grid(
        5,
        &[
            &[0, 0, 0, 0, 0],
            &[0, 149, 255, 149, 0],
            &[0, 255, 255, 255, 0],
            &[0, 149, 255, 149, 0],
            &[0, 0, 0, 0, 0],
        ],
    );
    assert_eq!(
        grown.values, expected,
        "a diagonal neighbour is farther, so less of it is covered"
    );
}

#[test]
fn a_solid_block_grows_from_its_edges_and_stays_solid_inside() {
    let mut coverage = Coverage::new(7, 7);
    coverage.add(&[255; 9], 3, (2, 2));

    let grown = coverage.grown(1.0);

    let expected = grid(
        7,
        &[
            &[0, 0, 0, 0, 0, 0, 0],
            &[0, 149, 255, 255, 255, 149, 0],
            &[0, 255, 255, 255, 255, 255, 0],
            &[0, 255, 255, 255, 255, 255, 0],
            &[0, 255, 255, 255, 255, 255, 0],
            &[0, 149, 255, 255, 255, 149, 0],
            &[0, 0, 0, 0, 0, 0, 0],
        ],
    );
    assert_eq!(grown.values, expected);
}

#[test]
fn full_coverage_paints_the_opaque_colour() {
    let mut pixels = [[10, 20, 30, 40]];
    let mut coverage = Coverage::new(1, 1);
    coverage.add(&[255], 1, (0, 0));

    blend(&mut pixels, &coverage, 0x11_22_33);

    assert_eq!(pixels, [[0x33, 0x22, 0x11, 255]], "blue, green, red, alpha");
}

#[test]
fn no_coverage_leaves_the_pixel_as_it_was() {
    let mut pixels = [[10, 20, 30, 40]];

    blend(&mut pixels, &Coverage::new(1, 1), 0x11_22_33);

    assert_eq!(pixels, [[10, 20, 30, 40]]);
}

#[test]
fn partial_coverage_blends_over_what_is_there_premultiplied() {
    let mut pixels = [[0, 0, 0, 0], [255, 0, 0, 255]];
    let mut coverage = Coverage::new(2, 1);
    coverage.add(&[128, 128], 2, (0, 0));

    blend(&mut pixels, &coverage, 0xFF_FF_FF);

    assert_eq!(
        pixels,
        [[128, 128, 128, 128], [255, 128, 128, 255]],
        "half white over nothing, then over opaque blue"
    );
}

#[test]
fn blending_never_lowers_the_alpha_a_see_through_background_keeps_for_the_mouse() {
    let mut pixels = [[0, 0, 0, 1], [0, 0, 0, 1]];
    let mut coverage = Coverage::new(2, 1);
    coverage.add(&[1, 0], 2, (0, 0));

    blend(&mut pixels, &coverage, 0x00_00_00);

    assert_eq!(pixels, [[0, 0, 0, 2], [0, 0, 0, 1]]);
}

#[test]
fn a_finer_texture_is_averaged_over_each_pixel() {
    let mut coverage = Coverage::new(2, 1);

    coverage.add_scaled(&[255, 255, 0, 255, 255, 255, 0, 0], 4, (0, 0), 2);

    assert_eq!(coverage.values, [255, 64], "four of four, then one of four");
}

#[test]
fn a_finer_texture_left_of_the_canvas_is_cut_off_by_whole_pixels() {
    let mut coverage = Coverage::new(2, 1);

    coverage.add_scaled(&[100, 200], 2, (-1, 0), 2);

    assert_eq!(
        coverage.values,
        [50, 0],
        "the fine pixel at -1 lies in canvas pixel -1, not 0"
    );
}

#[test]
fn opposite_edges_that_both_reach_into_a_pixel_cover_it_together() {
    let mut coverage = Coverage::new(1, 3);
    coverage.add(&[255, 0, 255], 1, (0, 0));

    let grown = coverage.grown(0.25);

    assert_eq!(
        grown.values,
        [255, 128, 255],
        "a quarter from above and a quarter from below"
    );
}
