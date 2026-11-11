//! The flowing light on the side bumps (and the rim of the tray number's flash): pure numbers.
//!
//! Windows blue (#0063B1) brightening to white and back, alive. Two layers of those colours slide
//! at different speeds and in opposite directions; where they cross they mix, so the light keeps
//! shifting instead of just sliding. A frame is computed pixel by pixel into a premultiplied
//! buffer: colours mixed per row, then shaped by a mask (the glow's strength at each pixel).

/// Blue to white and back.
pub const COLORS: [u32; 6] = [0x0063B1, 0x2A82CC, 0x8CC0EC, 0xFFFFFF, 0x8CC0EC, 0x2A82CC];
/// Time for each layer to slide one period.
pub const FLOW_MS: u64 = 2600;
pub const FLOW2_MS: u64 = 3700;

/// Brightness d pixels out from a shape's edge: a thin bright line on the edge inside a soft haze
/// w wide, with a 1-pixel soft rim just inside. core: the bright line's width. The two layers are
/// mixed, not stacked, so the haze is lifted to what stacking them would give.
pub fn glow_fall(d: f64, w: f64, core: f64) -> f64 {
    if d <= -1.0 || d >= w {
        return 0.0;
    }
    let rim = if d < 0.0 { 1.0 + d } else { 1.0 };
    let d = d.max(0.0);
    let f = 1.0 - d / w;
    let v = (0.9 * f.powf(2.2) + if d < core { 0.4 * (1.0 - d / core) } else { 0.0 }).min(1.0);
    rim * v * (1.0 + 150.0 / 255.0 * (1.0 - v))
}

/// Not a simple fraction of h: the mix never repeats the same way.
pub fn period2(h: u32) -> u32 {
    (h as f64 * 0.62).round() as u32
}

/// The glow's colour at position y of a cycle `period` long, starting `shift` colours in: it eases
/// from each of COLORS to the next. 0xRRGGBB.
pub fn color_at(y: f64, period: f64, shift: f64) -> u32 {
    let n = COLORS.len() as f64;
    let pos = ((y.rem_euclid(period)) / period * n + shift).rem_euclid(n);
    let i = pos.floor() as usize;
    let k = pos - pos.floor();
    let k = k * k * (3.0 - 2.0 * k);
    let (c1, c2) = (COLORS[i], COLORS[(i + 1) % COLORS.len()]);
    let ch = |c: u32, s: u32| ((c >> s) & 0xFF) as f64;
    let mix = |s: u32| (ch(c1, s) + (ch(c2, s) - ch(c1, s)) * k).round() as u32;
    (mix(16) << 16) | (mix(8) << 8) | mix(0)
}

/// Distance from a rounded rectangle's edge, outward (negative inside), for a point (qx, qy) away
/// from where its straight sides end. Corners are superellipses |x|^n + |y|^n = rad^n (n = 2:
/// circle arcs). In a corner, the superellipse's own measure is divided by its slope so it's a true
/// distance (raw, it's ~20 % short at 45°).
pub fn round_rect_distance(qx: f64, qy: f64, rad: f64, n: f64) -> f64 {
    if qx > 0.0 && qy > 0.0 {
        let lp = (qx.powf(n) + qy.powf(n)).powf(1.0 / n);
        let gx = (qx / lp).powf(n - 1.0);
        let gy = (qy / lp).powf(n - 1.0);
        return (lp - rad) / (gx * gx + gy * gy).sqrt();
    }
    qx.max(qy) - rad // along a side: straight distance from the edge
}

/// A bell standing out of a screen edge, depth × (1 - u²)² for u = -1…1 along the edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BellEdge {
    /// How far its outline stands out.
    pub x: f64,
    /// How much a horizontal distance overstates the true one there (the slope).
    pub norm: f64,
    /// Its ends melting into the edge.
    pub fade: f64,
}

