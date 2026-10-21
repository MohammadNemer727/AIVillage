//! Standalone deterministic one-agent runtime.

use core::fmt;
use std::error::Error;

use serde::{Deserialize, Serialize};
use sim_kernels::{brain::EPISODE_CAPACITY, derive, gpu::GpuBrain, heading};

use crate::{
    BrainConfig, CHEMICAL_RECEPTOR_COUNT, PredictiveBrain, RETINAL_RAY_COUNT, RawSensoryFrame,
    RetinalSample, adapt_chemical_receptors, compose_raw_sensory_frame, decode_motor_outputs,
    environment::{
        DeadChannels, EnvironmentAdapter, EnvironmentCapabilities, EnvironmentObservation,
        EnvironmentTransition, PrimitiveAction, TransportStatus,
    },
    scale_v2_orientation_turn,
};

mod perception;
mod situations;

pub use perception::{MAX_SCENES, SENSE_DIMENSIONS, ScenePerception};
pub use situations::{ActionSituations, MAX_ACTION_SITUATIONS, SituationStats};

/// Explicit action-selection policy for a standalone founder.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ExperimentMode {
    /// Frozen predictive founder semantics (the default and legacy baseline).
    #[default]
    PredictiveBaseline,
    /// Deterministic, intrinsic-only causal experimentation.
    AffordanceDiscovery,
}

/// Number of primitive experiments the affordance learner chooses between.
pub const AFFORDANCE_ACTIONS: usize = 12;
/// Number of bounded outcome features the affordance learner tracks.
pub const OUTCOME_FEATURES: usize = 8;
/// Index of the hand-contact feature in [`AffordanceLearner::expected_after`].
pub const HAND_CONTACT_FEATURE: usize = 5;
/// Index of the largest apparent retinal motion feature.
pub const VISUAL_MOTION_FEATURE: usize = 6;
/// Index of the largest per-ray view change feature.
pub const VISUAL_CHANGE_FEATURE: usize = 7;
/// Retinal channels compared between consecutive views: red, green, blue, brightness, occlusion.
const VIEW_CHANNELS: usize = 5;
/// Fixed-point scale of the outcome averages.
const OUTCOME_SCALE: i32 = 256;
/// Smallest overall spread an outcome is judged against, so a nearly constant receptor cannot
/// turn receptor noise into huge contingency.
const SPREAD_FLOOR: i32 = 16 * OUTCOME_SCALE;
/// Score points for an outcome that differs from normal by one overall spread.
const SPREAD_POINTS: i64 = 64;
/// Score points one outcome feature can contribute at most.
const FEATURE_POINTS_CAP: i64 = 8 * SPREAD_POINTS;
/// Longest supported eligibility span; [`AffordanceConfig::eligibility_span`] selects how much
/// of it is used.
pub const MAX_ELIGIBILITY_SPAN: usize = 8;
/// Expectiles tracked per outcome feature, at asymmetries 0.1 to 0.9 in steps of 0.1.
pub const EXPECTILES: usize = 9;

/// What makes an experiment interesting to the affordance learner.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ScoringRule {
    /// How far an experiment's outcomes differ from normal, in average and spread (after Xu &
    /// Triesch's causal action influence).
    #[default]
    Contingency,
    /// How fast the learner's prediction error for an experiment is changing (after Oudeyer's
    /// learning progress): interesting while being learned, boring once mastered or
    /// unlearnable.
    LearningProgress,
}

/// What a learned-situation founder splits its situations on.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Perception {
    /// The [`SENSE_FACTS`] generic, designer-chosen sense facts.
    #[default]
    Designed,
    /// Up to [`MAX_SCENES`] scenes the founder learns from its raw senses; fact `k` holds while
    /// the founder is in scene `k`.
    Scenes,
}

/// A readable name for `fact` under `perception`, for reports.
#[must_use]
pub fn fact_name(perception: Perception, fact: usize) -> String {
    match perception {
        Perception::Designed => sense_fact_name(fact),
        Perception::Scenes => format!("scene {fact}"),
    }
}

/// How the affordance learner summarizes the outcomes that follow an experiment.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum OutcomeModel {
    /// Average and mean absolute deviation per outcome feature.
    #[default]
    Moments,
    /// Nine expectiles per outcome feature: asymmetric running averages that together describe
    /// the outcome distribution's shape, as in distributional reinforcement learning (Rowland et
    /// al.). They are compared by their mean absolute gap, a distributional distance after Xu &
    /// Triesch's causal action influence score, so any change in an outcome's shape counts, not
    /// only its average or spread. Unlike quantiles they move in proportion to the error, so they
    /// learn as fast as averages.
    Expectiles,
}

/// Tunable settings of the affordance learner. They are checkpointed with its statistics, so a
/// resumed run keeps the settings it started with.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct AffordanceConfig {
    /// What makes an experiment interesting.
    pub rule: ScoringRule,
    /// Tries after which an experiment's averages stop settling and keep tracking a changed
    /// world: the learner's memory window.
    pub action_horizon: u16,
    /// Decisions over which the overall averages adapt.
    pub overall_horizon: u16,
    /// Decisions over which an unused experiment's memory drifts back to normal.
    pub forgetting_horizon: u16,
    /// Recent experiments that share credit for each outcome, halving with each decision of
    /// age, so a consequence that arrives late can still reach its cause.
    pub eligibility_span: usize,
    /// Curiosity gained per decision since an experiment was last tried.
    pub recency_rate: u64,
    /// Largest curiosity an untried experiment can gather.
    pub recency_cap: u64,
    /// Decisions a chosen experiment is held for before the next choice, like an infant's
    /// bout of kicking. One chooses afresh every decision. Short bouts also bridge short delays
    /// between an action and its consequence.
    pub bout_length: u16,
    /// Which situations the learner tells apart.
    pub context: ContextMode,
    /// Tries of an experiment within a situation at which the situation's own statistics weigh
    /// as much as the pooled ones.
    pub context_prior: u16,
    /// How outcomes are summarized when judging contingency.
    pub outcome_model: OutcomeModel,
    /// What learned situations are split on.
    pub perception: Perception,
}

impl Default for AffordanceConfig {
    fn default() -> Self {
        Self {
            rule: ScoringRule::Contingency,
            action_horizon: 8,
            overall_horizon: 16,
            forgetting_horizon: 32,
            eligibility_span: 4,
            recency_rate: 4,
            recency_cap: 1_024,
            bout_length: 3,
            context: ContextMode::None,
            context_prior: 8,
            outcome_model: OutcomeModel::Moments,
            perception: Perception::Designed,
        }
    }
}

impl AffordanceConfig {
    /// Checks that every setting is within its supported range.
    ///
    /// # Errors
    ///
    /// Returns a description of the first setting out of range.
    pub fn validate(&self) -> Result<(), String> {
        if self.action_horizon == 0 || self.overall_horizon == 0 || self.forgetting_horizon == 0 {
            return Err("affordance horizons must be at least 1".to_owned());
        }
        if self.action_horizon > 1_024 || self.overall_horizon > 1_024 {
            return Err("affordance horizons must be at most 1024".to_owned());
        }
        if !(1..=MAX_ELIGIBILITY_SPAN).contains(&self.eligibility_span) {
            return Err(format!(
                "eligibility span must be between 1 and {MAX_ELIGIBILITY_SPAN}"
            ));
        }
        if self.recency_cap > u64::from(u32::MAX / 4) {
            return Err("recency cap is out of range".to_owned());
        }
        if !(1..=64).contains(&self.bout_length) {
            return Err("bout length must be between 1 and 64".to_owned());
        }
        if !(1..=1_024).contains(&self.context_prior) {
            return Err("context prior must be between 1 and 1024".to_owned());
        }
        Ok(())
    }
}

/// Running outcome statistics for every experiment, either over all situations or within one.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Statistics {
    /// Number of observed direct outcomes per experiment, saturating.
    pub visits: [u16; AFFORDANCE_ACTIONS],
    /// Recent average outcome following each experiment, scaled by 256.
    pub after: [[i32; OUTCOME_FEATURES]; AFFORDANCE_ACTIONS],
    /// Recent mean absolute deviation of the outcome following each experiment, scaled by 256.
    pub after_spread: [[i32; OUTCOME_FEATURES]; AFFORDANCE_ACTIONS],
    /// Recent average outcome over all experiments, scaled by 256.
    pub overall: [i32; OUTCOME_FEATURES],
    /// Recent mean absolute deviation of the outcome over all experiments, scaled by 256.
    pub overall_spread: [i32; OUTCOME_FEATURES],
    /// Whether `overall` has been set from a first outcome.
    pub primed: bool,
    /// Recent expectiles of the outcome following each experiment, scaled by 256.
    #[serde(default)]
    pub after_expectiles: [[[i32; EXPECTILES]; OUTCOME_FEATURES]; AFFORDANCE_ACTIONS],
    /// Recent expectiles of the outcome over all experiments, scaled by 256.
    #[serde(default)]
    pub overall_expectiles: [[i32; EXPECTILES]; OUTCOME_FEATURES],
}

/// Moves expectiles toward `value` over `horizon`: each is a running average that weighs values
/// above it by tau and values below it by 1 - tau (doubled, so tau 0.5 is the plain average).
fn track_expectiles(expectiles: &mut [i32; EXPECTILES], value: i32, horizon: i32) {
    let levels = i64::try_from(EXPECTILES).unwrap_or(i64::MAX) + 1;
    for (tau, expectile) in (1_i64..).zip(expectiles.iter_mut()) {
        let weight = if value >= *expectile {
            tau
        } else {
            levels - tau
        };
        let error = i64::from(value - *expectile);
        let step = error * 2 * weight / (levels * i64::from(horizon));
        *expectile += i32::try_from(step).unwrap_or_default();
    }
}

