//! Standalone deterministic one-agent runtime.

use core::fmt;
use std::error::Error;

use serde::{Deserialize, Serialize};
use sim_kernels::{brain::EPISODE_CAPACITY, derive, gpu::GpuBrain, heading};

use crate::{
    BrainConfig, CHEMICAL_RECEPTOR_COUNT, PredictiveBrain, RETINAL_RAY_COUNT, RawSensoryFrame,
    adapt_chemical_receptors, compose_raw_sensory_frame, decode_motor_outputs,
    environment::{
        DeadChannels, EnvironmentAdapter, EnvironmentCapabilities, EnvironmentObservation,
        EnvironmentTransition, PrimitiveAction, TransportStatus,
    },
    scale_v2_orientation_turn,
};

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
        Ok(())
    }
}

/// Checkpointed bounded statistics for intrinsic action discovery.
///
/// Under the default [`ScoringRule::Contingency`] the learner measures how far the outcomes that
/// follow an experiment differ from the outcomes overall, in both their average and their
/// spread. Each outcome is judged in units of its own overall spread, so a small visual change
/// can matter as much as a large change of body posture. An effect that follows regardless of
/// action, or that stops following, carries no contingency, so neither keeps attracting
/// experiments.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AffordanceLearner {
    /// Settings the statistics below were gathered with.
    #[serde(default)]
    pub config: AffordanceConfig,
    /// Number of observed outcomes per experiment, saturating.
    pub visits: [u16; AFFORDANCE_ACTIONS],
    /// Recent average outcome following each experiment, scaled by 256.
    pub after: [[i32; OUTCOME_FEATURES]; AFFORDANCE_ACTIONS],
    /// Recent average outcome over all decisions, scaled by 256.
    pub overall: [i32; OUTCOME_FEATURES],
    /// Recent mean absolute deviation of the outcome following each experiment, scaled by 256.
    #[serde(default)]
    pub after_spread: [[i32; OUTCOME_FEATURES]; AFFORDANCE_ACTIONS],
    /// Recent mean absolute deviation of the outcome over all decisions, scaled by 256.
    #[serde(default)]
    pub overall_spread: [i32; OUTCOME_FEATURES],
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
    /// Last selected experiment, whose outcome the next observation reports.
    pub previous_action: usize,
    /// Recent experiments, most recent first, that share credit for the next outcome.
    #[serde(default = "no_recent_actions")]
    pub recent_actions: [usize; MAX_ELIGIBILITY_SPAN],
    /// Previous retinal view, so the next outcome can report how much the view changed.
    #[serde(default)]
    pub previous_view: [[u16; VIEW_CHANNELS]; RETINAL_RAY_COUNT],
    /// Decisions left in the current bout before the next choice.
    #[serde(default)]
    pub bout_remaining: u16,
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
    pub const fn new(config: AffordanceConfig) -> Self {
        Self {
            config,
            visits: [0; AFFORDANCE_ACTIONS],
            after: [[0; OUTCOME_FEATURES]; AFFORDANCE_ACTIONS],
            overall: [0; OUTCOME_FEATURES],
            after_spread: [[0; OUTCOME_FEATURES]; AFFORDANCE_ACTIONS],
            overall_spread: [0; OUTCOME_FEATURES],
            error_fast: [0; AFFORDANCE_ACTIONS],
            error_slow: [0; AFFORDANCE_ACTIONS],
            last_tried: [0; AFFORDANCE_ACTIONS],
            previous_action: AFFORDANCE_ACTIONS,
            recent_actions: no_recent_actions(),
            previous_view: [[0; VIEW_CHANNELS]; RETINAL_RAY_COUNT],
            bout_remaining: 0,
        }
    }

    /// Checks settings and every stored experiment index, so a decoded learner cannot index
    /// outside its fixed tables.
    fn validate(&self) -> Result<(), String> {
        self.config.validate()?;
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

    /// Score points for a `difference` on `feature`, in units of that feature's overall spread.
    fn points(&self, feature: usize, difference: i32) -> i64 {
        let spread = i64::from(self.overall_spread[feature].max(SPREAD_FLOOR));
        (i64::from(difference.abs()) * SPREAD_POINTS / spread).min(FEATURE_POINTS_CAP)
    }

    /// How badly the learner predicted `now` after `action`, in score points scaled by 256.
    fn prediction_error(&self, action: usize, now: &[i32; OUTCOME_FEATURES]) -> i32 {
        let points: i64 = (0..OUTCOME_FEATURES)
            .map(|feature| self.points(feature, now[feature] - self.after[action][feature]))
            .sum();
        i32::try_from(points * i64::from(OUTCOME_SCALE)).unwrap_or(i32::MAX)
    }

    fn observe(&mut self, observation: &EnvironmentObservation) {
        let now = self.outcome(observation);
        if self.previous_action >= AFFORDANCE_ACTIONS {
            self.overall = now;
            return;
        }
        let config = self.config;
        let action = self.previous_action;
        let visits = self.visits[action].saturating_add(1);
        self.visits[action] = visits;
        if visits > 1 {
            // Learning progress compares the direct outcome's prediction error over a short and
            // a long window, before this outcome updates the prediction.
            let error = self.prediction_error(action, &now);
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
        for (age, credited) in self
            .recent_actions
            .into_iter()
            .take(config.eligibility_span)
            .enumerate()
        {
            if credited >= AFFORDANCE_ACTIONS || self.visits[credited] == 0 {
                continue;
            }
            let count = self.visits[credited];
            let horizon = i32::from(count.min(config.action_horizon)) << age;
            for (feature, value) in now.into_iter().enumerate() {
                // Spreads are measured against the averages before this outcome joins them;
                // an experiment's first outcome sets its average and has no spread yet.
                if count > 1 {
                    let deviation = (value - self.after[credited][feature]).abs();
                    self.after_spread[credited][feature] +=
                        (deviation - self.after_spread[credited][feature]) / horizon;
                }
                self.after[credited][feature] += (value - self.after[credited][feature]) / horizon;
            }
        }
        let overall_horizon = i32::from(config.overall_horizon);
        for (feature, value) in now.into_iter().enumerate() {
            let deviation = (value - self.overall[feature]).abs();
            self.overall_spread[feature] +=
                (deviation - self.overall_spread[feature]) / overall_horizon;
            self.overall[feature] += (value - self.overall[feature]) / overall_horizon;
        }
        let forgetting = i32::from(config.forgetting_horizon);
        for other in (0..AFFORDANCE_ACTIONS).filter(|other| *other != action) {
            for feature in 0..OUTCOME_FEATURES {
                self.after[other][feature] +=
                    (self.overall[feature] - self.after[other][feature]) / forgetting;
                self.after_spread[other][feature] +=
                    (self.overall_spread[feature] - self.after_spread[other][feature]) / forgetting;
            }
        }
    }

    /// Outcome the learner currently expects to follow `action`, in receptor units, or `None`
    /// before the action has been tried. This is the learner's prediction, read before the
    /// outcome arrives.
    #[must_use]
    pub fn expected_after(&self, action: usize) -> Option<[i32; OUTCOME_FEATURES]> {
        (self.visits[action] > 0).then(|| self.after[action].map(|value| value / OUTCOME_SCALE))
    }

    /// How strongly an experiment's outcomes differ from the overall outcomes, in average and
    /// in spread, with each feature judged in units of its overall spread.
    fn contingency(&self, action: usize) -> u32 {
        if self.visits[action] == 0 {
            return 0;
        }
        let points: i64 = (0..OUTCOME_FEATURES)
            .map(|feature| {
                let average = (self.after[action][feature] - self.overall[feature]).abs();
                let spread =
                    (self.after_spread[action][feature] - self.overall_spread[feature]).abs();
                self.points(feature, average.saturating_add(spread))
            })
            .sum();
        u32::try_from(points).unwrap_or(u32::MAX)
    }

    /// How fast an experiment's prediction error is changing, in either direction: rising
    /// error marks a consequence that changed, falling error one being learned.
    fn learning_progress(&self, action: usize) -> u32 {
        let gap = (self.error_slow[action] - self.error_fast[action]).unsigned_abs();
        gap / OUTCOME_SCALE.cast_unsigned()
    }

    fn interest(&self, action: usize) -> u32 {
        match self.config.rule {
            ScoringRule::Contingency => self.contingency(action),
            ScoringRule::LearningProgress => self.learning_progress(action),
        }
    }

    fn select(&mut self, seed: u64, tick: u64) -> (PrimitiveAction, u32) {
        let config = self.config;
        if self.bout_remaining > 0 && self.previous_action < AFFORDANCE_ACTIONS {
            // Continue the current bout: the experiment is held, but its outcomes keep teaching.
            self.bout_remaining -= 1;
            let held = self.previous_action;
            self.recent_actions.rotate_right(1);
            self.recent_actions[0] = held;
            self.last_tried[held] = tick;
            return (affordance_action(held), self.interest(held));
        }
        let mut best = 0;
        let mut best_score = 0_u32;
        for candidate in 0..AFFORDANCE_ACTIONS {
            // Curiosity regrows while an experiment goes untried, so exploration never stops
            // and a world that changed is noticed again.
            let since = if self.visits[candidate] == 0 {
                config.recency_cap
            } else {
                (tick - self.last_tried[candidate]).saturating_mul(config.recency_rate)
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
        self.recent_actions.rotate_right(1);
        self.recent_actions[0] = best;
        self.last_tried[best] = tick;
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
        u64::from(learner.bout_remaining),
    ]
    .into_iter()
    .enumerate()
    {
        fingerprint = derive(fingerprint, value, index as u64, 0x4346_4746);
    }
    for (age, action) in learner.recent_actions.iter().enumerate() {
        fingerprint = derive(fingerprint, *action as u64, age as u64, 0x5245_434E);
    }
    for (feature, value) in learner.overall_spread.iter().enumerate() {
        fingerprint = derive(
            fingerprint,
            i64::from(*value).cast_unsigned(),
            feature as u64,
            0x4F53_5052,
        );
    }
    for (feature, value) in learner.overall.iter().enumerate() {
        fingerprint = derive(
            fingerprint,
            i64::from(*value).cast_unsigned(),
            feature as u64,
            0x4F56_4552,
        );
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
    for (action, count) in learner.visits.iter().enumerate() {
        fingerprint = derive(fingerprint, u64::from(*count), action as u64, 0x5649_5349);
        fingerprint = derive(
            fingerprint,
            i64::from(learner.error_fast[action]).cast_unsigned(),
            action as u64,
            0x4552_4646,
        );
        fingerprint = derive(
            fingerprint,
            i64::from(learner.error_slow[action]).cast_unsigned(),
            action as u64,
            0x4552_5353,
        );
        fingerprint = derive(
            fingerprint,
            learner.last_tried[action],
            action as u64,
            0x4C41_5354,
        );
        for (feature, value) in learner.after_spread[action].iter().enumerate() {
            fingerprint = derive(
                fingerprint,
                i64::from(*value).cast_unsigned(),
                feature as u64,
                0x4153_5052,
            );
        }
        for (feature, value) in learner.after[action].iter().enumerate() {
            fingerprint = derive(
                fingerprint,
                i64::from(*value).cast_unsigned(),
                feature as u64,
                0x4146_5452,
            );
        }
    }
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
