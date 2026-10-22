//! Bounded V2 predictive brain: sparse recurrent rates with only local, endogenous learning.

use crate::{
    controller::OUTPUTS,
    math::{
        amplitude, clamp_i16, saturate_u32_to_i16, saturating_add_i16, saturating_sub_i16,
        small_i32, small_u8,
    },
    senses::{CHEMICAL_RECEPTOR_COUNT, RETINAL_RAY_COUNT, RawSensoryFrame},
};

/// Baseline V2 brain size. The controller deliberately stays below the first-cycle 128-neuron
/// ceiling so it remains feasible on the target laptop.
pub const NEURONS: usize = RECEPTORS + INTERNEURONS + MOTORS + MODULATORS;
/// Receptor population, driven directly by the compressed sensory context.
pub const RECEPTORS: usize = 16;
/// Recurrent interneuron population.
pub const INTERNEURONS: usize = 32;
/// Motor population; its rates become the first controller outputs.
pub const MOTORS: usize = 8;
/// Modulatory population driven by damage, metabolic improvement, and surprise.
pub const MODULATORS: usize = 8;
/// Ring-buffer capacity for lifetime episodic traces.
pub const EPISODE_CAPACITY: usize = 8;
/// Number of motor-action bins tracked by the contingency table.
pub const CONTINGENCY_ACTIONS: usize = 4;
/// Number of compressed context channels tracked per contingency row.
pub const CONTINGENCY_FEATURES: usize = 4;
/// Raw toxin-stress amplitude at which the swallowing reflex is withdrawn.
pub const TOXIN_WITHDRAWAL_THRESHOLD: u16 = 12;

const MOTOR_START: usize = RECEPTORS + INTERNEURONS;
/// Sparse recurrent input offsets for each interneuron, before the developmental stride.
const RECURRENT_OFFSETS: [usize; 4] = [1, 7, 13, 29];
const MODULATOR_START: usize = MOTOR_START + MOTORS;
/// Width of the compressed sensory context the brain predicts and records.
pub const CONTEXT_CHANNELS: usize = 4;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "The context width is a small compile-time constant."
)]
const CONTEXT_DIVISOR: i32 = CONTEXT_CHANNELS as i32;
#[allow(
    clippy::manual_midpoint,
    reason = "Operands are widened i16 rates, and this arithmetic must remain rust-gpu compatible."
)]
fn mean_two(left: i32, right: i32) -> i32 {
    (left + right) / 2
}
#[allow(
    clippy::cast_possible_truncation,
    reason = "Fixed sensor geometry is a small compile-time constant."
)]
const VISION_DIVISOR: u32 = (RETINAL_RAY_COUNT * 2) as u32;
#[allow(
    clippy::cast_possible_truncation,
    reason = "Fixed sensor geometry is a small compile-time constant."
)]
const CHEMISTRY_DIVISOR: u32 = CHEMICAL_RECEPTOR_COUNT as u32;

/// A deliberately compact, non-inherited V2 episodic trace.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct EpisodicTrace {
    /// Compressed receptor/interoceptive context.
    pub context: [i16; CONTEXT_CHANNELS],
    /// The motor action bin active when the episode was recorded.
    pub action: u8,
    /// Subsequent compressed sensory/physiological change.
    pub subsequent_change: [i16; CONTEXT_CHANNELS],
    /// Remaining deterministic retention ticks.
    pub retention: u16,
}

/// Mutable V2 lifetime brain state. It is reset at birth and intentionally has no genome field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PredictiveBrain {
    /// Rate-coded activity for receptor, interneuron, motor, then modulatory populations.
    pub activity: [i16; NEURONS],
    /// Decaying per-neuron eligibility traces.
    pub eligibility: [i16; NEURONS],
    /// The only learned parameters: bounded per-neuron biases.
    pub learned_bias: [i16; NEURONS],
    /// Next-sensation prediction, compressed to the context channels.
    pub prediction: [i16; CONTEXT_CHANNELS],
    /// Context observed on the previous step.
    pub previous_context: [i16; CONTEXT_CHANNELS],
    /// Capacity-limited, decaying lifetime episodes.
    pub episodes: [EpisodicTrace; EPISODE_CAPACITY],
    /// Ring-buffer slot the next episode overwrites.
    pub next_episode: usize,
    /// Action-conditioned (rows) contingency estimates (columns).
    pub contingencies: [[i16; CONTINGENCY_FEATURES]; CONTINGENCY_ACTIONS],
}

