//! Plain-old-data buffer layouts for running kernels on a GPU.
//!
//! Every field is a 32-bit scalar or an array of them, so `#[repr(C)]` and the std430 storage
//! buffer layout agree byte for byte with no padding. Packing widens kernel values losslessly;
//! unpacking narrows them back, so a pack/unpack round trip is the identity on any state the
//! kernels can produce. Host and shader share these conversions, keeping one source of truth.

use crate::{
    brain::{
        BrainConfig, BrainStep, CONTEXT_CHANNELS, CONTINGENCY_ACTIONS, CONTINGENCY_FEATURES,
        EPISODE_CAPACITY, EpisodicTrace, NEURONS, PredictiveBrain,
    },
    controller::OUTPUTS,
    senses::{CHEMICAL_RECEPTOR_COUNT, RETINAL_RAY_COUNT, RawSensoryFrame, RetinalSample},
};

/// Workgroup width shared by the shader entry points and the host dispatch.
pub const WORKGROUP_SIZE: u32 = 64;

/// GPU layout of [`EpisodicTrace`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "bytemuck", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct GpuEpisode {
    /// See [`EpisodicTrace::context`].
    pub context: [i32; CONTEXT_CHANNELS],
    /// See [`EpisodicTrace::subsequent_change`].
    pub subsequent_change: [i32; CONTEXT_CHANNELS],
    /// See [`EpisodicTrace::action`].
    pub action: u32,
    /// See [`EpisodicTrace::retention`].
    pub retention: u32,
}

/// GPU layout of [`PredictiveBrain`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "bytemuck", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct GpuBrain {
    /// See [`PredictiveBrain::activity`].
    pub activity: [i32; NEURONS],
    /// See [`PredictiveBrain::eligibility`].
    pub eligibility: [i32; NEURONS],
    /// See [`PredictiveBrain::learned_bias`].
    pub learned_bias: [i32; NEURONS],
    /// See [`PredictiveBrain::prediction`].
    pub prediction: [i32; CONTEXT_CHANNELS],
    /// See [`PredictiveBrain::previous_context`].
    pub previous_context: [i32; CONTEXT_CHANNELS],
    /// See [`PredictiveBrain::episodes`].
    pub episodes: [GpuEpisode; EPISODE_CAPACITY],
    /// See [`PredictiveBrain::contingencies`].
    pub contingencies: [[i32; CONTINGENCY_FEATURES]; CONTINGENCY_ACTIONS],
    /// See [`PredictiveBrain::next_episode`].
    pub next_episode: u32,
}

/// GPU layout of [`RawSensoryFrame`]. Each ray is `[red, green, blue, brightness, motion,
/// occlusion]`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "bytemuck", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct GpuFrame {
    /// Retinal samples in [`RetinalSample`] field order.
    pub retinal: [[i32; 6]; RETINAL_RAY_COUNT],
    /// See [`RawSensoryFrame::chemicals`].
    pub chemicals: [u32; CHEMICAL_RECEPTOR_COUNT],
    /// Remaining scalar receptors in [`RawSensoryFrame`] field order, starting at `vibration`.
    pub scalars: [i32; 12],
}

/// GPU layout of [`BrainConfig`]; the four switches are packed as bit flags.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "bytemuck", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct GpuBrainConfig {
    /// See [`BrainConfig::connection_stride`].
    pub connection_stride: u32,
    /// See [`BrainConfig::plasticity_scale`].
    pub plasticity_scale: i32,
    /// Bit 0 plasticity, bit 1 episodic memory, bit 2 action contingency, bit 3 prediction.
    pub flags: u32,
}

/// GPU layout of [`BrainStep`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "bytemuck", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct GpuBrainStep {
    /// See [`BrainStep::outputs`].
    pub outputs: [i32; OUTPUTS],
    /// One when [`BrainStep::learned`], otherwise zero.
    pub learned: u32,
}

/// Narrows a value that was widened from `i16` by [`GpuBrain::pack`] or similar.
#[allow(
    clippy::cast_possible_truncation,
    reason = "Packed values originate from i16 and round-trip exactly."
)]
const fn narrow_i16(value: i32) -> i16 {
    value as i16
}

/// Narrows a value that was widened from `u16`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Packed values originate from u16 and round-trip exactly."
)]
const fn narrow_u16(value: i32) -> u16 {
    value as u16
}

fn widen<const N: usize>(values: &[i16; N]) -> [i32; N] {
    let mut wide = [0; N];
    for index in 0..N {
        wide[index] = i32::from(values[index]);
    }
    wide
}

fn narrow<const N: usize>(values: &[i32; N]) -> [i16; N] {
    let mut narrow = [0; N];
    for index in 0..N {
        narrow[index] = narrow_i16(values[index]);
    }
    narrow
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "Kernel indices and strides are far below u32::MAX."
)]
const fn small_u32(value: usize) -> u32 {
    value as u32
}