impl Statistics {
    /// Starts the overall averages from a first outcome.
    fn prime(&mut self, now: &[i32; OUTCOME_FEATURES]) {
        self.overall = *now;
        self.overall_expectiles = now.map(|value| [value; EXPECTILES]);
        self.primed = true;
    }

    /// Credits `now` to `action`, taken `age` decisions ago; credit halves with each decision of
    /// age. Spreads are measured against the averages before this outcome joins them, and an
    /// experiment's first outcome sets its average and has no spread yet.
    fn credit(
        &mut self,
        action: usize,
        age: usize,
        now: &[i32; OUTCOME_FEATURES],
        config: &AffordanceConfig,
    ) {
        let count = self.visits[action];
        if count == 0 {
            return;
        }
        let horizon = i32::from(count.min(config.action_horizon)) << age;
        for (feature, value) in now.iter().copied().enumerate() {
            if count > 1 {
                let deviation = (value - self.after[action][feature]).abs();
                self.after_spread[action][feature] +=
                    (deviation - self.after_spread[action][feature]) / horizon;
                track_expectiles(&mut self.after_expectiles[action][feature], value, horizon);
            } else if age == 0 {
                self.after_expectiles[action][feature] = [value; EXPECTILES];
            }
            self.after[action][feature] += (value - self.after[action][feature]) / horizon;
        }
    }

    /// Moves the overall averages toward `now`, or starts them from it.
    fn observe_overall(&mut self, now: &[i32; OUTCOME_FEATURES], config: &AffordanceConfig) {
        if !self.primed {
            self.prime(now);
            return;
        }
        let horizon = i32::from(config.overall_horizon);
        for (feature, value) in now.iter().copied().enumerate() {
            track_expectiles(&mut self.overall_expectiles[feature], value, horizon);
            let deviation = (value - self.overall[feature]).abs();
            self.overall_spread[feature] += (deviation - self.overall_spread[feature]) / horizon;
            self.overall[feature] += (value - self.overall[feature]) / horizon;
        }
    }

    /// Lets every experiment but `kept` drift back toward the overall outcome.
    fn forget(&mut self, kept: usize, config: &AffordanceConfig) {
        let forgetting = i32::from(config.forgetting_horizon);
        for other in (0..AFFORDANCE_ACTIONS).filter(|other| *other != kept) {
            for feature in 0..OUTCOME_FEATURES {
                self.after[other][feature] +=
                    (self.overall[feature] - self.after[other][feature]) / forgetting;
                self.after_spread[other][feature] +=
                    (self.overall_spread[feature] - self.after_spread[other][feature]) / forgetting;
                for (expectile, normal) in self.after_expectiles[other][feature]
                    .iter_mut()
                    .zip(self.overall_expectiles[feature])
                {
                    *expectile += (normal - *expectile) / forgetting;
                }
            }
        }
    }

    /// Score points for a `difference` on `feature`, in units of that feature's overall spread.
    fn points(&self, feature: usize, difference: i32) -> i64 {
        let spread = i64::from(self.overall_spread[feature].max(SPREAD_FLOOR));
        (i64::from(difference.abs()) * SPREAD_POINTS / spread).min(FEATURE_POINTS_CAP)
    }

    /// How badly `now` was predicted after `action`, in score points scaled by 256.
    fn prediction_error(&self, action: usize, now: &[i32; OUTCOME_FEATURES]) -> i32 {
        let points: i64 = (0..OUTCOME_FEATURES)
            .map(|feature| self.points(feature, now[feature] - self.after[action][feature]))
            .sum();
        i32::try_from(points * i64::from(OUTCOME_SCALE)).unwrap_or(i32::MAX)
    }

    /// How strongly an experiment's outcomes differ from the overall outcomes, with each
    /// feature judged in units of its overall spread: by average and spread, or by the mean gap
    /// between their expectiles.
    fn contingency(&self, action: usize, model: OutcomeModel) -> u32 {
        if self.visits[action] == 0 {
            return 0;
        }
        let points: i64 = (0..OUTCOME_FEATURES)
            .map(|feature| {
                let difference = match model {
                    OutcomeModel::Moments => {
                        let average = (self.after[action][feature] - self.overall[feature]).abs();
                        let spread = (self.after_spread[action][feature]
                            - self.overall_spread[feature])
                            .abs();
                        average.saturating_add(spread)
                    }
                    OutcomeModel::Expectiles => {
                        let gap: i64 = self.after_expectiles[action][feature]
                            .iter()
                            .zip(self.overall_expectiles[feature])
                            .map(|(after, normal)| i64::from((after - normal).abs()))
                            .sum();
                        let count = i64::try_from(EXPECTILES).unwrap_or(i64::MAX);
                        i32::try_from(gap / count).unwrap_or(i32::MAX)
                    }
                };
                self.points(feature, difference)
            })
            .sum();
        u32::try_from(points).unwrap_or(u32::MAX)
    }

    fn fingerprint(&self, mut fingerprint: u64, tag: u64) -> u64 {
        let words = self
            .visits
            .iter()
            .map(|value| i64::from(*value))
            .chain(self.after.iter().flatten().map(|value| i64::from(*value)))
            .chain(
                self.after_spread
                    .iter()
                    .flatten()
                    .map(|value| i64::from(*value)),
            )
            .chain(self.overall.iter().map(|value| i64::from(*value)))
            .chain(self.overall_spread.iter().map(|value| i64::from(*value)))
            .chain([i64::from(self.primed)])
            .chain(
                self.after_expectiles
                    .iter()
                    .flatten()
                    .flatten()
                    .map(|value| i64::from(*value)),
            )
            .chain(
                self.overall_expectiles
                    .iter()
                    .flatten()
                    .map(|value| i64::from(*value)),
            );
        for (index, value) in words.enumerate() {
            fingerprint = derive(fingerprint, value.cast_unsigned(), index as u64, tag);
        }
        fingerprint
    }
}

/// Situation bits a context-conditioned learner reads from its senses. They are fixed, bounded
/// receptor thresholds and never name an object, place, or task.
pub mod context_bits {
    /// The eye rays nearest straight ahead are blocked within about two nodes.
    pub const CLOSE_AHEAD: usize = 1;
    /// Something moves noticeably on some eye ray.
    pub const MOTION_IN_VIEW: usize = 2;
    /// The body felt touch or hand contact.
    pub const CONTACT: usize = 4;
    /// The eyes are tilted down.
    pub const LOOKING_DOWN: usize = 8;
}

/// Number of situations a context-conditioned learner distinguishes.
pub const CONTEXTS: usize = 16;
/// Occlusion from which a ray counts as blocked close ahead (about 2.2 nodes away).
const CLOSE_OCCLUSION: u16 = 160;
/// Retinal motion from which something counts as moving in view.
const NOTICEABLE_MOTION: u16 = 64;
/// Eye pitch below which the eyes count as tilted down (centred at 128).
const LOOKING_DOWN_BELOW: u16 = 112;

/// Which situations the affordance learner tells apart.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ContextMode {
    /// One situation: contingency is pooled over everything (after Xu & Triesch).
    #[default]
    None,
    /// Sixteen situations from four sense bits (see [`context_bits`]): contingency is judged
    /// within the current situation, borrowing from the pooled statistics while a situation is
    /// still rarely seen.
    SenseBits,
    /// Situations the founder discovers itself (after Oudeyer's IAC): it starts with one and
    /// splits a situation on one of [`SENSE_FACTS`] generic sense facts only when the effect of
    /// some experiment clearly differs between the two sides.
    Learned,
    /// Situations the founder discovers separately for each experiment: each experiment splits
    /// its own situations on the sense facts that change its effect, so a distinction that
    /// matters for one experiment never fragments another's experience.
    PerAction,
}

/// Generic sense facts a learned-situation founder may split on: for each of four directions
/// whether something is near, blocked within range, moving, or standing out in colour, and five
/// body facts. They are fixed receptor thresholds that never name an object; the founder chooses
/// which, if any, matter.
pub const SENSE_FACTS: usize = 21;
/// Eye rays of each direction: ahead, left, behind, right.
const SECTORS: [[usize; 3]; 4] = [[15, 0, 1], [3, 4, 5], [7, 8, 9], [11, 12, 13]];
const SECTOR_NAMES: [&str; 4] = ["ahead", "left", "behind", "right"];
const SECTOR_FACT_NAMES: [&str; 4] = ["near", "blocked", "moving", "stands out"];
const BODY_FACT_NAMES: [&str; 5] = [
    "touch",
    "hand contact",
    "looking down",
    "looking up",
    "in the air",
];
/// Occlusion from which a ray counts as blocked within range (about 4.5 nodes away).
const BLOCKED_OCCLUSION: u16 = 64;
/// Colour difference (sum over red, green, blue) from the average of all rays from which a
/// direction stands out.
const COLOUR_CONTRAST: u32 = 96;
/// Eye pitch above which the eyes count as tilted up (centred at 128).
const LOOKING_UP_ABOVE: u16 = 144;
/// Recent experiences kept for judging splits.
pub const EXPERIENCE_MEMORY: usize = 256;
/// Decisions between attempts to split a situation.
const SPLIT_INTERVAL: u64 = 32;
/// Experiences each side of a split needs before the split is judged.
const SPLIT_MIN_SIDE: i64 = 16;
/// Tries of an experiment each side of a split needs for the experiment to count.
const SPLIT_MIN_TRIES: i64 = 8;
/// Score points by which an experiment's effect must differ between the two sides.
const SPLIT_GAIN: i64 = 128;

