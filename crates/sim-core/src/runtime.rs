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

mod episodic;
mod homeostasis;
mod predictive;

pub use episodic::{
    EPISODIC_CAPACITY, EPISODIC_ENCODER_VERSION, EPISODIC_SCHEMA_VERSION, EPISODIC_TOP_K,
    EpisodicConfig, EpisodicEvent, EpisodicMemory, EpisodicPolicyDecision, EpisodicSummary,
    EpisodicWrite,
};
pub use homeostasis::{
    ENERGY_AT_BIRTH, ENERGY_MAX, ENERGY_SET_POINT, FEATURES as HOMEOSTATIC_FEATURES, Homeostasis,
    JAW_EXPERIMENT, ROOTING_REST, ROOTING_SMELL, homeostasis_fingerprint,
};
pub use predictive::{
    ACTION_FEATURES as PREDICTIVE_ACTION_FEATURES, LATENT_FEATURES, REPLAY_CAPACITY,
    RecurrentPredictor, SENSE_FEATURES, TRUNCATED_SEQUENCE, UPDATE_BATCH, UPDATE_CADENCE,
    developmental_key as predictive_developmental_key, sense_features,
};

/// Explicit action-selection policy for a standalone founder.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ExperimentMode {
    /// Frozen predictive founder semantics (the default and legacy baseline).
    #[default]
    PredictiveBaseline,
    /// Deterministic, intrinsic-only causal experimentation.
    AffordanceDiscovery,
    /// Live energy: learns from its own drive reduction while hungry, explores by curiosity
    /// while sated.
    Homeostatic,
}

/// Number of primitive experiments the live policy chooses between.
///
/// The first 12 preserve the original action indices; use and next-item were appended so old
/// checkpoints can be expanded without reinterpreting learned rows.
pub const AFFORDANCE_ACTIONS: usize = 14;
const LEGACY_AFFORDANCE_ACTIONS: usize = 12;
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