impl GpuBrain {
    /// Packs kernel brain state for upload.
    #[must_use]
    pub fn pack(brain: &PredictiveBrain) -> Self {
        let mut episodes = [GpuEpisode {
            context: [0; CONTEXT_CHANNELS],
            subsequent_change: [0; CONTEXT_CHANNELS],
            action: 0,
            retention: 0,
        }; EPISODE_CAPACITY];
        for index in 0..EPISODE_CAPACITY {
            let episode = &brain.episodes[index];
            episodes[index] = GpuEpisode {
                context: widen(&episode.context),
                subsequent_change: widen(&episode.subsequent_change),
                action: u32::from(episode.action),
                retention: u32::from(episode.retention),
            };
        }
        let mut contingencies = [[0; CONTINGENCY_FEATURES]; CONTINGENCY_ACTIONS];
        for action in 0..CONTINGENCY_ACTIONS {
            contingencies[action] = widen(&brain.contingencies[action]);
        }
        Self {
            activity: widen(&brain.activity),
            eligibility: widen(&brain.eligibility),
            learned_bias: widen(&brain.learned_bias),
            prediction: widen(&brain.prediction),
            previous_context: widen(&brain.previous_context),
            episodes,
            contingencies,
            next_episode: small_u32(brain.next_episode),
        }
    }

    /// Restores kernel brain state from a packed buffer element.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "Episode actions and retention were widened from u8 and u16."
    )]
    pub fn unpack(&self) -> PredictiveBrain {
        let mut episodes = [EpisodicTrace {
            context: [0; CONTEXT_CHANNELS],
            action: 0,
            subsequent_change: [0; CONTEXT_CHANNELS],
            retention: 0,
        }; EPISODE_CAPACITY];
        for index in 0..EPISODE_CAPACITY {
            let packed = &self.episodes[index];
            episodes[index] = EpisodicTrace {
                context: narrow(&packed.context),
                action: packed.action as u8,
                subsequent_change: narrow(&packed.subsequent_change),
                retention: packed.retention as u16,
            };
        }
        let mut contingencies = [[0; CONTINGENCY_FEATURES]; CONTINGENCY_ACTIONS];
        for action in 0..CONTINGENCY_ACTIONS {
            contingencies[action] = narrow(&self.contingencies[action]);
        }
        PredictiveBrain {
            activity: narrow(&self.activity),
            eligibility: narrow(&self.eligibility),
            learned_bias: narrow(&self.learned_bias),
            prediction: narrow(&self.prediction),
            previous_context: narrow(&self.previous_context),
            episodes,
            next_episode: self.next_episode as usize,
            contingencies,
        }
    }
}

impl GpuFrame {
    /// Packs a receptor frame for upload.
    #[must_use]
    pub fn pack(frame: &RawSensoryFrame) -> Self {
        let mut retinal = [[0; 6]; RETINAL_RAY_COUNT];
        for index in 0..RETINAL_RAY_COUNT {
            let ray = &frame.retinal[index];
            retinal[index] = [
                i32::from(ray.red),
                i32::from(ray.green),
                i32::from(ray.blue),
                i32::from(ray.brightness),
                i32::from(ray.motion),
                i32::from(ray.occlusion),
            ];
        }
        let mut chemicals = [0; CHEMICAL_RECEPTOR_COUNT];
        for index in 0..CHEMICAL_RECEPTOR_COUNT {
            chemicals[index] = u32::from(frame.chemicals[index]);
        }
        Self {
            retinal,
            chemicals,
            scalars: [
                i32::from(frame.vibration),
                i32::from(frame.touch),
                i32::from(frame.joint_position),
                i32::from(frame.muscle_tension),
                i32::from(frame.acceleration),
                i32::from(frame.orientation),
                i32::from(frame.energy),
                i32::from(frame.gut_pressure),
                i32::from(frame.toxin_stress),
                i32::from(frame.hydration),
                i32::from(frame.fatigue),
                i32::from(frame.damage),
            ],
        }
    }

    /// Restores a receptor frame from a packed buffer element.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "Chemical amplitudes were widened from u16."
    )]
    pub fn unpack(&self) -> RawSensoryFrame {
        let mut frame = RawSensoryFrame::EMPTY;
        for index in 0..RETINAL_RAY_COUNT {
            let packed = &self.retinal[index];
            frame.retinal[index] = RetinalSample {
                red: narrow_u16(packed[0]),
                green: narrow_u16(packed[1]),
                blue: narrow_u16(packed[2]),
                brightness: narrow_u16(packed[3]),
                motion: narrow_i16(packed[4]),
                occlusion: narrow_u16(packed[5]),
            };
        }
        for index in 0..CHEMICAL_RECEPTOR_COUNT {
            frame.chemicals[index] = self.chemicals[index] as u16;
        }
        let scalars = &self.scalars;
        frame.vibration = narrow_u16(scalars[0]);
        frame.touch = narrow_u16(scalars[1]);
        frame.joint_position = narrow_u16(scalars[2]);
        frame.muscle_tension = narrow_u16(scalars[3]);
        frame.acceleration = narrow_i16(scalars[4]);
        frame.orientation = narrow_u16(scalars[5]);
        frame.energy = narrow_u16(scalars[6]);
        frame.gut_pressure = narrow_u16(scalars[7]);
        frame.toxin_stress = narrow_u16(scalars[8]);
        frame.hydration = narrow_u16(scalars[9]);
        frame.fatigue = narrow_u16(scalars[10]);
        frame.damage = narrow_u16(scalars[11]);
        frame
    }
}

