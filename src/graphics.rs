pub struct SharedGraphics {
    pub active: bool,
    pub buffer: Vec<u32>,
    pub width: usize,
    pub height: usize,
    pub title: String,
    pub fg_color: u32,
    pub bg_color: u32,
    pub updated: bool,
    pub mode: i32,
    /// Last point drawn, used by `LINE -(x, y)`.
    pub last_point: (i64, i64),
    /// Set by the front end when the graphics window could not be opened.
    pub error: Option<String>,
}

impl Default for SharedGraphics {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedGraphics {
    pub fn new() -> Self {
        Self {
            active: false,
            buffer: vec![],
            width: 0,
            height: 0,
            title: String::new(),
            fg_color: 0xFFFFFF,
            bg_color: 0x000000,
            updated: false,
            mode: 0,
            last_point: (0, 0),
            error: None,
        }
    }

    /// Switch screen mode. Mode 0 returns to text mode and closes the window.
    pub fn screen(&mut self, mode: i32) -> Result<(), String> {
        let (w, h) = match mode {
            0 => {
                self.mode = 0;
                self.active = false;
                self.buffer.clear();
                self.width = 0;
                self.height = 0;
                return Ok(());
            }
            1 | 7 | 13 => (320, 200),
            2 | 8 => (640, 200),
            9 | 10 => (640, 350),
            11 | 12 => (640, 480),
            _ => {
                return Err(format!(
                    "Illegal function call: SCREEN {} is not supported",
                    mode
                ))
            }
        };
        self.mode = mode;
        self.width = w;
        self.height = h;
        self.buffer = vec![self.bg_color; w * h];
        self.title = format!("QBasic Window (SCREEN {})", mode);
        self.active = true;
        self.updated = true;
        self.last_point = (w as i64 / 2, h as i64 / 2);
        Ok(())
    }

    fn in_bounds(&self, x: i64, y: i64) -> bool {
        x >= 0 && y >= 0 && x < self.width as i64 && y < self.height as i64
    }

    pub fn cls(&mut self) {
        let bg = self.bg_color;
        self.buffer.iter_mut().for_each(|px| *px = bg);
        self.updated = true;
    }

    fn put(&mut self, x: i64, y: i64, c: u32) {
        if self.in_bounds(x, y) {
            self.buffer[y as usize * self.width + x as usize] = c;
        }
    }

    pub fn pset(&mut self, x: i64, y: i64, color: Option<u32>) {
        let c = color.unwrap_or(self.fg_color);
        self.put(x, y, c);
        self.last_point = (x, y);
        self.updated = true;
    }

    pub fn getpx(&self, x: i64, y: i64) -> u32 {
        if self.in_bounds(x, y) {
            self.buffer[y as usize * self.width + x as usize]
        } else {
            0
        }
    }