impl Default for PredictiveBrain {
    fn default() -> Self {
        Self {
            activity: [0; NEURONS],
            eligibility: [0; NEURONS],
            learned_bias: [0; NEURONS],
            prediction: [0; CONTEXT_CHANNELS],
            previous_context: [0; CONTEXT_CHANNELS],
            episodes: [EpisodicTrace::default(); EPISODE_CAPACITY],
            next_episode: 0,
            contingencies: [[0; CONTINGENCY_FEATURES]; CONTINGENCY_ACTIONS],
        }
    }
}

/// Per-agent parameters and experiment switches for one brain step.
///
/// The caller derives these from the developmental genome and world controls, so the kernel
/// never sees genomes or world state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Each switch is an independent experimental ablation."
)]
pub struct BrainConfig {
    /// Extra offset applied to the four sparse recurrent inputs, at least one.
    pub connection_stride: usize,
    /// Multiplier on the learned-bias update, at least one.
    pub plasticity_scale: i32,
    /// Whether the learned-bias update may run.
    pub plasticity_enabled: bool,
    /// Whether episodes are recorded and decayed.
    pub episodic_memory_enabled: bool,
    /// Whether the action-contingency table is updated.
    pub action_contingency_enabled: bool,
    /// Whether the prediction tracks the latest context.
    pub prediction_enabled: bool,
}

/// Result of one brain step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrainStep {
    /// Controller-width motor outputs, including reflex overrides.
    pub outputs: [i16; OUTPUTS],
    /// True when the learned-bias update ran; callers count these as learning updates.
    pub learned: bool,
}

/// Endogenous third-factor signals for one step; never an external reward.
#[derive(Clone, Copy)]
struct EndogenousEvents {
    surprise: i32,
    metabolic_improvement: bool,
    damage: bool,
}

impl EndogenousEvents {
    /// Learning-rate gate: damage counts double, then metabolic improvement and high surprise.
    fn gate(self) -> i32 {
        i32::from(self.damage) * 2
            + i32::from(self.metabolic_improvement)
            + i32::from(self.surprise > 24)
    }
}

impl PredictiveBrain {
    /// Returns the seven independent controller readouts used for blank signal outputs.
    ///
    /// Kept as a fixed-array indexed projection so the same implementation compiles for SPIR-V.
    #[must_use]
    pub fn signal_outputs(&self) -> [i16; OUTPUTS - MOTORS] {
        let damage = i32::from(self.activity[MODULATOR_START]);
        let metabolic = i32::from(self.activity[MODULATOR_START + 1]);
        let surprise = i32::from(self.activity[MODULATOR_START + 2]);
        [
            clamp_i16(surprise, i16::MIN, i16::MAX),
            clamp_i16(damage, i16::MIN, i16::MAX),
            clamp_i16(metabolic, i16::MIN, i16::MAX),
            clamp_i16(mean_two(damage, surprise), i16::MIN, i16::MAX),
            clamp_i16(mean_two(metabolic, surprise), i16::MIN, i16::MAX),
            clamp_i16(mean_two(damage, metabolic), i16::MIN, i16::MAX),
            clamp_i16((damage + metabolic + surprise) / 3, i16::MIN, i16::MAX),
        ]
    }

    /// Advances the brain by one tick from a raw receptor frame.
    #[must_use]
    pub fn step(&mut self, config: &BrainConfig, frame: &RawSensoryFrame) -> BrainStep {
        let context = compressed_brain_context(frame);
        let previous = self.activity;
        self.update_rates(&previous, context, config.connection_stride);
        let events = self.detect_events(context);
        self.update_modulators(&previous, events);
        let learned = self.learn(events, config);
        self.record_outcome(context, config);

        let mut outputs = [0; OUTPUTS];
        for motor in 0..MOTORS {
            outputs[motor] = self.activity[MOTOR_START + motor];
        }
        // The seven previously unwritten blank-signal channels are distinct controller
        // projections of the three endogenous event populations. This preserves every existing
        // actuator and checkpoint/GPU layout without aliases or external schedules.
        let signals = self.signal_outputs();
        let mut signal = 0;
        while signal < signals.len() {
            outputs[MOTORS + signal] = signals[signal];
            signal += 1;
        }
        apply_reflexes(&mut outputs, frame);
        BrainStep { outputs, learned }
    }