pub fn bell_edge(u: f64, depth: f64, half: f64) -> Option<BellEdge> {
    let u = u.abs();
    if u >= 1.0 {
        return None;
    }
    let e = 1.0 - u * u;
    let slope = 4.0 * depth * u * e / half;
    let fade = ((1.0 - u) * 4.0).min(1.0);
    Some(BellEdge { x: depth * e * e, norm: (1.0 + slope * slope).sqrt(), fade: fade * fade * (3.0 - 2.0 * fade) })
}

/// The mask of a bell (filled, with a haze mg wide round it) centred in a w × h window: m (0-255)
/// per pixel, row by row. side -1: on the left edge of the screen, standing out to the right;
/// +1: mirrored.
pub fn bell_mask(w: usize, h: usize, depth: f64, half: f64, mg: f64, core: f64, side: i32) -> Vec<u8> {
    let mut m = vec![0u8; w * h];
    for j in 0..h / 2 {
        let Some(b) = bell_edge((h as f64 / 2.0 - j as f64 - 0.5) / half, depth, half) else { continue };
        for c in 0..w {
            let d = (c as f64 + 0.5 - b.x) / b.norm; // distance from the outline, outward
            let v = (255.0 * b.fade * if d < 0.0 { 1.0 } else { glow_fall(d, mg, core) }).round() as u8;
            if v == 0 {
                if d > 0.0 {
                    break; // only darker further out
                }
                continue;
            }
            let col = if side < 0 { c } else { w - 1 - c };
            m[j * w + col] = v;
            m[(h - 1 - j) * w + col] = v;
        }
    }
    m
}

/// A small picture laid on top of a frame, premultiplied 0xAARRGGBB, `size` square.
pub struct Stamp<'a> {
    pub px: &'a [u32],
    pub size: usize,
    pub x: i32,
    pub y: i32,
    pub alpha: f64,
}

/// One frame of light along the height (the bumps' way: one layer flows up, the other down),
/// shaped by mask, into out (premultiplied 0xAARRGGBB, w × h). t: the clock in ms; slow: how many
/// times slower than FLOW_MS; white: 0-1, light filling it.
pub fn compose(out: &mut [u32], mask: &[u8], w: usize, h: usize, t: u64, slow: u64, white: f64, stamp: Option<&Stamp>) {
    let pa = h as u64;
    let pb = period2(h as u32).max(1) as u64;
    let oa = (t * pa / (slow * FLOW_MS)) % pa;
    let ob = pb - (t * pb / (slow * FLOW2_MS)) % pb;
    let wf = (220.0 * white.clamp(0.0, 1.0)).round() / 255.0;
    let shift = (COLORS.len() / 2) as f64;
    for y in 0..h {
        let a = color_at((y as u64 + oa) as f64, pa as f64, 0.0);
        let b = color_at((y as u64 + ob) as f64, pb as f64, shift);
        let ch = |s: u32| {
            let v = ((a >> s) & 0xFF) as f64 * (105.0 / 255.0) + ((b >> s) & 0xFF) as f64 * (150.0 / 255.0);
            v + (255.0 - v) * wf
        };
        let (r, g, bl) = (ch(16), ch(8), ch(0));
        for x in 0..w {
            let m = mask[y * w + x] as f64;
            out[y * w + x] = if m == 0.0 {
                0
            } else {
                let k = m / 255.0;
                ((m as u32) << 24) | (((r * k).round() as u32) << 16) | (((g * k).round() as u32) << 8) | (bl * k).round() as u32
            };
        }
    }
    if let Some(s) = stamp {
        for sy in 0..s.size {
            for sx in 0..s.size {
                let (x, y) = (s.x + sx as i32, s.y + sy as i32);
                if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                    continue;
                }
                let src = s.px[sy * s.size + sx];
                let sa = ((src >> 24) & 0xFF) as f64 / 255.0 * s.alpha;
                if sa <= 0.0 {
                    continue;
                }
                let o = &mut out[y as usize * w + x as usize];
                *o = over(src, s.alpha, *o);
            }
        }
    }
}

