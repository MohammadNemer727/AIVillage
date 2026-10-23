//! Experiment 003 prediction ablation: one recorded trace replayed under all four episodic
//! switch settings, plus the online prediction error an arm experienced during its own life.
//!
//! Each condition mirrors the live runtime order exactly: the recurrent predictor observes the
//! transition, the private memory completes the preceding event, and only then may retrieval
//! amend the next prediction. Amendment therefore also changes write salience, as it does live.

use std::collections::BTreeMap;

use serde::Serialize;
use sim_core::runtime::{
    EpisodicConfig, EpisodicMemory, RecurrentPredictor, SENSE_FEATURES, amend_prediction,
    learn_transition, predictive_developmental_key, sense_features,
};
use sim_format::FounderSpec;

use crate::TraceRecord;

const TRAIN_PERCENT: usize = 80;
const MODALITIES: [(&str, core::ops::Range<usize>); 3] =
    [("vision", 0..12), ("body", 12..18), ("held_item", 18..24)];

/// Fixed Experiment 003 conditions, named as in the preregistration.
pub const CONDITIONS: [(&str, EpisodicConfig); 4] = [
    (
        "A",
        EpisodicConfig {
            prediction_enabled: false,
            policy_enabled: false,
            graph_enabled: false,
        },
    ),
    (
        "B",
        EpisodicConfig {
            prediction_enabled: false,
            policy_enabled: true,
            graph_enabled: false,
        },
    ),
    (
        "C",
        EpisodicConfig {
            prediction_enabled: true,
            policy_enabled: false,
            graph_enabled: false,
        },
    ),
    (
        "D",
        EpisodicConfig {
            prediction_enabled: true,
            policy_enabled: true,
            graph_enabled: false,
        },
    ),
];

/// Pooled absolute error for one modality; totals let reports pool seeds exactly.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ModalityError {
    /// Number of scalar predictions scored.
    pub feature_samples: u64,
    /// Sum of absolute errors.
    pub absolute_error_total: u64,
    /// Mean absolute error multiplied by 1000.
    pub mae_milli: u64,
}

/// Transition-level paired outcome counts for one modality.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct PairedCounts {
    /// Transitions where the treatment had lower total error.
    pub treatment_wins: u64,
    /// Transitions with equal total error.
    pub ties: u64,
    /// Transitions where the control had lower total error.
    pub treatment_losses: u64,
}

/// Paired frozen-replay comparison of one condition against its prediction-off counterpart.
#[derive(Clone, Debug, Serialize)]
pub struct Comparison {
    /// Prediction-on condition.
    pub treatment: &'static str,
    /// Matching prediction-off condition.
    pub control: &'static str,
    /// Paired counts by modality.
    pub modalities: BTreeMap<&'static str, PairedCounts>,
}

/// Frozen-replay report for one recorded trace.
#[derive(Clone, Debug, Serialize)]
pub struct FrozenReplayReport {
    /// Genome and deterministic runtime seed.
    pub seed: u64,
    /// Individual whose contiguous trace supplied every condition.
    pub agent_id: u64,
    /// Prefix transitions supplied identically to every condition.
    pub train_transitions: usize,
    /// Untouched suffix transitions scored without memory writes.
    pub held_out_transitions: usize,
    /// Held-out error by condition and modality.
    pub conditions: BTreeMap<&'static str, BTreeMap<&'static str, ModalityError>>,
    /// C against A and D against B.
    pub comparisons: Vec<Comparison>,
    /// The policy switch cannot affect prediction on fixed actions; A=B and C=D exactly.
    pub policy_switch_invariant: bool,
}

/// Online one-step prediction error over an arm's own complete trace.
#[derive(Clone, Debug, Serialize)]
pub struct OnlinePredictionReport {
    /// Genome and deterministic runtime seed.
    pub seed: u64,
    /// Individual whose trace was scored.
    pub agent_id: u64,
    /// Switches the arm ran with.
    pub config: EpisodicConfig,
    /// Transitions with a pending prediction.
    pub scored_transitions: usize,
    /// Error by modality.
    pub prediction: BTreeMap<&'static str, ModalityError>,
}