/// Tunable settings of the affordance learner. They are checkpointed with its statistics, so a
/// resumed run keeps the settings it started with.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct AffordanceConfig {
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
/// The learner measures how far the outcomes that
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
    pub visits: Vec<u16>,
    /// Recent average outcome following each experiment, scaled by 256.
    pub after: Vec<[i32; OUTCOME_FEATURES]>,
    /// Recent average outcome over all decisions, scaled by 256.
    pub overall: [i32; OUTCOME_FEATURES],
    /// Recent mean absolute deviation of the outcome following each experiment, scaled by 256.
    #[serde(default)]
    pub after_spread: Vec<[i32; OUTCOME_FEATURES]>,
    /// Recent mean absolute deviation of the outcome over all decisions, scaled by 256.
    #[serde(default)]
    pub overall_spread: [i32; OUTCOME_FEATURES],
    /// Decision tick at which each experiment was last selected.
    pub last_tried: Vec<u64>,
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
    pub fn new(config: AffordanceConfig) -> Self {
        Self {
            config,
            visits: vec![0; AFFORDANCE_ACTIONS],
            after: vec![[0; OUTCOME_FEATURES]; AFFORDANCE_ACTIONS],
            overall: [0; OUTCOME_FEATURES],
            after_spread: vec![[0; OUTCOME_FEATURES]; AFFORDANCE_ACTIONS],
            overall_spread: [0; OUTCOME_FEATURES],
            last_tried: vec![0; AFFORDANCE_ACTIONS],
            previous_action: AFFORDANCE_ACTIONS,
            recent_actions: no_recent_actions(),
            previous_view: [[0; VIEW_CHANNELS]; RETINAL_RAY_COUNT],
            bout_remaining: 0,
        }
    }

    /// Checks settings and every stored experiment index, so a decoded learner cannot index
    /// outside its fixed tables.
    fn migrate_and_validate(&mut self) -> Result<(), String> {
        let lengths = [
            self.visits.len(),
            self.after.len(),
            self.after_spread.len(),
            self.last_tried.len(),
        ];
        if lengths
            .into_iter()
            .all(|length| length == LEGACY_AFFORDANCE_ACTIONS)
        {
            self.visits.resize(AFFORDANCE_ACTIONS, 0);
            self.after.resize(AFFORDANCE_ACTIONS, [0; OUTCOME_FEATURES]);
            self.after_spread
                .resize(AFFORDANCE_ACTIONS, [0; OUTCOME_FEATURES]);
            self.last_tried.resize(AFFORDANCE_ACTIONS, 0);
        }
        if [
            self.visits.len(),
            self.after.len(),
            self.after_spread.len(),
            self.last_tried.len(),
        ]
        .into_iter()
        .any(|length| length != AFFORDANCE_ACTIONS)
        {
            return Err("affordance action table has the wrong length".to_owned());
        }
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

    fn observe(&mut self, observation: &EnvironmentObservation) {
        let now = self.outcome(observation);
        if self.previous_action >= AFFORDANCE_ACTIONS {
            self.overall = now;
            return;
        }
        let config = self.config;
        let action = self.previous_action;
        self.visits[action] = self.visits[action].saturating_add(1);
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

    fn select(&mut self, seed: u64, tick: u64) -> (PrimitiveAction, u32) {
        let config = self.config;
        if self.bout_remaining > 0 && self.previous_action < AFFORDANCE_ACTIONS {
            // Continue the current bout: the experiment is held, but its outcomes keep teaching.
            self.bout_remaining -= 1;
            let held = self.previous_action;
            self.recent_actions.rotate_right(1);
            self.recent_actions[0] = held;
            self.last_tried[held] = tick;
            return (affordance_action(held), self.contingency(held));
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
                .contingency(candidate)
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

impl AffordanceLearner {
    /// Records an experiment chosen by another policy, so its outcome is credited to it and the
    /// next choice starts afresh.
    fn note_choice(&mut self, action: usize, tick: u64) {
        self.previous_action = action;
        self.bout_remaining = 0;
        self.recent_actions.rotate_right(1);
        self.recent_actions[0] = action;
        self.last_tried[action] = tick;
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
        11 => action.acoustic = 256,
        12 => action.use_pulse = 256,
        13 => action.next_slot = 256,
        _ => {}
    }
    action
}

/// Internal stores felt by the external one-agent runtime: frozen at these values unless the
/// endpoint reports what the body feels (body contract version 6).
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
    /// Live energy and learned values in homeostatic mode; empty otherwise.
    #[serde(default)]
    pub homeostasis: Homeostasis,
    /// Compact lifetime-only recurrent next-sense predictor.
    #[serde(default)]
    pub recurrent_predictor: RecurrentPredictor,
    /// Private bounded lifetime episodic records and their independent ablation settings.
    #[serde(default)]
    pub episodic_memory: EpisodicMemory,
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
                homeostasis: Homeostasis::default(),
                recurrent_predictor: RecurrentPredictor::new(predictive::developmental_key(
                    &developmental_config,
                )),
                episodic_memory: EpisodicMemory::default(),
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
        if mode == ExperimentMode::Homeostatic {
            runtime.state.homeostasis = Homeostasis::newborn();
            runtime.state.physiology.energy = ENERGY_AT_BIRTH;
        }
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

    /// Sets independent episodic prediction and policy ablations.
    ///
    /// Existing records are retained, so callers can perform paired inference ablations without
    /// changing the experience available to either arm.
    pub const fn set_episodic_config(&mut self, config: EpisodicConfig) {
        self.state.episodic_memory.config = config;
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
    #[allow(
        clippy::large_types_passed_by_value,
        clippy::too_many_lines,
        reason = "the observation is recorded by value in the returned transition"
    )]
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
                pre_episodic_action: PrimitiveAction::default(),
                episodic_write: EpisodicWrite::default(),
                episodic_policy: EpisodicPolicyDecision::default(),
                affordance_fingerprint: learner_fingerprint(&self.state),
                brain_fingerprint: brain_fingerprint(&self.state.brain),
            });
        }
        let mut external = observation.senses;
        observation.capabilities.dead.silence(&mut external);
        let smell = [external.chemicals[0], external.chemicals[1]];
        if let Some(felt) = observation.interoception {
            // A body that reports what it feels replaces the fixed stores with its own: energy
            // is how fed it is, damage how far from full health.
            self.state.physiology.energy = felt.fullness;
            self.state.physiology.damage = 255_u16.saturating_sub(felt.health);
        }
        let reward = if self.state.experiment_mode == ExperimentMode::Homeostatic {
            let reward = self.state.homeostasis.live(&observation);
            self.state.physiology.energy = self.state.homeostasis.energy;
            reward
        } else {
            0
        };
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
        let current_source = sense_features(&observation);
        let policy_summaries = self
            .state
            .episodic_memory
            .retrieve(&self.state.recurrent_predictor.hidden, &current_source);
        let (mut action, intrinsic_score, recorded_baseline) = match self.state.experiment_mode {
            ExperimentMode::PredictiveBaseline => (baseline_action, 0, PrimitiveAction::default()),
            ExperimentMode::AffordanceDiscovery => {
                self.state.affordance.observe(&observation);
                let (action, score) = self.state.affordance.select(self.seed, decision_tick);
                (action, score, baseline_action)
            }
            ExperimentMode::Homeostatic => {
                let (action, score) = self.homeostatic_choice(&observation, smell, reward);
                (action, score, baseline_action)
            }
        };
        let pre_episodic_action = action;
        let (recommended_action, mut episodic_policy) =
            EpisodicMemory::policy_decision(&policy_summaries, action);
        if self.state.episodic_memory.config.policy_enabled {
            action = recommended_action;
        } else {
            episodic_policy.action_changed = false;
        }
        let had_prediction = self.state.recurrent_predictor.prediction_ready;
        let latent_before = self.state.recurrent_predictor.hidden.clone();
        let source_before = self.state.recurrent_predictor.previous_senses.clone();
        let pending_prediction = self.state.recurrent_predictor.prediction.clone();
        let preceding_action = self.state.recurrent_predictor.previous_action;
        let uncertainty = self.state.recurrent_predictor.uncertainty();
        self.state
            .recurrent_predictor
            .observe(&observation, action, decision_tick);
        let episodic_write = if had_prediction {
            self.state.episodic_memory.observe(
                &latent_before,
                &self.state.recurrent_predictor.hidden,
                &source_before,
                &current_source,
                preceding_action,
                &pending_prediction,
                uncertainty,
                &observation,
                decision_tick,
            )
        } else {
            EpisodicWrite::default()
        };
        if self.state.episodic_memory.config.prediction_enabled {
            let summaries = self.state.episodic_memory.retrieve_for_action(
                &self.state.recurrent_predictor.hidden,
                &current_source,
                self.state.recurrent_predictor.previous_action,
            );
            self.state
                .recurrent_predictor
                .apply_episodic_prediction(&summaries);
        }
        self.state.chemical_adaptation = adaptation;
        self.state.decision_tick += 1;
        Ok(EnvironmentTransition {
            decision_tick,
            observation,
            action,
            baseline_action: recorded_baseline,
            intrinsic_score,
            pre_episodic_action,
            episodic_write,
            episodic_policy,
            affordance_fingerprint: learner_fingerprint(&self.state),
            brain_fingerprint: brain_fingerprint(&self.state.brain),
        })
    }

    /// Learns from the drive reduction that followed the previous decision, then acts: on
    /// learned value while hungry, by curiosity while sated. Curiosity keeps learning throughout.
    fn homeostatic_choice(
        &mut self,
        observation: &EnvironmentObservation,
        smell: [u16; 2],
        reward: i64,
    ) -> (PrimitiveAction, u32) {
        let tick = self.state.decision_tick;
        let state = &mut self.state;
        state.affordance.observe(observation);
        let features = state.homeostasis.features(observation, smell);
        state.homeostasis.learn(reward, &features);
        let (index, score) = if state.homeostasis.rooting(smell) {
            state.affordance.note_choice(JAW_EXPERIMENT, tick);
            (JAW_EXPERIMENT, 0)
        } else if state.homeostasis.hungry() {
            let (index, value) = state.homeostasis.choose(&features, self.seed, tick);
            state.affordance.note_choice(index, tick);
            (index, u32::try_from(value.max(0)).unwrap_or(u32::MAX))
        } else {
            let (_, score) = state.affordance.select(self.seed, tick);
            (state.affordance.previous_action, score)
        };
        state.homeostasis.remember(index, features);
        (affordance_action(index), score)
    }
}