/// src (premultiplied, × alpha) over dst (premultiplied).
pub fn over(src: u32, alpha: f64, dst: u32) -> u32 {
    let sa = ((src >> 24) & 0xFF) as f64 / 255.0 * alpha;
    let mut r = 0u32;
    for s in [24u32, 16, 8, 0] {
        let sv = ((src >> s) & 0xFF) as f64 * alpha;
        let dv = ((dst >> s) & 0xFF) as f64;
        r |= ((sv + dv * (1.0 - sa)).round().min(255.0) as u32) << s;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn fall() {
        assert_eq!(glow_fall(10.0, 10.0, 3.0), 0.0);
        assert_eq!(glow_fall(-1.0, 10.0, 3.0), 0.0);
        assert!(glow_fall(0.0, 10.0, 3.0) > glow_fall(5.0, 10.0, 3.0));
        assert!(glow_fall(5.0, 10.0, 3.0) > glow_fall(9.0, 10.0, 3.0));
    }

    #[test]
    fn colours() {
        assert_eq!(color_at(0.0, 60.0, 0.0), 0x0063B1);
        assert_eq!(color_at(30.0, 60.0, 0.0), 0xFFFFFF, "half-way is white");
        assert_eq!(color_at(60.0, 60.0, 0.0), color_at(0.0, 60.0, 0.0), "wraps");
        assert_eq!(color_at(0.0, 60.0, 3.0), 0xFFFFFF);
        assert_eq!(period2(100), 62);
    }

    #[test]
    fn rounded_rectangle_distances() {
        assert!(near(round_rect_distance(-5.0, 13.0, 10.0, 2.0), 3.0));
        assert!(near(round_rect_distance(-5.0, 3.0, 10.0, 2.0), -7.0));
        assert!(near(round_rect_distance(3.0, 4.0, 5.0, 2.0), 0.0), "on the arc");
        assert!(near(round_rect_distance(6.0, 8.0, 5.0, 2.0), 5.0));
    }

    #[test]
    fn bell() {
        let b = bell_edge(0.0, 20.0, 100.0).unwrap();
        assert!(near(b.x, 20.0) && near(b.norm, 1.0));
        assert_eq!(bell_edge(1.0, 20.0, 100.0), None);
        assert_eq!(bell_edge(-0.3, 20.0, 100.0), bell_edge(0.3, 20.0, 100.0));
        assert!(bell_edge(0.9, 20.0, 100.0).unwrap().fade < 1.0);
    }

    #[test]
    fn bell_mask_sides_mirror() {
        let (w, h) = (40, 200);
        let l = bell_mask(w, h, 20.0, 80.0, 10.0, 3.0, -1);
        let r = bell_mask(w, h, 20.0, 80.0, 10.0, 3.0, 1);
        let mid = h / 2;
        assert_eq!(l[mid * w], 255, "filled at the edge, middle");
        assert_eq!(l[mid * w + w - 1], 0, "nothing at the far side");
        for x in 0..w {
            assert_eq!(l[mid * w + x], r[mid * w + w - 1 - x]);
        }
        assert_eq!(l[0], 0, "nothing past its ends");
    }

    #[test]
    fn compose_is_premultiplied_and_shaped() {
        let (w, h) = (4, 10);
        let mut mask = vec![0u8; w * h];
        mask[5 * w] = 255;
        mask[5 * w + 1] = 128;
        let mut out = vec![0xDEADBEEFu32; w * h];
        compose(&mut out, &mask, w, h, 0, 2, 0.0, None);
        assert_eq!(out[0], 0, "no mask, no light");
        assert_eq!(out[5 * w] >> 24, 255);
        assert_eq!(out[5 * w + 1] >> 24, 128);
        for s in [16, 8, 0] {
            assert!(((out[5 * w + 1] >> s) & 0xFF) <= 128, "colour never over alpha");
        }
        compose(&mut out, &mask, w, h, 0, 2, 1.0, None);
        assert!(((out[5 * w] >> 16) & 0xFF) > 200, "filled with light: near white");
    }

    #[test]
    fn over_blends() {
        assert_eq!(over(0xFF000000, 1.0, 0xFFFFFFFF), 0xFF000000);
        assert_eq!(over(0xFF000000, 0.0, 0x80402010), 0x80402010);
    }
}
