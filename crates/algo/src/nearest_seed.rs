//! Nearest seed pixel by straight-line (Euclidean) distance, for every pixel of a grid: the
//! feature transform that goes with a distance transform. Nearest seeds along each column first,
//! then the lower envelope of squared-distance parabolas along each row (Felzenszwalb and
//! Huttenlocher, "Distance Transforms of Sampled Functions", Theory of Computing 8, 2012), keeping
//! which seed each parabola stands for. Linear in the pixel count.

/// No seed in this column (column pass), or in the whole grid.
const NONE: u32 = u32::MAX;

/// For each pixel of the `w` × `h` grid `seeds` (row-major), the index (`y * w + x`) of the
/// nearest pixel whose seed is `true`, by Euclidean distance; a seed is its own nearest seed.
/// Among equally near seeds one is chosen, the same one every time.
///
/// `None` when the grid has no seed, when `seeds` is not `w * h` long, or when the grid has
/// `u32::MAX` pixels or more (the indices are stored as `u32` to keep the buffer small), or when
/// there is not enough memory for the result.
pub fn nearest_seed(seeds: &[bool], w: usize, h: usize) -> Option<Vec<u32>> {
    let n = w.checked_mul(h)?;
    if n != seeds.len() || n >= NONE as usize || !seeds.contains(&true) {
        return None;
    }
    // Column pass: the row of the nearest seed in the same column, NONE if the column has none.
    let mut near: Vec<u32> = Vec::new();
    near.try_reserve_exact(n).ok()?;
    near.extend(seeds.iter().enumerate().map(|(i, &s)| if s { (i / w) as u32 } else { NONE }));
    for y in 1..h {
        let (above, rest) = near.split_at_mut(y * w);
        for (d, &u) in rest.iter_mut().take(w).zip(&above[(y - 1) * w..]) {
            if *d == NONE {
                *d = u;
            }
        }
    }
    for y in (0..h.saturating_sub(1)).rev() {
        let (upto, below) = near.split_at_mut((y + 1) * w);
        for (d, &b) in upto[y * w..].iter_mut().zip(below.iter()) {
            if b != NONE && (*d == NONE || b.abs_diff(y as u32) < d.abs_diff(y as u32)) {
                *d = b;
            }
        }
    }
    // Row pass: per row, the column whose (column-nearest) seed is nearest overall.
    let mut env = Envelope { row: vec![NONE; w], g2: vec![0; w], hull: Vec::with_capacity(w), starts: Vec::with_capacity(w) };
    for (y, line) in near.chunks_exact_mut(w).enumerate() {
        env.resolve_row(y as u32, line);
    }
    Some(near)
}

/// Scratch for the row pass, kept across rows so it is allocated once.
struct Envelope {
    /// The row being resolved, as the column pass left it.
    row: Vec<u32>,
    /// Squared vertical distance from each column's nearest seed.
    g2: Vec<i64>,
    /// Columns whose parabolas form the lower envelope, left to right.
    hull: Vec<usize>,
    /// Where each hull parabola starts being the lowest.
    starts: Vec<f64>,
}

impl Envelope {
    /// Replaces `line` (row `y` after the column pass) with each pixel's nearest seed index.
    /// The grid has a seed, so some column of every row has a candidate.
    fn resolve_row(&mut self, y: u32, line: &mut [u32]) {
        let w = line.len();
        self.row.copy_from_slice(line);
        for (g, &r) in self.g2.iter_mut().zip(&self.row) {
            *g = if r == NONE { 0 } else { i64::from(r.abs_diff(y)).pow(2) };
        }
        self.hull.clear();
        self.starts.clear();
        for q in (0..w).filter(|&q| self.row[q] != NONE) {
            let mut s = f64::NEG_INFINITY;
            while let Some(&p) = self.hull.last() {
                // Where the parabola of column q starts to lie below that of column p (p < q).
                let num = (self.g2[q] + (q * q) as i64) - (self.g2[p] + (p * p) as i64);
                s = num as f64 / (2 * (q - p)) as f64;
                if self.hull.len() > 1 && self.starts.last().is_some_and(|&z| s <= z) {
                    self.hull.pop();
                    self.starts.pop();
                } else {
                    break;
                }
            }
            self.hull.push(q);
            self.starts.push(s);
        }
        let mut k = 0;
        for (x, out) in line.iter_mut().enumerate() {
            while self.starts.get(k + 1).is_some_and(|&z| z <= x as f64) {
                k += 1;
            }
            if let Some(&q) = self.hull.get(k) {
                *out = self.row[q] * w as u32 + q as u32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d2(a: usize, b: usize, w: usize) -> usize {
        let (ax, ay, bx, by) = (a % w, a / w, b % w, b / w);
        ax.abs_diff(bx).pow(2) + ay.abs_diff(by).pow(2)
    }

    /// The nearest seed found is as near as the nearest seed a full search finds, on grids of
    /// every shape with scattered, clustered and single seeds.
    #[test]
    fn matches_brute_force() {
        let mut state = 0x9e37_79b9u32;
        let mut rand = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for (w, h) in [(1, 1), (1, 9), (9, 1), (7, 5), (16, 16), (31, 3)] {
            for density in [1, 5, 50] {
                let seeds: Vec<bool> = (0..w * h).map(|_| rand() % 100 < density).collect();
                let Some(near) = nearest_seed(&seeds, w, h) else {
                    assert!(!seeds.contains(&true));
                    continue;
                };
                for (i, &s) in near.iter().enumerate() {
                    let s = s as usize;
                    assert!(seeds[s], "{w}x{h}: pixel {i} maps to a non-seed");
                    let best = (0..w * h).filter(|&j| seeds[j]).map(|j| d2(i, j, w)).min().unwrap();
                    assert_eq!(d2(i, s, w), best, "{w}x{h} density {density}: pixel {i}");
                }
            }
        }
    }

    #[test]
    fn no_seed_or_wrong_size_is_none() {
        assert!(nearest_seed(&[false; 4], 2, 2).is_none());
        assert!(nearest_seed(&[true; 3], 2, 2).is_none());
        assert!(nearest_seed(&[], 0, 0).is_none());
    }
}
