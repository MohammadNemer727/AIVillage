//! Compact deterministic lifetime-only recurrent sensory predictor.
//!
//! The recurrent encoder is a genome-keyed fixed reservoir. A bounded lifetime replay decodes the
//! median residual of nearby recurrent states; auxiliary decoder deltas retain a compact
//! parametric learning diagnostic. This keeps online updates small enough for the request path
//! while still producing a recurrent representation. All arithmetic is integer and therefore
//! exactly replayable in one build.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::manual_midpoint,
    clippy::needless_range_loop,
    reason = "all receptor and learned values are explicitly bounded before compact fixed-point casts"
)]

use serde::{Deserialize, Serialize};
use sim_kernels::derive;

use super::episodic::EpisodicSummary;
use crate::environment::{EnvironmentObservation, PrimitiveAction, RETINA_COLUMNS, RETINA_ROWS};

/// Number of bounded sensory features predicted one decision ahead.
pub const SENSE_FEATURES: usize = 24;
/// Width of the recurrent lifetime representation.
pub const LATENT_FEATURES: usize = 16;
/// Number of previous-action features supplied to the recurrent encoder.
pub const ACTION_FEATURES: usize = 8;
/// Truncation boundary for recurrent state history.
pub const TRUNCATED_SEQUENCE: u8 = 8;
/// Online decoder update cadence.
pub const UPDATE_CADENCE: u64 = 2;
/// Number of replay samples rehearsed per update.
pub const UPDATE_BATCH: usize = 8;
/// Bounded rehearsal capacity.
pub const REPLAY_CAPACITY: usize = 256;

const DECODER_WIDTH: usize = SENSE_FEATURES * (LATENT_FEATURES + 1);
const LEARNING_DIVISOR: i32 = 1 << 10;
const NEIGHBORS: usize = 9;

/// One bounded training example retained for rehearsal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PredictiveReplay {
    latent: Vec<i16>,
    target: Vec<i16>,
}

/// Checkpointable acquired state of the recurrent predictor.
///
/// `decoder_delta`, `momentum`, `hidden`, and replay are lifetime state and start at zero/empty.
/// The immutable recurrent and decoder base parameters are regenerated from `developmental_key`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct RecurrentPredictor {
    /// Stable key derived from the individual's developmental configuration.
    pub developmental_key: u64,
    /// Fixed-size recurrent latent state.
    pub hidden: Vec<i16>,
    /// Lifetime-only auxiliary parametric-decoder offsets.
    pub decoder_delta: Vec<i16>,
    /// Lifetime-only auxiliary optimizer momentum.
    pub momentum: Vec<i16>,
    /// Prediction made at the preceding decision boundary.
    pub prediction: Vec<i16>,
    /// Sensory anchor used by the residual next-sense decoder.
    pub previous_senses: Vec<i16>,
    /// Previous emitted action, conditioning the next recurrent transition.
    pub previous_action: PrimitiveAction,
    /// Whether a prediction is available to score.
    pub prediction_ready: bool,
    /// Decisions since the last recurrent truncation boundary.
    pub sequence_step: u8,
    /// Bounded deterministic rehearsal memory.
    pub replay: Vec<PredictiveReplay>,
    /// Ring position overwritten by the next rehearsal sample.
    pub replay_cursor: usize,
    /// Number of online decoder updates.
    pub updates: u64,
    /// Most recently scored absolute error per feature.
    pub last_absolute_error: Vec<u16>,
    /// Divisor of the decoder gradient step; developmentally expressed, smaller learns faster.
    pub learning_divisor: i32,
    /// Decisions between online decoder updates; developmentally expressed.
    pub update_period: u64,
    /// Replay samples rehearsed per update; developmentally expressed.
    pub rehearsal_batch: usize,
}

impl Default for RecurrentPredictor {
    fn default() -> Self {
        Self::new(0)
    }
}