/// The [`SENSE_FACTS`] an observation makes true, one bit each.
#[must_use]
pub fn sense_facts(observation: &EnvironmentObservation) -> u32 {
    let retinal = &observation.senses.retinal;
    let colour = |sample: &RetinalSample| {
        u32::from(sample.red) + u32::from(sample.green) + u32::from(sample.blue)
    };
    let rays = u32::try_from(RETINAL_RAY_COUNT).unwrap_or(u32::MAX);
    let average = retinal.iter().map(colour).sum::<u32>() / rays;
    let mut facts = 0;
    for (sector, rays) in SECTORS.iter().enumerate() {
        let samples = rays.map(|ray| retinal[ray]);
        let checks = [
            samples
                .iter()
                .any(|sample| sample.occlusion >= CLOSE_OCCLUSION),
            samples
                .iter()
                .any(|sample| sample.occlusion >= BLOCKED_OCCLUSION),
            samples
                .iter()
                .any(|sample| sample.motion.unsigned_abs() >= NOTICEABLE_MOTION),
            samples
                .iter()
                .any(|sample| colour(sample).abs_diff(average) >= COLOUR_CONTRAST),
        ];
        for (index, check) in checks.into_iter().enumerate() {
            facts |= u32::from(check) << (sector * 4 + index);
        }
    }
    let body = observation.body;
    let checks = [
        observation.senses.touch > 0,
        body.hand_contact > 0,
        body.eye_pitch < LOOKING_DOWN_BELOW,
        body.eye_pitch > LOOKING_UP_ABOVE,
        body.grounded == 0,
    ];
    for (index, check) in checks.into_iter().enumerate() {
        facts |= u32::from(check) << (16 + index);
    }
    facts
}

/// A readable name for sense fact `fact`, for reports.
#[must_use]
pub fn sense_fact_name(fact: usize) -> String {
    if fact < 16 {
        format!("{} {}", SECTOR_FACT_NAMES[fact % 4], SECTOR_NAMES[fact / 4])
    } else {
        BODY_FACT_NAMES
            .get(fact - 16)
            .copied()
            .unwrap_or("unknown")
            .to_owned()
    }
}

/// One remembered decision: the sense facts it was made in, its situation, the experiment, and
/// the outcome that followed, in receptor units.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Experience {
    /// Sense facts true when the decision was made.
    pub facts: u32,
    /// Learned situation the decision was made in.
    pub situation: u8,
    /// Experiment chosen.
    pub action: u8,
    /// Outcome that followed, in receptor units.
    pub outcome: [i16; OUTCOME_FEATURES],
}

/// A situation the founder split on a sense fact: decisions in `parent` where `fact` holds
/// belong to `child` from then on.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Split {
    /// Situation that was split.
    pub parent: u8,
    /// Sense fact it was split on.
    pub fact: u8,
    /// New situation for decisions where the fact holds.
    pub child: u8,
    /// Outcomes seen when the split was made.
    pub at: u64,
}

/// The situation an observation puts the founder in, as a [`context_bits`] combination.
#[must_use]
pub fn sense_context(observation: &EnvironmentObservation) -> usize {
    let retinal = &observation.senses.retinal;
    let ahead = [RETINAL_RAY_COUNT - 1, 0, 1];
    let mut context = 0;
    if ahead
        .iter()
        .any(|ray| retinal[*ray].occlusion >= CLOSE_OCCLUSION)
    {
        context |= context_bits::CLOSE_AHEAD;
    }
    if retinal
        .iter()
        .any(|sample| sample.motion.unsigned_abs() >= NOTICEABLE_MOTION)
    {
        context |= context_bits::MOTION_IN_VIEW;
    }
    if observation.senses.touch > 0 || observation.body.hand_contact > 0 {
        context |= context_bits::CONTACT;
    }
    if observation.body.eye_pitch < LOOKING_DOWN_BELOW {
        context |= context_bits::LOOKING_DOWN;
    }
    context
}

/// Checkpointed bounded statistics for intrinsic action discovery.
///
/// Under the default [`ScoringRule::Contingency`] the learner measures how far the outcomes that
/// follow an experiment differ from the outcomes overall, in both their average and their
/// spread. Each outcome is judged in units of its own overall spread, so a small visual change
/// can matter as much as a large change of body posture. An effect that follows regardless of
/// action, or that stops following, carries no contingency, so neither keeps attracting
/// experiments. With a [`ContextMode`] other than `None` the same is judged within the current
/// situation as well.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AffordanceLearner {
    /// Settings the statistics below were gathered with.
    #[serde(default)]
    pub config: AffordanceConfig,
    /// Statistics pooled over every situation.
    pub pooled: Statistics,
    /// Statistics within each of the [`CONTEXTS`] situations, used with
    /// a [`ContextMode`] other than `None`. Held on the heap: together they are large.
    #[serde(default = "blank_contexts")]
    pub contexts: Box<[Statistics]>,
    /// Recent prediction error of each experiment's direct outcome over a short window, in
    /// score points scaled by 256.
    #[serde(default)]
    pub error_fast: [i32; AFFORDANCE_ACTIONS],
    /// The same prediction error over a long window; its gap to the short window is learning
    /// progress.
    #[serde(default)]
    pub error_slow: [i32; AFFORDANCE_ACTIONS],
    /// Decision tick at which each experiment was last selected.
    pub last_tried: [u64; AFFORDANCE_ACTIONS],
    /// Decision tick at which each experiment was last selected in each situation, so curiosity
    /// is per situation under a [`ContextMode`] other than `None`.
    #[serde(default)]
    pub last_tried_in: [[u64; AFFORDANCE_ACTIONS]; CONTEXTS],
    /// Last selected experiment, whose outcome the next observation reports.
    pub previous_action: usize,
    /// Recent experiments, most recent first, that share credit for the next outcome.
    #[serde(default = "no_recent_actions")]
    pub recent_actions: [usize; MAX_ELIGIBILITY_SPAN],
    /// The situation each recent experiment was chosen in.
    #[serde(default)]
    pub recent_contexts: [usize; MAX_ELIGIBILITY_SPAN],
    /// The situation the latest observation puts the founder in.
    #[serde(default)]
    pub current_context: usize,
    /// The sense facts of the latest observation, under [`ContextMode::Learned`].
    #[serde(default)]
    pub current_facts: u32,
    /// The splits the founder has made, in order, under [`ContextMode::Learned`].
    #[serde(default)]
    pub splits: Vec<Split>,
    /// Recent experiences for judging splits, a ring of at most [`EXPERIENCE_MEMORY`].
    #[serde(default)]
    pub experiences: Vec<Experience>,
    /// Where the next experience is written once the ring is full.
    #[serde(default)]
    pub next_experience: usize,
    /// Outcomes observed so far.
    #[serde(default)]
    pub outcomes_seen: u64,
    /// Each experiment's own situations, under [`ContextMode::PerAction`].
    #[serde(default)]
    pub per_action: Vec<ActionSituations>,
    /// The sense facts each recent experiment was chosen with.
    #[serde(default)]
    pub recent_facts: [u32; MAX_ELIGIBILITY_SPAN],
    /// Scenes learned from raw senses, under [`Perception::Scenes`].
    #[serde(default)]
    pub scenes: ScenePerception,
    /// Previous retinal view, so the next outcome can report how much the view changed.
    #[serde(default)]
    pub previous_view: [[u16; VIEW_CHANNELS]; RETINAL_RAY_COUNT],
    /// Decisions left in the current bout before the next choice.
    #[serde(default)]
    pub bout_remaining: u16,
}

fn blank_contexts() -> Box<[Statistics]> {
    vec![Statistics::default(); CONTEXTS].into_boxed_slice()
}

const fn no_recent_actions() -> [usize; MAX_ELIGIBILITY_SPAN] {
    [AFFORDANCE_ACTIONS; MAX_ELIGIBILITY_SPAN]
}

impl Default for AffordanceLearner {
    fn default() -> Self {
        Self::new(AffordanceConfig::default())
    }
}

impl AffordanceLearner {
    /// A blank learner with the given settings.
    #[must_use]
    pub fn new(config: AffordanceConfig) -> Self {
        Self {
            config,
            pooled: Statistics::default(),
            contexts: blank_contexts(),
            error_fast: [0; AFFORDANCE_ACTIONS],
            error_slow: [0; AFFORDANCE_ACTIONS],
            last_tried: [0; AFFORDANCE_ACTIONS],
            last_tried_in: [[0; AFFORDANCE_ACTIONS]; CONTEXTS],
            previous_action: AFFORDANCE_ACTIONS,
            recent_actions: no_recent_actions(),
            recent_contexts: [0; MAX_ELIGIBILITY_SPAN],
            current_context: 0,
            current_facts: 0,
            splits: Vec::new(),
            experiences: Vec::new(),
            next_experience: 0,
            outcomes_seen: 0,
            per_action: if config.context == ContextMode::PerAction {
                vec![ActionSituations::default(); AFFORDANCE_ACTIONS]
            } else {
                Vec::new()
            },
            recent_facts: [0; MAX_ELIGIBILITY_SPAN],
            scenes: ScenePerception::default(),
            previous_view: [[0; VIEW_CHANNELS]; RETINAL_RAY_COUNT],
            bout_remaining: 0,
        }
    }

    /// Checks settings and every stored experiment and situation index, so a decoded learner
    /// cannot index outside its fixed tables.
    fn validate(&self) -> Result<(), String> {
        self.config.validate()?;
        if self.contexts.len() != CONTEXTS {
            return Err(format!("affordance learner needs {CONTEXTS} situations"));
        }
        if self.bout_remaining >= self.config.bout_length {
            return Err("affordance bout is out of range".to_owned());
        }
        if self.previous_action > AFFORDANCE_ACTIONS
            || self
                .recent_actions
                .iter()
                .any(|action| *action > AFFORDANCE_ACTIONS)
        {
            return Err("affordance experiment index is out of range".to_owned());
        }
        if self.current_context >= CONTEXTS
            || self
                .recent_contexts
                .iter()
                .any(|context| *context >= CONTEXTS)
        {
            return Err("affordance situation index is out of range".to_owned());
        }
        let splits_valid = self.splits.len() < CONTEXTS
            && self.splits.iter().enumerate().all(|(index, split)| {
                usize::from(split.child) == index + 1
                    && split.parent < split.child
                    && usize::from(split.fact) < SENSE_FACTS
            });
        let experiences_valid = self.experiences.len() <= EXPERIENCE_MEMORY
            && self.next_experience < EXPERIENCE_MEMORY
            && self.experiences.iter().all(|experience| {
                usize::from(experience.situation) <= self.splits.len()
                    && usize::from(experience.action) < AFFORDANCE_ACTIONS
            });
        let per_action_valid = (self.per_action.is_empty()
            && self.config.context != ContextMode::PerAction)
            || (self.per_action.len() == AFFORDANCE_ACTIONS
                && self
                    .per_action
                    .iter()
                    .all(|situations| situations.is_valid(SENSE_FACTS)));
        if !splits_valid || !experiences_valid || !per_action_valid || !self.scenes.is_valid() {
            return Err("learned situations are out of range".to_owned());
        }
        Ok(())
    }

