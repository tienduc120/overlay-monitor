//! Coverage: how much of each pixel a shape covers, from 0 (none) to 255 (all). The overlay's outlined text is drawn
//! from the glyphs' coverage alone: grown into the outline's, then each blended into premultiplied pixels.

use std::iter;

/// The coverage of a whole pixel.
const FULL: u8 = u8::MAX;

/// A rectangle of coverage values, row by row from the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Coverage {
    width: usize,
    height: usize,
    values: Vec<u8>,
}

impl Coverage {
    /// Nothing covered.
    pub(crate) fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            values: vec![0; width.saturating_mul(height)],
        }
    }

    /// Adds a texture `texture_width` pixels wide whose top-left lands at (`left`, `top`). A pixel covered twice keeps
    /// the larger value; what falls outside is cut off.
    pub(crate) fn add(&mut self, texture: &[u8], texture_width: usize, (left, top): (i32, i32)) {
        if texture_width == 0 {
            return;
        }
        for (row, values) in texture.chunks(texture_width).enumerate() {
            if let Some(line) = from_origin(top, row).and_then(|y| self.row_mut(y)) {
                add_row(line, values, left);
            }
        }
    }

    /// Adds a texture rasterized `scale` times finer than the canvas, `texture_width` fine pixels wide, its top-left at
    /// (`left`, `top`) in fine pixels: each canvas pixel takes the mean of the `scale` by `scale` fine pixels over it.
    pub(crate) fn add_scaled(&mut self, texture: &[u8], texture_width: usize, (left, top): (i32, i32), scale: u8) {
        if texture_width == 0 || scale == 0 {
            return;
        }
        let fine = i32::from(scale);
        let step = usize::from(scale);
        // How far into its canvas pixel the texture's first fine pixel lies.
        let skew = |origin: i32| usize::try_from(origin.rem_euclid(fine)).unwrap_or(0);
        let (skew_x, skew_y) = (skew(left), skew(top));
        let width = (skew_x + texture_width).div_ceil(step);
        let height = (skew_y + texture.len().div_ceil(texture_width)).div_ceil(step);
        let mut sums = vec![0_u32; width * height];
        for (row, values) in texture.chunks(texture_width).enumerate() {
            let line = (skew_y + row) / step * width;
            let Some(sums) = sums.get_mut(line..line + width) else {
                continue;
            };
            // The first canvas pixel holds only the fine pixels after the skew.
            let (head, rest) = values.split_at((step - skew_x).min(values.len()));
            for (sum, group) in sums.iter_mut().zip(iter::once(head).chain(rest.chunks(step))) {
                *sum += group.iter().map(|&value| u32::from(value)).sum::<u32>();
            }
        }
        let area = u32::from(scale) * u32::from(scale);
        let averaged: Vec<u8> = sums
            .iter()
            .map(|&sum| u8::try_from((sum + area / 2) / area).unwrap_or(FULL))
            .collect();
        self.add(&averaged, width, (left.div_euclid(fine), top.div_euclid(fine)));
    }

    /// The shape grown by `radius` pixels in every direction, corners round: each pixel is covered as far as it lies
    /// within `radius` of the shape. A covered pixel's value says how far the shape reaches past its centre, so the
    /// grown edge is as smooth as the original one.
    pub(crate) fn grown(&self, radius: f32) -> Self {
        let runs = runs(radius, self.width.max(self.height));
        let mut reached = Reached::new(self.values.len());
        for (index, &value) in self.values.iter().enumerate() {
            if value == 0 {
                continue;
            }
            let (x, y) = (index % self.width, index / self.width);
            // Whatever a pixel deep inside reaches, a nearer edge pixel reaches further, so only it needs to spread.
            if self.is_deep_inside(x, y) {
                reached.cover_fully(index);
            } else {
                self.spread((x, y), value, &runs, &mut reached);
            }
        }
        Self {
            width: self.width,
            height: self.height,
            values: reached.coverage(),
        }
    }

    fn row_mut(&mut self, y: usize) -> Option<&mut [u8]> {
        let start = y.checked_mul(self.width)?;
        self.values.get_mut(start..start.checked_add(self.width)?)
    }

    fn index(&self, x: usize, y: usize) -> Option<usize> {
        (x < self.width && y < self.height).then(|| y * self.width + x)
    }

    /// Whether the pixel and the eight around it are all fully covered.
    fn is_deep_inside(&self, x: usize, y: usize) -> bool {
        (-1..=1).all(|dy| {
            (-1..=1).all(|dx| {
                x.checked_add_signed(dx)
                    .zip(y.checked_add_signed(dy))
                    .and_then(|(x, y)| self.index(x, y))
                    .and_then(|index| self.values.get(index))
                    .is_some_and(|&value| value == FULL)
            })
        })
    }

    /// Records how far the growth of the pixel at (`x`, `y`), whose own coverage is `value`, covers each pixel `runs`
    /// reach.
    fn spread(&self, (x, y): (usize, usize), value: u8, runs: &[Run], reached: &mut Reached) {
        for run in runs {
            let Some(line) = y.checked_add_signed(run.dy).filter(|&line| line < self.height) else {
                continue;
            };
            // Pixels left of the canvas are skipped; the zip stops at its right edge.
            let (first, skipped) = match x.checked_add_signed(run.first_dx) {
                Some(first) => (first, 0),
                None => (0, run.first_dx.unsigned_abs() - x),
            };
            let plane = reached.plane_mut(run.side);
            let Some(targets) = plane.get_mut(line * self.width + first..(line + 1) * self.width) else {
                continue;
            };
            for (target, &depth) in targets.iter_mut().zip(run.depths.iter().skip(skipped)) {
                let level = (depth + i32::from(value)).clamp(0, i32::from(FULL));
                *target = (*target).max(u8::try_from(level).unwrap_or(FULL));
            }
        }
    }
}