impl RecurrentPredictor {
    /// Creates a newborn predictor with immutable parameters keyed by development and no
    /// acquired model, optimizer, hidden, prediction, or replay state.
    #[must_use]
    pub fn new(developmental_key: u64) -> Self {
        Self {
            developmental_key,
            hidden: vec![0; LATENT_FEATURES],
            decoder_delta: vec![0; DECODER_WIDTH],
            momentum: vec![0; DECODER_WIDTH],
            prediction: vec![0; SENSE_FEATURES],
            previous_senses: vec![0; SENSE_FEATURES],
            previous_action: PrimitiveAction::default(),
            prediction_ready: false,
            sequence_step: 0,
            replay: Vec::new(),
            replay_cursor: 0,
            updates: 0,
            last_absolute_error: vec![0; SENSE_FEATURES],
            learning_divisor: LEARNING_DIVISOR,
            update_period: UPDATE_CADENCE,
            rehearsal_batch: UPDATE_BATCH,
        }
    }

    /// Validates all fixed-size and bounded checkpoint fields.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.hidden.len() != LATENT_FEATURES
            || self.decoder_delta.len() != DECODER_WIDTH
            || self.momentum.len() != DECODER_WIDTH
            || self.prediction.len() != SENSE_FEATURES
            || self.previous_senses.len() != SENSE_FEATURES
            || self.last_absolute_error.len() != SENSE_FEATURES
            || self.replay.len() > REPLAY_CAPACITY
            || self.learning_divisor < 1
            || self.update_period < 1
            || self.rehearsal_batch < 1
            || self.rehearsal_batch > REPLAY_CAPACITY
            || self.replay_cursor >= REPLAY_CAPACITY
            || self.sequence_step >= TRUNCATED_SEQUENCE
            || self.replay.iter().any(|sample| {
                sample.latent.len() != LATENT_FEATURES || sample.target.len() != SENSE_FEATURES
            })
        {
            return Err("recurrent predictor state is out of range".to_owned());
        }
        Ok(())
    }

    /// Scores the pending prediction, performs a bounded replay update when due, and advances
    /// the recurrent state to predict the next observation.
    pub fn observe(
        &mut self,
        observation: &EnvironmentObservation,
        next_action: PrimitiveAction,
        decision_tick: u64,
    ) {
        let target = sense_features(observation);
        if self.prediction_ready {
            for feature in 0..SENSE_FEATURES {
                self.last_absolute_error[feature] =
                    self.prediction[feature].abs_diff(target[feature]);
            }
            self.remember(core::array::from_fn(|feature| {
                target[feature].saturating_sub(self.previous_senses[feature])
            }));
            if decision_tick > 0 && decision_tick.is_multiple_of(self.update_period) {
                self.train_replay();
            }
        }
        self.advance(&target, &next_action);
        self.predict(&target);
        self.previous_senses.copy_from_slice(&target);
        self.previous_action = next_action;
        self.prediction_ready = true;
        self.sequence_step += 1;
        if self.sequence_step == TRUNCATED_SEQUENCE {
            // Stop-gradient boundary: hidden values remain causal state, but no training history
            // crosses this boundary. Decoder-only online learning makes this explicit.
            self.sequence_step = 0;
        }
    }

    /// Advances latent state and prediction without changing decoder, optimizer, replay, or
    /// update counters. Intended for preregistered held-out evaluation.
    pub fn infer(&mut self, observation: &EnvironmentObservation, next_action: PrimitiveAction) {
        let senses = sense_features(observation);
        self.advance(&senses, &next_action);
        self.predict(&senses);
        self.previous_senses.copy_from_slice(&senses);
        self.previous_action = next_action;
        self.prediction_ready = true;
        self.sequence_step = (self.sequence_step + 1) % TRUNCATED_SEQUENCE;
    }

    fn remember(&mut self, target: [i16; SENSE_FEATURES]) {
        let sample = PredictiveReplay {
            latent: self.hidden.clone(),
            target: target.to_vec(),
        };
        if self.replay.len() < REPLAY_CAPACITY {
            self.replay.push(sample);
        } else {
            self.replay[self.replay_cursor] = sample;
        }
        self.replay_cursor = (self.replay_cursor + 1) % REPLAY_CAPACITY;
    }

    fn train_replay(&mut self) {
        if self.replay.is_empty() {
            return;
        }
        // Evenly spaced deterministic rehearsal covers old and recent entries without RNG state.
        for batch in 0..self.rehearsal_batch {
            let index = (self.replay_cursor + batch * self.replay.len() / self.rehearsal_batch)
                % self.replay.len();
            let sample = self.replay[index].clone();
            self.train_sample(&sample);
        }
        self.updates += 1;
    }

    fn train_sample(&mut self, sample: &PredictiveReplay) {
        for output in 0..SENSE_FEATURES {
            let predicted = self.decode(output, &sample.latent);
            let error = i32::from(sample.target[output]) - i32::from(predicted);
            for input in 0..=LATENT_FEATURES {
                let activation = if input == LATENT_FEATURES {
                    256
                } else {
                    i32::from(sample.latent[input])
                };
                let index = output * (LATENT_FEATURES + 1) + input;
                let gradient = (error * activation / self.learning_divisor).clamp(-16, 16);
                let velocity = (i32::from(self.momentum[index]) * 3 / 4 + gradient).clamp(-64, 64);
                self.momentum[index] = velocity as i16;
                self.decoder_delta[index] =
                    (i32::from(self.decoder_delta[index]) + velocity).clamp(-512, 512) as i16;
            }
        }
    }

    fn advance(&mut self, senses: &[i16; SENSE_FEATURES], action: &PrimitiveAction) {
        let actions = action_features(action);
        let previous = self.hidden.clone();
        let sensory_change: [i16; SENSE_FEATURES] = if self.prediction_ready {
            core::array::from_fn(|feature| {
                senses[feature].saturating_sub(self.previous_senses[feature])
            })
        } else {
            [0; SENSE_FEATURES]
        };
        for unit in 0..LATENT_FEATURES {
            let mut candidate = 0_i32;
            for (input, value) in sensory_change.iter().chain(actions.iter()).enumerate() {
                candidate +=
                    i32::from(*value) * fixed_weight(self.developmental_key, unit, input, 1);
            }
            for (input, value) in previous.iter().enumerate() {
                candidate +=
                    i32::from(*value) * fixed_weight(self.developmental_key, unit, input, 2);
            }
            // Keep the projection in the same fixed-point range as its receptor inputs.
            // Dividing by 2048 made ordinary latent values round to only a few integer steps,
            // leaving the decoder effectively bias-only.
            candidate = (candidate / 256).clamp(-256, 256);
            // A unit has one developmental time constant. The TBPTT bookkeeping phase must not
            // alter inference: crossing a stop-gradient boundary is not a sensory event.
            let gate_seed = fixed_weight(self.developmental_key, unit, 0, 3);
            let keep = (128 + gate_seed * 4).clamp(32, 224);
            self.hidden[unit] =
                ((i32::from(previous[unit]) * keep + candidate * (256 - keep)) / 256) as i16;
        }
    }

    fn predict(&mut self, anchor: &[i16; SENSE_FEATURES]) {
        let replay_residual = self.neighbor_residual();
        for output in 0..SENSE_FEATURES {
            self.prediction[output] = anchor[output]
                .saturating_add(replay_residual[output])
                .clamp(-255, 255);
        }
    }

    /// Amends only body/interoceptive prediction from consistent same-action consequences.
    ///
    /// The episodic key has an explicit body-drive component but no independently validated
    /// visual-context metric. Episodic residuals therefore cannot overwrite vision or held-item
    /// prediction. The median gives the fixed top-k a robust aggregation rule; no usable match
    /// falls back exactly to the recurrent prediction.
    pub fn apply_episodic_prediction(&mut self, summaries: &[EpisodicSummary]) {
        let usable: Vec<_> = summaries
            .iter()
            .filter(|summary| {
                summary.valid
                    && summary.action == self.previous_action
                    && summary.evidence > summary.contradictions
            })
            .collect();
        if usable.is_empty() {
            return;
        }
        for feature in 12..18 {
            let mut residuals: Vec<_> = usable
                .iter()
                .map(|summary| summary.sensory_delta[feature])
                .collect();
            residuals.sort_unstable();
            let residual = residuals[residuals.len() / 2];
            self.prediction[feature] = self.previous_senses[feature]
                .saturating_add(residual)
                .clamp(-255, 255);
        }
    }

    /// Mean recently observed absolute error, used as a bounded uncertainty estimate.
    #[must_use]
    pub fn uncertainty(&self) -> u16 {
        let total: u32 = self
            .last_absolute_error
            .iter()
            .map(|error| u32::from(*error))
            .sum();
        u16::try_from(total / SENSE_FEATURES as u32).unwrap_or(u16::MAX)
    }

    /// Returns the per-feature median residual among nearby recurrent states. Median residuals
    /// optimize the reported absolute-error objective and fall back exactly to persistence until
    /// enough lifetime examples exist; this avoids turning isolated resets or damage spikes into
    /// a persistent decoder bias.
    fn neighbor_residual(&self) -> [i16; SENSE_FEATURES] {
        if self.replay.len() < NEIGHBORS {
            return [0; SENSE_FEATURES];
        }
        let mut nearest = [(u32::MAX, 0_usize); NEIGHBORS];
        for (index, sample) in self.replay.iter().enumerate() {
            let distance = sample
                .latent
                .iter()
                .zip(&self.hidden)
                .map(|(left, right)| u32::from(left.abs_diff(*right)))
                .sum();
            if distance < nearest[NEIGHBORS - 1].0 {
                nearest[NEIGHBORS - 1] = (distance, index);
                nearest.sort_unstable();
            }
        }
        core::array::from_fn(|feature| {
            let mut values = core::array::from_fn::<_, NEIGHBORS, _>(|slot| {
                self.replay[nearest[slot].1].target[feature]
            });
            values.sort_unstable();
            values[NEIGHBORS / 2]
        })
    }

    fn decode(&self, output: usize, latent: &[i16]) -> i16 {
        let mut sum = 0;
        for input in 0..=LATENT_FEATURES {
            let activation = if input == LATENT_FEATURES {
                256
            } else {
                i32::from(latent[input])
            };
            let delta = i32::from(self.decoder_delta[output * (LATENT_FEATURES + 1) + input]);
            sum += activation * delta / 16;
        }
        (sum / 256).clamp(-255, 255) as i16
    }

    /// Stable fingerprint of all acquired predictor state.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut result = derive(
            self.developmental_key,
            self.updates,
            self.replay_cursor as u64,
            0x5250_5244,
        );
        for (index, value) in self
            .hidden
            .iter()
            .chain(&self.decoder_delta)
            .chain(&self.momentum)
            .chain(&self.prediction)
            .chain(&self.previous_senses)
            .enumerate()
        {
            result = derive(
                result,
                i64::from(*value).cast_unsigned(),
                index as u64,
                0x4C49_4645,
            );
        }
        // Tuning that differs from the legacy constants is part of the individual; the legacy
        // tuning leaves every fingerprint exactly as it was.
        if self.learning_divisor != LEARNING_DIVISOR
            || self.update_period != UPDATE_CADENCE
            || self.rehearsal_batch != UPDATE_BATCH
        {
            result = derive(
                result,
                self.learning_divisor.cast_unsigned().into(),
                (self.update_period << 32) | self.rehearsal_batch as u64,
                0x5455_4E45,
            );
        }
        result
    }

    /// Applies the developmentally expressed update tuning. Acquired state is untouched.
    pub fn set_tuning(
        &mut self,
        learning_divisor: i32,
        update_period: u64,
        rehearsal_batch: usize,
    ) {
        self.learning_divisor = learning_divisor.max(1);
        self.update_period = update_period.max(1);
        self.rehearsal_batch = rehearsal_batch.clamp(1, REPLAY_CAPACITY);
    }
}

