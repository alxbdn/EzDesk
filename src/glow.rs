//! EzDesk's light, as numbers: Windows blue (#0063B1) brightening to white and back. The rails,
//! the tray number, the dock, the menus and the Settings guide all flow through this palette
//! (with Direct2D: see ui::glow_stops); the held window's border takes its colour from it too.

/// Blue to white and back.
pub const COLORS: [u32; 6] = [0x0063B1, 0x2A82CC, 0x8CC0EC, 0xFFFFFF, 0x8CC0EC, 0x2A82CC];
/// Time for the light to flow one period.
pub const FLOW_MS: u64 = 2600;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours() {
        assert_eq!(color_at(0.0, 60.0, 0.0), 0x0063B1);
        assert_eq!(color_at(30.0, 60.0, 0.0), 0xFFFFFF, "half-way is white");
        assert_eq!(color_at(60.0, 60.0, 0.0), color_at(0.0, 60.0, 0.0), "wraps");
        assert_eq!(color_at(0.0, 60.0, 3.0), 0xFFFFFF);
    }
}