    /// Updates receptor, interneuron, then motor rates from the previous tick's activity.
    fn update_rates(
        &mut self,
        previous: &[i16; NEURONS],
        context: [i16; CONTEXT_CHANNELS],
        connection_stride: usize,
    ) {
        for node in 0..RECEPTORS {
            self.activity[node] = context[node % CONTEXT_CHANNELS].clamp(-512, 512);
        }
        for node in RECEPTORS..MOTOR_START {
            let local = node - RECEPTORS;
            let mut sum = i32::from(self.learned_bias[node]);
            sum += i32::from(self.activity[local % RECEPTORS]);
            // Four deterministic local recurrent inputs: no dense matrix allocation or traversal.
            for input in 0..RECURRENT_OFFSETS.len() {
                let offset = RECURRENT_OFFSETS[input] + connection_stride;
                sum += i32::from(previous[(node + offset) % NEURONS]) / 4;
            }
            let time_constant = 1 + small_i32(local % 4);
            let target = sum.clamp(-1_024, 1_024);
            self.activity[node] = clamp_i16(
                (i32::from(previous[node]) * (time_constant - 1) + target) / time_constant,
                i16::MIN,
                i16::MAX,
            );
        }
        for node in MOTOR_START..MODULATOR_START {
            let local = node - MOTOR_START;
            let sum = i32::from(self.learned_bias[node])
                + i32::from(self.activity[RECEPTORS + (local * 5) % INTERNEURONS])
                + i32::from(previous[(node + 11) % NEURONS]) / 3;
            self.activity[node] = clamp_i16(sum, -512, 512);
        }
    }

    /// Compares the new context with the prediction and previous context.
    fn detect_events(&self, context: [i16; CONTEXT_CHANNELS]) -> EndogenousEvents {
        let mut total_error = 0;
        for channel in 0..CONTEXT_CHANNELS {
            total_error +=
                (i32::from(self.prediction[channel]) - i32::from(context[channel])).abs();
        }
        EndogenousEvents {
            surprise: total_error / CONTEXT_DIVISOR,
            metabolic_improvement: context[2] > self.previous_context[2],
            damage: context[3] > self.previous_context[3],
        }
    }

    fn update_modulators(&mut self, previous: &[i16; NEURONS], events: EndogenousEvents) {
        for node in MODULATOR_START..NEURONS {
            let event_input = match (node - MODULATOR_START) % 3 {
                0 => i32::from(events.damage) * 256,
                1 => i32::from(events.metabolic_improvement) * 256,
                _ => events.surprise.clamp(0, 256),
            };
            self.activity[node] = clamp_i16(
                i32::from(previous[node]) * 3 / 4
                    + event_input
                    + i32::from(self.learned_bias[node]),
                -512,
                512,
            );
        }
    }

    /// Decays eligibility traces and, when gated, applies the learned-bias update.
    fn learn(&mut self, events: EndogenousEvents, config: &BrainConfig) -> bool {
        for node in 0..NEURONS {
            self.eligibility[node] = clamp_i16(
                i32::from(self.eligibility[node]) * 7 / 8 + i32::from(self.activity[node]) / 32,
                -128,
                128,
            );
        }
        // This is the only learned-weight update. Its third factor is exclusively damage,
        // metabolic improvement, or local prediction surprise; no external reward enters here.
        let gate = events.gate();
        if !config.plasticity_enabled || gate == 0 {
            return false;
        }
        for node in 0..NEURONS {
            self.learned_bias[node] = clamp_i16(
                i32::from(self.learned_bias[node])
                    + i32::from(self.eligibility[node]) * gate * config.plasticity_scale / 32,
                -96,
                96,
            );
        }
        true
    }