fn fixed_weight(key: u64, output: usize, input: usize, domain: u64) -> i32 {
    let word = derive(key, output as u64, input as u64, 0x4752_5500 | domain);
    i32::try_from(word % 17).unwrap_or_default() - 8
}

fn action_features(action: &PrimitiveAction) -> [i16; ACTION_FEATURES] {
    [
        action.thrust / 2,
        action.turn / 2,
        action.strafe / 2,
        action.look_pitch / 2,
        action.jump as i16,
        action.hand_pulse as i16,
        action.jaw as i16,
        ((u32::from(action.use_pulse) + u32::from(action.next_slot)) / 2) as i16,
    ]
}

/// Nonsemantic bounded feature vector grouped as vision (0..12), body/interoception (12..18),
/// and held-item physics (18..24).
#[must_use]
pub fn sense_features(observation: &EnvironmentObservation) -> [i16; SENSE_FEATURES] {
    let mut result = [0_i16; SENSE_FEATURES];
    if let Some(retina) = observation.retina {
        for region in 0..4 {
            let row_start = (region / 2) * (RETINA_ROWS / 2);
            let row_end = if region / 2 == 0 {
                RETINA_ROWS / 2
            } else {
                RETINA_ROWS
            };
            let col_start = (region % 2) * (RETINA_COLUMNS / 2);
            let col_end = col_start + RETINA_COLUMNS / 2;
            let mut brightness = 0_i32;
            let mut occlusion = 0_i32;
            let mut motion = 0_i32;
            let mut count = 0_i32;
            for row in row_start..row_end {
                for column in col_start..col_end {
                    let sample = retina.points[row * RETINA_COLUMNS + column];
                    brightness += i32::from(sample.brightness);
                    occlusion += i32::from(sample.occlusion);
                    motion += i32::from(sample.motion);
                    count += 1;
                }
            }
            result[region * 3] = (brightness / count) as i16;
            result[region * 3 + 1] = (occlusion / count) as i16;
            result[region * 3 + 2] = (motion / count) as i16;
        }
    } else {
        for region in 0..4 {
            let rays = &observation.senses.retinal[region * observation.senses.retinal.len() / 4
                ..(region + 1) * observation.senses.retinal.len() / 4];
            result[region * 3] = (rays
                .iter()
                .map(|ray| u32::from(ray.brightness))
                .sum::<u32>()
                / rays.len() as u32) as i16;
            result[region * 3 + 1] = (rays.iter().map(|ray| u32::from(ray.occlusion)).sum::<u32>()
                / rays.len() as u32) as i16;
            result[region * 3 + 2] = (rays.iter().map(|ray| i32::from(ray.motion)).sum::<i32>()
                / rays.len() as i32) as i16;
        }
    }
    let felt = observation.interoception.unwrap_or_default();
    result[12..18].copy_from_slice(&[
        felt.health as i16,
        felt.fullness as i16,
        felt.air as i16,
        felt.pain as i16,
        observation.body.eye_pitch as i16,
        ((u32::from(observation.body.grounded) + u32::from(observation.body.hand_contact)) / 2)
            as i16,
    ]);
    let held = observation.held_item.unwrap_or_default();
    result[18..].copy_from_slice(&[
        held.occupied as i16,
        held.red as i16,
        held.green as i16,
        held.blue as i16,
        held.amount as i16,
        ((u32::from(held.capacity) + u32::from(held.durability)) / 2) as i16,
    ]);
    result
}