/// Fingerprint of whichever learner the experiment mode uses, or zero for the baseline.
fn learner_fingerprint(state: &AgentRuntimeState) -> u64 {
    match state.experiment_mode {
        ExperimentMode::PredictiveBaseline => 0,
        ExperimentMode::AffordanceDiscovery => affordance_fingerprint(&state.affordance),
        ExperimentMode::Homeostatic => derive(
            affordance_fingerprint(&state.affordance),
            homeostasis_fingerprint(&state.homeostasis),
            0,
            0x484F_4D53,
        ),
    }
}

/// Stable fingerprint of all checkpointed intrinsic learner statistics.
#[must_use]
pub fn affordance_fingerprint(learner: &AffordanceLearner) -> u64 {
    let mut fingerprint = derive(0, learner.previous_action as u64, 0, 0x4146_4650);
    let config = learner.config;
    for (index, value) in [
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
    #[serde(default)]
    homeostasis: Homeostasis,
    #[serde(default)]
    recurrent_predictor: RecurrentPredictor,
    #[serde(default)]
    episodic_memory: EpisodicMemory,
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
            homeostasis: state.homeostasis,
            recurrent_predictor: state.recurrent_predictor,
            episodic_memory: state.episodic_memory,
        }
    }
}

impl TryFrom<StateRepr> for AgentRuntimeState {
    type Error = String;