    /// Bounded body and visual outcome. Vision is summarized without identities or locations:
    /// the largest apparent motion on any ray, and the largest change of any ray since the
    /// previous view. Taking the largest rather than the mean keeps a small object salient
    /// instead of diluting it across the whole retina.
    fn outcome(&mut self, observation: &EnvironmentObservation) -> [i32; OUTCOME_FEATURES] {
        let body = observation.body;
        let mut motion = 0_i32;
        let mut change = 0_i32;
        for (sample, previous) in observation
            .senses
            .retinal
            .iter()
            .zip(&mut self.previous_view)
        {
            let view = [
                sample.red,
                sample.green,
                sample.blue,
                sample.brightness,
                sample.occlusion,
            ];
            motion = motion.max(i32::from(sample.motion.unsigned_abs()));
            change = change.max(i32::from(
                view.iter()
                    .zip(*previous)
                    .map(|(now, before)| now.abs_diff(before))
                    .max()
                    .unwrap_or_default(),
            ));
            *previous = view;
        }
        [
            i32::from(observation.senses.touch),
            i32::from(observation.senses.acceleration),
            i32::from(body.eye_pitch) - 128,
            i32::from(body.grounded),
            i32::from(body.vertical_motion),
            i32::from(body.hand_contact),
            motion.min(255),
            change,
        ]
        .map(|value| value * OUTCOME_SCALE)
    }

    fn observe(&mut self, observation: &EnvironmentObservation) {
        let now = self.outcome(observation);
        let previous_context = self.current_context;
        let previous_facts = self.current_facts;
        if matches!(
            self.config.context,
            ContextMode::Learned | ContextMode::PerAction
        ) {
            self.current_facts = match self.config.perception {
                Perception::Designed => sense_facts(observation),
                Perception::Scenes => self
                    .scenes
                    .observe(observation)
                    .map_or(0, |scene| 1 << scene),
            };
        }
        self.current_context = match self.config.context {
            ContextMode::None | ContextMode::PerAction => 0,
            ContextMode::SenseBits => sense_context(observation),
            ContextMode::Learned => self.situation_of(self.current_facts),
        };
        if self.previous_action >= AFFORDANCE_ACTIONS {
            self.pooled.prime(&now);
            return;
        }
        let config = self.config;
        let action = self.previous_action;
        let visits = self.pooled.visits[action].saturating_add(1);
        self.pooled.visits[action] = visits;
        if visits > 1 {
            // Learning progress compares the direct outcome's prediction error over a short and
            // a long window, before this outcome updates the prediction.
            let error = self.pooled.prediction_error(action, &now);
            if visits == 2 {
                self.error_fast[action] = error;
                self.error_slow[action] = error;
            } else {
                let fast = i32::from((config.action_horizon / 2).max(1));
                let slow = i32::from(config.action_horizon) * 2;
                self.error_fast[action] += (error - self.error_fast[action]) / fast;
                self.error_slow[action] += (error - self.error_slow[action]) / slow;
            }
        }
        let credited = self
            .recent_actions
            .into_iter()
            .zip(self.recent_contexts)
            .take(config.eligibility_span)
            .enumerate()
            .filter(|(_, (credited, _))| *credited < AFFORDANCE_ACTIONS);
        for (age, (credited, _)) in credited.clone() {
            self.pooled.credit(credited, age, &now, &config);
        }
        self.pooled.observe_overall(&now, &config);
        self.pooled.forget(action, &config);
        if matches!(
            config.context,
            ContextMode::SenseBits | ContextMode::Learned
        ) {
            // The direct outcome belongs to the situation the last experiment was chosen in.
            let situation = &mut self.contexts[previous_context];
            situation.visits[action] = situation.visits[action].saturating_add(1);
            for (age, (credited, context)) in credited {
                self.contexts[context].credit(credited, age, &now, &config);
            }
            let situation = &mut self.contexts[previous_context];
            situation.observe_overall(&now, &config);
            situation.forget(action, &config);
        }
        self.outcomes_seen += 1;
        if config.context == ContextMode::Learned {
            self.remember(previous_facts, previous_context, action, &now);
            if self.outcomes_seen.is_multiple_of(SPLIT_INTERVAL) {
                self.try_split();
                self.current_context = self.situation_of(self.current_facts);
            }
        }
        if config.context == ContextMode::PerAction {
            self.learn_per_action(previous_facts, action, &now);
            self.remember(previous_facts, 0, action, &now);
            if self.outcomes_seen.is_multiple_of(SPLIT_INTERVAL) {
                self.try_split_per_action();
            }
        }
    }

    /// Updates each experiment's own situation statistics with an outcome that followed
    /// `action`, chosen with `facts`.
    fn learn_per_action(&mut self, facts: u32, action: usize, now: &[i32; OUTCOME_FEATURES]) {
        let config = self.config;
        let situation = self.per_action[action].situation_of(facts);
        let stats = &mut self.per_action[action].stats[situation];
        stats.visits = stats.visits.saturating_add(1);
        for (age, (credited, credited_facts)) in self
            .recent_actions
            .into_iter()
            .zip(self.recent_facts)
            .take(config.eligibility_span)
            .enumerate()
        {
            if credited < AFFORDANCE_ACTIONS {
                let situations = &mut self.per_action[credited];
                let situation = situations.situation_of(credited_facts);
                situations.stats[situation].credit(age, now, &config);
            }
        }
        for (other, situations) in self.per_action.iter_mut().enumerate() {
            let situation = situations.situation_of(facts);
            situations.stats[situation].observe_overall(now, &config);
            if other != action {
                situations.stats[situation].forget(&config);
            }
        }
    }

    /// How much splitting `action`'s `situation` on `fact` would change the action's effect:
    /// the gap between its effect (its average outcome minus every experiment's average outcome
    /// in the same situation of its tree) on the two sides, in score points.
    fn action_split_gain(
        &self,
        action: usize,
        situations: &[usize],
        situation: usize,
        fact: usize,
    ) -> i64 {
        let mut tries = [0_i64; 2];
        let mut sums = [[0_i64; OUTCOME_FEATURES]; 2];
        let mut side_count = [0_i64; 2];
        let mut side_sums = [[0_i64; OUTCOME_FEATURES]; 2];
        for (experience, here) in self.experiences.iter().zip(situations) {
            if *here != situation {
                continue;
            }
            let side = usize::from(experience.facts & (1 << fact) != 0);
            let mine = usize::from(experience.action) == action;
            side_count[side] += 1;
            tries[side] += i64::from(mine);
            for (feature, value) in experience.outcome.iter().enumerate() {
                side_sums[side][feature] += i64::from(*value);
                if mine {
                    sums[side][feature] += i64::from(*value);
                }
            }
        }
        if side_count.iter().any(|count| *count < SPLIT_MIN_SIDE)
            || tries.iter().any(|count| *count < SPLIT_MIN_TRIES)
        {
            return 0;
        }
        let scale = i64::from(OUTCOME_SCALE);
        (0..OUTCOME_FEATURES)
            .map(|feature| {
                let effect = |side: usize| {
                    sums[side][feature] * scale / tries[side]
                        - side_sums[side][feature] * scale / side_count[side]
                };
                let gap = (effect(1) - effect(0)).abs();
                self.pooled
                    .points(feature, i32::try_from(gap).unwrap_or(i32::MAX))
            })
            .sum()
    }

    /// For each experiment, splits its situation and sense fact with the largest gain, if it is
    /// clear enough and the experiment has room for another situation.
    fn try_split_per_action(&mut self) {
        for action in 0..AFFORDANCE_ACTIONS {
            let tree = &self.per_action[action];
            if tree.stats.len() >= MAX_ACTION_SITUATIONS {
                continue;
            }
            let situations: Vec<usize> = self
                .experiences
                .iter()
                .map(|experience| tree.situation_of(experience.facts))
                .collect();
            let mut best: Option<(i64, usize, usize)> = None;
            for situation in 0..tree.stats.len() {
                for fact in 0..SENSE_FACTS {
                    let gain = self.action_split_gain(action, &situations, situation, fact);
                    if gain >= SPLIT_GAIN && best.is_none_or(|(most, _, _)| gain > most) {
                        best = Some((gain, situation, fact));
                    }
                }
            }
            if let Some((_, parent, fact)) = best {
                let at = self.outcomes_seen;
                self.per_action[action].split(parent, fact, at);
            }
        }
    }

    /// The learned situation of an observation with `facts`: splits refine situations in the
    /// order they were made.
    fn situation_of(&self, facts: u32) -> usize {
        self.splits.iter().fold(0, |situation, split| {
            if situation == usize::from(split.parent) && facts & (1 << split.fact) != 0 {
                usize::from(split.child)
            } else {
                situation
            }
        })
    }