impl GpuBrainConfig {
    /// Packs per-agent brain parameters for upload.
    #[must_use]
    pub fn pack(config: &BrainConfig) -> Self {
        Self {
            connection_stride: small_u32(config.connection_stride),
            plasticity_scale: config.plasticity_scale,
            flags: u32::from(config.plasticity_enabled)
                | u32::from(config.episodic_memory_enabled) << 1
                | u32::from(config.action_contingency_enabled) << 2
                | u32::from(config.prediction_enabled) << 3,
        }
    }

    /// Restores per-agent brain parameters.
    #[must_use]
    pub const fn unpack(&self) -> BrainConfig {
        BrainConfig {
            connection_stride: self.connection_stride as usize,
            plasticity_scale: self.plasticity_scale,
            plasticity_enabled: self.flags & 1 != 0,
            episodic_memory_enabled: self.flags & 2 != 0,
            action_contingency_enabled: self.flags & 4 != 0,
            prediction_enabled: self.flags & 8 != 0,
        }
    }
}

impl GpuBrainStep {
    /// Packs one brain step result.
    #[must_use]
    pub fn pack(step: &BrainStep) -> Self {
        Self {
            outputs: widen(&step.outputs),
            learned: u32::from(step.learned),
        }
    }

    /// Restores one brain step result.
    #[must_use]
    pub fn unpack(&self) -> BrainStep {
        BrainStep {
            outputs: narrow(&self.outputs),
            learned: self.learned != 0,
        }
    }
}

/// Runs one brain step on packed buffers. This is the exact body of the GPU entry point, so the
/// host can execute it too and confirm both paths agree.
pub fn brain_step_packed(
    brain: &mut GpuBrain,
    frame: &GpuFrame,
    config: &GpuBrainConfig,
) -> GpuBrainStep {
    let mut state = brain.unpack();
    let step = state.step(&config.unpack(), &frame.unpack());
    *brain = GpuBrain::pack(&state);
    GpuBrainStep::pack(&step)
}

#[cfg(test)]
mod tests {
    use super::{GpuBrain, GpuBrainConfig, GpuFrame, brain_step_packed};
    use crate::{
        brain::{BrainConfig, PredictiveBrain},
        senses::RawSensoryFrame,
    };

    const CONFIG: BrainConfig = BrainConfig {
        connection_stride: 5,
        plasticity_scale: 3,
        plasticity_enabled: true,
        episodic_memory_enabled: true,
        action_contingency_enabled: false,
        prediction_enabled: true,
    };

    fn frame(seed: u16) -> RawSensoryFrame {
        let mut frame = RawSensoryFrame {
            energy: seed % 200,
            damage: (seed * 7) % 90,
            toxin_stress: seed % 17,
            acceleration: -i16::try_from(seed % 50).unwrap(),
            ..RawSensoryFrame::EMPTY
        };
        for (ray, sample) in frame.retinal.iter_mut().enumerate() {
            let ray = u16::try_from(ray).unwrap();
            sample.brightness = (seed + ray * 13) % 255;
            sample.motion = i16::try_from((seed + ray) % 40).unwrap() - 20;
        }
        frame
    }

    #[test]
    fn packing_round_trips() {
        let mut brain = PredictiveBrain::default();
        for seed in 0..20 {
            let _ = brain.step(&CONFIG, &frame(seed));
            assert_eq!(GpuBrain::pack(&brain).unpack(), brain);
            assert_eq!(GpuFrame::pack(&frame(seed)).unpack(), frame(seed));
        }
        assert_eq!(GpuBrainConfig::pack(&CONFIG).unpack(), CONFIG);
    }

    #[test]
    fn packed_step_matches_direct_step() {
        let mut direct = PredictiveBrain::default();
        let mut packed = GpuBrain::pack(&direct);
        for seed in 0..50 {
            let expected = direct.step(&CONFIG, &frame(seed));
            let actual = brain_step_packed(
                &mut packed,
                &GpuFrame::pack(&frame(seed)),
                &GpuBrainConfig::pack(&CONFIG),
            );
            assert_eq!(actual.unpack(), expected);
            assert_eq!(packed.unpack(), direct);
        }
    }
}