    /// Records the emitted action and observed change into episodic memory, contingencies,
    /// and prediction.
    fn record_outcome(&mut self, context: [i16; CONTEXT_CHANNELS], config: &BrainConfig) {
        // The most active motor wins; ties go to the lowest index.
        let mut winner = 0;
        for motor in 1..MOTORS {
            if self.activity[MOTOR_START + motor] > self.activity[MOTOR_START + winner] {
                winner = motor;
            }
        }
        let action = winner % CONTINGENCY_ACTIONS;
        let mut change = [0; CONTEXT_CHANNELS];
        for channel in 0..CONTEXT_CHANNELS {
            change[channel] = clamp_i16(
                i32::from(context[channel]) - i32::from(self.previous_context[channel]),
                -255,
                255,
            );
        }
        if config.episodic_memory_enabled {
            self.episodes[self.next_episode] = EpisodicTrace {
                context: self.previous_context,
                action: small_u8(action),
                subsequent_change: change,
                retention: 256,
            };
            self.next_episode = (self.next_episode + 1) % EPISODE_CAPACITY;
            for episode in 0..EPISODE_CAPACITY {
                let retention = self.episodes[episode].retention;
                self.episodes[episode].retention = if retention > 0 { retention - 1 } else { 0 };
            }
        }
        if config.action_contingency_enabled {
            self.update_action_contingency(action, change);
        }
        if config.prediction_enabled {
            self.prediction = context;
        }
        self.previous_context = context;
    }

    /// Updates only the row associated with an emitted motor-action bin.
    ///
    /// Keeping this local update separate makes action-versus-observation and
    /// contingency-degradation controls testable without granting agents any labeled outcome.
    pub fn update_action_contingency(
        &mut self,
        action: usize,
        observed_change: [i16; CONTINGENCY_FEATURES],
    ) {
        for feature in 0..CONTINGENCY_FEATURES {
            self.contingencies[action][feature] = clamp_i16(
                i32::from(self.contingencies[action][feature]) * 7 / 8
                    + i32::from(observed_change[feature]) / 8,
                -255,
                255,
            );
        }
    }
}

/// Interoception-gated founder reflexes applied on top of learned motor output.
fn apply_reflexes(outputs: &mut [i16; OUTPUTS], frame: &RawSensoryFrame) {
    // Founders retain only a physical swallowing reflex while their raw gut-pressure
    // receptor is low. This is not an edible label or fitness schedule; local chemistry
    // may carry delayed toxin and still enters the predictive brain unchanged.
    if frame.toxin_stress >= TOXIN_WITHDRAWAL_THRESHOLD {
        // Withdrawal overrides arbitrary untrained motor activity until raw interoception
        // recovers; this keeps founders from repeatedly swallowing one toxic patch.
        outputs[4] = 0;
    } else if frame.gut_pressure < 96 {
        outputs[4] = outputs[4].max(160);
    }
    // A low-amplitude forward reflex lets non-learning founders sample space. It is gated
    // only by noisy interoception and leaves turning and long-term action selection to the
    // inherited predictive controller.
    if frame.energy < 96 {
        outputs[1] = outputs[1].max(48);
    }
    if frame.toxin_stress >= 5 {
        outputs[3] = outputs[3].max(160);
    }
}

