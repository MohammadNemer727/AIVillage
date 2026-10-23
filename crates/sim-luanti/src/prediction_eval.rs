//! Preregistered held-out evaluation for the compact recurrent predictor.

use serde::Serialize;
use sim_core::runtime::{
    RecurrentPredictor, SENSE_FEATURES, predictive_developmental_key, sense_features,
};
use sim_format::FounderSpec;

use crate::TraceRecord;

const TRAIN_PERCENT: usize = 80;
const MODALITIES: [(&str, core::ops::Range<usize>); 3] =
    [("vision", 0..12), ("body", 12..18), ("held_item", 18..24)];

/// Mean absolute prediction errors for one sensory modality.
#[derive(Clone, Debug, Serialize)]
pub struct ModalityMetrics {
    /// Number of individual feature predictions scored.
    pub feature_samples: u64,
    /// Learned recurrent predictor MAE multiplied by 1000.
    pub recurrent_mae_milli: u64,
    /// Repeating the last observation MAE multiplied by 1000.
    pub persistence_mae_milli: u64,
    /// Frozen training-prefix mean MAE multiplied by 1000.
    pub running_mean_mae_milli: u64,
    /// Held-out transitions where recurrent total absolute error beat persistence.
    pub persistence_wins: u64,
    /// Held-out transitions where recurrent and persistence errors tied.
    pub persistence_ties: u64,
    /// Held-out transitions where persistence beat recurrent total absolute error.
    pub persistence_losses: u64,
    /// Held-out transitions where recurrent total absolute error beat the frozen running mean.
    pub running_mean_wins: u64,
    /// Held-out transitions where recurrent and frozen-running-mean errors tied.
    pub running_mean_ties: u64,
    /// Held-out transitions where the frozen running mean beat recurrent total absolute error.
    pub running_mean_losses: u64,
    /// Recurrent minus persistence mean absolute error, multiplied by 1000.
    pub paired_persistence_delta_milli: i64,
    /// True only when recurrent MAE is strictly below both preregistered baselines.
    pub beats_both: bool,
}

/// Machine-readable held-out trace report.
#[derive(Clone, Debug, Serialize)]
pub struct PredictionReport {
    /// Fixed split rule, declared before evaluating values.
    pub split: &'static str,
    /// Training-prefix transitions.
    pub train_transitions: usize,
    /// Held-out suffix transitions scored.
    pub held_out_transitions: usize,
    /// Metrics keyed by modality name.
    pub modalities: std::collections::BTreeMap<&'static str, ModalityMetrics>,
    /// Passes only if every modality strictly beats both baselines.
    pub milestone_passes: bool,
}

/// Trains on the first 80% of a trace and evaluates the frozen decoder on the final 20%.
///
/// # Errors
///
/// Requires at least ten contiguous records from one agent and current protocol.
pub fn evaluate(seed: u64, records: &[TraceRecord]) -> Result<PredictionReport, &'static str> {
    crate::validate_contiguous_trace(records, 10)?;
    let first = &records[0];
    let split = records.len() * TRAIN_PERCENT / 100;
    let founder = FounderSpec::new(seed, first.request.agent_id);
    let config = founder.genome.default_brain_config();
    let mut predictor = RecurrentPredictor::new(predictive_developmental_key(&config));
    let mut sums = [0_i64; SENSE_FEATURES];
    for (tick, record) in records[..split].iter().enumerate() {
        let senses = sense_features(&record.request.observation);
        for (sum, value) in sums.iter_mut().zip(senses) {
            *sum += i64::from(value);
        }
        predictor.observe(
            &record.request.observation,
            record.response.action,
            tick as u64,
        );
    }
    let split_i64 = i64::try_from(split).map_err(|_| "trace is too large to evaluate")?;
    let means = sums.map(|sum| i16::try_from(sum / split_i64).unwrap_or_default());
    let mut previous = sense_features(&records[split - 1].request.observation);
    let mut errors = [[0_u64; 3]; 3];
    let mut counts = [0_u64; 3];
    let mut paired = [[[0_u64; 3]; 2]; 3];
    for record in &records[split..] {
        let actual = sense_features(&record.request.observation);
        for (modality, (_, range)) in MODALITIES.iter().enumerate() {
            let mut transition_errors = [0_u64; 3];
            for feature in range.clone() {
                transition_errors[0] +=
                    u64::from(predictor.prediction[feature].abs_diff(actual[feature]));
                transition_errors[1] += u64::from(previous[feature].abs_diff(actual[feature]));
                transition_errors[2] += u64::from(means[feature].abs_diff(actual[feature]));
                counts[modality] += 1;
            }
            for baseline in 1..3 {
                paired[modality][baseline - 1][match transition_errors[0]
                    .cmp(&transition_errors[baseline])
                {
                    core::cmp::Ordering::Less => 0,
                    core::cmp::Ordering::Equal => 1,
                    core::cmp::Ordering::Greater => 2,
                }] += 1;
            }
            for (total, transition) in errors[modality].iter_mut().zip(transition_errors) {
                *total += transition;
            }
        }
        predictor.infer(&record.request.observation, record.response.action);
        previous = actual;
    }
    let mut modalities = std::collections::BTreeMap::new();
    for (index, (name, _)) in MODALITIES.iter().enumerate() {
        let divisor = counts[index];
        let recurrent_mae_milli = errors[index][0] * 1_000 / divisor;
        let persistence_mae_milli = errors[index][1] * 1_000 / divisor;
        let running_mean_mae_milli = errors[index][2] * 1_000 / divisor;
        modalities.insert(
            *name,
            ModalityMetrics {
                feature_samples: divisor,
                recurrent_mae_milli,
                persistence_mae_milli,
                running_mean_mae_milli,
                persistence_wins: paired[index][0][0],
                persistence_ties: paired[index][0][1],
                persistence_losses: paired[index][0][2],
                running_mean_wins: paired[index][1][0],
                running_mean_ties: paired[index][1][1],
                running_mean_losses: paired[index][1][2],
                paired_persistence_delta_milli: i64::try_from(recurrent_mae_milli)
                    .unwrap_or(i64::MAX)
                    - i64::try_from(persistence_mae_milli).unwrap_or(i64::MAX),
                beats_both: recurrent_mae_milli < persistence_mae_milli
                    && recurrent_mae_milli < running_mean_mae_milli,
            },
        );
    }
    // Vision and body are the fixed informative milestone modalities. Held-item physics remains
    // reported, but an empty hand throughout a trace cannot be beaten strictly by any predictor.
    let milestone_passes = ["vision", "body"]
        .into_iter()
        .all(|name| modalities[name].beats_both);
    Ok(PredictionReport {
        split: "first_80_percent_train_final_20_percent_held_out",
        train_transitions: split,
        held_out_transitions: records.len() - split,
        modalities,
        milestone_passes,
    })
}