    pub fn line(&mut self, x1: i64, y1: i64, x2: i64, y2: i64, color: Option<u32>) {
        let c = color.unwrap_or(self.fg_color);
        self.last_point = (x2, y2);
        self.updated = true;
        let Some((mut x0, mut y0, xe, ye)) = self.clip_line(x1, y1, x2, y2) else {
            return;
        };
        let dx = (xe - x0).abs();
        let sx = if x0 < xe { 1 } else { -1 };
        let dy = -(ye - y0).abs();
        let sy = if y0 < ye { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.put(x0, y0, c);
            if x0 == xe && y0 == ye {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    /// Liang-Barsky clipping against the screen rectangle. Returns the
    /// visible segment rounded to pixels, or `None` if nothing is visible.
    fn clip_line(&self, x1: i64, y1: i64, x2: i64, y2: i64) -> Option<(i64, i64, i64, i64)> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        let (xmin, ymin) = (0.0, 0.0);
        let (xmax, ymax) = ((self.width - 1) as f64, (self.height - 1) as f64);
        let (fx1, fy1) = (x1 as f64, y1 as f64);
        let (dx, dy) = (x2 as f64 - fx1, y2 as f64 - fy1);
        let mut t0: f64 = 0.0;
        let mut t1: f64 = 1.0;
        for (p, q) in [
            (-dx, fx1 - xmin),
            (dx, xmax - fx1),
            (-dy, fy1 - ymin),
            (dy, ymax - fy1),
        ] {
            if p == 0.0 {
                if q < 0.0 {
                    return None;
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    t0 = t0.max(r);
                } else {
                    t1 = t1.min(r);
                }
                if t0 > t1 {
                    return None;
                }
            }
        }
        Some((
            (fx1 + t0 * dx).round() as i64,
            (fy1 + t0 * dy).round() as i64,
            (fx1 + t1 * dx).round() as i64,
            (fy1 + t1 * dy).round() as i64,
        ))
    }

    pub fn rect(&mut self, x1: i64, y1: i64, x2: i64, y2: i64, color: Option<u32>, filled: bool) {
        let (xa, xb) = (x1.min(x2), x1.max(x2));
        let (ya, yb) = (y1.min(y2), y1.max(y2));
        if filled {
            let c = color.unwrap_or(self.fg_color);
            let x_lo = xa.max(0);
            let x_hi = xb.min(self.width as i64 - 1);
            let y_lo = ya.max(0);
            let y_hi = yb.min(self.height as i64 - 1);
            for y in y_lo..=y_hi {
                for x in x_lo..=x_hi {
                    self.put(x, y, c);
                }
            }
        } else {
            self.line(xa, ya, xb, ya, color);
            self.line(xb, ya, xb, yb, color);
            self.line(xb, yb, xa, yb, color);
            self.line(xa, yb, xa, ya, color);
        }
        self.last_point = (x2, y2);
        self.updated = true;
    }

    /// Draws a circle outline. Each visible row and column is scanned once,
    /// so the cost is bounded by the screen size no matter how large the
    /// radius is, and the outline has no gaps.
    pub fn circle(&mut self, cx: i64, cy: i64, r: i64, color: Option<u32>) {
        let c = color.unwrap_or(self.fg_color);
        self.last_point = (cx, cy);
        self.updated = true;
        if r < 0 {
            return;
        }
        if r == 0 {
            self.put(cx, cy, c);
            return;
        }
        let rf = r as f64;
        let (w, h) = (self.width as i64, self.height as i64);
        for y in (cy - r).max(0)..=(cy + r).min(h - 1) {
            let dy = (y - cy) as f64;
            let dx = (rf * rf - dy * dy).max(0.0).sqrt().round() as i64;
            self.put(cx - dx, y, c);
            self.put(cx + dx, y, c);
        }
        for x in (cx - r).max(0)..=(cx + r).min(w - 1) {
            let dx = (x - cx) as f64;
            let dy = (rf * rf - dx * dx).max(0.0).sqrt().round() as i64;
            self.put(x, cy - dy, c);
            self.put(x, cy + dy, c);
        }
    }

    /// Flood fill from (x, y) up to pixels of the `border` colour, as QBasic's
    /// `PAINT` does.
    pub fn paint(&mut self, x: i64, y: i64, fill: u32, border: u32) {
        if !self.in_bounds(x, y) || self.getpx(x, y) == border {
            return;
        }
        let mut visited = vec![false; self.width * self.height];
        let mut stack = vec![(x, y)];
        while let Some((px, py)) = stack.pop() {
            if !self.in_bounds(px, py) {
                continue;
            }
            let idx = py as usize * self.width + px as usize;
            if visited[idx] || self.buffer[idx] == border {
                continue;
            }
            visited[idx] = true;
            self.buffer[idx] = fill;
            stack.push((px + 1, py));
            stack.push((px - 1, py));
            stack.push((px, py + 1));
            stack.push((px, py - 1));
        }
        self.updated = true;
    }
}

pub fn qb_color(c: u32) -> u32 {
    if c > 0xFF {
        return c & 0xFFFFFF;
    }
    PALETTE[c as usize]
}

pub static PALETTE: [u32; 256] = build_palette();

const fn build_palette() -> [u32; 256] {
    let mut p = [0u32; 256];
    let ega: [u32; 16] = [
        0x000000, 0x0000AA, 0x00AA00, 0x00AAAA, 0xAA0000, 0xAA00AA, 0xAA5500, 0xAAAAAA, 0x555555,
        0x5555FF, 0x55FF55, 0x55FFFF, 0xFF5555, 0xFF55FF, 0xFFFF55, 0xFFFFFF,
    ];
    let mut i = 0;
    while i < 16 {
        p[i] = ega[i];
        i += 1;
    }
    let mut k = 0;
    while k < 16 {
        let v = (k as u32 * 17) & 0xFF;
        p[16 + k] = (v << 16) | (v << 8) | v;
        k += 1;
    }
    let mut n = 0;
    while n < 224 {
        let h = (n * 360 / 224) as i32;
        let (r, g, b) = hsv_to_rgb_const(h, 100, 100);
        p[32 + n] = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        n += 1;
    }
    p
}

const fn hsv_to_rgb_const(h: i32, s: i32, v: i32) -> (i32, i32, i32) {
    let c = v * s / 100;
    let h_seg = h / 60;
    let h_rem = h - h_seg * 60;
    let x = c * (60 - abs_const(h_rem - 30) * 2) / 60;
    let (r1, g1, b1) = match h_seg {
        0 => (c, x, 0),
        1 => (x, c, 0),
        2 => (0, c, x),
        3 => (0, x, c),
        4 => (x, 0, c),
        _ => (c, 0, x),
    };
    let m = v - c;
    let r = (r1 + m) * 255 / 100;
    let g = (g1 + m) * 255 / 100;
    let b = (b1 + m) * 255 / 100;
    (clamp_const(r), clamp_const(g), clamp_const(b))
}
const fn abs_const(x: i32) -> i32 {
    if x < 0 {
        -x
    } else {
        x
    }
}
const fn clamp_const(x: i32) -> i32 {
    if x < 0 {
        0
    } else if x > 255 {
        255
    } else {
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen12() -> SharedGraphics {
        let mut g = SharedGraphics::new();
        g.screen(12).unwrap();
        g
    }

    #[test]
    fn huge_coordinates_are_clipped_quickly() {
        let mut g = screen12();
        g.line(0, 0, 2_000_000_000, 5, Some(15));
        g.line(
            -9_000_000_000,
            -9_000_000_000,
            9_000_000_000,
            9_000_000_000,
            Some(1),
        );
        g.circle(0, 0, 2_000_000_000, Some(15));
        g.rect(0, 0, 900_000_000, 900_000_000, Some(1), true);
        assert_eq!(g.getpx(639, 479), 1);
    }

    #[test]
    fn circle_outline_is_symmetric() {
        let mut g = screen12();
        g.circle(100, 100, 10, Some(7));
        for (x, y) in [(110, 100), (90, 100), (100, 110), (100, 90)] {
            assert_eq!(g.getpx(x, y), 7, "({}, {})", x, y);
        }
        assert_eq!(g.getpx(100, 100), 0);
    }

    #[test]
    fn paint_stops_at_border_colour() {
        let mut g = screen12();
        g.rect(10, 10, 20, 20, Some(4), false);
        g.pset(15, 15, Some(9)); // a different colour inside the box
        g.paint(12, 12, 2, 4);
        assert_eq!(g.getpx(15, 15), 2);
        assert_eq!(g.getpx(10, 10), 4);
        assert_eq!(g.getpx(25, 25), 0);
    }

    #[test]
    fn screen_zero_returns_to_text_mode() {
        let mut g = screen12();
        g.screen(0).unwrap();
        assert!(!g.active);
        assert!(g.screen(99).is_err());
    }
}
