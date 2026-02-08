/// Row-major flat grid. No per-cell objects, f32 friendly.
/// Supports E-W wrapping (cylindrical topology).
#[derive(Clone, Debug)]
pub struct Grid<T> {
    pub data: Vec<T>,
    pub w: usize,
    pub h: usize,
}

impl<T: Copy + Default> Grid<T> {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            data: vec![T::default(); w * h],
            w,
            h,
        }
    }

    #[inline]
    pub fn idx(&self, x: usize, y: usize) -> usize {
        debug_assert!(x < self.w && y < self.h);
        y * self.w + x
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> T {
        self.data[self.idx(x, y)]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, v: T) {
        let i = self.idx(x, y);
        self.data[i] = v;
    }
}

/// Wrap x-coordinate for E-W wrapping. y is clamped (polar boundary).
/// Returns None if y is out of bounds.
#[inline]
pub fn wrap_xy(x: i32, y: i32, w: usize, h: usize) -> Option<(usize, usize)> {
    if y < 0 || y >= h as i32 {
        return None; // N/S polar boundary: no wrap
    }
    let wx = ((x % w as i32) + w as i32) as usize % w;
    Some((wx, y as usize))
}

/// 4-connected neighbors with E-W wrapping.
pub fn neighbors4_wrap(x: usize, y: usize, w: usize, h: usize) -> impl Iterator<Item = (usize, usize)> {
    let offsets: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
    let mut out = [(0usize, 0usize); 4];
    let mut n = 0;
    for (dx, dy) in offsets {
        if let Some(pos) = wrap_xy(x as i32 + dx, y as i32 + dy, w, h) {
            out[n] = pos;
            n += 1;
        }
    }
    out.into_iter().take(n)
}

/// 8-connected neighbors with E-W wrapping.
pub fn neighbors8_wrap(x: usize, y: usize, w: usize, h: usize) -> impl Iterator<Item = (usize, usize)> {
    let offsets: [(i32, i32); 8] = [
        (-1, -1), (0, -1), (1, -1),
        (-1, 0),           (1, 0),
        (-1, 1),  (0, 1),  (1, 1),
    ];
    let mut out = [(0usize, 0usize); 8];
    let mut n = 0;
    for (dx, dy) in offsets {
        if let Some(pos) = wrap_xy(x as i32 + dx, y as i32 + dy, w, h) {
            out[n] = pos;
            n += 1;
        }
    }
    out.into_iter().take(n)
}
