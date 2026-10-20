/// Mixes a world seed, tick, entity key, and subsystem tag into a well-distributed value.
///
/// Every random-looking simulation decision is derived from this stateless mixer, so replay does
/// not depend on a shared mutable RNG or on evaluation order.
#[must_use]
pub const fn derive(seed: u64, tick: u64, id: u64, subsystem: u64) -> u64 {
    let mut value = seed ^ tick.rotate_left(17) ^ id.rotate_left(33) ^ subsystem;
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
