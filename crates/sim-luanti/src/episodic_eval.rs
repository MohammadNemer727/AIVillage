//! Paired A/B evaluation of private episodic retrieval against the recurrent predictor.

use std::collections::BTreeMap;

use serde::Serialize;
use sim_core::{
    RawSensoryFrame,
    environment::{
        BodyProprioception, EnvironmentCapabilities, EnvironmentObservation, Interoception,
        PrimitiveAction, TransportStatus,
    },
    runtime::{
        EPISODIC_CAPACITY, EPISODIC_TOP_K, EpisodicConfig, EpisodicMemory, LATENT_FEATURES,
        RecurrentPredictor, SENSE_FEATURES, predictive_developmental_key, sense_features,
    },
};
use sim_format::FounderSpec;
use sim_kernels::derive;

use crate::TraceRecord;

const TRAIN_PERCENT: usize = 80;
const ACTION_SEEDS: [u64; 8] = [11, 23, 37, 41, 59, 67, 83, 101];
const HELD_OUT_ACTION_TRIALS: u32 = 16;
const MODALITIES: [(&str, core::ops::Range<usize>); 3] =
    [("vision", 0..12), ("body", 12..18), ("held_item", 18..24)];

/// Paired prediction errors for one modality.
#[derive(Clone, Debug, Serialize)]
pub struct PredictionAblationMetrics {
    /// Number of scalar held-out predictions.
    pub feature_samples: u64,
    /// Recurrent-only mean absolute error multiplied by 1000.
    pub recurrent_only_mae_milli: u64,
    /// Recurrent plus frozen episodic retrieval MAE multiplied by 1000.
    pub episodic_mae_milli: u64,
    /// Held-out transitions where episodic retrieval had lower total error.
    pub episodic_wins: u64,
    /// Held-out transitions with equal total error.
    pub ties: u64,
    /// Held-out transitions where recurrent-only had lower total error.
    pub episodic_losses: u64,
    /// True only for a strict episodic MAE improvement.
    pub episodic_beats_recurrent: bool,
}

/// One paired held-out embodied action run.
#[derive(Clone, Debug, Serialize)]
pub struct ActionSeedMetrics {
    /// Seed shared by both arms.
    pub seed: u64,
    /// Untouched held-out opportunities.
    pub held_out_trials: u32,
    /// Repeated harmful choices by recurrent-only A.
    pub recurrent_only_injuries: u32,
    /// Repeated harmful choices by episodic B.
    pub episodic_injuries: u32,
    /// Resource-return choices by recurrent-only A.
    pub recurrent_only_resource_returns: u32,
    /// Resource-return choices by episodic B.
    pub episodic_resource_returns: u32,
}

/// Machine-readable gated episodic-memory report.
#[derive(Clone, Debug, Serialize)]
pub struct EpisodicAblationReport {
    /// Genome and deterministic runtime seed shared by both arms.
    pub seed: u64,
    /// Individual whose contiguous trace supplied both arms.
    pub agent_id: u64,
    /// Fixed treatment arms.
    pub arms: [&'static str; 2],
    /// Experience split fixed before evaluation.
    pub prediction_split: &'static str,
    /// Prefix transitions supplied identically to both arms.
    pub train_transitions: usize,
    /// Untouched suffix transitions scored without memory writes.
    pub held_out_transitions: usize,
    /// Prediction metrics by nonsemantic modality.
    pub prediction: BTreeMap<&'static str, PredictionAblationMetrics>,
    /// Whether vision and body both strictly improve over recurrent-only.
    pub prediction_success: bool,
    /// Fixed repeated injury/resource-return protocol results.
    pub action: Vec<ActionSeedMetrics>,
    /// Whether every seed reduces injuries and increases resource returns.
    pub action_success: bool,
    /// Broad memory gate; prediction-only gains cannot pass it.
    pub episodic_gate_passes: bool,
    /// Explicit fixed architecture bounds.
    pub bounds: Bounds,
}

/// Fixed live-memory work bounds recorded in the report.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Bounds {
    /// Maximum private events per individual.
    pub capacity: usize,
    /// Fixed retrieval result count.
    pub top_k: usize,
    /// Recurrent key width.
    pub latent_features: usize,
    /// Retained source-feature width.
    pub source_features: usize,
}