/// Compresses only receptor amplitudes into the brain's local predictive-learning channels:
/// mean vision, mean chemistry, energy minus gut pressure, and damage plus toxin stress.
#[must_use]
pub fn compressed_brain_context(frame: &RawSensoryFrame) -> [i16; CONTEXT_CHANNELS] {
    let mut vision = 0_u32;
    for ray in 0..RETINAL_RAY_COUNT {
        vision += u32::from(frame.retinal[ray].brightness) + u32::from(frame.retinal[ray].green);
    }
    let mut chemistry = 0_u32;
    for receptor in 0..CHEMICAL_RECEPTOR_COUNT {
        chemistry += u32::from(frame.chemicals[receptor]);
    }
    [
        saturate_u32_to_i16(vision / VISION_DIVISOR),
        saturate_u32_to_i16(chemistry / CHEMISTRY_DIVISOR),
        saturating_sub_i16(amplitude(frame.energy), amplitude(frame.gut_pressure)),
        saturating_add_i16(amplitude(frame.damage), amplitude(frame.toxin_stress)),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        BrainConfig, CONTINGENCY_FEATURES, MODULATOR_START, MOTOR_START, PredictiveBrain,
        compressed_brain_context,
    };
    use crate::senses::RawSensoryFrame;

    const ALL_ENABLED: BrainConfig = BrainConfig {
        connection_stride: 1,
        plasticity_scale: 1,
        plasticity_enabled: true,
        episodic_memory_enabled: true,
        action_contingency_enabled: true,
        prediction_enabled: true,
    };

    fn frame(energy: u16, damage: u16) -> RawSensoryFrame {
        RawSensoryFrame {
            energy,
            damage,
            ..RawSensoryFrame::EMPTY
        }
    }

    #[test]
    fn prediction_tracks_context_unless_ablated() {
        let input = frame(80, 30);
        let mut brain = PredictiveBrain::default();
        let _ = brain.step(&ALL_ENABLED, &input);
        assert_eq!(brain.prediction, compressed_brain_context(&input));

        let mut ablated = PredictiveBrain::default();
        let config = BrainConfig {
            prediction_enabled: false,
            ..ALL_ENABLED
        };
        let _ = ablated.step(&config, &input);
        assert_eq!(ablated.prediction, [0; 4]);
    }

    #[test]
    fn learning_requires_an_endogenous_event_and_plasticity() {
        let mut brain = PredictiveBrain::default();
        assert!(!brain.step(&ALL_ENABLED, &frame(0, 0)).learned);
        assert!(brain.step(&ALL_ENABLED, &frame(0, 90)).learned);

        let mut frozen = PredictiveBrain::default();
        let config = BrainConfig {
            plasticity_enabled: false,
            ..ALL_ENABLED
        };
        let _ = frozen.step(&config, &frame(0, 0));
        assert!(!frozen.step(&config, &frame(0, 90)).learned);
        assert_eq!(frozen.learned_bias, [0; super::NEURONS]);
    }

    #[test]
    fn contingency_degrades_without_reinforcement() {
        let mut brain = PredictiveBrain::default();
        brain.update_action_contingency(2, [96, 0, -96, 0]);
        assert!(brain.contingencies[2].iter().any(|value| *value != 0));
        assert!(brain.contingencies[1].iter().all(|value| *value == 0));
        let strength = |brain: &PredictiveBrain| -> i32 {
            brain.contingencies[2]
                .iter()
                .map(|value| i32::from(value.unsigned_abs()))
                .sum()
        };
        let learned = strength(&brain);
        for _ in 0..16 {
            brain.update_action_contingency(2, [0; CONTINGENCY_FEATURES]);
        }
        assert!(strength(&brain) < learned);
    }

    #[test]
    fn toxin_withdrawal_overrides_swallowing_reflex() {
        let mut brain = PredictiveBrain::default();
        let calm = brain.step(&ALL_ENABLED, &RawSensoryFrame::EMPTY);
        assert!(calm.outputs[4] >= 160);
        let toxic = RawSensoryFrame {
            toxin_stress: super::TOXIN_WITHDRAWAL_THRESHOLD,
            ..RawSensoryFrame::EMPTY
        };
        assert_eq!(brain.step(&ALL_ENABLED, &toxic).outputs[4], 0);
    }

    #[test]
    fn reproduction_signal_is_a_natural_independent_controller_readout() {
        let mut brain = PredictiveBrain::default();
        let calm = brain.step(&ALL_ENABLED, &frame(0, 0));
        assert_eq!(calm.outputs[8], 0);

        let damaged = brain.step(&ALL_ENABLED, &frame(120, 90));
        assert!(damaged.outputs[8] > 0);
        assert_eq!(damaged.outputs[8], brain.activity[MODULATOR_START + 2]);
        assert_ne!(damaged.outputs[8], brain.activity[MOTOR_START]);

        let signal = damaged.outputs[8];
        brain.learned_bias[MOTOR_START] = -96;
        let next = brain.step(&ALL_ENABLED, &frame(120, 90));
        assert_ne!(next.outputs[0], damaged.outputs[0]);
        assert!(next.outputs[8] > 0);
        assert_ne!(next.outputs[8], next.outputs[0]);
        assert_ne!(signal, 0);
    }

    #[test]
    fn all_predictive_signal_channels_have_distinct_histories() {
        let mut brain = PredictiveBrain::default();
        let frames = [frame(0, 0), frame(120, 90), frame(240, 0), frame(0, 45)];
        let mut histories = [[0; 4]; 7];
        for tick in 0..frames.len() {
            let step = brain.step(&ALL_ENABLED, &frames[tick]);
            for signal in 0..histories.len() {
                histories[signal][tick] = step.outputs[super::MOTORS + signal];
            }
        }
        for left in 0..histories.len() {
            for right in 0..left {
                assert_ne!(histories[left], histories[right]);
            }
        }
    }
}
