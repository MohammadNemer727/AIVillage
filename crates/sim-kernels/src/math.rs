/// Returns the unit heading for a turn modulo 256, scaled by 256.
///
/// Sixteen quantized directions are intentional: they are deterministic across targets and avoid
/// libm in the authoritative simulation.
#[must_use]
pub fn heading(turns: u8) -> (i32, i32) {
    const DIRECTIONS: [(i32, i32); 16] = [
        (256, 0),
        (237, 98),
        (181, 181),
        (98, 237),
        (0, 256),
        (-98, 237),
        (-181, 181),
        (-237, 98),
        (-256, 0),
        (-237, -98),
        (-181, -181),
        (-98, -237),
        (0, -256),
        (98, -237),
        (181, -181),
        (237, -98),
    ];
    DIRECTIONS[usize::from(turns >> 4)]
}

/// Returns `floor(sqrt(value))` using only integer operations.
#[must_use]
pub const fn integer_sqrt(value: u64) -> u64 {
    let mut remainder = value;
    let mut root = 0_u64;
    let mut bit = 1_u64 << 62;
    while bit > remainder {
        bit >>= 2;
    }
    while bit != 0 {
        if remainder >= root + bit {
            remainder -= root + bit;
            root = (root >> 1) + bit;
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    root
}

/// Clamps to `[min, max]` and narrows. Callers pass `i16` bounds, so the cast is lossless.
#[allow(
    clippy::cast_possible_truncation,
    reason = "The value is clamped to i16 bounds immediately before narrowing."
)]
pub(crate) fn clamp_i16(value: i32, min: i16, max: i16) -> i16 {
    value.clamp(i32::from(min), i32::from(max)) as i16
}

/// Converts a bounded receptor amplitude, saturating instead of panicking.
#[allow(
    clippy::cast_possible_wrap,
    reason = "The value is capped at i16::MAX before the cast."
)]
pub(crate) const fn amplitude(value: u16) -> i16 {
    (if value > 0x7FFF { 0x7FFF } else { value }) as i16
}

/// Narrows an unsigned sum, saturating at `i16::MAX`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "The value is capped at i16::MAX before the cast."
)]
pub(crate) const fn saturate_u32_to_i16(value: u32) -> i16 {
    (if value > 0x7FFF { 0x7FFF } else { value }) as i16
}

/// Converts a small, bounded array index or count to `i32`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "Callers pass bounded kernel indices far below i32::MAX."
)]
pub(crate) const fn small_i32(value: usize) -> i32 {
    value as i32
}

/// Converts a small, bounded array index to `u8`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "Callers pass bounded kernel indices below 256."
)]
pub(crate) const fn small_u8(value: usize) -> u8 {
    value as u8
}

/// `i16::saturating_add`, computed in `i32` because rust-gpu lacks the saturating intrinsics.
pub(crate) fn saturating_add_i16(left: i16, right: i16) -> i16 {
    clamp_i16(i32::from(left) + i32::from(right), i16::MIN, i16::MAX)
}

/// `i16::saturating_sub`, computed in `i32` because rust-gpu lacks the saturating intrinsics.
pub(crate) fn saturating_sub_i16(left: i16, right: i16) -> i16 {
    clamp_i16(i32::from(left) - i32::from(right), i16::MIN, i16::MAX)
}

#[cfg(test)]
mod tests {
    use super::{clamp_i16, heading, integer_sqrt};

    #[test]
    fn integer_sqrt_floors() {
        for (value, root) in [
            (0, 0),
            (1, 1),
            (3, 1),
            (4, 2),
            (99, 9),
            (100, 10),
            (u64::MAX, 4_294_967_295),
        ] {
            assert_eq!(integer_sqrt(value), root);
        }
    }

    #[test]
    fn heading_quantizes_to_sixteen_directions() {
        assert_eq!(heading(0), (256, 0));
        assert_eq!(heading(15), (256, 0));
        assert_eq!(heading(64), (0, 256));
        assert_eq!(heading(255), (237, -98));
    }

    #[test]
    fn clamp_i16_saturates_to_bounds() {
        assert_eq!(clamp_i16(i32::MAX, -512, 512), 512);
        assert_eq!(clamp_i16(i32::MIN, -512, 512), -512);
        assert_eq!(clamp_i16(-7, -512, 512), -7);
    }
}