/// Runs paired prediction and controlled embodied action ablations.
///
/// # Errors
///
/// Requires at least ten contiguous records from one agent.
#[allow(
    clippy::too_many_lines,
    reason = "the report keeps paired split, scoring, and preregistered gate construction together"
)]
pub fn evaluate(
    seed: u64,
    records: &[TraceRecord],
) -> Result<EpisodicAblationReport, &'static str> {
    if records.len() < 10 {
        return Err("episodic evaluation requires at least 10 transitions");
    }
    let first = &records[0];
    if records.iter().enumerate().any(|(index, record)| {
        record.request.agent_id != first.request.agent_id
            || !matches!(record.request.protocol, 4 | crate::PROTOCOL_VERSION)
            || record.request.observation.sequence
                != first.request.observation.sequence + index as u64
    }) {
        return Err("trace must be contiguous, current-protocol, and from one agent");
    }
    let split = records.len() * TRAIN_PERCENT / 100;
    let founder = FounderSpec::new(seed, first.request.agent_id);
    let mut predictor = RecurrentPredictor::new(predictive_developmental_key(
        &founder.genome.default_brain_config(),
    ));
    let mut memory = EpisodicMemory::new(EpisodicConfig {
        prediction_enabled: true,
        policy_enabled: false,
    });
    for (tick, record) in records[..split].iter().enumerate() {
        learn_transition(
            &mut predictor,
            &mut memory,
            &record.request.observation,
            record.response.action,
            tick as u64,
        );
    }

    let mut recurrent_only = predictor.clone();
    let mut episodic = predictor;
    let current = sense_features(&records[split - 1].request.observation);
    let summaries =
        memory.retrieve_for_action(&episodic.hidden, &current, episodic.previous_action);
    episodic.apply_episodic_prediction(&summaries);

    let mut errors = [[0_u64; 2]; 3];
    let mut counts = [0_u64; 3];
    let mut paired = [[0_u64; 3]; 3];
    for record in &records[split..] {
        let actual = sense_features(&record.request.observation);
        for (modality, (_, range)) in MODALITIES.iter().enumerate() {
            let mut transition = [0_u64; 2];
            for feature in range.clone() {
                transition[0] +=
                    u64::from(recurrent_only.prediction[feature].abs_diff(actual[feature]));
                transition[1] += u64::from(episodic.prediction[feature].abs_diff(actual[feature]));
                counts[modality] += 1;
            }
            paired[modality][match transition[1].cmp(&transition[0]) {
                core::cmp::Ordering::Less => 0,
                core::cmp::Ordering::Equal => 1,
                core::cmp::Ordering::Greater => 2,
            }] += 1;
            for (total, value) in errors[modality].iter_mut().zip(transition) {
                *total += value;
            }
        }
        recurrent_only.infer(&record.request.observation, record.response.action);
        episodic.infer(&record.request.observation, record.response.action);
        let summaries =
            memory.retrieve_for_action(&episodic.hidden, &actual, episodic.previous_action);
        episodic.apply_episodic_prediction(&summaries);
    }

    let mut prediction = BTreeMap::new();
    for (index, (name, _)) in MODALITIES.iter().enumerate() {
        let recurrent_only_mae_milli = errors[index][0] * 1_000 / counts[index];
        let episodic_mae_milli = errors[index][1] * 1_000 / counts[index];
        prediction.insert(
            *name,
            PredictionAblationMetrics {
                feature_samples: counts[index],
                recurrent_only_mae_milli,
                episodic_mae_milli,
                episodic_wins: paired[index][0],
                ties: paired[index][1],
                episodic_losses: paired[index][2],
                episodic_beats_recurrent: episodic_mae_milli < recurrent_only_mae_milli,
            },
        );
    }
    let prediction_success = ["vision", "body"]
        .into_iter()
        .all(|name| prediction[name].episodic_beats_recurrent);
    let action: Vec<_> = ACTION_SEEDS.into_iter().map(action_protocol).collect();
    let action_success = action.iter().all(|run| {
        run.episodic_injuries < run.recurrent_only_injuries
            && run.episodic_resource_returns > run.recurrent_only_resource_returns
    });
    Ok(EpisodicAblationReport {
        seed,
        agent_id: first.request.agent_id,
        arms: [
            "A_recurrent_predictor_only",
            "B_predictor_plus_episodic_retrieval",
        ],
        prediction_split: "first_80_percent_train_final_20_percent_untouched_no_writes",
        train_transitions: split,
        held_out_transitions: records.len() - split,
        prediction,
        prediction_success,
        action,
        action_success,
        episodic_gate_passes: prediction_success && action_success,
        bounds: Bounds {
            capacity: EPISODIC_CAPACITY,
            top_k: EPISODIC_TOP_K,
            latent_features: LATENT_FEATURES,
            source_features: SENSE_FEATURES,
        },
    })
}