    fn remember(
        &mut self,
        facts: u32,
        situation: usize,
        action: usize,
        now: &[i32; OUTCOME_FEATURES],
    ) {
        let experience = Experience {
            facts,
            situation: u8::try_from(situation).unwrap_or_default(),
            action: u8::try_from(action).unwrap_or_default(),
            outcome: now.map(|value| i16::try_from(value / OUTCOME_SCALE).unwrap_or_default()),
        };
        if self.experiences.len() < EXPERIENCE_MEMORY {
            self.experiences.push(experience);
        } else {
            self.experiences[self.next_experience] = experience;
        }
        self.next_experience = (self.next_experience + 1) % EXPERIENCE_MEMORY;
    }

    /// How much splitting `situation` on `fact` would change what some experiment does: the
    /// largest, over experiments tried enough on both sides, of the gap between its effect (its
    /// average outcome minus the side's average outcome) on the two sides, in score points.
    fn split_gain(&self, situation: usize, fact: usize) -> i64 {
        let mut tries = [[0_i64; AFFORDANCE_ACTIONS]; 2];
        let mut sums = [[[0_i64; OUTCOME_FEATURES]; AFFORDANCE_ACTIONS]; 2];
        let mut side_count = [0_i64; 2];
        let mut side_sums = [[0_i64; OUTCOME_FEATURES]; 2];
        for experience in &self.experiences {
            if usize::from(experience.situation) != situation {
                continue;
            }
            let side = usize::from(experience.facts & (1 << fact) != 0);
            let action = usize::from(experience.action);
            tries[side][action] += 1;
            side_count[side] += 1;
            for (feature, value) in experience.outcome.iter().enumerate() {
                sums[side][action][feature] += i64::from(*value);
                side_sums[side][feature] += i64::from(*value);
            }
        }
        if side_count.iter().any(|count| *count < SPLIT_MIN_SIDE) {
            return 0;
        }
        let scale = i64::from(OUTCOME_SCALE);
        (0..AFFORDANCE_ACTIONS)
            .filter(|action| tries.iter().all(|side| side[*action] >= SPLIT_MIN_TRIES))
            .map(|action| {
                (0..OUTCOME_FEATURES)
                    .map(|feature| {
                        let effect = |side: usize| {
                            sums[side][action][feature] * scale / tries[side][action]
                                - side_sums[side][feature] * scale / side_count[side]
                        };
                        let gap = (effect(1) - effect(0)).abs();
                        self.pooled
                            .points(feature, i32::try_from(gap).unwrap_or(i32::MAX))
                    })
                    .sum::<i64>()
            })
            .max()
            .unwrap_or(0)
    }

    /// Splits the situation and sense fact with the largest gain, if it is clear enough and
    /// room remains. The new situation starts from a copy of its parent's experience.
    fn try_split(&mut self) {
        if self.splits.len() + 1 >= CONTEXTS {
            return;
        }
        let mut best: Option<(i64, usize, usize)> = None;
        for situation in 0..=self.splits.len() {
            for fact in 0..SENSE_FACTS {
                let gain = self.split_gain(situation, fact);
                if gain >= SPLIT_GAIN && best.is_none_or(|(most, _, _)| gain > most) {
                    best = Some((gain, situation, fact));
                }
            }
        }
        let Some((_, parent, fact)) = best else {
            return;
        };
        let child = self.splits.len() + 1;
        self.splits.push(Split {
            parent: u8::try_from(parent).unwrap_or_default(),
            fact: u8::try_from(fact).unwrap_or_default(),
            child: u8::try_from(child).unwrap_or_default(),
            at: self.outcomes_seen,
        });
        self.contexts[child] = self.contexts[parent];
        self.last_tried_in[child] = self.last_tried_in[parent];
        for experience in &mut self.experiences {
            if usize::from(experience.situation) == parent && experience.facts & (1 << fact) != 0 {
                experience.situation = u8::try_from(child).unwrap_or_default();
            }
        }
    }

    /// Outcome the learner currently expects to follow `action`, in receptor units, or `None`
    /// before the action has been tried. This is the learner's prediction, read before the
    /// outcome arrives.
    #[must_use]
    pub fn expected_after(&self, action: usize) -> Option<[i32; OUTCOME_FEATURES]> {
        (self.pooled.visits[action] > 0)
            .then(|| self.pooled.after[action].map(|value| value / OUTCOME_SCALE))
    }

    /// Contingency of `action` in the current situation. A situation's own statistics are
    /// blended with the pooled ones by how often the action has been tried there, so a rarely
    /// seen situation borrows from general experience instead of starting from nothing.
    fn contextual_contingency(&self, action: usize) -> u32 {
        let model = self.config.outcome_model;
        let pooled = u64::from(self.pooled.contingency(action, model));
        let (tried, local) = match self.config.context {
            ContextMode::None => return u32::try_from(pooled).unwrap_or(u32::MAX),
            ContextMode::SenseBits | ContextMode::Learned => {
                let situation = &self.contexts[self.current_context];
                (
                    u64::from(situation.visits[action]),
                    u64::from(situation.contingency(action, model)),
                )
            }
            ContextMode::PerAction => {
                let situations = &self.per_action[action];
                let stats = &situations.stats[situations.situation_of(self.current_facts)];
                (u64::from(stats.visits), u64::from(stats.contingency()))
            }
        };
        let prior = u64::from(self.config.context_prior);
        u32::try_from((tried * local + prior * pooled) / (tried + prior)).unwrap_or(u32::MAX)
    }

    /// How fast an experiment's prediction error is changing, in either direction: rising
    /// error marks a consequence that changed, falling error one being learned.
    fn learning_progress(&self, action: usize) -> u32 {
        let gap = (self.error_slow[action] - self.error_fast[action]).unsigned_abs();
        gap / OUTCOME_SCALE.cast_unsigned()
    }

    fn interest(&self, action: usize) -> u32 {
        match self.config.rule {
            ScoringRule::Contingency => self.contextual_contingency(action),
            ScoringRule::LearningProgress => self.learning_progress(action),
        }
    }

    fn remember_choice(&mut self, action: usize, tick: u64) {
        self.recent_actions.rotate_right(1);
        self.recent_actions[0] = action;
        self.recent_contexts.rotate_right(1);
        self.recent_contexts[0] = self.current_context;
        self.recent_facts.rotate_right(1);
        self.recent_facts[0] = self.current_facts;
        self.last_tried[action] = tick;
        self.last_tried_in[self.current_context][action] = tick;
        if let Some(situations) = self.per_action.get_mut(action) {
            let situation = situations.situation_of(self.current_facts);
            situations.last_tried[situation] = tick;
        }
    }

    fn select(&mut self, seed: u64, tick: u64) -> (PrimitiveAction, u32) {
        let config = self.config;
        if self.bout_remaining > 0 && self.previous_action < AFFORDANCE_ACTIONS {
            // Continue the current bout: the experiment is held, but its outcomes keep teaching.
            self.bout_remaining -= 1;
            let held = self.previous_action;
            self.remember_choice(held, tick);
            return (affordance_action(held), self.interest(held));
        }
        let mut best = 0;
        let mut best_score = 0_u32;
        for candidate in 0..AFFORDANCE_ACTIONS {
            // Curiosity regrows while an experiment goes untried, so exploration never stops
            // and a world that changed is noticed again. Telling situations apart, curiosity is
            // per situation: an experiment never tried here is as tempting as one never tried.
            let (tried, last) = match config.context {
                ContextMode::None => (self.pooled.visits[candidate], self.last_tried[candidate]),
                ContextMode::SenseBits | ContextMode::Learned => (
                    self.contexts[self.current_context].visits[candidate],
                    self.last_tried_in[self.current_context][candidate],
                ),
                ContextMode::PerAction => {
                    let situations = &self.per_action[candidate];
                    let situation = situations.situation_of(self.current_facts);
                    (
                        situations.stats[situation].visits,
                        situations.last_tried[situation],
                    )
                }
            };
            let since = if tried == 0 {
                config.recency_cap
            } else {
                (tick - last).saturating_mul(config.recency_rate)
            };
            let curiosity = u32::try_from(since.min(config.recency_cap)).unwrap_or_default();
            // Seed-keyed tie breaking is endogenous deterministic initialization, not a schedule.
            let tie = (derive(seed, tick, candidate as u64, 0x4146_464F) & 0xff) as u32;
            let score = self
                .interest(candidate)
                .saturating_add(curiosity)
                .saturating_add(tie);
            if score > best_score {
                best = candidate;
                best_score = score;
            }
        }
        self.previous_action = best;
        self.bout_remaining = config.bout_length - 1;
        self.remember_choice(best, tick);
        (affordance_action(best), best_score)
    }
}

fn affordance_action(index: usize) -> PrimitiveAction {
    let mut action = PrimitiveAction::default();
    match index {
        0 => action.thrust = 256,
        1 => action.thrust = -256,
        2 => action.turn = 256,
        3 => action.turn = -256,
        4 => action.strafe = 256,
        5 => action.strafe = -256,
        6 => action.look_pitch = 256,
        7 => action.look_pitch = -256,
        8 => action.jump = 256,
        9 => action.hand_pulse = 256,
        10 => action.jaw = 256,
        _ => action.acoustic = 256,
    }
    action
}

/// Frozen internal stores used by the external one-agent runtime.
///
/// The defaults sit far enough above the founder reflex thresholds that receptor noise cannot
/// trigger the low-energy, swallowing, or toxin reflexes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FrozenPhysiology {
    /// Energy store.
    pub energy: u16,
    /// Gut store.
    pub gut: u16,
    /// Hydration store.
    pub hydration: u16,
    /// Fatigue store.
    pub fatigue: u16,
    /// Toxin store.
    pub toxin: u16,
    /// Damage store.
    pub damage: u16,
}

impl Default for FrozenPhysiology {
    fn default() -> Self {
        Self {
            energy: 120,
            gut: 120,
            hydration: 120,
            fatigue: 0,
            toxin: 0,
            damage: 0,
        }
    }
}

impl FrozenPhysiology {
    /// Returns the stores in [`compose_raw_sensory_frame`] order.
    #[must_use]
    pub const fn stores(self) -> [u16; 6] {
        [
            self.energy,
            self.gut,
            self.toxin,
            self.hydration,
            self.fatigue,
            self.damage,
        ]
    }
}

