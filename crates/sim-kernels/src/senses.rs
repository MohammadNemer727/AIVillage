//! Raw V2 receptor frames and receptor-level transforms.

use crate::hash::derive;

/// Number of fixed retinal samples exposed by the V2 raw sensory interface.
pub const RETINAL_RAY_COUNT: usize = 16;
/// Number of overlapping, adapting V2 chemical receptors.
pub const CHEMICAL_RECEPTOR_COUNT: usize = 4;

/// One broad-band sample from a retinal ray.
///
/// These are receptor amplitudes, not world facts: no identity, entity class, position,
/// distance, edibility, or material label is present.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct RetinalSample {
    /// Broad red-sensitive intensity.
    pub red: u16,
    /// Broad green-sensitive intensity.
    pub green: u16,
    /// Broad blue-sensitive intensity.
    pub blue: u16,
    /// Ambient/local luminance.
    pub brightness: u16,
    /// Signed apparent image motion, bounded to receptor range.
    pub motion: i16,
    /// Light blocked before the range limit, zero when unobstructed.
    pub occlusion: u16,
}

/// Raw V2 receptor frame. It intentionally contains only bounded receptor amplitudes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RawSensoryFrame {
    /// Fixed-geometry broad-band retinal amplitudes.
    pub retinal: [RetinalSample; RETINAL_RAY_COUNT],
    /// Overlapping, adapting local chemical concentrations.
    pub chemicals: [u16; CHEMICAL_RECEPTOR_COUNT],
    /// Local vibration amplitude from physical acoustic emissions, with range and masking.
    pub vibration: u16,
    /// Contact amplitude from zero to the receptor bound.
    pub touch: u16,
    /// Joint-angle proprioceptive amplitude.
    pub joint_position: u16,
    /// Muscle-tension proprioceptive amplitude.
    pub muscle_tension: u16,
    /// Signed body acceleration amplitude.
    pub acceleration: i16,
    /// Body orientation encoded as a bounded angular amplitude.
    pub orientation: u16,
    /// Noisy metabolic-energy interoceptive amplitude.
    pub energy: u16,
    /// Noisy gut-pressure interoceptive amplitude.
    pub gut_pressure: u16,
    /// Noisy toxin-stress interoceptive amplitude.
    pub toxin_stress: u16,
    /// Noisy hydration interoceptive amplitude.
    pub hydration: u16,
    /// Noisy fatigue interoceptive amplitude.
    pub fatigue: u16,
    /// Noisy damage interoceptive amplitude.
    pub damage: u16,
}

impl RawSensoryFrame {
    /// A frame with every receptor silent.
    pub const EMPTY: Self = Self {
        retinal: [RetinalSample {
            red: 0,
            green: 0,
            blue: 0,
            brightness: 0,
            motion: 0,
            occlusion: 0,
        }; RETINAL_RAY_COUNT],
        chemicals: [0; CHEMICAL_RECEPTOR_COUNT],
        vibration: 0,
        touch: 0,
        joint_position: 0,
        muscle_tension: 0,
        acceleration: 0,
        orientation: 0,
        energy: 0,
        gut_pressure: 0,
        toxin_stress: 0,
        hydration: 0,
        fatigue: 0,
        damage: 0,
    };
}

/// Adds deterministic ±4 receptor noise to an interoceptive store and bounds it to `0..=255`.
///
/// Noise is keyed by world seed, tick, the sensing agent's cell, and a per-receptor tag.
#[must_use]
pub fn noisy_receptor(value: u16, seed: u64, tick: u64, cell: u64, tag: u64) -> u16 {
    let noise = i32::try_from(derive(seed, tick, cell, tag) % 9).unwrap_or(0) - 4;
    let bounded = (i32::from(value) + noise).clamp(0, 255);
    u16::try_from(bounded).unwrap_or(u16::MAX)
}