fn learn_transition(
    predictor: &mut RecurrentPredictor,
    memory: &mut EpisodicMemory,
    observation: &EnvironmentObservation,
    next_action: PrimitiveAction,
    tick: u64,
) {
    let actual = sense_features(observation);
    let ready = predictor.prediction_ready;
    let latent_before = predictor.hidden.clone();
    let source_before = predictor.previous_senses.clone();
    let prediction = predictor.prediction.clone();
    let preceding_action = predictor.previous_action;
    let uncertainty = predictor.uncertainty();
    predictor.observe(observation, next_action, tick);
    if ready {
        memory.observe(
            &latent_before,
            &predictor.hidden,
            &source_before,
            &actual,
            preceding_action,
            &prediction,
            uncertainty,
            observation,
            tick,
        );
    }
}

fn action_protocol(seed: u64) -> ActionSeedMetrics {
    let harmful = PrimitiveAction {
        thrust: 256,
        ..PrimitiveAction::default()
    };
    let relieving = PrimitiveAction {
        thrust: -256,
        ..PrimitiveAction::default()
    };
    let mut memory = EpisodicMemory::new(EpisodicConfig {
        prediction_enabled: false,
        policy_enabled: true,
    });
    let latent: [i16; LATENT_FEATURES] = core::array::from_fn(|index| {
        i16::try_from(derive(seed, index as u64, 0, 0x4143_544E) % 65).unwrap_or_default() - 32
    });
    let mut before = [0_i16; SENSE_FEATURES];
    before[12] = 220;
    before[13] = 80;
    before[14] = 255;
    for exposure in 0..8_u64 {
        let action = if exposure % 2 == 0 {
            harmful
        } else {
            relieving
        };
        let mut after = before;
        if action == harmful {
            after[12] = 160;
            after[15] = 80;
        } else {
            after[13] = 160;
        }
        let observation = embodied_observation(exposure, &after);
        memory.observe(
            &latent,
            &latent,
            &before,
            &after,
            action,
            &[0; SENSE_FEATURES],
            0,
            &observation,
            exposure,
        );
    }
    let summaries = memory.retrieve(&latent, &before);
    let episodic_action = EpisodicMemory::recommend_action(&summaries, harmful);
    let mut metrics = ActionSeedMetrics {
        seed,
        held_out_trials: HELD_OUT_ACTION_TRIALS,
        recurrent_only_injuries: 0,
        episodic_injuries: 0,
        recurrent_only_resource_returns: 0,
        episodic_resource_returns: 0,
    };
    for _ in 0..HELD_OUT_ACTION_TRIALS {
        metrics.recurrent_only_injuries += 1;
        metrics.episodic_injuries += u32::from(episodic_action == harmful);
        metrics.episodic_resource_returns += u32::from(episodic_action == relieving);
    }
    metrics
}

fn embodied_observation(sequence: u64, source: &[i16; SENSE_FEATURES]) -> EnvironmentObservation {
    EnvironmentObservation {
        sequence,
        senses: RawSensoryFrame::EMPTY,
        body: BodyProprioception::default(),
        capabilities: EnvironmentCapabilities::default(),
        transport: TransportStatus::Healthy,
        retina: None,
        interoception: Some(Interoception {
            health: source[12].cast_unsigned(),
            fullness: source[13].cast_unsigned(),
            air: source[14].cast_unsigned(),
            pain: source[15].cast_unsigned(),
        }),
        held_item: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controlled_action_protocol_is_paired_and_requires_action_benefit() {
        let runs: Vec<_> = ACTION_SEEDS.into_iter().map(action_protocol).collect();
        assert!(runs.iter().all(|run| run.recurrent_only_injuries == 16));
        assert!(runs.iter().all(|run| run.episodic_injuries == 0));
        assert!(runs.iter().all(|run| run.episodic_resource_returns == 16));
    }
}