/// Replays the first 80% of `records` with learning and writes, then scores the untouched final
/// 20% without writes, separately under each of the four switch settings.
///
/// # Errors
///
/// Requires at least ten contiguous records from one agent.
pub fn frozen_replay(
    seed: u64,
    records: &[TraceRecord],
) -> Result<FrozenReplayReport, &'static str> {
    validate(records)?;
    let split = records.len() * TRAIN_PERCENT / 100;
    let per_condition: Vec<Vec<[u64; 3]>> = CONDITIONS
        .iter()
        .map(|(_, config)| held_out_errors(seed, records, split, *config))
        .collect();
    let conditions = CONDITIONS
        .iter()
        .zip(&per_condition)
        .map(|((name, _), errors)| (*name, pooled(errors)))
        .collect();
    let comparisons = [(2, 0), (3, 1)]
        .into_iter()
        .map(|(treatment, control)| Comparison {
            treatment: CONDITIONS[treatment].0,
            control: CONDITIONS[control].0,
            modalities: paired(&per_condition[treatment], &per_condition[control]),
        })
        .collect();
    Ok(FrozenReplayReport {
        seed,
        agent_id: records[0].request.agent_id,
        train_transitions: split,
        held_out_transitions: records.len() - split,
        conditions,
        comparisons,
        policy_switch_invariant: per_condition[0] == per_condition[1]
            && per_condition[2] == per_condition[3],
    })
}

/// Scores the one-step prediction an arm made at every transition of its own life.
///
/// # Errors
///
/// Requires at least ten contiguous records from one agent.
pub fn online_prediction(
    seed: u64,
    records: &[TraceRecord],
    config: EpisodicConfig,
) -> Result<OnlinePredictionReport, &'static str> {
    validate(records)?;
    let (mut predictor, mut memory) = newborn(seed, records, config);
    let mut errors = Vec::with_capacity(records.len());
    for (tick, record) in records.iter().enumerate() {
        let actual = sense_features(&record.request.observation);
        if predictor.prediction_ready {
            errors.push(transition_error(&predictor.prediction, &actual));
        }
        learn_transition(
            &mut predictor,
            &mut memory,
            &record.request.observation,
            record.response.action,
            tick as u64,
        );
    }
    Ok(OnlinePredictionReport {
        seed,
        agent_id: records[0].request.agent_id,
        config,
        scored_transitions: errors.len(),
        prediction: pooled(&errors),
    })
}

fn validate(records: &[TraceRecord]) -> Result<(), &'static str> {
    crate::validate_contiguous_trace(records, 10)
}

fn newborn(
    seed: u64,
    records: &[TraceRecord],
    config: EpisodicConfig,
) -> (RecurrentPredictor, EpisodicMemory) {
    let founder = FounderSpec::new(seed, records[0].request.agent_id);
    (
        RecurrentPredictor::new(predictive_developmental_key(
            &founder.genome.default_brain_config(),
        )),
        EpisodicMemory::new(config),
    )
}

fn held_out_errors(
    seed: u64,
    records: &[TraceRecord],
    split: usize,
    config: EpisodicConfig,
) -> Vec<[u64; 3]> {
    let (mut predictor, mut memory) = newborn(seed, records, config);
    for (tick, record) in records[..split].iter().enumerate() {
        learn_transition(
            &mut predictor,
            &mut memory,
            &record.request.observation,
            record.response.action,
            tick as u64,
        );
    }
    records[split..]
        .iter()
        .map(|record| {
            let actual = sense_features(&record.request.observation);
            let error = transition_error(&predictor.prediction, &actual);
            predictor.infer(&record.request.observation, record.response.action);
            amend_prediction(&mut predictor, &memory, &actual);
            error
        })
        .collect()
}

fn transition_error(prediction: &[i16], actual: &[i16; SENSE_FEATURES]) -> [u64; 3] {
    core::array::from_fn(|modality| {
        MODALITIES[modality]
            .1
            .clone()
            .map(|feature| u64::from(prediction[feature].abs_diff(actual[feature])))
            .sum()
    })
}

fn pooled(errors: &[[u64; 3]]) -> BTreeMap<&'static str, ModalityError> {
    MODALITIES
        .iter()
        .enumerate()
        .map(|(modality, (name, range))| {
            let feature_samples = (errors.len() * range.len()) as u64;
            let absolute_error_total = errors.iter().map(|error| error[modality]).sum::<u64>();
            (
                *name,
                ModalityError {
                    feature_samples,
                    absolute_error_total,
                    mae_milli: (absolute_error_total * 1_000)
                        .checked_div(feature_samples)
                        .unwrap_or_default(),
                },
            )
        })
        .collect()
}