/// Exact mutable runtime state. It serializes through a validated representation, so a decoded
/// state can never index outside the brain's fixed buffers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "StateRepr", into = "StateRepr")]
pub struct AgentRuntimeState {
    /// Mutable predictive brain.
    pub brain: PredictiveBrain,
    /// Derived developmental brain configuration.
    pub developmental_config: BrainConfig,
    /// Leaky chemical receptor adaptation.
    pub chemical_adaptation: [u16; CHEMICAL_RECEPTOR_COUNT],
    /// Frozen stores.
    pub physiology: FrozenPhysiology,
    /// Decisions taken so far; keys receptor noise.
    pub decision_tick: u64,
    /// Sequence number the next observation must carry.
    pub next_sequence: u64,
    /// Selected policy; baseline remains the default and preserves V2 behavior.
    #[serde(default)]
    pub experiment_mode: ExperimentMode,
    /// Exact intrinsic learner state when experiment mode is enabled.
    #[serde(default)]
    pub affordance: AffordanceLearner,
}

/// Rejected observation; the runtime state is unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeError {
    /// The observation is a duplicate, stale, or from the future.
    UnexpectedSequence {
        /// Sequence the runtime was waiting for.
        expected: u64,
        /// Sequence the observation carried.
        received: u64,
    },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedSequence { expected, received } => write!(
                formatter,
                "expected observation sequence {expected}, received {received}"
            ),
        }
    }
}

impl Error for RuntimeError {}

/// Single-agent execution state, intentionally independent of [`crate::World`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentRuntime {
    seed: u64,
    agent_id: u64,
    state: AgentRuntimeState,
}

impl AgentRuntime {
    /// Constructs a fresh runtime with a blank lifetime brain.
    #[must_use]
    pub fn new(seed: u64, agent_id: u64, developmental_config: BrainConfig) -> Self {
        Self::from_state(
            seed,
            agent_id,
            AgentRuntimeState {
                brain: PredictiveBrain::default(),
                developmental_config,
                chemical_adaptation: [0; CHEMICAL_RECEPTOR_COUNT],
                physiology: FrozenPhysiology::default(),
                decision_tick: 0,
                next_sequence: 0,
                experiment_mode: ExperimentMode::PredictiveBaseline,
                affordance: AffordanceLearner::default(),
            },
        )
    }

    /// Constructs a runtime with an explicit, versioned action-selection policy.
    #[must_use]
    pub fn new_with_mode(
        seed: u64,
        agent_id: u64,
        developmental_config: BrainConfig,
        mode: ExperimentMode,
    ) -> Self {
        let mut runtime = Self::new(seed, agent_id, developmental_config);
        runtime.state.experiment_mode = mode;
        runtime
    }

    /// Constructs an affordance-discovery runtime with explicit learner settings.
    ///
    /// # Errors
    ///
    /// Returns a description of the first learner setting out of range.
    pub fn new_with_affordance(
        seed: u64,
        agent_id: u64,
        developmental_config: BrainConfig,
        affordance: AffordanceConfig,
    ) -> Result<Self, String> {
        affordance.validate()?;
        let mut runtime = Self::new_with_mode(
            seed,
            agent_id,
            developmental_config,
            ExperimentMode::AffordanceDiscovery,
        );
        runtime.state.affordance = AffordanceLearner::new(affordance);
        Ok(runtime)
    }

    /// Restores an exact runtime state.
    #[must_use]
    pub const fn from_state(seed: u64, agent_id: u64, state: AgentRuntimeState) -> Self {
        Self {
            seed,
            agent_id,
            state,
        }
    }

    /// Returns the run seed that keys receptor noise.
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// Returns the stable agent ID that keys receptor noise.
    #[must_use]
    pub const fn agent_id(&self) -> u64 {
        self.agent_id
    }

    /// Returns the exact mutable state for checkpointing.
    #[must_use]
    pub const fn state(&self) -> &AgentRuntimeState {
        &self.state
    }

    /// Consumes the next observation and emits primitive motor commands.
    ///
    /// An observation reporting a transport fault is consumed but not sensed: the brain and
    /// decision tick stay unchanged, and the emitted action is zero so the body holds still.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::UnexpectedSequence`] for a duplicate, stale, or future
    /// observation, leaving the runtime unchanged.
    pub fn step(
        &mut self,
        observation: EnvironmentObservation,
    ) -> Result<EnvironmentTransition, RuntimeError> {
        if observation.sequence != self.state.next_sequence {
            return Err(RuntimeError::UnexpectedSequence {
                expected: self.state.next_sequence,
                received: observation.sequence,
            });
        }
        self.state.next_sequence = observation.sequence + 1;
        if observation.transport != TransportStatus::Healthy {
            return Ok(EnvironmentTransition {
                decision_tick: self.state.decision_tick,
                observation,
                action: PrimitiveAction::default(),
                baseline_action: PrimitiveAction::default(),
                intrinsic_score: 0,
                affordance_fingerprint: if self.state.experiment_mode
                    == ExperimentMode::AffordanceDiscovery
                {
                    affordance_fingerprint(&self.state.affordance)
                } else {
                    0
                },
                brain_fingerprint: brain_fingerprint(&self.state.brain),
            });
        }
        let mut external = observation.senses;
        observation.capabilities.dead.silence(&mut external);
        let (chemicals, adaptation) =
            adapt_chemical_receptors(external.chemicals, self.state.chemical_adaptation);
        external.chemicals = chemicals;
        let frame = compose_raw_sensory_frame(
            external,
            self.state.physiology.stores(),
            self.seed,
            self.state.decision_tick,
            self.agent_id,
        );
        let step = self
            .state
            .brain
            .step(&self.state.developmental_config, &frame);
        let decision_tick = self.state.decision_tick;
        let baseline_action = decode_motor_outputs(&step.outputs);
        let (action, intrinsic_score, recorded_baseline) =
            if self.state.experiment_mode == ExperimentMode::AffordanceDiscovery {
                self.state.affordance.observe(&observation);
                let (action, score) = self.state.affordance.select(self.seed, decision_tick);
                (action, score, baseline_action)
            } else {
                (baseline_action, 0, PrimitiveAction::default())
            };
        self.state.chemical_adaptation = adaptation;
        self.state.decision_tick += 1;
        Ok(EnvironmentTransition {
            decision_tick,
            observation,
            action,
            baseline_action: recorded_baseline,
            intrinsic_score,
            affordance_fingerprint: if self.state.experiment_mode
                == ExperimentMode::AffordanceDiscovery
            {
                affordance_fingerprint(&self.state.affordance)
            } else {
                0
            },
            brain_fingerprint: brain_fingerprint(&self.state.brain),
        })
    }
}

/// Stable fingerprint of all checkpointed intrinsic learner statistics.
#[must_use]
pub fn affordance_fingerprint(learner: &AffordanceLearner) -> u64 {
    let mut fingerprint = derive(0, learner.previous_action as u64, 0, 0x4146_4650);
    let config = learner.config;
    for (index, value) in [
        u64::from(config.rule == ScoringRule::LearningProgress),
        u64::from(config.action_horizon),
        u64::from(config.overall_horizon),
        u64::from(config.forgetting_horizon),
        config.eligibility_span as u64,
        config.recency_rate,
        config.recency_cap,
        u64::from(config.bout_length),
        match config.context {
            ContextMode::None => 0,
            ContextMode::SenseBits => 1,
            ContextMode::Learned => 2,
            ContextMode::PerAction => 3,
        },
        u64::from(config.context_prior),
        u64::from(config.outcome_model == OutcomeModel::Expectiles),
        u64::from(config.perception == Perception::Scenes),
        u64::from(learner.bout_remaining),
        learner.current_context as u64,
    ]
    .into_iter()
    .enumerate()
    {
        fingerprint = derive(fingerprint, value, index as u64, 0x4346_4746);
    }
    for (age, (action, context)) in learner
        .recent_actions
        .iter()
        .zip(learner.recent_contexts)
        .enumerate()
    {
        fingerprint = derive(fingerprint, *action as u64, age as u64, 0x5245_434E);
        fingerprint = derive(fingerprint, context as u64, age as u64, 0x5243_5458);
    }
    for (ray, view) in learner.previous_view.iter().enumerate() {
        for (channel, value) in view.iter().enumerate() {
            fingerprint = derive(
                fingerprint,
                u64::from(*value),
                (ray * VIEW_CHANNELS + channel) as u64,
                0x5649_4557,
            );
        }
    }
    for action in 0..AFFORDANCE_ACTIONS {
        for (tag, value) in [
            (0x4552_4646, i64::from(learner.error_fast[action])),
            (0x4552_5353, i64::from(learner.error_slow[action])),
            (
                0x4C41_5354,
                i64::try_from(learner.last_tried[action]).unwrap_or(i64::MAX),
            ),
        ] {
            fingerprint = derive(fingerprint, value.cast_unsigned(), action as u64, tag);
        }
    }
    fingerprint = learner.pooled.fingerprint(fingerprint, 0x504F_4F4C);
    if config.context == ContextMode::PerAction {
        fingerprint = per_action_fingerprint(learner, fingerprint);
    }
    if matches!(
        config.context,
        ContextMode::Learned | ContextMode::PerAction
    ) {
        fingerprint = learned_situations_fingerprint(learner, fingerprint);
    }
    if matches!(
        config.context,
        ContextMode::SenseBits | ContextMode::Learned
    ) {
        for (context, statistics) in learner.contexts.iter().enumerate() {
            fingerprint = statistics.fingerprint(fingerprint, 0x4354_0000 | context as u64);
            for (action, tick) in learner.last_tried_in[context].iter().enumerate() {
                fingerprint = derive(
                    fingerprint,
                    *tick,
                    action as u64,
                    0x4C54_0000 | context as u64,
                );
            }
        }
    }
    fingerprint
}