/// The side of a pixel that growth comes in from, or the pixel's own coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Above,
    Below,
    Own,
}

/// Pixels in one row that a covered pixel's growth reaches from the same side, left to right: the row's offset, the
/// first pixel's offset along it, and how far inside the radius the covered pixel's centre lies from each, in coverage
/// steps (255 per pixel; below 0 when only the covered part past the centre reaches).
#[derive(Clone, Debug)]
struct Run {
    dy: isize,
    first_dx: isize,
    side: Side,
    depths: Vec<i32>,
}

/// How far growth from each side covers each pixel, a plane per side. Growth from one side covers a pixel from that
/// edge in, so the furthest reach counts; growth from opposite sides, as in a gap narrower than the outline, covers it
/// from both edges, so the two add up.
struct Reached {
    left: Vec<u8>,
    right: Vec<u8>,
    above: Vec<u8>,
    below: Vec<u8>,
    own: Vec<u8>,
}

impl Reached {
    fn new(len: usize) -> Self {
        Self {
            left: vec![0; len],
            right: vec![0; len],
            above: vec![0; len],
            below: vec![0; len],
            own: vec![0; len],
        }
    }

    fn cover_fully(&mut self, index: usize) {
        if let Some(own) = self.own.get_mut(index) {
            *own = FULL;
        }
    }

    fn plane_mut(&mut self, side: Side) -> &mut Vec<u8> {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
            Side::Above => &mut self.above,
            Side::Below => &mut self.below,
            Side::Own => &mut self.own,
        }
    }

    fn coverage(&self) -> Vec<u8> {
        let across = self
            .left
            .iter()
            .zip(&self.right)
            .map(|(&left, &right)| left.saturating_add(right));
        let down = self
            .above
            .iter()
            .zip(&self.below)
            .map(|(&above, &below)| above.saturating_add(below));
        across
            .zip(down)
            .zip(&self.own)
            .map(|((across, down), &own)| own.max(across).max(down))
            .collect()
    }
}

/// The side growth by (`dx`, `dy`) comes in from, seen from the pixel it reaches: the longer axis decides.
fn side(dx: i16, dy: i16) -> Side {
    if dx == 0 && dy == 0 {
        Side::Own
    } else if dx.unsigned_abs() >= dy.unsigned_abs() {
        if dx > 0 { Side::Left } else { Side::Right }
    } else if dy > 0 {
        Side::Above
    } else {
        Side::Below
    }
}

/// `i` pixels on from `origin`, unless that is before the canvas's first pixel.
fn from_origin(origin: i32, i: usize) -> Option<usize> {
    usize::try_from(i64::from(origin).checked_add(i64::try_from(i).ok()?)?).ok()
}

fn add_row(line: &mut [u8], values: &[u8], left: i32) {
    for (column, &value) in values.iter().enumerate() {
        if let Some(pixel) = from_origin(left, column).and_then(|x| line.get_mut(x)) {
            *pixel = (*pixel).max(value);
        }
    }
}

/// The runs of pixels a covered pixel's growth by `radius` reaches, at most `span` pixels away. Its shape can lie up
/// to a pixel past its centre, so the reach is a pixel longer than the radius.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a float-to-int `as` saturates, and these are a few pixels, or a few thousand coverage steps"
)]
fn runs(radius: f32, span: usize) -> Vec<Run> {
    let limit = ((radius + 1.0).ceil() as i16).min(i16::try_from(span).unwrap_or(i16::MAX));
    let mut runs: Vec<Run> = Vec::new();
    for dy in -limit..=limit {
        for dx in -limit..=limit {
            let distance = f32::from(dx).hypot(f32::from(dy));
            if distance >= radius + 1.0 {
                continue;
            }
            let depth = ((radius - distance) * f32::from(FULL)).round() as i32;
            let (dy, dx, side) = (isize::from(dy), isize::from(dx), side(dx, dy));
            // Pixels in reach are one stretch of each row, so a pixel continues the last run unless the side changes.
            match runs.last_mut() {
                Some(run) if run.dy == dy && run.side == side => run.depths.push(depth),
                _ => runs.push(Run {
                    dy,
                    first_dx: dx,
                    side,
                    depths: vec![depth],
                }),
            }
        }
    }
    runs
}

/// Paints the opaque colour `rgb` (`0xRRGGBB`) over premultiplied BGRA `pixels`, each as far as `coverage` covers it.
/// The pixels are the coverage's, row by row.
pub(crate) fn blend(pixels: &mut [[u8; 4]], coverage: &Coverage, rgb: u32) {
    let [_, red, green, blue] = rgb.to_be_bytes();
    let colour = [blue, green, red, FULL];
    for (pixel, &alpha) in pixels.iter_mut().zip(&coverage.values) {
        if alpha == 0 {
            continue;
        }
        for (channel, &source) in pixel.iter_mut().zip(&colour) {
            *channel = over(source, alpha, *channel);
        }
    }
}

/// `source` at `alpha` over `destination`, both premultiplied: `source * alpha + destination * (1 - alpha)`, rounded.
fn over(source: u8, alpha: u8, destination: u8) -> u8 {
    let mixed = u32::from(source) * u32::from(alpha) + u32::from(destination) * u32::from(FULL - alpha);
    // At most 255 * 255 + 127, which divided by 255 fits a u8 again.
    u8::try_from((mixed + 127) / 255).unwrap_or(FULL)
}

#[cfg(test)]
mod tests;