fn paired(treatment: &[[u64; 3]], control: &[[u64; 3]]) -> BTreeMap<&'static str, PairedCounts> {
    MODALITIES
        .iter()
        .enumerate()
        .map(|(modality, (name, _))| {
            let mut counts = PairedCounts::default();
            for (left, right) in treatment.iter().zip(control) {
                match left[modality].cmp(&right[modality]) {
                    core::cmp::Ordering::Less => counts.treatment_wins += 1,
                    core::cmp::Ordering::Equal => counts.ties += 1,
                    core::cmp::Ordering::Greater => counts.treatment_losses += 1,
                }
            }
            (*name, counts)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use sim_core::{
        RawSensoryFrame,
        environment::{
            BodyProprioception, EnvironmentCapabilities, EnvironmentObservation, Interoception,
            PrimitiveAction, TransportStatus,
        },
        runtime::ExperimentMode,
    };

    use super::*;
    use crate::{ApplicationTrace, AppliedPose, Bridge, BridgeRequest, PROTOCOL_VERSION};

    fn felt(sequence: u64) -> BridgeRequest {
        // A sawtooth hunger pattern gives memory relieving and depleting consequences to keep.
        let fullness = 40 + u16::try_from(sequence % 8).unwrap_or_default() * 25;
        BridgeRequest {
            protocol: PROTOCOL_VERSION,
            agent_id: 9,
            body_slot: None,
            observation: EnvironmentObservation {
                sequence,
                senses: RawSensoryFrame::EMPTY,
                body: BodyProprioception::default(),
                capabilities: EnvironmentCapabilities::default(),
                transport: TransportStatus::Healthy,
                retina: None,
                interoception: Some(Interoception {
                    health: 200,
                    fullness,
                    air: 255,
                    pain: 0,
                }),
                held_item: None,
            },
            trace: ApplicationTrace {
                applied_server_step: 10 + sequence,
                applied_steps: 1,
                accumulated_dtime_us: 100_000,
                applied_action: PrimitiveAction::default(),
                pose: AppliedPose {
                    x_millinode: 0,
                    y_millinode: 1_000,
                    z_millinode: 0,
                    yaw_milliradian: -1_571,
                },
            },
        }
    }

    fn trace(config: EpisodicConfig) -> Vec<TraceRecord> {
        let mut bridge = Bridge::new_with_mode(4, 9, ExperimentMode::AffordanceDiscovery);
        bridge.set_episodic_config(config);
        for sequence in 0..120 {
            bridge.handle(felt(sequence)).expect("transition");
        }
        bridge.drain_trace()
    }

    #[test]
    fn policy_switch_cannot_change_frozen_prediction() {
        let report = frozen_replay(4, &trace(CONDITIONS[3].1)).expect("frozen replay");
        assert!(report.policy_switch_invariant);
        assert_eq!(report.train_transitions, 96);
        assert_eq!(report.held_out_transitions, 24);
        assert_eq!(report.conditions["A"], report.conditions["B"]);
        assert_eq!(report.conditions["C"], report.conditions["D"]);
        let body = report.comparisons[0].modalities["body"];
        assert_eq!(body.treatment_wins + body.ties + body.treatment_losses, 24);
    }

    #[test]
    fn online_prediction_scores_every_pending_prediction_and_respects_the_switch() {
        let records = trace(CONDITIONS[2].1);
        let on = online_prediction(4, &records, CONDITIONS[2].1).expect("online");
        let off = online_prediction(4, &records, CONDITIONS[0].1).expect("online");
        assert_eq!(on.scored_transitions, records.len() - 1);
        assert_eq!(on.prediction["vision"], off.prediction["vision"]);
        assert_eq!(
            on.prediction["body"].feature_samples,
            off.prediction["body"].feature_samples
        );
    }

    #[test]
    fn short_or_broken_traces_are_rejected() {
        let records = trace(CONDITIONS[0].1);
        assert!(frozen_replay(4, &records[..9]).is_err());
        let mut gap = records;
        gap.remove(20);
        assert!(frozen_replay(4, &gap).is_err());
    }
}