fn per_action_fingerprint(learner: &AffordanceLearner, mut fingerprint: u64) -> u64 {
    for (action, situations) in learner.per_action.iter().enumerate() {
        let tag = 0x5041_0000 | action as u64;
        for split in &situations.splits {
            let word =
                u64::from(split.parent) | u64::from(split.fact) << 8 | u64::from(split.child) << 16;
            fingerprint = derive(fingerprint, word, split.at, tag);
        }
        for (index, value) in situations
            .stats
            .iter()
            .flat_map(SituationStats::words)
            .chain(
                situations
                    .last_tried
                    .iter()
                    .map(|tick| i64::try_from(*tick).unwrap_or(i64::MAX)),
            )
            .enumerate()
        {
            fingerprint = derive(fingerprint, value.cast_unsigned(), index as u64, tag);
        }
    }
    for (age, facts) in learner.recent_facts.iter().enumerate() {
        fingerprint = derive(fingerprint, u64::from(*facts), age as u64, 0x5246_4354);
    }
    fingerprint
}

fn scenes_fingerprint(scenes: &ScenePerception, mut fingerprint: u64) -> u64 {
    let words = [
        i64::from(scenes.seen),
        scenes.typical_distance,
        i64::try_from(scenes.current).unwrap_or(i64::MAX),
    ]
    .into_iter()
    .chain(scenes.mean.iter().map(|value| i64::from(*value)))
    .chain(scenes.spread.iter().map(|value| i64::from(*value)))
    .chain(scenes.members.iter().map(|value| i64::from(*value)))
    .chain(
        scenes
            .prototypes
            .iter()
            .flatten()
            .map(|value| i64::from(*value)),
    );
    for (index, value) in words.enumerate() {
        fingerprint = derive(
            fingerprint,
            value.cast_unsigned(),
            index as u64,
            0x5343_454E,
        );
    }
    fingerprint
}

fn learned_situations_fingerprint(learner: &AffordanceLearner, mut fingerprint: u64) -> u64 {
    if learner.config.perception == Perception::Scenes {
        fingerprint = scenes_fingerprint(&learner.scenes, fingerprint);
    }
    for split in &learner.splits {
        let word =
            u64::from(split.parent) | u64::from(split.fact) << 8 | u64::from(split.child) << 16;
        fingerprint = derive(fingerprint, word, split.at, 0x5350_4C54);
    }
    for (index, experience) in learner.experiences.iter().enumerate() {
        let mut word = u64::from(experience.facts) << 16
            | u64::from(experience.situation) << 8
            | u64::from(experience.action);
        for value in experience.outcome {
            word = derive(word, i64::from(value).cast_unsigned(), 0, 0x4F55_5443);
        }
        fingerprint = derive(fingerprint, word, index as u64, 0x4558_5045);
    }
    fingerprint = derive(
        fingerprint,
        learner.next_experience as u64,
        learner.outcomes_seen,
        0x5249_4E47,
    );
    fingerprint = derive(
        fingerprint,
        u64::from(learner.current_facts),
        0,
        0x4641_4354,
    );
    fingerprint
}

/// Returns a stable fingerprint of every field of a brain.
#[must_use]
pub fn brain_fingerprint(brain: &PredictiveBrain) -> u64 {
    brain_words(brain).iter().fold(0, |signature, word| {
        derive(signature, i64::from(*word).cast_unsigned(), 0, 0x4252_4149)
    })
}

fn brain_words(brain: &PredictiveBrain) -> Vec<i32> {
    bytemuck::cast_slice(&[GpuBrain::pack(brain)]).to_vec()
}

/// Serialized form of [`AgentRuntimeState`]. The brain travels as its lossless GPU packing.
#[derive(Deserialize, Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Mirrors the independent ablation switches of BrainConfig."
)]
struct StateRepr {
    brain: Vec<i32>,
    connection_stride: usize,
    plasticity_scale: i32,
    plasticity_enabled: bool,
    episodic_memory_enabled: bool,
    action_contingency_enabled: bool,
    prediction_enabled: bool,
    chemical_adaptation: [u16; CHEMICAL_RECEPTOR_COUNT],
    physiology: FrozenPhysiology,
    decision_tick: u64,
    next_sequence: u64,
    experiment_mode: ExperimentMode,
    affordance: AffordanceLearner,
}

impl From<AgentRuntimeState> for StateRepr {
    fn from(state: AgentRuntimeState) -> Self {
        let config = state.developmental_config;
        Self {
            brain: brain_words(&state.brain),
            connection_stride: config.connection_stride,
            plasticity_scale: config.plasticity_scale,
            plasticity_enabled: config.plasticity_enabled,
            episodic_memory_enabled: config.episodic_memory_enabled,
            action_contingency_enabled: config.action_contingency_enabled,
            prediction_enabled: config.prediction_enabled,
            chemical_adaptation: state.chemical_adaptation,
            physiology: state.physiology,
            decision_tick: state.decision_tick,
            next_sequence: state.next_sequence,
            experiment_mode: state.experiment_mode,
            affordance: state.affordance,
        }
    }
}

impl TryFrom<StateRepr> for AgentRuntimeState {
    type Error = String;

    fn try_from(repr: StateRepr) -> Result<Self, Self::Error> {
        let packed: GpuBrain = bytemuck::try_pod_read_unaligned(bytemuck::cast_slice(&repr.brain))
            .map_err(|_| format!("brain has {} words", repr.brain.len()))?;
        let brain = packed.unpack();
        if GpuBrain::pack(&brain) != packed || brain.next_episode >= EPISODE_CAPACITY {
            return Err("brain state is out of range".to_owned());
        }
        if repr.connection_stride == 0 || repr.plasticity_scale < 1 {
            return Err("developmental configuration is out of range".to_owned());
        }
        repr.affordance.validate()?;
        Ok(Self {
            brain,
            developmental_config: BrainConfig {
                connection_stride: repr.connection_stride,
                plasticity_scale: repr.plasticity_scale,
                plasticity_enabled: repr.plasticity_enabled,
                episodic_memory_enabled: repr.episodic_memory_enabled,
                action_contingency_enabled: repr.action_contingency_enabled,
                prediction_enabled: repr.prediction_enabled,
            },
            chemical_adaptation: repr.chemical_adaptation,
            physiology: repr.physiology,
            decision_tick: repr.decision_tick,
            next_sequence: repr.next_sequence,
            experiment_mode: repr.experiment_mode,
            affordance: repr.affordance,
        })
    }
}

/// Deterministic in-memory endpoint for fixtures. Its world is a fixed ring of 16 coloured
/// sectors around the body; turning moves the ring across the rays and thrust changes the
/// apparent motion, so actions have consequences the brain can learn from. Chemistry and
/// vibration are dead, as in the Luanti milestone.
#[derive(Clone, Debug)]
pub struct FakeEnvironment {
    seed: u64,
    sequence: u64,
    heading: u8,
    speed: i16,
    previous_speed: i16,
    fault: TransportStatus,
}

impl FakeEnvironment {
    /// Creates a deterministic fake endpoint.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            seed,
            sequence: 0,
            heading: 0,
            speed: 0,
            previous_speed: 0,
            fault: TransportStatus::Healthy,
        }
    }

    /// Makes the next observation report `status` instead of a healthy transport.
    pub const fn fail_next(&mut self, status: TransportStatus) {
        self.fault = status;
    }

    fn sector(&self, angle: u8, tag: u64) -> u16 {
        u16::try_from(derive(self.seed, 0, u64::from(angle >> 4), tag) % 256).unwrap_or_default()
    }
}

impl EnvironmentAdapter for FakeEnvironment {
    fn observe(&mut self) -> EnvironmentObservation {
        let mut senses = RawSensoryFrame::EMPTY;
        for (ray, sample) in senses.retinal.iter_mut().enumerate() {
            let offset = u8::try_from(ray * 256 / RETINAL_RAY_COUNT).unwrap_or_default();
            let angle = self.heading.wrapping_add(offset);
            sample.red = self.sector(angle, 0x5245_4400);
            sample.green = self.sector(angle, 0x4752_4E00);
            sample.blue = self.sector(angle, 0x424C_5500);
            sample.brightness = self.sector(angle, 0x4252_4900);
            sample.occlusion = self.sector(angle, 0x4F43_4300);
            sample.motion =
                i16::try_from(i32::from(self.speed) * heading(offset).0 / 256).unwrap_or_default();
        }
        senses.touch = if self.speed > 0 && senses.retinal[0].occlusion > 200 {
            255
        } else {
            0
        };
        senses.joint_position = u16::from(self.heading);
        senses.orientation = u16::from(self.heading);
        senses.muscle_tension = self.speed.unsigned_abs();
        senses.acceleration = (self.speed - self.previous_speed).clamp(-255, 255);
        let observation = EnvironmentObservation {
            sequence: self.sequence,
            senses,
            body: crate::environment::BodyProprioception::default(),
            capabilities: EnvironmentCapabilities {
                body_contract_version: 0,
                checkpoint_resume: false,
                dead: DeadChannels {
                    chemicals: [true; CHEMICAL_RECEPTOR_COUNT],
                    vibration: true,
                    ..DeadChannels::default()
                },
            },
            transport: self.fault,
        };
        self.fault = TransportStatus::Healthy;
        self.sequence += 1;
        observation
    }

    fn act(&mut self, action: &PrimitiveAction) {
        self.heading = self
            .heading
            .wrapping_add_signed(scale_v2_orientation_turn(action.turn));
        self.previous_speed = self.speed;
        self.speed = (self.speed * 7 / 8 + action.thrust / 8).clamp(-255, 255);
    }
}

#[cfg(test)]
mod tests {
    use sim_kernels::gpu::GpuFrame;