    fn try_from(mut repr: StateRepr) -> Result<Self, Self::Error> {
        let packed: GpuBrain = bytemuck::try_pod_read_unaligned(bytemuck::cast_slice(&repr.brain))
            .map_err(|_| format!("brain has {} words", repr.brain.len()))?;
        let brain = packed.unpack();
        if GpuBrain::pack(&brain) != packed || brain.next_episode >= EPISODE_CAPACITY {
            return Err("brain state is out of range".to_owned());
        }
        if repr.connection_stride == 0 || repr.plasticity_scale < 1 {
            return Err("developmental configuration is out of range".to_owned());
        }
        repr.affordance.migrate_and_validate()?;
        repr.homeostasis.migrate_and_validate()?;
        let expected_key = predictive::developmental_key(&BrainConfig {
            connection_stride: repr.connection_stride,
            plasticity_scale: repr.plasticity_scale,
            plasticity_enabled: repr.plasticity_enabled,
            episodic_memory_enabled: repr.episodic_memory_enabled,
            action_contingency_enabled: repr.action_contingency_enabled,
            prediction_enabled: repr.prediction_enabled,
        });
        if repr.recurrent_predictor.developmental_key == 0
            && repr.recurrent_predictor == RecurrentPredictor::default()
        {
            repr.recurrent_predictor = RecurrentPredictor::new(expected_key);
        }
        if repr.recurrent_predictor.developmental_key != expected_key {
            return Err(
                "recurrent predictor does not match developmental configuration".to_owned(),
            );
        }
        repr.recurrent_predictor.validate()?;
        repr.episodic_memory.migrate_and_validate()?;
        if (repr.experiment_mode == ExperimentMode::Homeostatic) != repr.homeostasis.active() {
            return Err("homeostatic state does not match the experiment mode".to_owned());
        }
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
            homeostasis: repr.homeostasis,
            recurrent_predictor: repr.recurrent_predictor,
            episodic_memory: repr.episodic_memory,
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
            retina: None,
            interoception: None,
            held_item: None,
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
        HOMEOSTATIC_FEATURES, MAX_ELIGIBILITY_SPAN, RuntimeError, affordance_action,
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
    fn affordance_settings_survive_a_checkpoint() {
        let settings = AffordanceConfig {
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
        let longer_memory = run(AffordanceConfig {
            action_horizon: 16,
            ..AffordanceConfig::default()
        });
        assert_eq!(default, run(AffordanceConfig::default()));
        assert_ne!(default, longer_memory);
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
    fn every_live_policy_action_maps_to_one_primitive() {
        let actions: Vec<_> = (0..AFFORDANCE_ACTIONS).map(affordance_action).collect();
        assert_eq!(actions.len(), 14);
        assert_eq!(actions[0].thrust, 256);
        assert_eq!(actions[1].thrust, -256);
        assert_eq!(actions[2].turn, 256);
        assert_eq!(actions[3].turn, -256);
        assert_eq!(actions[4].strafe, 256);
        assert_eq!(actions[5].strafe, -256);
        assert_eq!(actions[6].look_pitch, 256);
        assert_eq!(actions[7].look_pitch, -256);
        assert_eq!(actions[8].jump, 256);
        assert_eq!(actions[9].hand_pulse, 256);
        assert_eq!(actions[10].jaw, 256);
        assert_eq!(actions[11].acoustic, 256);
        assert_eq!(actions[12].use_pulse, 256);
        assert_eq!(actions[13].next_slot, 256);
        for (index, action) in actions.iter().enumerate() {
            assert!(
                actions
                    .iter()
                    .enumerate()
                    .all(|(other, candidate)| other == index || candidate != action)
            );
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

    #[test]
    fn legacy_twelve_action_tables_expand_without_reinterpreting_rows() {
        let runtime = affordance_runtime(AffordanceConfig::default());
        let mut json = serde_json::to_value(runtime.state()).expect("encode");
        for field in ["visits", "after", "after_spread", "last_tried"] {
            json["affordance"][field]
                .as_array_mut()
                .expect("action table")
                .truncate(12);
        }
        let decoded =
            serde_json::from_value::<super::AgentRuntimeState>(json).expect("migrate legacy state");
        assert_eq!(decoded.affordance.visits.len(), AFFORDANCE_ACTIONS);
        assert!(
            decoded.affordance.visits[12..]
                .iter()
                .all(|visits| *visits == 0)
        );
    }

    #[test]
    fn legacy_homeostatic_weights_append_blank_action_rows() {
        let runtime =
            AgentRuntime::new_with_mode(71, 4, config(), super::ExperimentMode::Homeostatic);
        let mut json = serde_json::to_value(runtime.state()).expect("encode");
        for field in ["visits", "after", "after_spread", "last_tried"] {
            json["affordance"][field]
                .as_array_mut()
                .expect("action table")
                .truncate(12);
        }
        json["homeostasis"]["weights"]
            .as_array_mut()
            .expect("weights")
            .truncate(12 * HOMEOSTATIC_FEATURES);
        let decoded =
            serde_json::from_value::<super::AgentRuntimeState>(json).expect("migrate legacy state");
        assert_eq!(
            decoded.homeostasis.weights.len(),
            AFFORDANCE_ACTIONS * HOMEOSTATIC_FEATURES
        );
        assert!(
            decoded.homeostasis.weights[12 * HOMEOSTATIC_FEATURES..]
                .iter()
                .all(|weight| *weight == 0)
        );
    }
}