/// Derives the immutable predictor key from inherited development, not acquired parent state.
#[must_use]
pub fn developmental_key(config: &crate::BrainConfig) -> u64 {
    derive(
        config.connection_stride as u64,
        i64::from(config.plasticity_scale).cast_unsigned(),
        u64::from(config.prediction_enabled),
        0x4445_564B,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RawSensoryFrame,
        environment::{EnvironmentCapabilities, TransportStatus},
    };

    fn observation(sequence: u64, brightness: u16) -> EnvironmentObservation {
        let mut senses = RawSensoryFrame::EMPTY;
        for ray in &mut senses.retinal {
            ray.brightness = brightness;
        }
        EnvironmentObservation {
            sequence,
            senses,
            body: crate::environment::BodyProprioception::default(),
            capabilities: EnvironmentCapabilities::default(),
            transport: TransportStatus::Healthy,
            retina: None,
            interoception: None,
            held_item: None,
        }
    }

    #[test]
    fn newborn_has_no_acquired_or_optimizer_state() {
        let predictor = RecurrentPredictor::new(91);
        assert!(predictor.hidden.iter().all(|value| *value == 0));
        assert!(predictor.decoder_delta.iter().all(|value| *value == 0));
        assert!(predictor.momentum.iter().all(|value| *value == 0));
        assert_eq!(predictor.replay.len(), 0);
        assert_eq!(predictor.updates, 0);
    }

    #[test]
    fn online_updates_are_deterministic_and_checkpointable() {
        let run = || {
            let mut predictor = RecurrentPredictor::new(91);
            for tick in 0..24 {
                predictor.observe(
                    &observation(tick, (tick * 7 % 256) as u16),
                    PrimitiveAction {
                        turn: if tick % 2 == 0 { 128 } else { -128 },
                        ..PrimitiveAction::default()
                    },
                    tick,
                );
            }
            predictor
        };
        let learned = run();
        assert_eq!(learned, run());
        assert!(learned.updates > 0);
        assert!(learned.decoder_delta.iter().any(|value| *value != 0));
        let encoded = serde_json::to_string(&learned).unwrap();
        let restored: RecurrentPredictor = serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored, learned);
        assert_eq!(restored.fingerprint(), learned.fingerprint());
    }

    #[test]
    fn next_prediction_is_conditioned_on_the_action_about_to_be_applied() {
        let input = observation(0, 80);
        let mut left = RecurrentPredictor::new(91);
        let mut right = left.clone();
        left.infer(
            &input,
            PrimitiveAction {
                turn: -256,
                ..PrimitiveAction::default()
            },
        );
        right.infer(
            &input,
            PrimitiveAction {
                turn: 256,
                ..PrimitiveAction::default()
            },
        );
        assert_ne!(left.hidden, right.hidden);
        assert_eq!(left.previous_action.turn, -256);
        assert_eq!(right.previous_action.turn, 256);
    }

    #[test]
    fn repeated_residuals_predict_change_while_constant_senses_predict_persistence() {
        let action = PrimitiveAction::default();
        let mut changing = RecurrentPredictor::new(91);
        for tick in 0..24 {
            changing.observe(&observation(tick, 40 + tick as u16), action, tick);
        }
        for feature in [0, 3, 6, 9] {
            assert_eq!(changing.prediction[feature], 64);
        }

        let mut constant = RecurrentPredictor::new(91);
        for tick in 0..24 {
            constant.observe(&observation(tick, 80), action, tick);
        }
        for feature in [0, 3, 6, 9] {
            assert_eq!(constant.prediction[feature], 80);
        }
    }

    #[test]
    fn episodic_prediction_cannot_overwrite_vision() {
        let mut predictor = RecurrentPredictor::new(91);
        predictor.previous_action = PrimitiveAction {
            thrust: 256,
            ..PrimitiveAction::default()
        };
        predictor.previous_senses = vec![40; SENSE_FEATURES];
        predictor.prediction = vec![55; SENSE_FEATURES];
        let summary = EpisodicSummary {
            valid: true,
            action: predictor.previous_action,
            sensory_delta: [20; SENSE_FEATURES],
            evidence: 1,
            ..EpisodicSummary::default()
        };
        predictor.apply_episodic_prediction(&[summary]);
        assert!(predictor.prediction[..12].iter().all(|value| *value == 55));
        assert!(
            predictor.prediction[12..18]
                .iter()
                .all(|value| *value == 60)
        );
        assert!(predictor.prediction[18..].iter().all(|value| *value == 55));
    }
}