    use super::{
        AFFORDANCE_ACTIONS, AffordanceConfig, AgentRuntime, FakeEnvironment, FrozenPhysiology,
        MAX_ELIGIBILITY_SPAN, RuntimeError, ScoringRule,
    };
    use crate::{
        BrainConfig, RawSensoryFrame, compose_raw_sensory_frame,
        environment::{
            EnvironmentAdapter, EnvironmentObservation, PrimitiveAction, TransportStatus,
        },
    };

    // Thresholds of the founder reflexes in `sim_kernels::brain::apply_reflexes`.
    const LOW_ENERGY_REFLEX_BELOW: u16 = 96;
    const SWALLOW_REFLEX_BELOW: u16 = 96;
    const TOXIN_TURN_REFLEX_FROM: u16 = 5;

    fn config() -> BrainConfig {
        BrainConfig {
            connection_stride: 1,
            plasticity_scale: 1,
            plasticity_enabled: true,
            episodic_memory_enabled: true,
            action_contingency_enabled: true,
            prediction_enabled: true,
        }
    }

    #[test]
    fn frozen_physiology_never_reaches_founder_reflexes() {
        let stores = FrozenPhysiology::default().stores();
        for seed in 0..32 {
            for agent_id in 0..8 {
                for tick in 0..256 {
                    let frame = compose_raw_sensory_frame(
                        RawSensoryFrame::EMPTY,
                        stores,
                        seed,
                        tick,
                        agent_id,
                    );
                    assert!(frame.energy >= LOW_ENERGY_REFLEX_BELOW);
                    assert!(frame.gut_pressure >= SWALLOW_REFLEX_BELOW);
                    assert!(frame.toxin_stress < TOXIN_TURN_REFLEX_FROM);
                }
            }
        }
    }

    #[test]
    fn receptor_noise_is_keyed_by_agent_and_decision_tick() {
        let stores = FrozenPhysiology::default().stores();
        let noise = |tick, agent_id| {
            let frame =
                compose_raw_sensory_frame(RawSensoryFrame::EMPTY, stores, 7, tick, agent_id);
            GpuFrame::pack(&frame).scalars
        };
        assert!((0..16).any(|agent| noise(3, 0) != noise(3, agent)));
        assert!((0..16).any(|tick| noise(0, 2) != noise(tick, 2)));
    }

    #[test]
    fn duplicate_stale_and_future_observations_are_rejected_without_change() {
        let mut fake = FakeEnvironment::new(9);
        let observations: Vec<EnvironmentObservation> = (0..3).map(|_| fake.observe()).collect();
        let mut runtime = AgentRuntime::new(71, 4, config());
        assert!(runtime.step(observations[0]).is_ok());
        assert!(runtime.step(observations[1]).is_ok());
        let before = runtime.clone();
        for (observation, received) in [(observations[1], 1), (observations[0], 0)] {
            assert_eq!(
                runtime.step(observation),
                Err(RuntimeError::UnexpectedSequence {
                    expected: 2,
                    received
                })
            );
        }
        let future = EnvironmentObservation {
            sequence: 5,
            ..observations[2]
        };
        assert!(runtime.step(future).is_err());
        assert_eq!(runtime, before);
        assert!(runtime.step(observations[2]).is_ok());
    }

    #[test]
    fn transport_faults_force_a_zero_action_without_sensing() {
        for fault in [
            TransportStatus::Timeout,
            TransportStatus::Rejected,
            TransportStatus::Disconnected,
        ] {
            let mut fake = FakeEnvironment::new(9);
            let mut runtime = AgentRuntime::new(71, 4, config());
            for _ in 0..6 {
                let transition = runtime.step(fake.observe()).expect("in sequence");
                fake.act(&transition.action);
            }
            let before = runtime.state().clone();
            fake.fail_next(fault);
            let faulted = runtime.step(fake.observe()).expect("in sequence");
            assert_eq!(faulted.observation.transport, fault);
            assert_eq!(faulted.action, PrimitiveAction::default());
            assert_eq!(faulted.decision_tick, before.decision_tick);
            assert_eq!(runtime.state().brain, before.brain);
            assert_eq!(runtime.state().decision_tick, before.decision_tick);
            assert_eq!(runtime.state().next_sequence, before.next_sequence + 1);
            assert!(runtime.step(faulted.observation).is_err(), "fault replayed");

            let recovered = runtime.step(fake.observe()).expect("in sequence");
            assert_eq!(recovered.observation.transport, TransportStatus::Healthy);
            assert_eq!(recovered.decision_tick, before.decision_tick);
            assert_ne!(runtime.state().brain, before.brain);
        }
    }

    #[test]
    fn dead_channels_are_silenced_before_sensing() {
        let mut fake = FakeEnvironment::new(9);
        let clean = fake.observe();
        let mut noisy = clean;
        noisy.senses.chemicals = [255; 4];
        noisy.senses.vibration = 255;
        let mut a = AgentRuntime::new(71, 4, config());
        let mut b = a.clone();
        let a = a.step(clean).map(|transition| transition.action);
        let b = b.step(noisy).map(|transition| transition.action);
        assert_eq!(a, b);
    }

    #[test]
    fn out_of_range_receptor_amplitudes_are_rejected_on_decode() {
        let observation = FakeEnvironment::new(9).observe();
        let mut json = serde_json::to_value(observation).expect("encode");
        assert_eq!(
            serde_json::from_value::<EnvironmentObservation>(json.clone()).expect("decode"),
            observation
        );
        json["senses"]["retinal"][3]["red"] = 256.into();
        assert!(serde_json::from_value::<EnvironmentObservation>(json.clone()).is_err());
        json["senses"]["retinal"][3]["red"] = 0.into();
        json["senses"]["acceleration"] = (-256).into();
        assert!(serde_json::from_value::<EnvironmentObservation>(json).is_err());
    }

    #[test]
    fn corrupt_runtime_state_is_rejected_on_decode() {
        let runtime = AgentRuntime::new(71, 4, config());
        let mut json = serde_json::to_value(runtime.state()).expect("encode");
        json["brain"].as_array_mut().expect("brain words").pop();
        assert!(serde_json::from_value::<super::AgentRuntimeState>(json).is_err());
    }

    fn affordance_runtime(learner: AffordanceConfig) -> AgentRuntime {
        AgentRuntime::new_with_affordance(71, 4, config(), learner).expect("valid settings")
    }

    #[test]
    fn out_of_range_affordance_settings_are_rejected() {
        for invalid in [
            AffordanceConfig {
                action_horizon: 0,
                ..AffordanceConfig::default()
            },
            AffordanceConfig {
                eligibility_span: MAX_ELIGIBILITY_SPAN + 1,
                ..AffordanceConfig::default()
            },
            AffordanceConfig {
                forgetting_horizon: 0,
                ..AffordanceConfig::default()
            },
        ] {
            assert!(AgentRuntime::new_with_affordance(71, 4, config(), invalid).is_err());
        }
    }

    #[test]
    fn affordance_settings_and_learning_progress_survive_a_checkpoint() {
        let settings = AffordanceConfig {
            rule: ScoringRule::LearningProgress,
            action_horizon: 16,
            eligibility_span: 2,
            ..AffordanceConfig::default()
        };
        let mut runtime = affordance_runtime(settings);
        let mut environment = FakeEnvironment::new(71);
        for _ in 0..40 {
            let transition = runtime.step(environment.observe()).expect("in sequence");
            environment.act(&transition.action);
        }
        let encoded = serde_json::to_string(runtime.state()).expect("encode");
        let decoded: super::AgentRuntimeState = serde_json::from_str(&encoded).expect("decode");
        assert_eq!(&decoded, runtime.state());
        assert_eq!(decoded.affordance.config, settings);
        let mut resumed = AgentRuntime::from_state(71, 4, decoded);
        for _ in 0..40 {
            let observation = environment.observe();
            let expected = runtime.step(observation).expect("in sequence");
            assert_eq!(resumed.step(observation).expect("in sequence"), expected);
            environment.act(&expected.action);
        }
    }

    #[test]
    fn settings_change_behaviour_and_are_fingerprinted() {
        let run = |settings| {
            let mut runtime = affordance_runtime(settings);
            let mut environment = FakeEnvironment::new(71);
            (0..60)
                .map(|_| {
                    let transition = runtime.step(environment.observe()).expect("in sequence");
                    environment.act(&transition.action);
                    transition.affordance_fingerprint
                })
                .collect::<Vec<_>>()
        };
        let default = run(AffordanceConfig::default());
        let progress = run(AffordanceConfig {
            rule: ScoringRule::LearningProgress,
            ..AffordanceConfig::default()
        });
        assert_eq!(default, run(AffordanceConfig::default()));
        assert_ne!(default, progress);
    }

    #[test]
    fn chosen_experiments_are_held_for_a_bout() {
        let mut runtime = affordance_runtime(AffordanceConfig {
            bout_length: 3,
            ..AffordanceConfig::default()
        });
        let mut environment = FakeEnvironment::new(71);
        let actions: Vec<_> = (0..30)
            .map(|_| {
                let transition = runtime.step(environment.observe()).expect("in sequence");
                environment.act(&transition.action);
                transition.action
            })
            .collect();
        for bout in actions.chunks(3) {
            assert!(bout.iter().all(|action| *action == bout[0]), "{bout:?}");
        }
    }

    #[test]
    fn corrupt_affordance_indices_are_rejected_on_decode() {
        let runtime = affordance_runtime(AffordanceConfig::default());
        let mut json = serde_json::to_value(runtime.state()).expect("encode");
        json["affordance"]["previous_action"] = serde_json::json!(AFFORDANCE_ACTIONS + 1);
        assert!(serde_json::from_value::<super::AgentRuntimeState>(json).is_err());
        let mut json = serde_json::to_value(runtime.state()).expect("encode");
        json["affordance"]["config"]["eligibility_span"] = serde_json::json!(0);
        assert!(serde_json::from_value::<super::AgentRuntimeState>(json).is_err());
    }
}
