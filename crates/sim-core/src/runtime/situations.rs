//! Situations learned separately for each experiment.
//!
//! What matters depends on the experiment: whether something is near ahead matters for a hand
//! pulse, whether the body is already moving matters for thrust. Each experiment therefore grows
//! its own small tree of situations over the generic sense facts, so the distinctions that serve
//! one experiment never fragment another's experience.

use serde::{Deserialize, Serialize};

use super::{
    AffordanceConfig, FEATURE_POINTS_CAP, OUTCOME_FEATURES, SPREAD_FLOOR, SPREAD_POINTS, Split,
};

/// Most situations one experiment can tell apart.
pub const MAX_ACTION_SITUATIONS: usize = 8;

/// Running outcome statistics of one experiment within one of its situations, beside the
/// outcomes of every experiment there.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SituationStats {
    /// Direct outcomes of the experiment observed here, saturating.
    pub visits: u16,
    /// Recent average outcome following the experiment here, scaled by 256.
    pub after: [i32; OUTCOME_FEATURES],
    /// Recent mean absolute deviation of that outcome, scaled by 256.
    pub after_spread: [i32; OUTCOME_FEATURES],
    /// Recent average outcome of any experiment here, scaled by 256.
    pub overall: [i32; OUTCOME_FEATURES],
    /// Recent mean absolute deviation of that outcome, scaled by 256.
    pub overall_spread: [i32; OUTCOME_FEATURES],
    /// Whether `overall` has been set from a first outcome.
    pub primed: bool,
}

impl SituationStats {
    /// Credits `now` to the experiment, taken `age` decisions ago; credit halves with each
    /// decision of age, as for pooled statistics.
    pub(super) fn credit(
        &mut self,
        age: usize,
        now: &[i32; OUTCOME_FEATURES],
        config: &AffordanceConfig,
    ) {
        if self.visits == 0 {
            return;
        }
        let horizon = i32::from(self.visits.min(config.action_horizon)) << age;
        for (feature, value) in now.iter().copied().enumerate() {
            if self.visits > 1 {
                let deviation = (value - self.after[feature]).abs();
                self.after_spread[feature] += (deviation - self.after_spread[feature]) / horizon;
            }
            self.after[feature] += (value - self.after[feature]) / horizon;
        }
    }

    /// Moves the outcomes of any experiment here toward `now`, or starts them from it.
    pub(super) fn observe_overall(
        &mut self,
        now: &[i32; OUTCOME_FEATURES],
        config: &AffordanceConfig,
    ) {
        if !self.primed {
            self.overall = *now;
            self.primed = true;
            return;
        }
        let horizon = i32::from(config.overall_horizon);
        for (feature, value) in now.iter().copied().enumerate() {
            let deviation = (value - self.overall[feature]).abs();
            self.overall_spread[feature] += (deviation - self.overall_spread[feature]) / horizon;
            self.overall[feature] += (value - self.overall[feature]) / horizon;
        }
    }

    /// Lets the experiment's memory here drift back toward the outcome of any experiment.
    pub(super) fn forget(&mut self, config: &AffordanceConfig) {
        let forgetting = i32::from(config.forgetting_horizon);
        for feature in 0..OUTCOME_FEATURES {
            self.after[feature] += (self.overall[feature] - self.after[feature]) / forgetting;
            self.after_spread[feature] +=
                (self.overall_spread[feature] - self.after_spread[feature]) / forgetting;
        }
    }

    /// How strongly the experiment's outcomes here differ from any experiment's, in average and
    /// spread, each feature judged in units of its spread here.
    pub(super) fn contingency(&self) -> u32 {
        if self.visits == 0 {
            return 0;
        }
        let points: i64 = (0..OUTCOME_FEATURES)
            .map(|feature| {
                let average = (self.after[feature] - self.overall[feature]).abs();
                let spread = (self.after_spread[feature] - self.overall_spread[feature]).abs();
                let usual = i64::from(self.overall_spread[feature].max(SPREAD_FLOOR));
                (i64::from(average.saturating_add(spread)) * SPREAD_POINTS / usual)
                    .min(FEATURE_POINTS_CAP)
            })
            .sum();
        u32::try_from(points).unwrap_or(u32::MAX)
    }

    pub(super) fn words(&self) -> impl Iterator<Item = i64> + '_ {
        [i64::from(self.visits), i64::from(self.primed)]
            .into_iter()
            .chain(self.after.iter().map(|value| i64::from(*value)))
            .chain(self.after_spread.iter().map(|value| i64::from(*value)))
            .chain(self.overall.iter().map(|value| i64::from(*value)))
            .chain(self.overall_spread.iter().map(|value| i64::from(*value)))
    }
}

/// One experiment's own situations: a tree of splits over sense facts, with statistics and the
/// tick of its last try in each.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionSituations {
    /// Splits in the order they were made.
    pub splits: Vec<Split>,
    /// Statistics of each situation, one more than there are splits.
    pub stats: Vec<SituationStats>,
    /// Decision tick at which the experiment was last tried in each situation.
    pub last_tried: Vec<u64>,
}

impl Default for ActionSituations {
    fn default() -> Self {
        Self {
            splits: Vec::new(),
            stats: vec![SituationStats::default()],
            last_tried: vec![0],
        }
    }
}

impl ActionSituations {
    /// The situation of a decision made with `facts`.
    #[must_use]
    pub fn situation_of(&self, facts: u32) -> usize {
        self.splits.iter().fold(0, |situation, split| {
            if situation == usize::from(split.parent) && facts & (1 << split.fact) != 0 {
                usize::from(split.child)
            } else {
                situation
            }
        })
    }

    pub(super) fn is_valid(&self, facts: usize) -> bool {
        self.splits.len() < MAX_ACTION_SITUATIONS
            && self.stats.len() == self.splits.len() + 1
            && self.last_tried.len() == self.stats.len()
            && self.splits.iter().enumerate().all(|(index, split)| {
                usize::from(split.child) == index + 1
                    && split.parent < split.child
                    && usize::from(split.fact) < facts
            })
    }

    /// Splits `parent` on `fact`; the new situation starts from a copy of its parent.
    pub(super) fn split(&mut self, parent: usize, fact: usize, at: u64) {
        let child = self.stats.len();
        self.splits.push(Split {
            parent: u8::try_from(parent).unwrap_or_default(),
            fact: u8::try_from(fact).unwrap_or_default(),
            child: u8::try_from(child).unwrap_or_default(),
            at,
        });
        self.stats.push(self.stats[parent]);
        self.last_tried.push(self.last_tried[parent]);
    }
}
