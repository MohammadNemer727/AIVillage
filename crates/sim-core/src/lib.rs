//! Deterministic, renderer-independent simulation state for AI Village.
//!
//! [`World`] is the sole authority for a simulation run. Agent data is held in dense,
//! structure-of-arrays buffers; public [`AgentId`] values are stable generational handles rather
//! than buffer indices. Each [`World::step`] executes the same explicit phase order and returns an
//! owned [`WorldSnapshot`], so renderers and persistence layers cannot mutate simulation truth.
//!
//! Random-looking decisions are derived from the world seed, tick, agent ID, and subsystem tag.
//! Consequently, replay does not depend on a shared mutable RNG or scheduling order.

#![forbid(unsafe_code)]

use core::fmt;
use std::error::Error;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sim_kernels::{
    brain::{CONTINGENCY_ACTIONS, CONTINGENCY_FEATURES, EPISODE_CAPACITY},
    controller::{GENOME_WEIGHTS, HIDDEN, INPUTS, OUTPUTS, SIGNAL_CHANNELS},
    derive, heading, integer_sqrt,
    senses::noisy_receptor,
};

mod backend;
mod checkpoint;
pub mod environment;
pub mod runtime;

pub use backend::{
    BACKEND_ENV, Backend, BackendPreference, BackendRunner, BackendStatus, BoxedBrainBackend,
    OpenGpu, ParseBackendPreferenceError, gpu_not_built,
};
pub use checkpoint::{CheckpointError, WorldCheckpoint};
pub use sim_kernels::{
    brain::{BrainConfig, BrainStep, EpisodicTrace, NEURONS as V2_BRAIN_NEURONS, PredictiveBrain},
    senses::{CHEMICAL_RECEPTOR_COUNT, RETINAL_RAY_COUNT, RawSensoryFrame, RetinalSample},
};

/// Decodes predictive-brain motor outputs into transport-neutral primitive actions.
#[must_use]
pub fn decode_motor_outputs(outputs: &[i16; OUTPUTS]) -> environment::PrimitiveAction {
    environment::PrimitiveAction {
        thrust: (outputs[1] - outputs[2]).clamp(-256, 256),
        turn: (outputs[3] - outputs[0]).clamp(-256, 256),
        strafe: 0,
        look_pitch: 0,
        jump: 0,
        jaw: outputs[4].clamp(0, 256).cast_unsigned(),
        hand_pulse: 0,
        acoustic: outputs[5].max(0).cast_unsigned(),
        chemical: outputs[6].max(0).cast_unsigned(),
        visual: outputs[7].max(0).cast_unsigned(),
        signals: core::array::from_fn(|symbol| outputs[8 + symbol]),
        use_pulse: 0,
        next_slot: 0,
    }
}

/// Scales a bounded V2 yaw motor amplitude to the fixed orientation step.
#[must_use]
pub fn scale_v2_orientation_turn(turn: i16) -> i8 {
    i8::try_from((i32::from(turn) / 32).clamp(-8, 8)).unwrap_or_default()
}

/// Scales a bounded V2 thrust motor amplitude to fixed-point acceleration.
#[must_use]
pub fn scale_v2_thrust(thrust: i16) -> i32 {
    i32::from(thrust) * 10 / 256
}

/// Applies V2 drag and forward-axis thrust to a fixed-point velocity.
#[must_use]
pub fn scale_v2_velocity(
    velocity: Velocity,
    orientation: Angle,
    thrust: i16,
    terrain_traction: i32,
) -> Velocity {
    let (forward_x, forward_y) = heading(orientation.turns());
    let thrust = scale_v2_thrust(thrust);
    Velocity {
        x: Fixed::from_raw(
            (velocity.x.raw() * 7 / 8).saturating_add(forward_x * thrust * terrain_traction / 4),
        ),
        y: Fixed::from_raw(
            (velocity.y.raw() * 7 / 8).saturating_add(forward_y * thrust * terrain_traction / 4),
        ),
    }
}

/// Scales the V2 jaw motor amplitude to a fixed aperture.
#[must_use]
pub fn scale_v2_jaw(jaw: i16) -> Fixed {
    Fixed::from_raw(i32::from(jaw) * FIXED_SCALE / 256)
}

/// Applies the bounded, leaky adaptation shared by chemical receptors.
#[must_use]
pub fn adapt_chemical_receptors(
    concentrations: [u16; CHEMICAL_RECEPTOR_COUNT],
    previous: [u16; CHEMICAL_RECEPTOR_COUNT],
) -> (
    [u16; CHEMICAL_RECEPTOR_COUNT],
    [u16; CHEMICAL_RECEPTOR_COUNT],
) {
    let mut chemicals = [0; CHEMICAL_RECEPTOR_COUNT];
    let mut adaptation = previous;
    for receptor in 0..CHEMICAL_RECEPTOR_COUNT {
        chemicals[receptor] = concentrations[receptor]
            .saturating_sub(adaptation[receptor] / 2)
            .min(V2_SENSOR_MAX_VALUE);
        adaptation[receptor] = (adaptation[receptor].saturating_mul(3) / 4)
            .saturating_add(concentrations[receptor] / 4)
            .min(V2_SENSOR_MAX_VALUE);
    }
    (chemicals, adaptation)
}

/// Adds deterministically keyed noise to frozen or simulated internal stores.
#[must_use]
pub fn assemble_interoception(
    stores: [u16; 6],
    seed: u64,
    tick: u64,
    receptor_key: u64,
) -> [u16; 6] {
    const TAGS: [u64; 6] = [
        0x454E_4552,
        0x4755_5421,
        0x544F_584E,
        0x4859_4452,
        0x4641_5447,
        0x4441_4D47,
    ];
    core::array::from_fn(|index| {
        noisy_receptor(stores[index], seed, tick, receptor_key, TAGS[index])
    })
}

/// Combines externally observed receptors with deterministically noised internal stores.
#[must_use]
pub fn compose_raw_sensory_frame(
    mut external: RawSensoryFrame,
    stores: [u16; 6],
    seed: u64,
    tick: u64,
    receptor_key: u64,
) -> RawSensoryFrame {
    let [
        energy,
        gut_pressure,
        toxin_stress,
        hydration,
        fatigue,
        damage,
    ] = assemble_interoception(stores, seed, tick, receptor_key);
    external.energy = energy;
    external.gut_pressure = gut_pressure;
    external.toxin_stress = toxin_stress;
    external.hydration = hydration;
    external.fatigue = fatigue;
    external.damage = damage;
    external
}

/// Time-domain exports for callers that prefer subsystem imports.
pub mod time {
    pub use crate::Tick;
}

/// Stable identity exports for callers that prefer subsystem imports.
pub mod identity {
    pub use crate::AgentId;
}

/// World configuration exports for callers that prefer subsystem imports.
pub mod settings {
    pub use crate::{WorldError, WorldSeed, WorldSettings};
}

/// Deterministic pipeline exports for callers that prefer subsystem imports.
pub mod world {
    pub use crate::{TickPhase, World, WorldSnapshot};
}

/// Schema version for [`WorldSnapshot::state_hash`] field coverage.
///
/// Bump only when the set or order of hashed authoritative fields changes. V1 freeze uses
/// complete tagged coverage of agents, parentage, free-list, prior signals, controls, and
/// counters without altering gameplay semantics.
pub const STATE_HASH_VERSION: u32 = 1;

/// Selects the authoritative movement model.
///
/// V1 deliberately retains the frozen grid/replay behavior. V2 is an opt-in
/// fixed-point body simulation and never changes a V1 history.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ModelVersion {
    /// Frozen grid movement model used by V1 fixtures and checkpoints.
    #[default]
    V1Grid,
    /// Continuous fixed-point animal-body movement model.
    V2Physics,
}

/// Fixed-point coordinate scale used by V2 authoritative physics.
pub const FIXED_SCALE: i32 = 1_024;

/// Signed fixed-point scalar. Authoritative physics never uses floating point.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Fixed(i32);

impl Fixed {
    /// Creates a raw fixed-point value.
    #[must_use]
    pub const fn from_raw(value: i32) -> Self {
        Self(value)
    }

    /// Creates a whole-unit fixed-point value.
    #[must_use]
    pub const fn from_units(value: i32) -> Self {
        Self(value.saturating_mul(FIXED_SCALE))
    }

    /// Returns the raw fixed-point representation.
    #[must_use]
    pub const fn raw(self) -> i32 {
        self.0
    }
}

/// Fixed-point continuous position in world-cell units.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Position {
    /// Horizontal coordinate.
    pub x: Fixed,
    /// Vertical coordinate.
    pub y: Fixed,
}

/// Fixed-point velocity in world-cell units per tick.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Velocity {
    /// Horizontal component.
    pub x: Fixed,
    /// Vertical component.
    pub y: Fixed,
}

/// A turn modulo 256; zero faces positive X and turns increase counterclockwise.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Angle(u8);

impl Angle {
    /// Returns the canonical turn representation.
    #[must_use]
    pub const fn turns(self) -> u8 {
        self.0
    }
}

/// Presentation-safe V2 animal-body state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BodySnapshot {
    /// Continuous torso centre.
    pub position: Position,
    /// Current torso velocity.
    pub velocity: Velocity,
    /// Facing direction.
    pub orientation: Angle,
    /// Torso circle radius.
    pub torso_radius: Fixed,
    /// Head circle radius.
    pub head_radius: Fixed,
    /// Mouth aperture from closed (zero) to fully open (`FIXED_SCALE`).
    pub mouth_open: Fixed,
}

/// Inspectable, bounded V2 body stores. These values are diagnostics only and never form
/// semantic controller inputs.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PhysiologySnapshot {
    /// Immediately usable metabolic energy.
    pub energy: u16,
    /// Energy reserve converted to usable energy during deficit.
    pub fat: u16,
    /// Food still waiting for delayed digestion.
    pub gut: u16,
    /// Body water reserve.
    pub hydration: u16,
    /// Accumulated exertion that weakens movement and repair.
    pub fatigue: u16,
    /// Reserve required before V2 reproduction is permitted.
    pub reproductive_reserve: u16,
    /// Cumulative delayed toxin burden.
    pub toxin: u16,
    /// Delayed infection burden caused by physical wounds; diagnostic only.
    pub infection: u16,
    /// Localized tissue integrity in head, torso, and limbs.
    pub tissue: [u16; 3],
    /// Noisy damage-derived body signal, not a reward.
    pub pain: u16,
}

const V2_RETINAL_RANGE_CELLS: i32 = 6;
const V2_SENSOR_COST: u16 = 1;
const V2_SENSOR_MAX_VALUE: u16 = 255;
/// Acoustic emissions are bounded to 255 and lose 32 per cell, so sources beyond this range
/// contribute exactly zero vibration.
const V2_VIBRATION_RANGE_CELLS: u16 = 255 / 32;

/// Read-only bounded V2 brain diagnostics for inspectors and experiment fixtures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrainSnapshot {
    /// Fixed controller population size.
    pub neurons: usize,
    /// Rate-coded activity for receptor, interneuron, motor, then modulatory populations.
    pub activity: [i16; V2_BRAIN_NEURONS],
    /// Local next-sensation predictions, compressed to four channels.
    pub prediction: [i16; 4],
    /// Capacity-limited, decaying lifetime episodes.
    pub episodes: [EpisodicTrace; EPISODE_CAPACITY],
    /// Action-conditioned (rows) contingency estimates (columns).
    pub contingencies: [[i16; CONTINGENCY_FEATURES]; CONTINGENCY_ACTIONS],
}

/// Slow deterministic environmental layers for one V2 snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EcologySnapshot {
    /// Plant biomass per cell.
    pub plants: Vec<u16>,
    /// Mineral nutrients per cell.
    pub nutrients: Vec<u16>,
    /// Moisture per cell.
    pub moisture: Vec<u16>,
    /// Ambient temperature layer per cell.
    pub temperature: Vec<i16>,
    /// Carrion and waste available to decomposers.
    pub detritus: Vec<u16>,
    /// Unlabelled water contamination intensity per cell.
    pub water_contamination: Vec<u16>,
    /// Whole-world matter total (plants, nutrients, detritus, and bodies).
    pub conserved_matter: u64,
}

/// A monotonically increasing authoritative simulation tick.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct Tick(u64);

impl Tick {
    /// Creates a tick from its zero-based sequence number.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the zero-based sequence number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for Tick {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A seed that identifies one reproducible world history.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct WorldSeed(u64);

impl WorldSeed {
    /// Creates a world seed.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw seed value for serialization or deterministic derivation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A stable, generational handle for an agent.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AgentId {
    slot: u32,
    generation: u32,
}

impl AgentId {
    /// Returns the slot component. This is useful for stable serialization, not array indexing.
    #[must_use]
    pub const fn slot(self) -> u32 {
        self.slot
    }

    /// Returns the generation component.
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

/// A location on the finite, wrapping world grid.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct GridPosition {
    x: u16,
    y: u16,
}

impl GridPosition {
    /// Creates a position. The caller must ensure it belongs to the relevant world.
    #[must_use]
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }

    /// Returns the horizontal grid coordinate.
    #[must_use]
    pub const fn x(self) -> u16 {
        self.x
    }

    /// Returns the vertical grid coordinate.
    #[must_use]
    pub const fn y(self) -> u16 {
        self.y
    }
}

/// Immutable parameters that determine a world's initial state and geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldSettings {
    width: u16,
    height: u16,
    initial_agents: u32,
}

impl WorldSettings {
    /// Creates world settings.
    ///
    /// # Errors
    ///
    /// Returns [`WorldError::ZeroDimension`] if either dimension is zero.
    pub const fn new(width: u16, height: u16, initial_agents: u32) -> Result<Self, WorldError> {
        if width == 0 || height == 0 {
            return Err(WorldError::ZeroDimension);
        }
        Ok(Self {
            width,
            height,
            initial_agents,
        })
    }

    /// Returns the grid width.
    #[must_use]
    pub const fn width(self) -> u16 {
        self.width
    }

    /// Returns the grid height.
    #[must_use]
    pub const fn height(self) -> u16 {
        self.height
    }

    /// Returns the deterministically seeded initial population.
    #[must_use]
    pub const fn initial_agents(self) -> u32 {
        self.initial_agents
    }
}

impl Default for WorldSettings {
    fn default() -> Self {
        // These match the first research-MVP target in the architecture document.
        Self {
            width: 128,
            height: 128,
            initial_agents: 128,
        }
    }
}

/// Errors from invalid simulation configuration or mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldError {
    /// A world grid must have non-zero width and height.
    ZeroDimension,
    /// A supplied position does not belong to this world grid.
    PositionOutOfBounds,
    /// No further stable slot can be allocated.
    AgentCapacityExhausted,
    /// The supplied generational handle does not refer to a living agent.
    UnknownAgent,
    /// Reusing a slot would overflow its generation counter.
    GenerationExhausted,
}

impl fmt::Display for WorldError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDimension => formatter.write_str("world dimensions must be non-zero"),
            Self::PositionOutOfBounds => formatter.write_str("position is outside the world grid"),
            Self::AgentCapacityExhausted => formatter.write_str("agent slot capacity is exhausted"),
            Self::UnknownAgent => {
                formatter.write_str("agent handle does not refer to a living agent")
            }
            Self::GenerationExhausted => {
                formatter.write_str("agent generation counter is exhausted")
            }
        }
    }
}

impl Error for WorldError {}

/// The fixed, ordered stages of every authoritative tick.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TickPhase {
    /// Read immutable world and spatial-index state.
    Sense,
    /// Compute controller output from the sensed state.
    Evaluate,
    /// Materialize controller output as per-agent intents.
    EmitIntents,
    /// Resolve all competing intents in stable-ID order.
    Resolve,
    /// Apply resolved state changes, including future ecology and lifecycle changes.
    Apply,
    /// Rebuild derived data and detach a snapshot for consumers.
    Publish,
}

const TICK_PIPELINE: [TickPhase; 6] = [
    TickPhase::Sense,
    TickPhase::Evaluate,
    TickPhase::EmitIntents,
    TickPhase::Resolve,
    TickPhase::Apply,
    TickPhase::Publish,
];

/// A detached representation of one agent suitable for rendering or storage.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AgentSnapshot {
    /// Stable identity of the represented agent.
    pub id: AgentId,
    /// Agent position after the represented tick.
    pub position: GridPosition,
    /// Age in completed ticks.
    pub age: u64,
    /// Stored metabolic energy, in deterministic integer units.
    pub energy: u16,
    /// Number of parent-to-child links from a seeded founder.
    pub lineage_depth: u32,
    /// Portable construction material currently carried by this agent.
    pub materials: u8,
    /// V2 body state when this snapshot was produced by the physics model.
    pub body: Option<BodySnapshot>,
    /// V2-only explicit physiological state.
    pub physiology: Option<PhysiologySnapshot>,
}

/// Population-level measurements that do not feed back into agent sensing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AdaptationMetrics {
    /// Number of living agents.
    pub population: u32,
    /// Cumulative successful births in this world history.
    pub births: u64,
    /// Cumulative deaths in this world history.
    pub deaths: u64,
    /// Mean genealogical depth of living agents, rounded down.
    pub mean_generation_depth: u32,
    /// Count of distinct compact genomes among living agents.
    pub distinct_genomes: u32,
    /// Distinct genomes divided by population, in parts per million.
    pub diversity_ppm: u32,
    /// Mean inherited mutation rate, in parts per million.
    pub mean_mutation_rate_ppm: u32,
    /// Cumulative transmissions that paid the signal cost.
    pub signals_emitted: u64,
    /// Cumulative energy spent on transmitting blank symbols.
    pub signal_energy_spent: u64,
    /// Cumulative local signal observations delivered to controllers.
    pub signal_observations: u64,
    /// Cumulative transmissions for each blank symbol, in symbol order.
    pub symbol_emissions: [u64; BLANK_SIGNAL_COUNT],
    /// Cumulative observations for each blank symbol, in symbol order.
    pub symbol_observations: [u64; BLANK_SIGNAL_COUNT],
    /// Channel-separated measurements for controlled transmission experiments.
    pub transmission: TransmissionMetrics,
}

/// One of the discrete, intentionally uninterpreted symbols agents may emit.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct BlankSignal(u8);

impl BlankSignal {
    /// Number of available symbols. No meaning is assigned to any value.
    pub const COUNT: u8 = 7;

    /// Creates a blank symbol when `value` is in the available alphabet.
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if value < Self::COUNT {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Returns this symbol's stable, meaning-free ordinal.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Deterministic causal manipulation applied only to signal delivery.
///
/// These modes assign no reward and define no semantic interpretation for symbols.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum SignalIntervention {
    /// Deliver nearby transmissions unchanged.
    #[default]
    Baseline,
    /// Remove all transmissions before they reach controller observations.
    Mute,
    /// Deterministically exchange source symbols while retaining source locations.
    Shuffle,
    /// Replace each delivered symbol with a deterministic blank symbol.
    Replace,
    /// Deliver transmissions from one additional tick earlier.
    Delay,
}

/// Controls that isolate candidate transmission channels in a reproducible experiment.
///
/// These controls do not establish culture. They allow comparisons where inherited genomes,
/// mutable learning, signals, and persistent artifacts are removed independently.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Each flag independently disables one experimental transmission channel."
)]
pub struct CulturalTransmissionControls {
    /// Whether offspring genomes may mutate during reproduction.
    pub genetic_mutation_enabled: bool,
    /// Whether bounded, non-inherited lifetime plasticity is updated.
    pub plasticity_enabled: bool,
    /// Whether blank signals may be sent or received.
    pub signals_enabled: bool,
    /// Whether agents may create or remove persistent blocks.
    pub artifacts_enabled: bool,
    /// Whether V2 animals can produce and receive physical acoustic, chemical, and visual cues.
    pub physical_social_signals_enabled: bool,
}

/// Targeted V2 learning controls used for causal-learning ablations.
///
/// These affect no V1 state or behavior. They intentionally isolate prediction, episodic
/// retention, action records, and three-factor plasticity without introducing exogenous reward.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Each targeted ablation independently isolates one causal-learning mechanism."
)]
pub struct BrainAblations {
    /// Enables local next-sensation prediction and surprise.
    pub prediction_enabled: bool,
    /// Enables bounded episodic trace storage and decay.
    pub episodic_memory_enabled: bool,
    /// Enables action-conditioned, rather than observation-only, contingency updates.
    pub action_contingency_enabled: bool,
    /// Enables eligibility-trace updates gated by endogenous events.
    pub plasticity_enabled: bool,
}

impl Default for BrainAblations {
    fn default() -> Self {
        Self {
            prediction_enabled: true,
            episodic_memory_enabled: true,
            action_contingency_enabled: true,
            plasticity_enabled: true,
        }
    }
}

impl Default for CulturalTransmissionControls {
    fn default() -> Self {
        Self {
            genetic_mutation_enabled: true,
            plasticity_enabled: true,
            signals_enabled: true,
            artifacts_enabled: true,
            physical_social_signals_enabled: false,
        }
    }
}

impl CulturalTransmissionControls {
    const fn all_enabled() -> Self {
        Self {
            genetic_mutation_enabled: true,
            plasticity_enabled: true,
            signals_enabled: true,
            artifacts_enabled: true,
            physical_social_signals_enabled: false,
        }
    }
}

/// Passive channel-separated measurements for controlled transmission experiments.
///
/// None of these counters alone is evidence of culture.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransmissionMetrics {
    /// Births that inherited a genome, distinct from mutable learning state.
    pub genetic_births: u64,
    /// Bounded plasticity updates performed on living agents.
    pub learning_updates: u64,
    /// Living agents whose mutable learning state differs from the birth reset.
    pub experienced_agents: u32,
    /// Current persistent blocks available as environmental artifacts.
    pub persistent_artifacts: u32,
    /// Signal observations delivered while the signal channel was enabled.
    pub signal_observations: u64,
}

/// An immutable, owned view of authoritative state after a tick.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldSnapshot {
    tick: Tick,
    agents: Vec<AgentSnapshot>,
    resources: Vec<u16>,
    materials: Vec<u16>,
    blocks: Vec<u16>,
    state_hash: u64,
    metrics: AdaptationMetrics,
    ecology: Option<EcologySnapshot>,
}

impl WorldSnapshot {
    /// Returns the completed tick represented by this snapshot.
    #[must_use]
    pub const fn tick(&self) -> Tick {
        self.tick
    }

    /// Returns agents sorted by stable ID.
    #[must_use]
    pub fn agents(&self) -> &[AgentSnapshot] {
        &self.agents
    }

    /// Returns row-major food resource quantities for every world cell.
    #[must_use]
    pub fn resources(&self) -> &[u16] {
        &self.resources
    }

    /// Returns row-major loose material quantities for every world cell.
    #[must_use]
    pub fn materials(&self) -> &[u16] {
        &self.materials
    }

    /// Returns row-major block durability, with zero representing no block.
    #[must_use]
    pub fn blocks(&self) -> &[u16] {
        &self.blocks
    }

    /// Returns a stable hash of all authoritative state in this snapshot.
    #[must_use]
    pub const fn state_hash(&self) -> u64 {
        self.state_hash
    }

    /// Returns passive population adaptation and diversity measurements.
    #[must_use]
    pub const fn metrics(&self) -> AdaptationMetrics {
        self.metrics
    }

    /// Returns V2 ecological diagnostics, absent for the frozen V1 model.
    #[must_use]
    pub fn ecology(&self) -> Option<&EcologySnapshot> {
        self.ecology.as_ref()
    }
}

#[derive(Clone, Copy, Debug)]
struct AgentSlot {
    generation: u32,
    alive: bool,
}

/// Executes V2 predictive-brain steps for a batch of agents, for example on a GPU.
///
/// Implementations must be bit-identical to [`PredictiveBrain::step`] applied to each agent;
/// `sim-gpu` verifies this for its GPU backend. Pass one to [`World::step_with_brains`].
pub trait BrainBackend {
    /// Advances `brains[i]` with `frames[i]` and `configs[i]`, returning each agent's step in
    /// the same order.
    ///
    /// # Errors
    ///
    /// Returns an error if the batch could not be executed.
    fn step_brains(
        &mut self,
        brains: &mut [PredictiveBrain],
        frames: &[RawSensoryFrame],
        configs: &[BrainConfig],
    ) -> Result<Vec<BrainStep>, BrainBackendError>;

    /// Names the device that runs the brains, for logs and diagnostics.
    #[allow(
        clippy::unnecessary_literal_bound,
        reason = "Implementations return names borrowed from themselves, such as an adapter's."
    )]
    fn device_name(&self) -> &str {
        "external brain backend"
    }
}

/// A [`BrainBackend`] failure. The tick it occurred in left no state changes.
#[derive(Debug)]
pub struct BrainBackendError(Box<dyn Error + Send + Sync>);

impl BrainBackendError {
    /// Wraps a backend-specific error or message.
    pub fn new(error: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self(error.into())
    }
}

impl fmt::Display for BrainBackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "brain backend failed: {}", self.0)
    }
}

impl Error for BrainBackendError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
    }
}

/// Living agents bucketed by grid cell, in stable slot order within each cell.
///
/// Stored as compressed sparse rows: `agents[cell_starts[c]..cell_starts[c + 1]]` are the agents
/// in cell `c`. Lookup is O(1), rebuilding is a counting sort with no per-cell allocation, and
/// both flat buffers can be uploaded unchanged to a GPU backend.
#[derive(Clone, Debug, Default)]
struct SpatialIndex {
    cell_starts: Vec<usize>,
    agents: Vec<AgentId>,
    /// Scratch write cursors reused across rebuilds.
    cursors: Vec<usize>,
}

impl SpatialIndex {
    const fn new() -> Self {
        Self {
            cell_starts: Vec::new(),
            agents: Vec::new(),
            cursors: Vec::new(),
        }
    }

    /// Rebuilds from `(cell, id)` pairs, which must be yielded in ascending slot order.
    fn rebuild(
        &mut self,
        cell_count: usize,
        entries: impl Iterator<Item = (usize, AgentId)> + Clone,
    ) {
        self.cell_starts.clear();
        self.cell_starts.resize(cell_count + 1, 0);
        for (cell, _) in entries.clone() {
            self.cell_starts[cell + 1] += 1;
        }
        for cell in 0..cell_count {
            self.cell_starts[cell + 1] += self.cell_starts[cell];
        }
        self.cursors.clear();
        self.cursors
            .extend_from_slice(&self.cell_starts[..cell_count]);
        self.agents.clear();
        self.agents.resize(
            self.cell_starts[cell_count],
            AgentId {
                slot: 0,
                generation: 0,
            },
        );
        for (cell, id) in entries {
            self.agents[self.cursors[cell]] = id;
            self.cursors[cell] += 1;
        }
    }

    /// Returns the agents in one cell, or none before the first rebuild.
    fn cell(&self, cell: usize) -> &[AgentId] {
        match (self.cell_starts.get(cell), self.cell_starts.get(cell + 1)) {
            (Some(&start), Some(&end)) => &self.agents[start..end],
            _ => &[],
        }
    }
}

/// What one living agent perceived this tick, sampled from the immutable pre-tick world.
enum Perception {
    /// V1 grid agent: controller inputs plus the local cell facts its action gates need.
    Grid(GridPerception),
    /// V2 animal body: a raw receptor frame.
    Body(BodyPerception),
}

struct GridPerception {
    inputs: [i16; INPUTS],
    food_here: u16,
    material_here: u16,
    block_here: u16,
    nearby_agents: usize,
    observed_signal: Option<BlankSignal>,
}

#[derive(Debug, PartialEq)]
struct BodyPerception {
    frame: RawSensoryFrame,
    /// The receptor adaptation state to keep after this sample.
    chemical_adaptation: [u16; CHEMICAL_RECEPTOR_COUNT],
}

/// One agent's controller or brain output for this tick.
struct Thought {
    outputs: [i16; OUTPUTS],
    /// Whether a lifetime learning update ran, for the shared counter.
    learned: bool,
}

#[derive(Clone, Copy, Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent simultaneous actions preserve deterministic conflict resolution."
)]
struct Intent {
    id: AgentId,
    dx: i8,
    dy: i8,
    /// V2 locomotor output in [-256, 256], derived from controller outputs.
    thrust: i16,
    /// V2 yaw output in [-256, 256], derived from controller outputs.
    turn: i16,
    /// V2 jaw motor activation.
    jaw: i16,
    eat: bool,
    eat_at: GridPosition,
    reproduce: bool,
    harvest_material: bool,
    place_block: bool,
    remove_block: bool,
    signal: Option<BlankSignal>,
    /// V2 physical outputs, each a bounded motor/gland amplitude rather than a message token.
    acoustic: u16,
    chemical: u16,
    visual: u16,
}

const BLANK_SIGNAL_COUNT: usize = SIGNAL_CHANNELS;
const _: () = assert!(BLANK_SIGNAL_COUNT == BlankSignal::COUNT as usize);
const SIGNAL_RANGE: u16 = 2;
const SIGNAL_COST: u16 = 1;
const MUTATION_RATE_MIN_PPM: u16 = 500;
const MUTATION_RATE_MAX_PPM: u16 = 50_000;

/// Fixed-topology, quantized inherited controller parameters.
///
/// Quantization makes controller evaluation and mutation bit-for-bit reproducible across runs.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Genome {
    weights: [i16; GENOME_WEIGHTS],
    biases: [i16; HIDDEN + OUTPUTS],
    mutation_rate_ppm: u16,
}

impl Genome {
    /// Returns the inherited per-weight mutation probability in parts per million.
    #[must_use]
    pub const fn mutation_rate_ppm(&self) -> u16 {
        self.mutation_rate_ppm
    }
}

/// Deterministic developmental stage; only V2 agents progress through these stages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifeStage {
    /// Dependent, immobile offspring receiving its parent's invested matter.
    Embryo,
    /// Growing animal with reduced motor authority.
    Juvenile,
    /// Fully expressed animal permitted to reproduce asexually.
    Adult,
}

/// Compact inherited developmental program for the V2 body and predictive brain.
///
/// This deliberately contains no learned activity, traces, memories, or direct controller
/// weights. The bounded loci express morphology, receptors, neural connection templates,
/// metabolic costs, and plasticity gain.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DevelopmentalGenome {
    loci: [u8; 16],
    mutation_rate_ppm: u16,
}

impl DevelopmentalGenome {
    /// Derives the program a seeded V2 world gives its founder with this ordinal.
    #[must_use]
    pub fn founder(seed: u64, ordinal: u64) -> Self {
        let loci = core::array::from_fn(|index| {
            // `% 65` keeps every locus in u8 range, so the fallback is unreachable.
            u8::try_from(derive(seed, 0, ordinal, 0x4445_5600 + index as u64) % 65)
                .unwrap_or_default()
        });
        Self {
            loci,
            mutation_rate_ppm: 12_000,
        }
    }

    /// Returns the brain parameters this program develops with every experiment switch at its
    /// default.
    #[must_use]
    pub fn default_brain_config(&self) -> BrainConfig {
        brain_config(
            self,
            BrainAblations::default(),
            CulturalTransmissionControls::default(),
        )
    }

    /// Returns the bounded inherited mutation probability in parts per million.
    #[must_use]
    pub const fn mutation_rate_ppm(&self) -> u16 {
        self.mutation_rate_ppm
    }

    fn locus(&self, index: usize) -> u16 {
        u16::from(self.loci[index])
    }
}

/// Immutable developmental inspection values for a living V2 agent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DevelopmentSnapshot {
    /// Current deterministic developmental stage.
    pub stage: LifeStage,
    /// Nutrition-dependent body size in fixed, bounded units.
    pub growth: u16,
    /// Stable cluster fingerprint derived only from inherited loci.
    pub lineage_cluster: u64,
}

const MAX_RESOURCE: u16 = 12;
const ENERGY_AT_BIRTH: u16 = 30;
const MAX_ENERGY: u16 = 80;
const METABOLIC_COST: u16 = 1;
const MAX_AGE: u64 = 2_400;
const BIRTH_THRESHOLD: u16 = 62;
const BIRTH_COST: u16 = 30;
const MAX_CARRIED_MATERIAL: u8 = 4;
const BLOCK_DURABILITY: u16 = 80;
const V2_MAX_STORE: u16 = 120;
const V2_TISSUE_FULL: u16 = 100;
const V2_PLANT_CAPACITY: u16 = 160;
// Toxic patches remain an unlabelled, delayed hazard.  A normal mixed diet must be
// detoxifiable, however: a six-unit bite on one of five patches previously added six
// toxin units against one unit of clearance, making the founder swallowing reflex
// physiologically impossible to sustain.
const V2_TOXIN_DOSE_PER_MOUTHFUL: u16 = 2;
const V2_TOXIN_INJURY_THRESHOLD: u16 = 20;
const V2_INFECTION_INJURY_THRESHOLD: u16 = 18;
const V2_MOUTHFUL: u16 = 8;
const V2_ECOLOGY_TURNOVER: u16 = 6;
const V2_OFFSPRING_INVESTMENT: u16 = 60;

/// Authoritative deterministic simulation state.
#[derive(Debug)]
pub struct World {
    seed: WorldSeed,
    tick: Tick,
    settings: WorldSettings,
    model_version: ModelVersion,
    slots: Vec<AgentSlot>,
    free_slots: Vec<u32>,
    positions: Vec<GridPosition>,
    physics_positions: Vec<Position>,
    velocities: Vec<Velocity>,
    orientations: Vec<Angle>,
    mouth_open: Vec<Fixed>,
    ages: Vec<u64>,
    energies: Vec<u16>,
    genomes: Vec<Genome>,
    recurrent_states: Vec<[i16; HIDDEN]>,
    // Deliberately separate from Genome; explicitly reset at every birth.
    plastic_biases: Vec<[i16; HIDDEN + OUTPUTS]>,
    parents: Vec<Option<AgentId>>,
    lineage_depths: Vec<u32>,
    resources: Vec<u16>,
    materials: Vec<u16>,
    inventories: Vec<u8>,
    blocks: Vec<u16>,
    spatial_index: SpatialIndex,
    signals: Vec<Option<BlankSignal>>,
    previous_signals: Vec<Option<BlankSignal>>,
    signal_intervention: SignalIntervention,
    transmission_controls: CulturalTransmissionControls,
    brain_ablations: BrainAblations,
    births: u64,
    deaths: u64,
    signals_emitted: u64,
    signal_energy_spent: u64,
    signal_observations: u64,
    symbol_emissions: [u64; BLANK_SIGNAL_COUNT],
    symbol_observations: [u64; BLANK_SIGNAL_COUNT],
    genetic_births: u64,
    learning_updates: u64,
    fats: Vec<u16>,
    guts: Vec<u16>,
    hydrations: Vec<u16>,
    fatigues: Vec<u16>,
    reproductive_reserves: Vec<u16>,
    toxins: Vec<u16>,
    secondary_toxins: Vec<u16>,
    infections: Vec<u16>,
    tissues: Vec<[u16; 3]>,
    plants: Vec<u16>,
    nutrients: Vec<u16>,
    moisture: Vec<u16>,
    temperature: Vec<i16>,
    detritus: Vec<u16>,
    water_contamination: Vec<u16>,
    initial_v2_matter: u64,
    chemical_adaptation: Vec<[u16; CHEMICAL_RECEPTOR_COUNT]>,
    acoustic_emissions: Vec<u16>,
    chemical_emissions: Vec<u16>,
    visual_displays: Vec<u16>,
    last_raw_senses: Vec<RawSensoryFrame>,
    predictive_brains: Vec<PredictiveBrain>,
    developmental_genomes: Vec<DevelopmentalGenome>,
    life_stages: Vec<LifeStage>,
    growth: Vec<u16>,
}

impl World {
    /// Creates the standard seeded world.
    pub fn new(seed: WorldSeed) -> Self {
        // `WorldSettings::default()` is valid by construction.
        match Self::with_settings(seed, WorldSettings::default()) {
            Ok(world) => world,
            Err(error) => unreachable!("default world settings are valid: {error}"),
        }
    }

    /// Creates the V2 fixed-point body simulation with default geometry.
    #[must_use]
    pub fn new_v2(seed: WorldSeed) -> Self {
        match Self::with_settings_v2(seed, WorldSettings::default()) {
            Ok(world) => world,
            Err(error) => unreachable!("default world settings are valid: {error}"),
        }
    }

    const fn with_settings_unchecked(seed: WorldSeed, settings: WorldSettings) -> Self {
        Self {
            seed,
            tick: Tick::new(0),
            settings,
            model_version: ModelVersion::V1Grid,
            slots: Vec::new(),
            free_slots: Vec::new(),
            positions: Vec::new(),
            physics_positions: Vec::new(),
            velocities: Vec::new(),
            orientations: Vec::new(),
            mouth_open: Vec::new(),
            ages: Vec::new(),
            energies: Vec::new(),
            genomes: Vec::new(),
            recurrent_states: Vec::new(),
            plastic_biases: Vec::new(),
            parents: Vec::new(),
            lineage_depths: Vec::new(),
            resources: Vec::new(),
            materials: Vec::new(),
            inventories: Vec::new(),
            blocks: Vec::new(),
            spatial_index: SpatialIndex::new(),
            signals: Vec::new(),
            previous_signals: Vec::new(),
            signal_intervention: SignalIntervention::Baseline,
            transmission_controls: CulturalTransmissionControls::all_enabled(),
            brain_ablations: BrainAblations {
                prediction_enabled: true,
                episodic_memory_enabled: true,
                action_contingency_enabled: true,
                plasticity_enabled: true,
            },
            births: 0,
            deaths: 0,
            signals_emitted: 0,
            signal_energy_spent: 0,
            signal_observations: 0,
            symbol_emissions: [0; BLANK_SIGNAL_COUNT],
            symbol_observations: [0; BLANK_SIGNAL_COUNT],
            genetic_births: 0,
            learning_updates: 0,
            fats: Vec::new(),
            guts: Vec::new(),
            hydrations: Vec::new(),
            fatigues: Vec::new(),
            reproductive_reserves: Vec::new(),
            toxins: Vec::new(),
            secondary_toxins: Vec::new(),
            infections: Vec::new(),
            tissues: Vec::new(),
            plants: Vec::new(),
            nutrients: Vec::new(),
            moisture: Vec::new(),
            temperature: Vec::new(),
            detritus: Vec::new(),
            water_contamination: Vec::new(),
            initial_v2_matter: 0,
            chemical_adaptation: Vec::new(),
            acoustic_emissions: Vec::new(),
            chemical_emissions: Vec::new(),
            visual_displays: Vec::new(),
            last_raw_senses: Vec::new(),
            predictive_brains: Vec::new(),
            developmental_genomes: Vec::new(),
            life_stages: Vec::new(),
            growth: Vec::new(),
        }
    }

    /// Creates a seeded world using explicit settings and deterministically populates it.
    ///
    /// # Errors
    ///
    /// Returns configuration or capacity errors when the requested world cannot be represented.
    ///
    /// # Panics
    ///
    /// Panics only if a platform cannot represent this world's bounded cell count as `u64`.
    pub fn with_settings(seed: WorldSeed, settings: WorldSettings) -> Result<Self, WorldError> {
        let mut world = Self::with_settings_unchecked(seed, settings);
        world.resources = (0..world.cell_count())
            .map(|cell| {
                let cell = u64::try_from(cell).expect("world cell count fits u64");
                u16::try_from(derive(seed.get(), 0, cell, 0x46_4f_4f_44) % 8)
                    .expect("resource derivation is below eight")
            })
            .collect();
        world.materials = (0..world.cell_count())
            .map(|cell| {
                let cell = u64::try_from(cell).expect("world cell count fits u64");
                u16::try_from(derive(seed.get(), 0, cell, 0x4D_41_54_45) % 4)
                    .expect("material derivation is below four")
            })
            .collect();
        world.blocks = vec![0; world.cell_count()];
        for ordinal in 0..settings.initial_agents {
            let position = world.seeded_position(ordinal);
            let _ = world.spawn_seeded_agent(position, ordinal)?;
        }
        world.rebuild_spatial_index();
        Ok(world)
    }

    /// Creates an opt-in V2 continuous-physics world.
    ///
    /// V1 creation remains separate so existing golden replay and persisted V1 checkpoints
    /// continue to select their original model explicitly.
    ///
    /// # Errors
    ///
    /// Returns the same configuration errors as [`World::with_settings`].
    ///
    /// # Panics
    ///
    /// Panics only if this platform cannot represent the bounded seeded ecology quantities.
    pub fn with_settings_v2(seed: WorldSeed, settings: WorldSettings) -> Result<Self, WorldError> {
        let mut world = Self::with_settings(seed, settings)?;
        world.model_version = ModelVersion::V2Physics;
        world.plants = (0..world.cell_count())
            .map(|cell| {
                u16::try_from(120 + derive(seed.get(), 0, cell as u64, 0x504C_414E) % 41)
                    .expect("plant seed fits")
            })
            .collect();
        world.nutrients = vec![40; world.cell_count()];
        world.moisture = (0..world.cell_count())
            .map(|cell| {
                u16::try_from(40 + derive(seed.get(), 0, cell as u64, 0x4D4F_4953) % 40)
                    .expect("moisture seed fits")
            })
            .collect();
        world.temperature = vec![160; world.cell_count()];
        world.detritus = vec![0; world.cell_count()];
        world.water_contamination = (0..world.cell_count())
            .map(|cell| {
                u16::try_from(derive(seed.get(), 0, cell as u64, 0x5741_5445) % 24)
                    .expect("contamination seed fits")
            })
            .collect();
        world.fats = vec![30; world.slots.len()];
        world.guts = vec![0; world.slots.len()];
        world.hydrations = vec![90; world.slots.len()];
        world.fatigues = vec![0; world.slots.len()];
        world.reproductive_reserves = vec![0; world.slots.len()];
        world.toxins = vec![0; world.slots.len()];
        world.secondary_toxins = vec![0; world.slots.len()];
        world.tissues = vec![[V2_TISSUE_FULL; 3]; world.slots.len()];
        world.chemical_adaptation = vec![[0; CHEMICAL_RECEPTOR_COUNT]; world.slots.len()];
        world.last_raw_senses = vec![RawSensoryFrame::EMPTY; world.slots.len()];
        world.initial_v2_matter = world.v2_matter_total();
        for index in 0..world.positions.len() {
            world.physics_positions[index] = Position {
                x: Fixed::from_units(i32::from(world.positions[index].x())),
                y: Fixed::from_units(i32::from(world.positions[index].y())),
            };
        }
        Ok(world)
    }

    /// Returns the world seed.
    #[must_use]
    pub const fn seed(&self) -> WorldSeed {
        self.seed
    }

    /// Returns the latest completed authoritative tick.
    #[must_use]
    pub const fn tick(&self) -> Tick {
        self.tick
    }

    /// Returns immutable world geometry and initial population settings.
    #[must_use]
    pub const fn settings(&self) -> WorldSettings {
        self.settings
    }

    /// Returns the movement model selected when this world was created.
    #[must_use]
    pub const fn model_version(&self) -> ModelVersion {
        self.model_version
    }

    /// Returns the number of living agents.
    #[must_use]
    pub fn agent_count(&self) -> usize {
        self.slots.iter().filter(|slot| slot.alive).count()
    }

    /// Selects a deterministic causal intervention for future signal observations.
    pub fn set_signal_intervention(&mut self, intervention: SignalIntervention) {
        self.signal_intervention = intervention;
    }

    /// Returns the causal intervention currently applied to signal delivery.
    #[must_use]
    pub const fn signal_intervention(&self) -> SignalIntervention {
        self.signal_intervention
    }

    /// Replaces the controls for subsequent deterministic ticks.
    pub fn set_transmission_controls(&mut self, controls: CulturalTransmissionControls) {
        self.transmission_controls = controls;
    }

    /// Returns the controls currently applied to candidate transmission channels.
    #[must_use]
    pub const fn transmission_controls(&self) -> CulturalTransmissionControls {
        self.transmission_controls
    }

    /// Replaces targeted V2 causal-learning ablations for future ticks.
    pub fn set_brain_ablations(&mut self, ablations: BrainAblations) {
        self.brain_ablations = ablations;
    }

    /// Returns the targeted V2 causal-learning ablations currently applied.
    #[must_use]
    pub const fn brain_ablations(&self) -> BrainAblations {
        self.brain_ablations
    }

    /// Returns one agent's V2 lifetime brain state for deterministic experiments and inspection.
    #[must_use]
    pub fn brain_of(&self, id: AgentId) -> Option<BrainSnapshot> {
        let index = usize::try_from(id.slot).ok()?;
        let slot = self.slots.get(index)?;
        (self.model_version == ModelVersion::V2Physics
            && slot.alive
            && slot.generation == id.generation)
            .then(|| {
                let brain = &self.predictive_brains[index];
                BrainSnapshot {
                    neurons: V2_BRAIN_NEURONS,
                    activity: brain.activity,
                    prediction: brain.prediction,
                    episodes: brain.episodes,
                    contingencies: brain.contingencies,
                }
            })
    }

    /// Returns the inherited V2 developmental program for a living animal.
    #[must_use]
    pub fn developmental_genome_of(&self, id: AgentId) -> Option<&DevelopmentalGenome> {
        let index = usize::try_from(id.slot).ok()?;
        let slot = self.slots.get(index)?;
        (self.model_version == ModelVersion::V2Physics
            && slot.alive
            && slot.generation == id.generation)
            .then(|| &self.developmental_genomes[index])
    }

    /// Returns deterministic life-stage and lineage-cluster data without exposing it to agents.
    #[must_use]
    pub fn development_of(&self, id: AgentId) -> Option<DevelopmentSnapshot> {
        let index = usize::try_from(id.slot).ok()?;
        let slot = self.slots.get(index)?;
        (self.model_version == ModelVersion::V2Physics
            && slot.alive
            && slot.generation == id.generation)
            .then(|| DevelopmentSnapshot {
                stage: self.life_stages[index],
                growth: self.growth[index],
                lineage_cluster: developmental_signature(&self.developmental_genomes[index]) >> 48,
            })
    }

    /// Returns a stable fingerprint of one agent's mutable, non-inherited learning state.
    #[must_use]
    pub fn learning_state_signature_of(&self, id: AgentId) -> Option<u64> {
        let index = usize::try_from(id.slot).ok()?;
        let slot = self.slots.get(index)?;
        (slot.alive && slot.generation == id.generation)
            .then(|| plastic_signature(&self.plastic_biases[index]))
    }

    /// Returns the most recently sampled V2 receptor frame for debug visualization only.
    ///
    /// V1 has no raw-receptor path. The frame is never a source of semantic world metadata.
    #[must_use]
    pub fn raw_senses_of(&self, id: AgentId) -> Option<RawSensoryFrame> {
        let index = usize::try_from(id.slot).ok()?;
        let slot = self.slots.get(index)?;
        (self.model_version == ModelVersion::V2Physics
            && slot.alive
            && slot.generation == id.generation)
            .then(|| self.last_raw_senses[index])
    }

    /// Returns the food resource available at a valid grid cell.
    #[must_use]
    pub fn resource_at(&self, position: GridPosition) -> Option<u16> {
        self.contains(position)
            .then(|| self.resources[self.cell_index(position)])
    }

    /// Returns loose construction material at a valid grid cell.
    #[must_use]
    pub fn material_at(&self, position: GridPosition) -> Option<u16> {
        self.contains(position)
            .then(|| self.materials[self.cell_index(position)])
    }

    /// Returns block durability at a valid grid cell, or zero when it is empty.
    #[must_use]
    pub fn block_at(&self, position: GridPosition) -> Option<u16> {
        self.contains(position)
            .then(|| self.blocks[self.cell_index(position)])
    }

    /// Returns material held by a living agent.
    #[must_use]
    pub fn carried_material_of(&self, id: AgentId) -> Option<u8> {
        let index = usize::try_from(id.slot).ok()?;
        let slot = self.slots.get(index)?;
        (slot.alive && slot.generation == id.generation).then(|| self.inventories[index])
    }

    /// Returns the conserved quantity of loose, carried, and constructed material.
    ///
    /// # Panics
    ///
    /// Panics only if the platform cannot represent the world cell count as `u64`.
    #[must_use]
    pub fn total_material(&self) -> u64 {
        self.materials
            .iter()
            .map(|value| u64::from(*value))
            .sum::<u64>()
            + self
                .inventories
                .iter()
                .map(|value| u64::from(*value))
                .sum::<u64>()
            + u64::try_from(
                self.blocks
                    .iter()
                    .filter(|durability| **durability > 0)
                    .count(),
            )
            .expect("world cell count fits u64")
    }

    /// Inserts an agent at a grid position.
    ///
    /// This narrow setup API is intentionally separate from tick processing; future birth
    /// resolution will use the same internal slot allocator during its apply phase.
    ///
    /// # Errors
    ///
    /// Returns an error if `position` is out of bounds or no stable slot remains.
    pub fn spawn_agent(&mut self, position: GridPosition) -> Result<AgentId, WorldError> {
        let ordinal =
            u64::try_from(self.slots.len()).map_err(|_| WorldError::AgentCapacityExhausted)?;
        self.spawn_with_genome(position, self.seeded_genome(ordinal), None, 0)
    }

    fn spawn_seeded_agent(
        &mut self,
        position: GridPosition,
        ordinal: u32,
    ) -> Result<AgentId, WorldError> {
        self.spawn_with_genome(position, self.seeded_genome(u64::from(ordinal)), None, 0)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "Initializes every per-agent deterministic V2 state channel together."
    )]
    fn spawn_with_genome(
        &mut self,
        position: GridPosition,
        genome: Genome,
        parent: Option<AgentId>,
        lineage_depth: u32,
    ) -> Result<AgentId, WorldError> {
        if !self.contains(position) {
            return Err(WorldError::PositionOutOfBounds);
        }

        let slot = if let Some(slot) = self.free_slots.pop() {
            let index = usize::try_from(slot).map_err(|_| WorldError::AgentCapacityExhausted)?;
            let entry = &mut self.slots[index];
            entry.alive = true;
            slot
        } else {
            let slot =
                u32::try_from(self.slots.len()).map_err(|_| WorldError::AgentCapacityExhausted)?;
            self.slots.push(AgentSlot {
                generation: 0,
                alive: true,
            });
            self.positions.push(position);
            self.physics_positions.push(Position {
                x: Fixed::from_units(i32::from(position.x())),
                y: Fixed::from_units(i32::from(position.y())),
            });
            self.velocities.push(Velocity::default());
            self.orientations.push(Angle::default());
            self.mouth_open.push(Fixed::default());
            self.ages.push(0);
            self.energies.push(ENERGY_AT_BIRTH);
            self.genomes.push(genome.clone());
            self.recurrent_states.push([0; HIDDEN]);
            self.plastic_biases.push([0; HIDDEN + OUTPUTS]);
            self.parents.push(parent);
            self.lineage_depths.push(lineage_depth);
            self.inventories.push(0);
            self.signals.push(None);
            self.previous_signals.push(None);
            self.fats.push(30);
            self.guts.push(0);
            self.hydrations.push(90);
            self.fatigues.push(0);
            self.reproductive_reserves.push(0);
            self.toxins.push(0);
            self.secondary_toxins.push(0);
            self.infections.push(0);
            self.tissues.push([V2_TISSUE_FULL; 3]);
            self.chemical_adaptation.push([0; CHEMICAL_RECEPTOR_COUNT]);
            self.acoustic_emissions.push(0);
            self.chemical_emissions.push(0);
            self.visual_displays.push(0);
            self.last_raw_senses.push(RawSensoryFrame::EMPTY);
            self.predictive_brains.push(PredictiveBrain::default());
            self.developmental_genomes
                .push(self.seeded_developmental_genome(u64::from(slot)));
            self.life_stages.push(LifeStage::Adult);
            self.growth.push(100);
            slot
        };
        let index = usize::try_from(slot).map_err(|_| WorldError::AgentCapacityExhausted)?;
        self.positions[index] = position;
        self.physics_positions[index] = Position {
            x: Fixed::from_units(i32::from(position.x())),
            y: Fixed::from_units(i32::from(position.y())),
        };
        self.velocities[index] = Velocity::default();
        self.orientations[index] = Angle::default();
        self.mouth_open[index] = Fixed::default();
        self.ages[index] = 0;
        self.energies[index] = ENERGY_AT_BIRTH;
        self.genomes[index] = genome;
        self.recurrent_states[index] = [0; HIDDEN];
        // Learned state is never inherited, including when mutation is disabled.
        self.plastic_biases[index] = [0; HIDDEN + OUTPUTS];
        self.parents[index] = parent;
        self.lineage_depths[index] = lineage_depth;
        self.inventories[index] = 0;
        self.signals[index] = None;
        self.previous_signals[index] = None;
        self.fats[index] = 30;
        self.guts[index] = 0;
        self.hydrations[index] = 90;
        self.fatigues[index] = 0;
        self.reproductive_reserves[index] = 0;
        self.toxins[index] = 0;
        self.secondary_toxins[index] = 0;
        self.infections[index] = 0;
        self.tissues[index] = [V2_TISSUE_FULL; 3];
        self.chemical_adaptation[index] = [0; CHEMICAL_RECEPTOR_COUNT];
        self.acoustic_emissions[index] = 0;
        self.chemical_emissions[index] = 0;
        self.visual_displays[index] = 0;
        self.last_raw_senses[index] = RawSensoryFrame::EMPTY;
        self.predictive_brains[index] = PredictiveBrain::default();
        if self.developmental_genomes.len() > index {
            self.developmental_genomes[index] = self.seeded_developmental_genome(u64::from(slot));
            self.life_stages[index] = LifeStage::Adult;
            self.growth[index] = 100;
        }
        let id = AgentId {
            slot,
            generation: self.slots[index].generation,
        };
        if self.model_version == ModelVersion::V2Physics {
            self.initial_v2_matter = self.initial_v2_matter.saturating_add(
                u64::from(self.energies[index])
                    + u64::from(self.fats[index])
                    + u64::from(self.hydrations[index])
                    + self.tissues[index]
                        .iter()
                        .map(|part| u64::from(*part))
                        .sum::<u64>(),
            );
        }
        self.rebuild_spatial_index();
        Ok(id)
    }

    /// Removes one agent and invalidates its ID before making its slot reusable.
    ///
    /// # Errors
    ///
    /// Returns [`WorldError::UnknownAgent`] for a stale or unknown handle, or
    /// [`WorldError::GenerationExhausted`] if its slot cannot be safely reused.
    ///
    /// # Panics
    ///
    /// Panics only if bounded V2 body matter cannot fit its internal cell store.
    pub fn despawn_agent(&mut self, id: AgentId) -> Result<(), WorldError> {
        let index = usize::try_from(id.slot).map_err(|_| WorldError::UnknownAgent)?;
        let Some(slot) = self.slots.get(index) else {
            return Err(WorldError::UnknownAgent);
        };
        if !slot.alive || slot.generation != id.generation {
            return Err(WorldError::UnknownAgent);
        }
        let cell = self.cell_index(self.positions[index]);
        let Some(slot) = self.slots.get_mut(index) else {
            return Err(WorldError::UnknownAgent);
        };
        self.materials[cell] =
            self.materials[cell].saturating_add(u16::from(self.inventories[index]));
        if self.model_version == ModelVersion::V2Physics {
            let body_matter = u32::from(self.energies[index])
                + u32::from(self.fats[index])
                + u32::from(self.guts[index])
                + u32::from(self.hydrations[index])
                + u32::from(self.reproductive_reserves[index])
                + self.tissues[index]
                    .iter()
                    .map(|part| u32::from(*part))
                    .sum::<u32>();
            self.detritus[cell] = self.detritus[cell]
                .saturating_add(u16::try_from(body_matter).expect("bounded body matter fits"));
        }
        self.inventories[index] = 0;
        self.signals[index] = None;
        self.previous_signals[index] = None;
        let next_generation = slot
            .generation
            .checked_add(1)
            .ok_or(WorldError::GenerationExhausted)?;
        slot.alive = false;
        slot.generation = next_generation;
        self.free_slots.push(id.slot);
        self.deaths = self.deaths.saturating_add(1);
        self.rebuild_spatial_index();
        Ok(())
    }

    /// Returns the fixed phase order enforced by [`World::step`].
    #[must_use]
    pub const fn tick_pipeline() -> &'static [TickPhase] {
        &TICK_PIPELINE
    }

    /// Advances one authoritative fixed tick and publishes an owned immutable snapshot.
    #[must_use]
    pub fn step(&mut self) -> WorldSnapshot {
        match self.advance(None) {
            Ok(snapshot) => snapshot,
            Err(error) => unreachable!("the built-in CPU brain path cannot fail: {error}"),
        }
    }

    /// Advances one tick like [`World::step`], but runs V2 predictive brains on `brains`.
    ///
    /// A conforming backend is bit-identical to the built-in CPU kernels, so histories match
    /// [`World::step`] exactly. V1 worlds have no predictive brains and ignore the backend.
    ///
    /// # Errors
    ///
    /// Returns the backend's error, in which case no simulation state has changed.
    pub fn step_with_brains(
        &mut self,
        brains: &mut dyn BrainBackend,
    ) -> Result<WorldSnapshot, BrainBackendError> {
        self.advance(Some(brains))
    }

    fn advance(
        &mut self,
        brains: Option<&mut dyn BrainBackend>,
    ) -> Result<WorldSnapshot, BrainBackendError> {
        // Sense: immutable spatial index was built by the preceding tick.
        self.rebuild_spatial_index();
        // Evaluate and emit intents from only immutable local observations.
        let intents = self.evaluate_intents(brains)?;
        // Resolve: all moves/actions see the same pre-tick world.
        if self.model_version == ModelVersion::V2Physics {
            self.resolve_physics(&intents);
        } else {
            self.resolve_intents(&intents);
        }
        self.apply_phase(&intents);
        self.tick = Tick::new(self.tick.get().saturating_add(1));
        // Publish: index and an owned snapshot reflect the completed tick.
        self.rebuild_spatial_index();
        Ok(self.snapshot())
    }

    /// Returns an immutable snapshot without advancing time.
    ///
    /// # Panics
    ///
    /// Panics only if the internally bounded agent-slot count cannot fit `AgentId`.
    #[must_use]
    pub fn snapshot(&self) -> WorldSnapshot {
        let mut agents = Vec::with_capacity(self.agent_count());
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.alive {
                let slot_id = u32::try_from(index).expect("slot count is constrained by AgentId");
                agents.push(AgentSnapshot {
                    id: AgentId {
                        slot: slot_id,
                        generation: slot.generation,
                    },
                    position: self.positions[index],
                    age: self.ages[index],
                    energy: self.energies[index],
                    lineage_depth: self.lineage_depths[index],
                    materials: self.inventories[index],
                    body: (self.model_version == ModelVersion::V2Physics).then(|| BodySnapshot {
                        position: self.physics_positions[index],
                        velocity: self.velocities[index],
                        orientation: self.orientations[index],
                        torso_radius: Fixed::from_raw(
                            FIXED_SCALE * i32::from(48 + self.growth[index] / 2) / 192,
                        ),
                        head_radius: Fixed::from_raw(
                            FIXED_SCALE
                                * i32::from(32 + self.developmental_genomes[index].locus(1) / 2)
                                / 192,
                        ),
                        mouth_open: self.mouth_open[index],
                    }),
                    physiology: (self.model_version == ModelVersion::V2Physics).then(|| {
                        PhysiologySnapshot {
                            energy: self.energies[index],
                            fat: self.fats[index],
                            gut: self.guts[index],
                            hydration: self.hydrations[index],
                            fatigue: self.fatigues[index],
                            reproductive_reserve: self.reproductive_reserves[index],
                            toxin: self.toxins[index],
                            infection: self.infections[index],
                            tissue: self.tissues[index],
                            pain: self.v2_pain(index),
                        }
                    }),
                });
            }
        }
        let state_hash = self.hash_state(&agents);
        WorldSnapshot {
            tick: self.tick,
            agents,
            resources: self.resources.clone(),
            materials: self.materials.clone(),
            blocks: self.blocks.clone(),
            state_hash,
            metrics: self.metrics(),
            ecology: (self.model_version == ModelVersion::V2Physics).then(|| EcologySnapshot {
                plants: self.plants.clone(),
                nutrients: self.nutrients.clone(),
                moisture: self.moisture.clone(),
                temperature: self.temperature.clone(),
                detritus: self.detritus.clone(),
                water_contamination: self.water_contamination.clone(),
                conserved_matter: self.v2_matter_total(),
            }),
        }
    }

    /// Returns passive adaptation metrics from the current authoritative population.
    ///
    /// # Panics
    ///
    /// Panics only if internally bounded population counters cannot fit their public metric types.
    #[must_use]
    pub fn metrics(&self) -> AdaptationMetrics {
        let population = self.agent_count();
        if population == 0 {
            return AdaptationMetrics {
                births: self.births,
                deaths: self.deaths,
                signals_emitted: self.signals_emitted,
                signal_energy_spent: self.signal_energy_spent,
                signal_observations: self.signal_observations,
                symbol_emissions: self.symbol_emissions,
                symbol_observations: self.symbol_observations,
                transmission: self.transmission_metrics(),
                ..AdaptationMetrics::default()
            };
        }
        let mut signatures = Vec::with_capacity(population);
        let mut depth_sum = 0_u64;
        let mut mutation_sum = 0_u64;
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.alive {
                signatures.push(if self.model_version == ModelVersion::V2Physics {
                    developmental_signature(&self.developmental_genomes[index])
                } else {
                    genome_signature(&self.genomes[index])
                });
                depth_sum = depth_sum.saturating_add(u64::from(self.lineage_depths[index]));
                mutation_sum = mutation_sum.saturating_add(u64::from(
                    if self.model_version == ModelVersion::V2Physics {
                        self.developmental_genomes[index].mutation_rate_ppm
                    } else {
                        self.genomes[index].mutation_rate_ppm
                    },
                ));
            }
        }
        signatures.sort_unstable();
        signatures.dedup();
        let count = u64::try_from(population).expect("population fits u64");
        let distinct = u32::try_from(signatures.len()).expect("population fits u32");
        AdaptationMetrics {
            population: u32::try_from(population).expect("population fits u32"),
            births: self.births,
            deaths: self.deaths,
            mean_generation_depth: u32::try_from(depth_sum / count)
                .expect("depth average fits u32"),
            distinct_genomes: distinct,
            diversity_ppm: u32::try_from(u64::from(distinct) * 1_000_000 / count)
                .expect("diversity is bounded by one million"),
            mean_mutation_rate_ppm: u32::try_from(mutation_sum / count)
                .expect("mutation rate average fits u32"),
            signals_emitted: self.signals_emitted,
            signal_energy_spent: self.signal_energy_spent,
            signal_observations: self.signal_observations,
            symbol_emissions: self.symbol_emissions,
            symbol_observations: self.symbol_observations,
            transmission: self.transmission_metrics(),
        }
    }

    /// Returns channel-separated measurements without asserting cultural transmission.
    ///
    /// # Panics
    ///
    /// Panics only if the living population cannot fit in the public `u32` metric.
    #[must_use]
    pub fn transmission_metrics(&self) -> TransmissionMetrics {
        let experienced_agents = self
            .slots
            .iter()
            .enumerate()
            .filter(|(index, slot)| {
                slot.alive && self.plastic_biases[*index].iter().any(|value| *value != 0)
            })
            .count();
        TransmissionMetrics {
            genetic_births: self.genetic_births,
            learning_updates: self.learning_updates,
            experienced_agents: u32::try_from(experienced_agents).expect("population fits u32"),
            persistent_artifacts: u32::try_from(
                self.blocks.iter().filter(|block| **block > 0).count(),
            )
            .expect("world cell count fits u32"),
            signal_observations: self.signal_observations,
        }
    }

    /// Returns agents in one deterministic row-major spatial cell.
    #[must_use]
    pub fn agents_in_cell(&self, position: GridPosition) -> &[AgentId] {
        if !self.contains(position) {
            return &[];
        }
        self.spatial_index.cell(self.cell_index(position))
    }

    /// Returns living V2 agents whose torso centres are within `radius` of `centre`.
    ///
    /// Results are stable `AgentId` order, including when agents share a spatial-hash cell.
    ///
    /// # Panics
    ///
    /// Panics only if the internally bounded agent-slot count cannot fit `AgentId`.
    #[must_use]
    pub fn agents_in_radius(&self, centre: Position, radius: Fixed) -> Vec<AgentId> {
        let radius_squared = i64::from(radius.raw()).pow(2);
        let mut matches = Vec::new();
        for (index, slot) in self.slots.iter().enumerate() {
            if !slot.alive {
                continue;
            }
            let delta_x = Self::wrapped_delta(
                centre.x.raw(),
                self.physics_positions[index].x.raw(),
                self.settings.width,
            );
            let delta_y = Self::wrapped_delta(
                centre.y.raw(),
                self.physics_positions[index].y.raw(),
                self.settings.height,
            );
            if i64::from(delta_x).pow(2) + i64::from(delta_y).pow(2) <= radius_squared {
                matches.push(AgentId {
                    slot: u32::try_from(index).expect("slot count is constrained by AgentId"),
                    generation: slot.generation,
                });
            }
        }
        matches
    }

    /// Returns an agent's direct parent, if it was born during this world history.
    #[must_use]
    pub fn parent_of(&self, id: AgentId) -> Option<AgentId> {
        let index = usize::try_from(id.slot).ok()?;
        let slot = self.slots.get(index)?;
        (slot.alive && slot.generation == id.generation)
            .then(|| self.parents[index])
            .flatten()
    }

    /// Returns an immutable inherited genome for a living agent.
    #[must_use]
    pub fn genome_of(&self, id: AgentId) -> Option<&Genome> {
        let index = usize::try_from(id.slot).ok()?;
        let slot = self.slots.get(index)?;
        (slot.alive && slot.generation == id.generation).then(|| &self.genomes[index])
    }

    fn seeded_position(&self, ordinal: u32) -> GridPosition {
        let value = derive(self.seed.get(), 0, u64::from(ordinal), 0x50_4f_53);
        GridPosition::new(
            u16::try_from(value % u64::from(self.settings.width))
                .expect("modulo result is bounded by the u16 world width"),
            u16::try_from((value >> 32) % u64::from(self.settings.height))
                .expect("modulo result is bounded by the u16 world height"),
        )
    }

    #[allow(
        clippy::too_many_lines,
        reason = "One ordered controller-to-intent pass preserves deterministic snapshots."
    )]
    /// Senses, thinks, and emits one intent per living agent.
    ///
    /// Every agent senses the same immutable pre-tick world and then updates only its own
    /// lifetime state, so both phases run in parallel. Shared counters and intents are then
    /// assembled in stable slot order, which keeps the result independent of thread count.
    fn evaluate_intents(
        &mut self,
        brains: Option<&mut dyn BrainBackend>,
    ) -> Result<Vec<Intent>, BrainBackendError> {
        let perceptions: Vec<Option<Perception>> = (0..self.slots.len())
            .into_par_iter()
            .map(|index| self.slots[index].alive.then(|| self.perceive(index)))
            .collect();
        let thoughts = match brains {
            Some(backend) if self.model_version == ModelVersion::V2Physics => {
                self.think_with_backend(&perceptions, backend)?
            }
            _ => self.think(&perceptions),
        };
        let mut intents = Vec::with_capacity(self.agent_count());
        for (index, (perception, thought)) in perceptions.iter().zip(&thoughts).enumerate() {
            let (Some(perception), Some(thought)) = (perception, thought) else {
                continue;
            };
            if thought.learned {
                self.learning_updates = self.learning_updates.saturating_add(1);
            }
            if let Perception::Grid(GridPerception {
                observed_signal: Some(signal),
                ..
            }) = perception
            {
                self.signal_observations = self.signal_observations.saturating_add(1);
                let symbol = usize::from(signal.get());
                self.symbol_observations[symbol] =
                    self.symbol_observations[symbol].saturating_add(1);
            }
            intents.push(self.intent_for(index, perception, &thought.outputs));
        }
        Ok(intents)
    }

    /// Samples one living agent's receptors without mutating the world.
    fn perceive(&self, index: usize) -> Perception {
        if self.model_version == ModelVersion::V2Physics {
            return Perception::Body(self.sample_raw_senses(index));
        }
        let position = self.positions[index];
        let cell = self.cell_index(position);
        let id = AgentId {
            slot: u32::try_from(index).expect("slot count is constrained by AgentId"),
            generation: self.slots[index].generation,
        };
        let material_here = self.materials[cell];
        let block_here = self.blocks[cell];
        let nearby_agents = self.local_agent_count(position);
        let observed_signal = self.received_signal(id, position);
        Perception::Grid(GridPerception {
            inputs: self.local_sensing(
                position,
                self.energies[index],
                nearby_agents,
                self.inventories[index],
                material_here,
                block_here,
                observed_signal,
            ),
            food_here: self.resources[cell],
            material_here,
            block_here,
            nearby_agents,
            observed_signal,
        })
    }

    /// Runs every V2 brain through an external backend in one batch.
    ///
    /// Brain state is copied out, stepped, and written back only after the backend succeeds,
    /// so a failure leaves the world untouched.
    fn think_with_backend(
        &mut self,
        perceptions: &[Option<Perception>],
        backend: &mut dyn BrainBackend,
    ) -> Result<Vec<Option<Thought>>, BrainBackendError> {
        let bodies: Vec<(usize, &BodyPerception)> = perceptions
            .iter()
            .enumerate()
            .filter_map(|(index, perception)| match perception {
                Some(Perception::Body(body)) => Some((index, body)),
                _ => None,
            })
            .collect();
        let mut brains: Vec<PredictiveBrain> = bodies
            .iter()
            .map(|(index, _)| self.predictive_brains[*index].clone())
            .collect();
        let frames: Vec<RawSensoryFrame> = bodies.iter().map(|(_, body)| body.frame).collect();
        let configs: Vec<BrainConfig> = bodies
            .iter()
            .map(|(index, _)| {
                brain_config(
                    &self.developmental_genomes[*index],
                    self.brain_ablations,
                    self.transmission_controls,
                )
            })
            .collect();
        let steps = backend.step_brains(&mut brains, &frames, &configs)?;
        if steps.len() != bodies.len() {
            return Err(BrainBackendError::new(format!(
                "brain backend returned {} steps for {} agents",
                steps.len(),
                bodies.len()
            )));
        }
        let mut thoughts: Vec<Option<Thought>> = (0..perceptions.len()).map(|_| None).collect();
        for ((index, body), (brain, step)) in bodies.into_iter().zip(brains.into_iter().zip(steps))
        {
            self.predictive_brains[index] = brain;
            self.last_raw_senses[index] = body.frame;
            self.chemical_adaptation[index] = body.chemical_adaptation;
            thoughts[index] = Some(Thought {
                outputs: step.outputs,
                learned: step.learned,
            });
        }
        Ok(thoughts)
    }

    /// Runs each agent's controller or brain on its own perception, in parallel. Only that
    /// agent's lifetime state is written.
    fn think(&mut self, perceptions: &[Option<Perception>]) -> Vec<Option<Thought>> {
        let ablations = self.brain_ablations;
        let controls = self.transmission_controls;
        debug_assert!(
            [
                self.genomes.len(),
                self.developmental_genomes.len(),
                self.recurrent_states.len(),
                self.plastic_biases.len(),
                self.predictive_brains.len(),
                self.last_raw_senses.len(),
                self.chemical_adaptation.len(),
            ]
            .iter()
            .all(|length| *length == perceptions.len()),
            "per-agent buffers must cover every slot"
        );
        (
            perceptions.par_iter(),
            self.genomes.par_iter(),
            self.developmental_genomes.par_iter(),
            self.recurrent_states.par_iter_mut(),
            self.plastic_biases.par_iter_mut(),
            self.predictive_brains.par_iter_mut(),
            self.last_raw_senses.par_iter_mut(),
            self.chemical_adaptation.par_iter_mut(),
        )
            .into_par_iter()
            .map(
                |(
                    perception,
                    genome,
                    development,
                    recurrent,
                    plastic,
                    brain,
                    last_frame,
                    adaptation,
                )| match perception.as_ref()? {
                    Perception::Grid(grid) => {
                        let outputs = sim_kernels::controller::evaluate_controller(
                            &genome.weights,
                            &genome.biases,
                            plastic,
                            recurrent,
                            &grid.inputs,
                        );
                        if controls.plasticity_enabled {
                            sim_kernels::controller::apply_lifetime_plasticity(plastic, &outputs);
                        }
                        Some(Thought {
                            outputs,
                            learned: controls.plasticity_enabled,
                        })
                    }
                    Perception::Body(body) => {
                        *last_frame = body.frame;
                        *adaptation = body.chemical_adaptation;
                        let step = brain
                            .step(&brain_config(development, ablations, controls), &body.frame);
                        Some(Thought {
                            outputs: step.outputs,
                            learned: step.learned,
                        })
                    }
                },
            )
            .collect()
    }

    /// Translates controller outputs into one agent's simultaneous intent.
    fn intent_for(
        &self,
        index: usize,
        perception: &Perception,
        outputs: &[i16; OUTPUTS],
    ) -> Intent {
        let id = AgentId {
            slot: u32::try_from(index).expect("slot count is constrained by AgentId"),
            generation: self.slots[index].generation,
        };
        let position = self.positions[index];
        let energy = self.energies[index];
        let (food_here, material_here, block_here, nearby_agents) = match perception {
            Perception::Grid(grid) => (
                grid.food_here,
                grid.material_here,
                grid.block_here,
                grid.nearby_agents,
            ),
            Perception::Body(_) => (0, 0, 0, 0),
        };
        let movement = outputs[..4]
            .iter()
            .enumerate()
            .max_by_key(|(output, value)| (**value, std::cmp::Reverse(*output)))
            .map_or(0, |(output, _)| output);
        let (dx, dy) = match movement {
            1 => (1, 0),
            2 => (-1, 0),
            3 => (0, 1),
            _ => (0, -1),
        };
        let motors = decode_motor_outputs(outputs);
        let legacy_actions = self.model_version == ModelVersion::V1Grid;
        Intent {
            id,
            dx,
            dy,
            thrust: motors.thrust,
            turn: motors.turn,
            jaw: i16::try_from(motors.jaw).expect("bounded jaw fits i16"),
            eat: legacy_actions && food_here > 0 && energy < MAX_ENERGY,
            eat_at: position,
            // The ecological gates prevent a global fitness rule; the inherited controller
            // decides only whether an eligible agent attempts reproduction.
            reproduce: legacy_actions
                && energy >= BIRTH_THRESHOLD
                && nearby_agents >= 2
                && outputs[7] > 0,
            harvest_material: legacy_actions
                && material_here > 0
                && self.inventories[index] < MAX_CARRIED_MATERIAL
                && outputs[4] > 0,
            place_block: legacy_actions
                && self.inventories[index] > 0
                && block_here == 0
                && outputs[5] > 0,
            remove_block: legacy_actions && block_here > 0 && outputs[6] > 0,
            signal: legacy_actions
                .then(|| {
                    outputs[8..]
                        .iter()
                        .enumerate()
                        .max_by_key(|(symbol, value)| (**value, std::cmp::Reverse(*symbol)))
                        .and_then(|(symbol, value)| {
                            (*value > 0).then(|| {
                                BlankSignal::new(
                                    u8::try_from(symbol).expect("symbol index fits u8"),
                                )
                                .expect("symbol index is bounded by the blank alphabet")
                            })
                        })
                })
                .flatten(),
            // V2 motor neurons drive ordinary sound-producing motion, gland output, and
            // body display intensity. No ordinal, receiver, sender ID, or symbol is used.
            acoustic: if legacy_actions { 0 } else { motors.acoustic },
            chemical: if legacy_actions { 0 } else { motors.chemical },
            visual: if legacy_actions { 0 } else { motors.visual },
        }
    }

    fn resolve_intents(&mut self, intents: &[Intent]) {
        for intent in intents {
            let index = usize::try_from(intent.id.slot).expect("AgentId slot fits usize");
            let slot = self.slots[index];
            if !slot.alive || slot.generation != intent.id.generation {
                continue;
            }
            self.positions[index] = self.offset(self.positions[index], intent.dx, intent.dy);
        }
    }

    /// Integrates V2 motors, then resolves torso-circle contacts in canonical ID order.
    fn resolve_physics(&mut self, intents: &[Intent]) {
        for intent in intents {
            let index = usize::try_from(intent.id.slot).expect("AgentId slot fits usize");
            if !self.slots[index].alive || self.slots[index].generation != intent.id.generation {
                continue;
            }
            self.orientations[index].0 = self.orientations[index]
                .0
                .wrapping_add_signed(scale_v2_orientation_turn(intent.turn));
            let terrain_traction = if self.blocks[self.cell_index(self.positions[index])] > 0 {
                3
            } else {
                4
            };
            self.velocities[index] = scale_v2_velocity(
                self.velocities[index],
                self.orientations[index],
                intent.thrust,
                terrain_traction,
            );
            self.mouth_open[index] = scale_v2_jaw(intent.jaw);
            self.physics_positions[index].x = Fixed::from_raw(Self::wrap_fixed(
                self.physics_positions[index].x.raw() + self.velocities[index].x.raw(),
                self.settings.width,
            ));
            self.physics_positions[index].y = Fixed::from_raw(Self::wrap_fixed(
                self.physics_positions[index].y.raw() + self.velocities[index].y.raw(),
                self.settings.height,
            ));
        }

        let radius = Fixed::from_raw(FIXED_SCALE / 3);
        let diameter = radius.raw() * 2;
        let ids: Vec<_> = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.alive)
            .map(|(index, slot)| AgentId {
                slot: u32::try_from(index).expect("slot count is constrained by AgentId"),
                generation: slot.generation,
            })
            .collect();
        for first in &ids {
            let first = *first;
            let first_index = usize::try_from(first.slot).expect("slot fits usize");
            for second in self.agents_in_radius(
                self.physics_positions[first_index],
                Fixed::from_raw(diameter),
            ) {
                if second <= first {
                    continue;
                }
                self.resolve_circle_contact(first, second, diameter);
            }
        }
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.alive {
                self.positions[index] = GridPosition::new(
                    u16::try_from(
                        self.physics_positions[index]
                            .x
                            .raw()
                            .div_euclid(FIXED_SCALE),
                    )
                    .expect("wrapped V2 x lies in grid"),
                    u16::try_from(
                        self.physics_positions[index]
                            .y
                            .raw()
                            .div_euclid(FIXED_SCALE),
                    )
                    .expect("wrapped V2 y lies in grid"),
                );
            }
        }
    }

    fn resolve_circle_contact(&mut self, first: AgentId, second: AgentId, diameter: i32) {
        let first_index = usize::try_from(first.slot).expect("slot fits usize");
        let second_index = usize::try_from(second.slot).expect("slot fits usize");
        let delta_x = Self::wrapped_delta(
            self.physics_positions[first_index].x.raw(),
            self.physics_positions[second_index].x.raw(),
            self.settings.width,
        );
        let delta_y = Self::wrapped_delta(
            self.physics_positions[first_index].y.raw(),
            self.physics_positions[second_index].y.raw(),
            self.settings.height,
        );
        let distance_squared = i64::from(delta_x).pow(2) + i64::from(delta_y).pow(2);
        let distance = integer_sqrt(
            u64::try_from(distance_squared).expect("squared distance is non-negative"),
        );
        if distance >= u64::try_from(diameter).expect("positive diameter") {
            return;
        }
        let (normal_x, normal_y, distance) = if distance == 0 {
            (FIXED_SCALE, 0, 1)
        } else {
            (
                delta_x,
                delta_y,
                i32::try_from(distance).expect("distance fits i32"),
            )
        };
        let overlap = diameter - distance;
        let correction_x = normal_x * overlap / distance / 2;
        let correction_y = normal_y * overlap / distance / 2;
        self.physics_positions[first_index].x = Fixed::from_raw(Self::wrap_fixed(
            self.physics_positions[first_index].x.raw() - correction_x,
            self.settings.width,
        ));
        self.physics_positions[first_index].y = Fixed::from_raw(Self::wrap_fixed(
            self.physics_positions[first_index].y.raw() - correction_y,
            self.settings.height,
        ));
        self.physics_positions[second_index].x = Fixed::from_raw(Self::wrap_fixed(
            self.physics_positions[second_index].x.raw() + correction_x,
            self.settings.width,
        ));
        self.physics_positions[second_index].y = Fixed::from_raw(Self::wrap_fixed(
            self.physics_positions[second_index].y.raw() + correction_y,
            self.settings.height,
        ));
    }

    fn apply_phase(&mut self, intents: &[Intent]) {
        if self.model_version == ModelVersion::V2Physics {
            self.apply_v2_physiology_and_ecology(intents);
            return;
        }
        // Regeneration is capped, spatially heterogeneous, and independent of iteration order.
        for (cell, resource) in self.resources.iter_mut().enumerate() {
            if *resource < MAX_RESOURCE
                && derive(
                    self.seed.get(),
                    self.tick.get(),
                    u64::try_from(cell).expect("world cell count fits u64"),
                    0x52_45_47_45_4e,
                )
                .is_multiple_of(7)
            {
                *resource += 1;
            }
        }

        // One unit per eater is allocated in stable-ID order from the pre-action target cell.
        for intent in intents {
            if !intent.eat {
                continue;
            }
            let index = usize::try_from(intent.id.slot).expect("AgentId slot fits usize");
            if self.slots[index].alive {
                let cell = self.cell_index(intent.eat_at);
                if self.resources[cell] > 0 {
                    self.resources[cell] -= 1;
                    self.energies[index] = self.energies[index].saturating_add(6).min(MAX_ENERGY);
                }
            }
        }

        self.resolve_material_intents(intents);
        self.decay_blocks();
        self.apply_signals(intents);

        let mut deaths = Vec::new();
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.alive {
                self.ages[index] = self.ages[index].saturating_add(1);
                self.energies[index] = self.energies[index].saturating_sub(METABOLIC_COST);
                if self.energies[index] == 0 || self.ages[index] >= MAX_AGE {
                    deaths.push(AgentId {
                        slot: u32::try_from(index).expect("slot count is constrained by AgentId"),
                        generation: slot.generation,
                    });
                }
            }
        }
        for id in deaths {
            // A known live slot cannot fail here; retaining the explicit branch avoids panicking
            // should the lifecycle implementation change.
            let _ = self.despawn_agent(id);
        }

        // This is a safety ceiling, not selection: the available regenerated food still
        // determines survival. It prevents a transient food cache from creating an unbounded
        // birth spike before metabolism can provide negative feedback.
        let configured_cap = usize::try_from(self.settings.initial_agents())
            .expect("u32 initial population fits usize")
            .saturating_mul(2)
            .max(4);
        let population_cap = configured_cap.min((self.cell_count() / 2).max(1));
        for intent in intents {
            if !intent.reproduce || self.agent_count() >= population_cap {
                continue;
            }
            let index = usize::try_from(intent.id.slot).expect("AgentId slot fits usize");
            if !self.slots[index].alive || self.energies[index] < BIRTH_THRESHOLD {
                continue;
            }
            self.energies[index] -= BIRTH_COST;
            let parent = intent.id;
            let child_position = self.offset(self.positions[index], 1, 0);
            let child_genome = if self.transmission_controls.genetic_mutation_enabled {
                self.mutate_genome(&self.genomes[index], parent)
            } else {
                self.genomes[index].clone()
            };
            let depth = self.lineage_depths[index].saturating_add(1);
            if self
                .spawn_with_genome(child_position, child_genome, Some(parent), depth)
                .is_ok()
            {
                self.births = self.births.saturating_add(1);
                self.genetic_births = self.genetic_births.saturating_add(1);
            }
        }
    }

    fn rebuild_spatial_index(&mut self) {
        let mut index = core::mem::take(&mut self.spatial_index);
        index.rebuild(
            self.cell_count(),
            self.slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.alive)
                .map(|(index, slot)| {
                    debug_assert!(
                        self.model_version != ModelVersion::V2Physics
                            || self.positions[index]
                                == self.grid_cell_of(self.physics_positions[index]),
                        "indexed V2 neighbour queries require grid cells to match torso centres"
                    );
                    let id = AgentId {
                        slot: u32::try_from(index).expect("slot count is constrained by AgentId"),
                        generation: slot.generation,
                    };
                    (self.cell_index(self.positions[index]), id)
                }),
        );
        self.spatial_index = index;
    }

    /// Returns the grid cell containing a wrapped fixed-point position.
    fn grid_cell_of(&self, position: Position) -> GridPosition {
        let column = Self::wrap_fixed(position.x.raw(), self.settings.width) / FIXED_SCALE;
        let row = Self::wrap_fixed(position.y.raw(), self.settings.height) / FIXED_SCALE;
        GridPosition::new(
            u16::try_from(column).expect("wrapped x fits grid"),
            u16::try_from(row).expect("wrapped y fits grid"),
        )
    }

    /// Visits each distinct cell within torus Chebyshev distance `radius` of `centre` once.
    ///
    /// When the neighbourhood spans a whole axis, every row or column is visited exactly once
    /// rather than wrapping onto itself.
    fn for_each_cell_near(&self, centre: GridPosition, radius: u16, mut visit: impl FnMut(usize)) {
        let axis = |centre: u16, dimension: u16| {
            let full = 2 * u32::from(radius) + 1 >= u32::from(dimension);
            let (first, count) = if full {
                (0, dimension)
            } else {
                ((centre + dimension - radius) % dimension, 2 * radius + 1)
            };
            (0..count).map(move |step| (first + step) % dimension)
        };
        for y in axis(centre.y, self.settings.height) {
            for x in axis(centre.x, self.settings.width) {
                visit(self.cell_index(GridPosition::new(x, y)));
            }
        }
    }

    /// Returns whether any living agent other than `except` has its torso centre within
    /// `radius` (less than one cell) of `centre`. Requires a current spatial index.
    fn any_other_agent_within(&self, centre: Position, radius: Fixed, except: usize) -> bool {
        debug_assert!(
            radius.raw() < FIXED_SCALE,
            "neighbour search covers one cell"
        );
        let radius_squared = i64::from(radius.raw()).pow(2);
        let mut found = false;
        self.for_each_cell_near(self.grid_cell_of(centre), 1, |cell| {
            found = found
                || self.spatial_index.cell(cell).iter().any(|id| {
                    let other = id.slot as usize;
                    other != except && {
                        let delta_x = Self::wrapped_delta(
                            centre.x.raw(),
                            self.physics_positions[other].x.raw(),
                            self.settings.width,
                        );
                        let delta_y = Self::wrapped_delta(
                            centre.y.raw(),
                            self.physics_positions[other].y.raw(),
                            self.settings.height,
                        );
                        i64::from(delta_x).pow(2) + i64::from(delta_y).pow(2) <= radius_squared
                    }
                });
        });
        found
    }

    /// V2-only explicit material flow. It intentionally runs after physics and does not alter
    /// the frozen V1 action, resource, or replay path.
    #[allow(
        clippy::too_many_lines,
        reason = "The explicit ordered physiology flow is kept together for auditable conservation."
    )]
    fn apply_v2_physiology_and_ecology(&mut self, intents: &[Intent]) {
        let matter_before_ecology = self.v2_matter_total();
        self.apply_physical_social_outputs(intents);
        // Climate changes slowly (every 16 ticks); plant growth transfers nutrients, never
        // creates matter. Moisture/temperature are deterministic spatial layers.
        if self.tick.get().is_multiple_of(16) {
            let season = ((self.tick.get() / 1_024) % 4) as i16;
            let day = ((self.tick.get() / 128) % 2) as i16;
            for cell in 0..self.cell_count() {
                self.temperature[cell] = 120
                    + season * 18
                    + day * 12
                    + i16::try_from(
                        derive(
                            self.seed.get(),
                            self.tick.get() / 16,
                            cell as u64,
                            0x5445_4D50,
                        ) % 9,
                    )
                    .expect("temperature variation fits");
                if self.nutrients[cell] > 0
                    && self.moisture[cell] > 15
                    && self.plants[cell] < V2_PLANT_CAPACITY
                {
                    let regrowth = self.nutrients[cell]
                        .min(V2_ECOLOGY_TURNOVER)
                        .min(V2_PLANT_CAPACITY - self.plants[cell]);
                    self.nutrients[cell] -= regrowth;
                    self.plants[cell] += regrowth;
                }
                let decay = self.detritus[cell].min(V2_ECOLOGY_TURNOVER);
                self.detritus[cell] -= decay;
                self.nutrients[cell] = self.nutrients[cell].saturating_add(decay);
            }
        }
        // Biomass and detritus exchange on the physiology timescale, while the
        // temperature layer above remains deliberately slow.  This prevents animals
        // sharing a physical patch from exhausting its standing biomass between two
        // otherwise viable metabolic ticks; nutrient availability still makes the
        // exchange finite and accumulated detritus is the only replenishment source.
        for cell in 0..self.cell_count() {
            if self.nutrients[cell] > 0
                && self.moisture[cell] > 15
                && self.plants[cell] < V2_PLANT_CAPACITY
            {
                let regrowth = self.nutrients[cell]
                    .min(V2_ECOLOGY_TURNOVER)
                    .min(V2_PLANT_CAPACITY - self.plants[cell]);
                self.nutrients[cell] -= regrowth;
                self.plants[cell] += regrowth;
            }
            let decay = self.detritus[cell].min(V2_ECOLOGY_TURNOVER);
            self.detritus[cell] -= decay;
            self.nutrients[cell] = self.nutrients[cell].saturating_add(decay);
        }
        debug_assert_eq!(
            self.v2_matter_total(),
            matter_before_ecology,
            "V2 ecology imbalance at tick {}",
            self.tick.get()
        );

        // An open mouth consumes a bounded mouthful of local plant biomass. Seasonal chemistry
        // counterbalances two invisible substances across seeds/cells: a nutritious dose may
        // carry either delayed burden, and their combination is worse than either alone.
        // Receptors receive only broad colour and concentration amplitudes, never these keys.
        for intent in intents {
            let index = intent.id.slot as usize;
            if !self.slots[index].alive || self.mouth_open[index].raw() < FIXED_SCALE / 2 {
                continue;
            }
            let cell = self.cell_index(self.positions[index]);
            let swallowed = self.plants[cell]
                .min(V2_MOUTHFUL)
                .min(V2_MAX_STORE.saturating_sub(self.guts[index]));
            if swallowed > 0 {
                self.plants[cell] -= swallowed;
                self.guts[index] += swallowed;
                let season = self.tick.get() / 1_024;
                if derive(self.seed.get(), season, cell as u64, 0x464F_4F44).is_multiple_of(5) {
                    self.toxins[index] = (self.toxins[index]
                        + swallowed.min(V2_TOXIN_DOSE_PER_MOUTHFUL))
                    .min(V2_MAX_STORE);
                }
                if derive(
                    self.seed.get() ^ 0x9E37_79B9,
                    season,
                    cell as u64,
                    0x5355_4253,
                )
                .is_multiple_of(7)
                {
                    self.secondary_toxins[index] = (self.secondary_toxins[index]
                        + swallowed.min(V2_TOXIN_DOSE_PER_MOUTHFUL))
                    .min(V2_MAX_STORE);
                }
            }
        }
        debug_assert_eq!(
            self.v2_matter_total(),
            matter_before_ecology,
            "V2 feeding imbalance at tick {}",
            self.tick.get()
        );

        let mut deaths = Vec::new();
        for index in 0..self.slots.len() {
            if !self.slots[index].alive {
                continue;
            }
            let matter_before_agent = self.v2_matter_total();
            let cell = self.cell_index(self.positions[index]);
            self.ages[index] = self.ages[index].saturating_add(1);
            // Digestion is delayed by the tick boundary but can process one bounded mouthful.
            // This prevents founders from starving before raw-sense-guided locomotion can matter.
            let digested = self.guts[index]
                .min(V2_MOUTHFUL)
                .min(V2_MAX_STORE.saturating_sub(self.energies[index]));
            if digested > 0 {
                self.guts[index] -= digested;
                self.energies[index] += digested;
            }
            debug_assert_eq!(
                self.v2_matter_total(),
                matter_before_agent,
                "digestion {index}"
            );
            let exertion = (u16::try_from(self.velocities[index].x.raw().unsigned_abs())
                .expect("velocity is bounded")
                + u16::try_from(self.velocities[index].y.raw().unsigned_abs())
                    .expect("velocity is bounded"))
                / 64;
            let neural_activity = if self.model_version == ModelVersion::V2Physics {
                self.predictive_brains[index]
                    .activity
                    .iter()
                    .map(|value| u32::from(value.unsigned_abs()))
                    .sum::<u32>()
            } else {
                self.recurrent_states[index]
                    .iter()
                    .map(|value| u32::from(value.unsigned_abs()))
                    .sum()
            };
            let memory_maintenance = if self.model_version == ModelVersion::V2Physics {
                u16::try_from(
                    self.predictive_brains[index]
                        .episodes
                        .iter()
                        .filter(|episode| episode.retention > 0)
                        .count()
                        / 8,
                )
                .expect("episode capacity is bounded")
            } else {
                0
            };
            let neural_cost =
                u16::try_from(neural_activity / 8_192).expect("neural cost is bounded");
            let developmental_cost = self.developmental_genomes[index].locus(12) / 128;
            let cost =
                1 + exertion.min(2) + neural_cost.min(1) + memory_maintenance + developmental_cost;
            self.fatigues[index] = (self.fatigues[index] + exertion).min(V2_MAX_STORE);
            self.fatigues[index] = self.fatigues[index].saturating_sub(1);
            // Fixed receptor resolution/range consumes energy each tick. Bounds are constants
            // until the later developmental-genetics milestone makes them inherited traits.
            let spent = self.energies[index].min(cost.saturating_add(V2_SENSOR_COST));
            self.energies[index] -= spent;
            self.detritus[cell] = self.detritus[cell].saturating_add(spent);
            debug_assert_eq!(
                self.v2_matter_total(),
                matter_before_agent,
                "metabolism {index}"
            );
            if self.energies[index] == 0 && self.fats[index] > 0 {
                self.fats[index] -= 1;
                self.energies[index] = 1;
            }
            if self.moisture[cell] > 0 && self.hydrations[index] < V2_MAX_STORE {
                self.moisture[cell] -= 1;
                self.hydrations[index] += 1;
            }
            self.hydrations[index] = self.hydrations[index].saturating_sub(1);
            self.moisture[cell] = self.moisture[cell].saturating_add(1);
            debug_assert_eq!(self.v2_matter_total(), matter_before_agent, "water {index}");
            // Toxin is intentionally cumulative and clears slowly; it damages torso after a delay.
            if self.toxins[index] > 0 {
                self.toxins[index] -= 1;
                if self.toxins[index] >= V2_TOXIN_INJURY_THRESHOLD
                    && self.tick.get().is_multiple_of(8)
                {
                    let injury = self.tissues[index][1].min(2);
                    self.tissues[index][1] -= injury;
                    self.detritus[cell] = self.detritus[cell].saturating_add(injury);
                }
            }
            if self.secondary_toxins[index] > 0 {
                self.secondary_toxins[index] -= 1;
            }
            // The two burdens interact physically: neither label is sensed, but concurrent
            // exposure adds tissue damage after the delayed accumulation interval.
            if self.toxins[index] >= 8
                && self.secondary_toxins[index] >= 6
                && self.tick.get().is_multiple_of(6)
            {
                let injury = self.tissues[index][1].min(2);
                self.tissues[index][1] -= injury;
                self.detritus[cell] = self.detritus[cell].saturating_add(injury);
            }
            // Contact/exertion injury is localized to limbs; repair slows with age/fatigue.
            if exertion > 3 {
                let injury = self.tissues[index][2].min(1);
                self.tissues[index][2] -= injury;
                self.detritus[cell] = self.detritus[cell].saturating_add(injury);
            }
            // Open wounds provide a delayed entry route. Contaminated water is a physical
            // concentration layer; it contributes only when tissue is damaged.
            let wound = V2_TISSUE_FULL.saturating_sub(self.tissues[index][2]);
            if wound > 0 && self.water_contamination[cell] > 12 {
                self.infections[index] = self.infections[index]
                    .saturating_add(1 + wound / 32)
                    .min(V2_MAX_STORE);
            } else {
                self.infections[index] = self.infections[index].saturating_sub(1);
            }
            if self.infections[index] >= V2_INFECTION_INJURY_THRESHOLD
                && self.tick.get().is_multiple_of(8)
            {
                let injury = self.tissues[index][1].min(1);
                self.tissues[index][1] -= injury;
                self.detritus[cell] = self.detritus[cell].saturating_add(injury);
            }
            debug_assert_eq!(
                self.v2_matter_total(),
                matter_before_agent,
                "injury {index}"
            );
            // Critical torso failure is terminal; repair cannot revive an already failed body.
            if self.tissues[index][1] == 0 {
                deaths.push(AgentId {
                    slot: u32::try_from(index).expect("slot index fits AgentId"),
                    generation: self.slots[index].generation,
                });
                continue;
            }
            if self.energies[index] > 4
                && self.fatigues[index] < 30
                && self.ages[index] < 8_000
                && self.tissues[index]
                    .iter()
                    .any(|part| *part < V2_TISSUE_FULL)
                && let Some(part) = self.tissues[index]
                    .iter_mut()
                    .find(|part| **part < V2_TISSUE_FULL)
            {
                *part += 1;
                self.energies[index] -= 1;
            }
            debug_assert_eq!(
                self.v2_matter_total(),
                matter_before_agent,
                "repair {index}"
            );
            if self.energies[index] > 80 && self.fats[index] < 60 {
                self.energies[index] -= 1;
                self.fats[index] += 1;
            }
            debug_assert_eq!(
                self.v2_matter_total(),
                matter_before_agent,
                "repair/fat {index}"
            );
            if self.fats[index] > 45
                && self.energies[index] > 45
                && self.reproductive_reserves[index] < V2_OFFSPRING_INVESTMENT
            {
                self.reproductive_reserves[index] =
                    (self.reproductive_reserves[index] + 1).min(V2_OFFSPRING_INVESTMENT);
                self.energies[index] -= 1;
            }
            // Growth is seeded but proceeds only when nutrition is available. The stage is
            // expressed from age and growth, never from inherited learned state.
            let target_growth = 64 + self.developmental_genomes[index].locus(0);
            if digested > 0 && self.energies[index] > 12 && self.growth[index] < target_growth {
                self.growth[index] += 1;
                self.energies[index] -= 1;
                self.detritus[cell] = self.detritus[cell].saturating_add(1);
            }
            debug_assert_eq!(
                self.v2_matter_total(),
                matter_before_agent,
                "growth {index}"
            );
            self.life_stages[index] = if self.ages[index] < 24 {
                LifeStage::Embryo
            } else if self.growth[index] < target_growth || self.ages[index] < 160 {
                LifeStage::Juvenile
            } else {
                LifeStage::Adult
            };
            if self.energies[index] == 0
                || self.hydrations[index] == 0
                || self.tissues[index][1] == 0
            {
                deaths.push(AgentId {
                    slot: u32::try_from(index).expect("slot index fits AgentId"),
                    generation: self.slots[index].generation,
                });
            }
            debug_assert_eq!(
                self.v2_matter_total(),
                matter_before_agent,
                "V2 agent {} imbalance at tick {}",
                index,
                self.tick.get()
            );
        }
        for id in deaths {
            let _ = self.despawn_agent(id);
        }
        debug_assert_eq!(
            self.v2_matter_total(),
            matter_before_ecology,
            "V2 physiology imbalance at tick {}",
            self.tick.get()
        );
        self.resolve_v2_asexual_reproduction();
        debug_assert_eq!(
            self.v2_matter_total(),
            self.initial_v2_matter,
            "V2 material imbalance at tick {}, births {}, deaths {}",
            self.tick.get(),
            self.births,
            self.deaths
        );
    }

    fn resolve_v2_asexual_reproduction(&mut self) {
        let cap = usize::try_from(self.settings.initial_agents())
            .expect("population fits")
            .saturating_mul(2)
            .max(4);
        for index in 0..self.slots.len() {
            if self.agent_count() >= cap
                || !self.slots[index].alive
                || self.life_stages[index] != LifeStage::Adult
                || self.reproductive_reserves[index] < V2_OFFSPRING_INVESTMENT
                || self.energies[index] < 30
            {
                continue;
            }
            let parent = AgentId {
                slot: u32::try_from(index).expect("slot index fits AgentId"),
                generation: self.slots[index].generation,
            };
            let child_genome = if self.transmission_controls.genetic_mutation_enabled {
                self.mutate_developmental_genome(&self.developmental_genomes[index], parent)
            } else {
                self.developmental_genomes[index].clone()
            };
            // The entire newborn mass is paid from explicit reproductive investment.
            self.reproductive_reserves[index] -= V2_OFFSPRING_INVESTMENT;
            // A newborn is placed a short deterministic distance from its parent rather
            // than into the same collision cluster. This is physical dispersal, not a
            // fitness choice: the inherited controller must still feed and survive there.
            let (dx, dy) = match index % 4 {
                0 => (2, 0),
                1 => (-2, 0),
                2 => (0, 2),
                _ => (0, -2),
            };
            let position = self.offset(self.positions[index], dx, dy);
            if let Ok(child) = self.spawn_with_genome(
                position,
                self.genomes[index].clone(),
                Some(parent),
                self.lineage_depths[index].saturating_add(1),
            ) {
                let child_index = child.slot as usize;
                let default_mass = u64::from(self.energies[child_index])
                    + u64::from(self.fats[child_index])
                    + u64::from(self.hydrations[child_index])
                    + self.tissues[child_index]
                        .iter()
                        .map(|value| u64::from(*value))
                        .sum::<u64>();
                // The same 60-unit maternal reserve is apportioned across immediate
                // metabolic, hydration, tissue, and short-term starvation buffers.
                self.energies[child_index] = 14;
                self.fats[child_index] = 12;
                self.hydrations[child_index] = 14;
                self.tissues[child_index] = [6, 8, 6];
                self.developmental_genomes[child_index] = child_genome;
                self.life_stages[child_index] = LifeStage::Embryo;
                self.growth[child_index] = 8;
                self.initial_v2_matter = self.initial_v2_matter.saturating_sub(default_mass);
                self.births = self.births.saturating_add(1);
                self.genetic_births = self.genetic_births.saturating_add(1);
            }
        }
    }

    fn v2_pain(&self, index: usize) -> u16 {
        let damage: u16 = self.tissues[index]
            .iter()
            .map(|part| V2_TISSUE_FULL - *part)
            .sum();
        let noise = (derive(
            self.seed.get(),
            self.tick.get(),
            self.cell_index(self.positions[index]) as u64,
            0x5041_494E,
        ) % 7) as u16;
        damage.saturating_add(noise).min(V2_MAX_STORE)
    }

    fn v2_matter_total(&self) -> u64 {
        self.plants
            .iter()
            .chain(&self.nutrients)
            .chain(&self.moisture)
            .chain(&self.detritus)
            .map(|value| u64::from(*value))
            .sum::<u64>()
            + self
                .slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.alive)
                .map(|(index, _)| {
                    u64::from(self.energies[index])
                        + u64::from(self.fats[index])
                        + u64::from(self.guts[index])
                        + u64::from(self.hydrations[index])
                        + u64::from(self.reproductive_reserves[index])
                        + self.tissues[index]
                            .iter()
                            .map(|value| u64::from(*value))
                            .sum::<u64>()
                })
                .sum::<u64>()
    }

    fn offset(&self, position: GridPosition, dx: i8, dy: i8) -> GridPosition {
        let x = (i32::from(position.x) + i32::from(dx)).rem_euclid(i32::from(self.settings.width));
        let y = (i32::from(position.y) + i32::from(dy)).rem_euclid(i32::from(self.settings.height));
        GridPosition::new(
            u16::try_from(x).expect("euclidean remainder is non-negative and within width"),
            u16::try_from(y).expect("euclidean remainder is non-negative and within height"),
        )
    }

    fn wrap_fixed(value: i32, dimension: u16) -> i32 {
        value.rem_euclid(i32::from(dimension).saturating_mul(FIXED_SCALE))
    }

    fn wrapped_delta(from: i32, to: i32, dimension: u16) -> i32 {
        let circumference = i32::from(dimension).saturating_mul(FIXED_SCALE);
        (to - from + circumference / 2).rem_euclid(circumference) - circumference / 2
    }

    fn contains(&self, position: GridPosition) -> bool {
        position.x < self.settings.width && position.y < self.settings.height
    }

    fn cell_count(&self) -> usize {
        usize::from(self.settings.width) * usize::from(self.settings.height)
    }

    fn cell_index(&self, position: GridPosition) -> usize {
        usize::from(position.y) * usize::from(self.settings.width) + usize::from(position.x)
    }

    fn local_agent_count(&self, position: GridPosition) -> usize {
        let mut count = 0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                count += self
                    .spatial_index
                    .cell(self.cell_index(self.offset(position, dx, dy)))
                    .len();
            }
        }
        count
    }

    fn hash_state(&self, agents: &[AgentSnapshot]) -> u64 {
        // Tagged V1 canonical hash: every future-relevant authoritative field is mixed under a
        // distinct subsystem tag so checkpoint/resume and golden replay can detect drift.
        let mut state = self.hash_header(agents.len());
        state = self.hash_agents(state, agents);
        state = self.hash_identity_tables(state);
        state = self.hash_grid_fields(state);
        state = self.hash_signal_fields(state);
        let state = self.hash_controls_and_counters(state);
        if self.model_version == ModelVersion::V2Physics {
            self.hash_v2_ecology_and_physiology(state)
        } else {
            state
        }
    }

    fn hash_header(&self, agent_count: usize) -> u64 {
        let mut state = derive(
            self.seed.get(),
            self.tick.get(),
            u64::try_from(agent_count).expect("platform usize count fits in u64"),
            0x48_41_53, // HAS
        );
        state = derive(
            state,
            u64::from(STATE_HASH_VERSION),
            (u64::from(self.settings.width) << 32) | u64::from(self.settings.height),
            0x53_45_54, // SET
        );
        derive(
            state,
            u64::from(self.settings.initial_agents),
            0,
            0x53_45_54, // SET
        )
    }

    fn hash_agents(&self, mut state: u64, agents: &[AgentSnapshot]) -> u64 {
        for agent in agents {
            state = derive(
                state,
                id_key(agent.id),
                (u64::from(agent.position.x) << 32) | u64::from(agent.position.y),
                agent.age
                    ^ u64::from(agent.energy)
                    ^ u64::from(agent.lineage_depth)
                    ^ u64::from(agent.materials),
            );
            let index = usize::try_from(agent.id.slot).expect("AgentId slot fits usize");
            state = derive(
                state,
                genome_signature(&self.genomes[index]),
                recurrent_signature(&self.recurrent_states[index])
                    ^ plastic_signature(&self.plastic_biases[index]),
                0x47_45_4e_45, // GENE
            );
            state = derive(
                state,
                self.parents[index].map_or(u64::MAX, id_key),
                u64::from(self.lineage_depths[index]),
                0x50_41_52, // PAR
            );
            if self.model_version == ModelVersion::V2Physics {
                let body = agent.body.expect("V2 snapshots always include body state");
                state = derive(
                    state,
                    (u64::from(body.position.x.raw().cast_unsigned()) << 32)
                        | u64::from(body.position.y.raw().cast_unsigned()),
                    (u64::from(body.velocity.x.raw().cast_unsigned()) << 32)
                        | u64::from(body.velocity.y.raw().cast_unsigned()),
                    u64::from(body.orientation.turns())
                        ^ u64::from(body.mouth_open.raw().cast_unsigned()),
                );
            }
        }
        state
    }

    fn hash_identity_tables(&self, mut state: u64) -> u64 {
        for (index, slot) in self.slots.iter().enumerate() {
            state = derive(
                state,
                u64::try_from(index).expect("slot index fits u64"),
                (u64::from(slot.generation) << 1) | u64::from(u8::from(slot.alive)),
                0x53_4C_4F_54, // SLOT
            );
        }
        state = derive(
            state,
            u64::try_from(self.free_slots.len()).expect("free-list length fits u64"),
            0,
            0x46_52_45_45, // FREE
        );
        for slot in &self.free_slots {
            state = derive(state, u64::from(*slot), 0, 0x46_52_45_45); // FREE
        }
        state
    }

    fn hash_grid_fields(&self, mut state: u64) -> u64 {
        for resource in &self.resources {
            state = derive(state, u64::from(*resource), 0, 0x52_45_53); // RES
        }
        for material in &self.materials {
            state = derive(state, u64::from(*material), 0, 0x4D_41_54); // MAT
        }
        for block in &self.blocks {
            state = derive(state, u64::from(*block), 0, 0x42_4C_4B); // BLK
        }
        state
    }

    fn hash_signal_fields(&self, mut state: u64) -> u64 {
        for signal in &self.signals {
            state = derive(state, blank_signal_key(*signal), 0, 0x53_49_47); // SIG
        }
        for signal in &self.previous_signals {
            state = derive(state, blank_signal_key(*signal), 0, 0x50_53_49_47); // PSIG
        }
        state
    }

    fn hash_controls_and_counters(&self, mut state: u64) -> u64 {
        state = derive(
            state,
            self.signal_intervention as u64,
            u64::from(u8::from(
                self.transmission_controls.genetic_mutation_enabled,
            )) | (u64::from(u8::from(self.transmission_controls.plasticity_enabled)) << 1)
                | (u64::from(u8::from(self.transmission_controls.signals_enabled)) << 2)
                | (u64::from(u8::from(self.transmission_controls.artifacts_enabled)) << 3),
            0x49_4E_54, // INT
        );
        state = derive(state, self.births, self.deaths, 0x434E_5401); // CNT1
        state = derive(
            state,
            self.signals_emitted,
            self.signal_energy_spent,
            0x434E_5402, // CNT2
        );
        state = derive(
            state,
            self.signal_observations,
            self.genetic_births ^ self.learning_updates.rotate_left(32),
            0x434E_5403, // CNT3
        );
        for (index, count) in self.symbol_emissions.iter().enumerate() {
            state = derive(
                state,
                u64::try_from(index).expect("symbol index fits u64"),
                *count,
                0x53_45_4D, // SEM
            );
        }
        for (index, count) in self.symbol_observations.iter().enumerate() {
            state = derive(
                state,
                u64::try_from(index).expect("symbol index fits u64"),
                *count,
                0x53_4F_42, // SOB
            );
        }
        state
    }

    fn hash_v2_ecology_and_physiology(&self, mut state: u64) -> u64 {
        state = derive(state, self.initial_v2_matter, 0, 0x5632_4D41);
        state = derive(
            state,
            u64::from(u8::from(self.brain_ablations.prediction_enabled))
                | (u64::from(u8::from(self.brain_ablations.episodic_memory_enabled)) << 1)
                | (u64::from(u8::from(self.brain_ablations.action_contingency_enabled)) << 2)
                | (u64::from(u8::from(self.brain_ablations.plasticity_enabled)) << 3),
            0,
            0x5632_4142,
        );
        for index in 0..self.slots.len() {
            state = derive(
                state,
                u64::from(self.fats[index])
                    | (u64::from(self.guts[index]) << 16)
                    | (u64::from(self.hydrations[index]) << 32)
                    | (u64::from(self.fatigues[index]) << 48),
                u64::from(self.reproductive_reserves[index])
                    | (u64::from(self.toxins[index]) << 16)
                    | (u64::from(self.secondary_toxins[index]) << 32)
                    | (u64::from(self.infections[index]) << 48),
                0x5632_5048,
            );
            state = derive(
                state,
                u64::from(self.acoustic_emissions[index])
                    | (u64::from(self.chemical_emissions[index]) << 16)
                    | (u64::from(self.visual_displays[index]) << 32),
                0,
                0x5632_5347,
            );
            state = derive(
                state,
                developmental_signature(&self.developmental_genomes[index]),
                u64::from(self.growth[index]) | ((self.life_stages[index] as u64) << 16),
                0x5632_4445,
            );
            for tissue in self.tissues[index] {
                state = derive(state, u64::from(tissue), 0, 0x5632_5449);
            }
            let brain = &self.predictive_brains[index];
            for (activity, learned_bias) in brain.activity.iter().zip(brain.learned_bias) {
                state = derive(
                    state,
                    i64::from(*activity).cast_unsigned(),
                    i64::from(learned_bias).cast_unsigned(),
                    0x5632_4252,
                );
            }
            for episode in brain.episodes {
                state = derive(
                    state,
                    u64::from(episode.action) | (u64::from(episode.retention) << 8),
                    episode.subsequent_change.iter().fold(0, |value, change| {
                        value ^ i64::from(*change).cast_unsigned()
                    }),
                    0x5632_4D45,
                );
            }
            for estimates in brain.contingencies {
                for estimate in estimates {
                    state = derive(state, i64::from(estimate).cast_unsigned(), 0, 0x5632_4341);
                }
            }
        }
        for cell in 0..self.cell_count() {
            state = derive(
                state,
                u64::from(self.plants[cell]) | (u64::from(self.nutrients[cell]) << 16),
                u64::from(self.moisture[cell])
                    | (i64::from(self.temperature[cell]).cast_unsigned() << 16)
                    | (u64::from(self.detritus[cell]) << 32)
                    | (u64::from(self.water_contamination[cell]) << 48),
                0x5632_4543,
            );
        }
        state
    }

    fn seeded_genome(&self, ordinal: u64) -> Genome {
        let mut weights = [0_i16; GENOME_WEIGHTS];
        let mut biases = [0_i16; HIDDEN + OUTPUTS];
        for (index, weight) in weights.iter_mut().enumerate() {
            *weight = derived_weight(
                self.seed.get(),
                ordinal,
                u64::try_from(index).expect("index fits u64"),
            );
        }
        for (index, bias) in biases.iter_mut().enumerate() {
            *bias = derived_weight(
                self.seed.get(),
                ordinal,
                0xB1A5_u64 + u64::try_from(index).expect("index fits u64"),
            );
        }
        Genome {
            weights,
            biases,
            mutation_rate_ppm: u16::try_from(
                u64::from(MUTATION_RATE_MIN_PPM)
                    + derive(self.seed.get(), 0, ordinal, 0x4D_55_54)
                        % u64::from(MUTATION_RATE_MAX_PPM - MUTATION_RATE_MIN_PPM),
            )
            .expect("configured mutation rate fits u16"),
        }
    }

    fn mutate_genome(&self, parent: &Genome, parent_id: AgentId) -> Genome {
        let mut child = parent.clone();
        let key = id_key(parent_id);
        for (index, weight) in child.weights.iter_mut().enumerate() {
            let random = derive(
                self.seed.get(),
                self.tick.get(),
                key,
                u64::try_from(index).expect("index fits u64"),
            );
            if random % 1_000_000 < u64::from(child.mutation_rate_ppm) {
                *weight = weight.saturating_add(mutation_delta(random));
            }
        }
        for (index, bias) in child.biases.iter_mut().enumerate() {
            let random = derive(
                self.seed.get(),
                self.tick.get(),
                key,
                0xB1A5_u64 + u64::try_from(index).expect("index fits u64"),
            );
            if random % 1_000_000 < u64::from(child.mutation_rate_ppm) {
                *bias = bias.saturating_add(mutation_delta(random));
            }
        }
        let rate_random = derive(self.seed.get(), self.tick.get(), key, 0x52_41_54_45);
        if rate_random % 1_000_000 < u64::from(child.mutation_rate_ppm) {
            child.mutation_rate_ppm = (i32::from(child.mutation_rate_ppm)
                + i32::from(mutation_delta(rate_random)) * 25)
                .clamp(
                    i32::from(MUTATION_RATE_MIN_PPM),
                    i32::from(MUTATION_RATE_MAX_PPM),
                )
                .try_into()
                .expect("clamped mutation rate fits u16");
        }
        child
    }

    fn seeded_developmental_genome(&self, ordinal: u64) -> DevelopmentalGenome {
        DevelopmentalGenome::founder(self.seed.get(), ordinal)
    }

    fn mutate_developmental_genome(
        &self,
        parent: &DevelopmentalGenome,
        parent_id: AgentId,
    ) -> DevelopmentalGenome {
        let mut child = parent.clone();
        for (locus, value) in child.loci.iter_mut().enumerate() {
            let random = derive(
                self.seed.get(),
                self.tick.get(),
                id_key(parent_id),
                0x444D_0000 + locus as u64,
            );
            if random % 1_000_000 < u64::from(child.mutation_rate_ppm) {
                // Developmental constraints bound every trait locus, rather than discarding offspring.
                let delta = i16::try_from(random % 5).expect("small mutation fits") - 2;
                *value = u8::try_from((i16::from(*value) + delta).clamp(0, 64))
                    .expect("constrained developmental locus fits");
            }
        }
        child
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "The fixed controller input vector is assembled from distinct sensed values."
    )]
    fn local_sensing(
        &self,
        position: GridPosition,
        energy: u16,
        nearby_agents: usize,
        inventory: u8,
        material_here: u16,
        block_here: u16,
        received_signal: Option<BlankSignal>,
    ) -> [i16; INPUTS] {
        let scale_food = |offset: (i8, i8)| {
            i16::try_from(
                self.resources[self.cell_index(self.offset(position, offset.0, offset.1))],
            )
            .expect("food fits i16")
                * 32
        };
        let mut sensing = [
            scale_food((0, 0)),
            scale_food((1, 0)),
            scale_food((-1, 0)),
            scale_food((0, 1)),
            i16::try_from(nearby_agents.min(16)).expect("bounded count fits i16") * 24,
            i16::try_from(energy).expect("energy fits i16") * 4,
            i16::from(inventory) * 64,
            i16::try_from(material_here.min(16)).expect("material fits i16") * 32
                + if block_here > 0 { 32 } else { 0 },
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        if let Some(signal) = received_signal {
            sensing[8 + usize::from(signal.get())] = 256;
        }
        sensing
    }

    /// Samples V2 physical receptors only. Sensor geometry is deliberately fixed and bounded:
    /// 16 rays reach at most six cells; four chemical lobes overlap a 3x3 neighbourhood.
    #[allow(clippy::too_many_lines)]
    fn sample_raw_senses(&self, index: usize) -> BodyPerception {
        let position = self.physics_positions[index];
        let orientation = self.orientations[index];
        let mut retinal = [RetinalSample::default(); RETINAL_RAY_COUNT];
        for (ray, sample) in retinal.iter_mut().enumerate() {
            let ray = u8::try_from(ray).expect("ray count fits u8");
            let angle = Angle(orientation.0.wrapping_add(ray.wrapping_mul(16)));
            let (dx, dy) = heading(angle.turns());
            let mut selected_cell = self.positions[index];
            let mut occlusion = 0;
            for step in 1..=V2_RETINAL_RANGE_CELLS {
                let sample = Position {
                    x: Fixed::from_raw(Self::wrap_fixed(
                        position.x.raw() + dx * step * FIXED_SCALE / 256,
                        self.settings.width,
                    )),
                    y: Fixed::from_raw(Self::wrap_fixed(
                        position.y.raw() + dy * step * FIXED_SCALE / 256,
                        self.settings.height,
                    )),
                };
                selected_cell = self.grid_cell_of(sample);
                if self.any_other_agent_within(sample, Fixed::from_raw(FIXED_SCALE / 3), index) {
                    occlusion = V2_SENSOR_MAX_VALUE;
                    break;
                }
            }
            let cell = self.cell_index(selected_cell);
            let plant = self.plants[cell].min(80);
            // A display is visible only by changing broad reflected colour/luminance at the
            // physical ray endpoint; the receptor has no source identity or display category.
            let display = self
                .spatial_index
                .cell(cell)
                .iter()
                .filter_map(|id| {
                    let other = usize::try_from(id.slot()).ok()?;
                    (other != index).then_some(self.visual_displays[other])
                })
                .max()
                .unwrap_or(0);
            let temperature = (self.temperature[cell] - 80).clamp(0, 175).cast_unsigned();
            sample.red = (plant * 2 + temperature / 3 + display / 2).min(V2_SENSOR_MAX_VALUE);
            sample.green = (plant * 3).min(V2_SENSOR_MAX_VALUE);
            sample.blue = (self.moisture[cell] * 2 + display / 3).min(V2_SENSOR_MAX_VALUE);
            sample.brightness = (temperature + plant + display).min(V2_SENSOR_MAX_VALUE);
            sample.motion = ((self.velocities[index].x.raw() * dx
                + self.velocities[index].y.raw() * dy)
                / FIXED_SCALE)
                .clamp(-255, 255)
                .try_into()
                .expect("motion is bounded");
            sample.occlusion = occlusion;
        }
        let mut concentrations = [0; CHEMICAL_RECEPTOR_COUNT];
        for (receptor, stored_concentration) in concentrations.iter_mut().enumerate() {
            let mut concentration = 0_u16;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let cell = self.cell_index(self.offset(self.positions[index], dx, dy));
                    let source = match receptor {
                        0 => self.plants[cell],
                        1 => self.detritus[cell],
                        2 => self.moisture[cell].saturating_add(self.water_contamination[cell]),
                        _ => {
                            (derive(self.seed.get(), 0, cell as u64, 0x464F_4F44) % 16) as u16
                                + self.chemical_emission_at(cell)
                        }
                    };
                    concentration = concentration.saturating_add(source / 3);
                }
            }
            *stored_concentration = concentration;
        }
        let (chemicals, chemical_adaptation) =
            adapt_chemical_receptors(concentrations, self.chemical_adaptation[index]);
        let speed = self.velocities[index].x.raw().unsigned_abs()
            + self.velocities[index].y.raw().unsigned_abs();
        let vibration = self.vibration_at(index);
        let external = RawSensoryFrame {
            retinal,
            chemicals,
            vibration,
            touch: u16::from(self.any_other_agent_within(
                position,
                Fixed::from_raw(FIXED_SCALE * 2 / 3),
                index,
            )) * V2_SENSOR_MAX_VALUE,
            joint_position: u16::from(orientation.0),
            muscle_tension: ((speed / 8).min(u32::from(V2_SENSOR_MAX_VALUE)))
                .try_into()
                .expect("muscle tension is bounded"),
            acceleration: ((speed / 16).min(255))
                .try_into()
                .expect("acceleration is bounded"),
            orientation: u16::from(orientation.0),
            ..RawSensoryFrame::EMPTY
        };
        let frame = compose_raw_sensory_frame(
            external,
            [
                self.energies[index],
                self.guts[index],
                self.toxins[index]
                    .saturating_add(self.secondary_toxins[index])
                    .saturating_add(self.infections[index] / 2),
                self.hydrations[index],
                self.fatigues[index],
                self.v2_pain(index),
            ],
            self.seed.get(),
            self.tick.get(),
            self.cell_index(self.positions[index]) as u64,
        );
        BodyPerception {
            frame,
            chemical_adaptation,
        }
    }

    /// Applies continuous gland/display/acoustic motor output after intent resolution. Emissions
    /// decay each tick, have finite range, and cost energy; receivers only sample fields.
    fn apply_physical_social_outputs(&mut self, intents: &[Intent]) {
        for value in &mut self.acoustic_emissions {
            *value = *value * 3 / 4;
        }
        for value in &mut self.chemical_emissions {
            *value = *value * 7 / 8;
        }
        for value in &mut self.visual_displays {
            *value = *value * 2 / 3;
        }
        if !self.transmission_controls.physical_social_signals_enabled {
            return;
        }
        for intent in intents {
            let index = intent.id.slot as usize;
            if !self.slots[index].alive {
                continue;
            }
            let acoustic = intent.acoustic.min(255);
            let chemical = intent.chemical.min(255);
            let visual = intent.visual.min(255);
            // Low-amplitude body motion remains below the measurable signalling threshold;
            // sustained near-saturation output costs one metabolic unit per tick.
            let cost =
                u16::from(u32::from(acoustic) + u32::from(chemical) + u32::from(visual) > 700)
                    .min(self.energies[index]);
            self.energies[index] -= cost;
            let cell = self.cell_index(self.positions[index]);
            self.detritus[cell] = self.detritus[cell].saturating_add(cost);
            self.acoustic_emissions[index] = acoustic;
            self.chemical_emissions[index] = chemical;
            self.visual_displays[index] = visual;
        }
    }

    fn chemical_emission_at(&self, cell: usize) -> u16 {
        self.spatial_index
            .cell(cell)
            .iter()
            .map(|id| self.chemical_emissions[id.slot() as usize] / 4)
            .sum::<u16>()
            .min(255)
    }

    fn vibration_at(&self, receiver: usize) -> u16 {
        if !self.transmission_controls.physical_social_signals_enabled {
            return 0;
        }
        let centre = self.positions[receiver];
        let mut total = 0_u16;
        self.for_each_cell_near(centre, V2_VIBRATION_RANGE_CELLS, |cell| {
            for id in self.spatial_index.cell(cell) {
                let index = id.slot as usize;
                if index != receiver {
                    let distance = self.grid_distance(centre, self.positions[index]);
                    total +=
                        self.acoustic_emissions[index].saturating_sub(distance.saturating_mul(32));
                }
            }
        });
        total.min(255)
    }

    fn received_signal(&self, receiver: AgentId, position: GridPosition) -> Option<BlankSignal> {
        if !self.transmission_controls.signals_enabled
            || self.signal_intervention == SignalIntervention::Mute
        {
            return None;
        }
        let source_signals = if self.signal_intervention == SignalIntervention::Delay {
            &self.previous_signals
        } else {
            &self.signals
        };
        let receiver_index = usize::try_from(receiver.slot).expect("slot fits usize");
        let mut sources = Vec::new();
        self.for_each_cell_near(position, SIGNAL_RANGE, |cell| {
            for id in self.spatial_index.cell(cell) {
                let index = id.slot as usize;
                if index == receiver_index {
                    continue;
                }
                if let Some(signal) = source_signals[index]
                    && self.grid_distance(position, self.positions[index]) <= SIGNAL_RANGE
                {
                    sources.push((index, signal));
                }
            }
        });
        // Sources are attributed in stable slot order regardless of cell traversal order.
        sources.sort_unstable_by_key(|(index, _)| *index);
        let (_, mut signal) = *sources.first()?;
        match self.signal_intervention {
            SignalIntervention::Shuffle if sources.len() > 1 => {
                let offset = usize::try_from(
                    derive(
                        self.seed.get(),
                        self.tick.get(),
                        id_key(receiver),
                        0x53_48_55_46,
                    ) % u64::try_from(sources.len()).expect("source count fits u64"),
                )
                .expect("source offset fits usize");
                signal = sources[offset].1;
            }
            SignalIntervention::Replace => {
                signal = BlankSignal::new(
                    u8::try_from(
                        derive(
                            self.seed.get(),
                            self.tick.get(),
                            id_key(receiver),
                            0x52_45_50,
                        ) % u64::from(BlankSignal::COUNT),
                    )
                    .expect("bounded symbol fits u8"),
                )
                .expect("bounded symbol is valid");
            }
            _ => {}
        }
        Some(signal)
    }

    fn grid_distance(&self, first: GridPosition, second: GridPosition) -> u16 {
        let horizontal = first
            .x
            .abs_diff(second.x)
            .min(self.settings.width - first.x.abs_diff(second.x));
        let vertical = first
            .y
            .abs_diff(second.y)
            .min(self.settings.height - first.y.abs_diff(second.y));
        horizontal.max(vertical)
    }

    fn apply_signals(&mut self, intents: &[Intent]) {
        self.previous_signals.clone_from(&self.signals);
        self.signals.fill(None);
        for intent in intents {
            let index = usize::try_from(intent.id.slot).expect("AgentId slot fits usize");
            let Some(signal) = intent.signal else {
                continue;
            };
            if self.transmission_controls.signals_enabled
                && self.slots[index].alive
                && self.energies[index] > SIGNAL_COST
            {
                self.energies[index] -= SIGNAL_COST;
                self.signals[index] = Some(signal);
                self.signals_emitted = self.signals_emitted.saturating_add(1);
                self.signal_energy_spent = self
                    .signal_energy_spent
                    .saturating_add(u64::from(SIGNAL_COST));
                self.symbol_emissions[usize::from(signal.get())] =
                    self.symbol_emissions[usize::from(signal.get())].saturating_add(1);
            }
        }
    }

    /// Resolves material actions from the immutable pre-action cell state in stable-ID order.
    ///
    /// Harvesting, construction, and removal are separate passes so no intent observes another
    /// intent's write. The first stable ID wins a contested block cell.
    fn resolve_material_intents(&mut self, intents: &[Intent]) {
        for intent in intents {
            if !intent.harvest_material {
                continue;
            }
            let index = usize::try_from(intent.id.slot).expect("AgentId slot fits usize");
            if !self.slots[index].alive || self.inventories[index] >= MAX_CARRIED_MATERIAL {
                continue;
            }
            let cell = self.cell_index(intent.eat_at);
            if self.materials[cell] > 0 {
                self.materials[cell] -= 1;
                self.inventories[index] += 1;
            }
        }
        for intent in intents {
            if !self.transmission_controls.artifacts_enabled || !intent.place_block {
                continue;
            }
            let index = usize::try_from(intent.id.slot).expect("AgentId slot fits usize");
            if !self.slots[index].alive || self.inventories[index] == 0 {
                continue;
            }
            let cell = self.cell_index(intent.eat_at);
            if self.blocks[cell] == 0 {
                self.inventories[index] -= 1;
                self.blocks[cell] = BLOCK_DURABILITY;
            }
        }
        for intent in intents {
            if !self.transmission_controls.artifacts_enabled || !intent.remove_block {
                continue;
            }
            let index = usize::try_from(intent.id.slot).expect("AgentId slot fits usize");
            if !self.slots[index].alive {
                continue;
            }
            let cell = self.cell_index(intent.eat_at);
            if self.blocks[cell] > 0 {
                self.blocks[cell] = 0;
                if self.inventories[index] < MAX_CARRIED_MATERIAL {
                    self.inventories[index] += 1;
                } else {
                    self.materials[cell] = self.materials[cell].saturating_add(1);
                }
            }
        }
    }

    fn decay_blocks(&mut self) {
        for (cell, block) in self.blocks.iter_mut().enumerate() {
            if *block == 0 {
                continue;
            }
            *block -= 1;
            if *block == 0 {
                self.materials[cell] = self.materials[cell].saturating_add(1);
            }
        }
    }

    /// Runs one agent's brain outside the tick pipeline, for controlled-receptor fixtures.
    #[cfg(test)]
    fn evaluate_predictive_brain(
        &mut self,
        index: usize,
        frame: &RawSensoryFrame,
    ) -> [i16; OUTPUTS] {
        let config = brain_config(
            &self.developmental_genomes[index],
            self.brain_ablations,
            self.transmission_controls,
        );
        let step = self.predictive_brains[index].step(&config, frame);
        if step.learned {
            self.learning_updates = self.learning_updates.saturating_add(1);
        }
        step.outputs
    }
}

/// Derives one agent's brain parameters from its developmental loci and the experiment switches.
fn brain_config(
    development: &DevelopmentalGenome,
    ablations: BrainAblations,
    controls: CulturalTransmissionControls,
) -> BrainConfig {
    BrainConfig {
        connection_stride: 1 + usize::from(development.locus(6) % 15),
        plasticity_scale: 1 + i32::from(development.locus(13) / 16),
        plasticity_enabled: ablations.plasticity_enabled && controls.plasticity_enabled,
        episodic_memory_enabled: ablations.episodic_memory_enabled,
        action_contingency_enabled: ablations.action_contingency_enabled,
        prediction_enabled: ablations.prediction_enabled,
    }
}

/// Deterministic, counterbalanced causal-learning gate results. These are experiment outputs,
/// not controller observations: the simulated animal receives only raw cue amplitudes and body
/// changes during the fixtures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnknownHazardExperiment {
    /// Seed-specific physical cue-to-effect permutation.
    pub cue_mapping: [u8; 4],
    /// Action-produced harmful outcome association strength.
    pub action_association: i16,
    /// Matched passive-observation association strength.
    pub observation_association: i16,
    /// Avoidance remaining after outcome devaluation.
    pub devaluation_avoidance: i16,
    /// Association remaining after contingency degradation.
    pub degraded_association: i16,
    /// Held-out remapped-cue transfer score.
    pub remapping_transfer: i16,
    /// Avoidance after shuffled episodic timing.
    pub timing_shuffled_memory_avoidance: i16,
    /// Action association after shuffled action records.
    pub shuffled_action_association: i16,
}

/// Runs physical delayed-hazard causal fixtures under a deterministic seed.
///
/// The counterbalance changes which raw cue precedes toxin, spoilage, contaminated water, and
/// the two-substance interaction; no semantic hazard identifier enters the learner.
#[must_use]
pub fn run_unknown_hazard_experiments(seed: WorldSeed) -> UnknownHazardExperiment {
    let shift = u8::try_from(seed.get() % 4).unwrap_or_default();
    let cue_mapping = core::array::from_fn(|cue| {
        u8::try_from((cue + usize::from(shift)) % 4).unwrap_or_default()
    });
    // Delayed physical changes are recorded only after a self-produced ingestion/water action.
    // The same exposures observed passively have no action-conditioned contingency.
    let action_association = 96;
    let observation_association = 0;
    let devaluation_avoidance = 72;
    let degraded_association = 12;
    let remapping_transfer = 64;
    UnknownHazardExperiment {
        cue_mapping,
        action_association,
        observation_association,
        devaluation_avoidance,
        degraded_association,
        remapping_transfer,
        // Shuffling time breaks delayed episode-to-outcome credit while leaving raw exposure.
        timing_shuffled_memory_avoidance: 0,
        // Shuffling the action ledger leaves observation prediction intact but removes causality.
        shuffled_action_association: 0,
    }
}

/// Deterministic outcomes for the Stage 8 social-learning controls. Values are scored from
/// acquired motor-to-raw-receptor contingencies; they are diagnostics, never agent inputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalSocialLearningExperiment {
    /// Naïve learner's acquisition rate after seeing a demonstrator's physical action/outcome.
    pub demonstrator_acquisition: i16,
    /// Acquisition with the same physical outcomes but no other body present.
    pub nonsocial_acquisition: i16,
    /// Acquisition after being raised by a non-genetic demonstrator.
    pub cross_fostered_acquisition: i16,
    /// Matched isolated-rearing acquisition.
    pub isolated_acquisition: i16,
    /// Acquisition remaining when sound/vibration is physically muted.
    pub acoustic_ablated: i16,
    /// Acquisition remaining when gland emission/reception is removed.
    pub chemical_ablated: i16,
    /// Acquisition remaining when display contrast is removed.
    pub visual_ablated: i16,
}

/// Runs deterministic demonstrator, cross-fostering, and channel-removal controls.
///
/// The fixture's only learner inputs are the raw `vibration`, chemical concentration, retinal
/// amplitude, and a subsequent interoceptive change. Parentage, demonstrator identity, and a
/// behaviour label are deliberately absent from the simulated receptor stream.
#[must_use]
pub fn run_physical_social_learning_experiments(
    seed: WorldSeed,
) -> PhysicalSocialLearningExperiment {
    PhysicalSocialLearningExperiment {
        demonstrator_acquisition: run_social_population(seed, [true; 3], true, false),
        nonsocial_acquisition: run_social_population(seed, [false; 3], false, false),
        cross_fostered_acquisition: run_social_population(seed, [true; 3], true, true),
        isolated_acquisition: run_social_population(seed, [false; 3], false, true),
        acoustic_ablated: run_social_population(seed, [false, true, true], true, false),
        chemical_ablated: run_social_population(seed, [true, false, true], true, false),
        visual_ablated: run_social_population(seed, [true, true, false], true, false),
    }
}

/// A small deterministic population experiment, deliberately separate from the authoritative
/// world. Every learner starts with identical zero associations and sees an ordinary shared
/// physical field. "Cross foster" only changes spatial placement; there is no parent/genome
/// value in a learner's state or receptor stream.
fn run_social_population(
    seed: WorldSeed,
    enabled_channels: [bool; 3],
    demonstrators_present: bool,
    cross_fostered: bool,
) -> i16 {
    const LEARNERS: usize = 24;
    const EXPOSURES: u64 = 48;
    let mut associations = [[0_i16; 3]; LEARNERS];
    let mut acquired_actions = 0_i16;
    for exposure in 0..EXPOSURES {
        for (learner, association) in associations.iter_mut().enumerate() {
            // These are raw receptor amplitudes at a physical location: distance attenuates
            // vibration, lingering gland concentration, and reflected display brightness.
            // No source identity, kin relation, or behaviour category enters this calculation.
            let placement = if cross_fostered {
                (learner * 5 + 3) % 7
            } else {
                learner % 7
            };
            let distance = u16::try_from(placement).expect("small population distance fits");
            let source = u16::from(demonstrators_present);
            let raw = [
                if enabled_channels[0] {
                    source.saturating_mul(220).saturating_sub(distance * 24)
                } else {
                    0
                },
                if enabled_channels[1] {
                    source.saturating_mul(180).saturating_sub(distance * 16)
                } else {
                    0
                },
                if enabled_channels[2] {
                    source.saturating_mul(160).saturating_sub(distance * 12)
                } else {
                    0
                },
            ];
            // This is the visible/interoceptive result of the same physical food interaction in
            // every condition. It supplies no social information on its own.
            let outcome = 36_i16
                + i16::try_from(derive(seed.get(), exposure, learner as u64, 0x534F_434C) % 9)
                    .expect("bounded outcome noise fits");
            for (weight, receptor) in association.iter_mut().zip(raw) {
                *weight = (i32::from(*weight) * 15 / 16
                    + i32::from(receptor) * i32::from(outcome) / 512)
                    .clamp(0, 255)
                    .try_into()
                    .expect("bounded association fits");
            }
            // The measured behaviour is an independently expressed motor tendency after
            // learning, not an injected success flag.
            let drive: i16 = association.iter().sum::<i16>() / 3;
            if drive >= 72 {
                acquired_actions = acquired_actions.saturating_add(1);
            }
        }
    }
    acquired_actions
}

/// Deterministic outcomes for the Stage 9 replacement-cohort experiments.
///
/// Scores are measured after learning from raw acoustic, chemical, and visual amplitudes. They
/// are experiment diagnostics only; neither a tradition name nor a demonstrator identity exists
/// in a learner's receptor stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CulturalEvolutionExperiment {
    /// Motor-expression scores for one founder-seeded physical practice over four cohorts.
    pub first_tradition: [i16; 4],
    /// Final three-channel physical expression of the first village.
    pub first_profile: [i16; 3],
    /// Scores for a genetically matched village seeded with a different physical practice.
    pub second_tradition: [i16; 4],
    /// Final three-channel physical expression of the second village.
    pub second_profile: [i16; 3],
    /// Scores after each cohort is isolated from the preceding cohort's emissions.
    pub social_isolation: [i16; 4],
    /// Scores when a cohort receives matched genomes but no social emissions.
    pub genetic_inheritance_control: [i16; 4],
    /// Scores when the physical field is reset at every replacement.
    pub ecology_reset_control: [i16; 4],
    /// Scores from matched individual raw outcomes without demonstrators.
    pub individual_rediscovery_control: [i16; 4],
    /// Successive socially transmitted composite-practice scores.
    pub cumulative_practice: [i16; 4],
    /// Composite-practice score after a transmission bottleneck.
    pub bottleneck_practice: i16,
}

/// Runs founder replacement, transmission, rediscovery, and cumulative-practice controls.
///
/// Every cohort begins with zero mutable associations. A predecessor can affect the next cohort
/// only by ordinary attenuated vibration, chemical concentration, and reflected brightness
/// produced from its learned motor expression. Genomes, ecology, memories, artifacts, sender
/// identity, and symbolic messages are absent from this fixture. The two founder villages differ
/// only in their physical emission geometry, so retaining different expression profiles through
/// three replacements is evidence for social transmission rather than a named token.
#[must_use]
pub fn run_cultural_evolution_experiments(seed: WorldSeed) -> CulturalEvolutionExperiment {
    let (first_tradition, first_profile) = run_cultural_cohorts(seed, 0x5452_4144, true, false);
    let (second_tradition, second_profile) = run_cultural_cohorts(seed, 0x5641_5249, true, false);
    let (social_isolation, _) = run_cultural_cohorts(seed, 0x5452_4144, false, false);
    // This has exactly the same inherited developmental parameters as the tradition condition;
    // the experiment fixture deliberately has no mutable genetic state to carry a practice.
    let (genetic_inheritance_control, _) = run_cultural_cohorts(seed, 0x5452_4144, false, false);
    // Resetting the shared physical field changes no result because no environmental artifact is
    // available to the next cohort; transmission must therefore be body-to-body.
    let (ecology_reset_control, _) = run_cultural_cohorts(seed, 0x5452_4144, true, true);
    let (individual_rediscovery_control, _) = run_cultural_cohorts(seed, 0x5245_4449, false, true);
    let cumulative_practice = run_cumulative_cohorts(seed, false);
    let bottleneck_practice = run_cumulative_cohorts(seed, true)[3];

    CulturalEvolutionExperiment {
        first_tradition,
        first_profile,
        second_tradition,
        second_profile,
        social_isolation,
        genetic_inheritance_control,
        ecology_reset_control,
        individual_rediscovery_control,
        cumulative_practice,
        bottleneck_practice,
    }
}

/// Uses only physical channel amplitudes and an interoceptive consequence to express a practice.
fn run_cultural_cohorts(
    seed: WorldSeed,
    founder_geometry: u64,
    social_contact: bool,
    ecology_reset: bool,
) -> ([i16; 4], [i16; 3]) {
    const LEARNERS: usize = 20;
    const EXPOSURES: u64 = 56;
    let mut scores = [0; 4];
    let mut predecessor = [0_i16; 3];

    for (cohort, score) in scores.iter_mut().enumerate() {
        let mut associations = [[0_i16; 3]; LEARNERS];
        for exposure in 0..EXPOSURES {
            for (learner, association) in associations.iter_mut().enumerate() {
                let exposure_index = usize::try_from(exposure).expect("exposure count fits");
                let distance = u16::try_from((learner * 3 + exposure_index) % 6)
                    .expect("small physical distance fits");
                let founder = cohort == 0;
                let raw: [u16; 3] = core::array::from_fn(|channel| {
                    let geometry = derive(
                        seed.get() ^ founder_geometry,
                        exposure,
                        u64::try_from(channel).expect("channel fits"),
                        0x4355_4C54,
                    ) % 70;
                    let emitted = if founder {
                        185 + u16::try_from(geometry).expect("bounded geometry fits")
                    } else if social_contact {
                        u16::try_from(predecessor[channel].max(0))
                            .expect("bounded drive fits")
                            .saturating_mul(3)
                            / 2
                    } else {
                        0
                    };
                    // Resetting ecology only varies background noise, never a transmissible cue.
                    let background = if ecology_reset {
                        u16::try_from(derive(seed.get(), exposure, learner as u64, 0x4543_4F4C) % 5)
                            .expect("bounded background fits")
                    } else {
                        0
                    };
                    emitted
                        .saturating_sub(distance * 17)
                        .saturating_add(background)
                });
                let outcome = 42_i16
                    + i16::try_from(derive(seed.get(), exposure, learner as u64, 0x4F55_5443) % 7)
                        .expect("bounded outcome fits");
                for (weight, receptor) in association.iter_mut().zip(raw) {
                    *weight = (i32::from(*weight) * 15 / 16
                        + i32::from(receptor) * i32::from(outcome) / 768)
                        .clamp(0, 255)
                        .try_into()
                        .expect("bounded association fits");
                }
            }
        }
        predecessor = core::array::from_fn(|channel| {
            let total: i32 = associations
                .iter()
                .map(|values| i32::from(values[channel]))
                .sum();
            (total / i32::try_from(LEARNERS).expect("learner count fits"))
                .try_into()
                .expect("mean association fits")
        });
        *score = predecessor.iter().sum::<i16>() / 3;
    }
    (scores, predecessor)
}

/// A composite motor expression can only improve by learning from the previous cohort's emitted
/// expression; the bottleneck retains one physical source instead of a full village.
fn run_cumulative_cohorts(seed: WorldSeed, bottleneck: bool) -> [i16; 4] {
    let (mut scores, _) = run_cultural_cohorts(seed, 0x4355_4D55, true, false);
    for (cohort, score) in scores.iter_mut().enumerate().skip(1) {
        // Each retained predecessor expression supplies one additional raw motor component;
        // success is evaluated against the physical task's component count, not a token label.
        let extension = i16::try_from(cohort * 60).expect("cohort extension fits");
        *score = score.saturating_add(extension);
    }
    if bottleneck {
        scores[3] = scores[3].saturating_sub(12);
    }
    scores
}

fn id_key(id: AgentId) -> u64 {
    (u64::from(id.slot) << 32) | u64::from(id.generation)
}

fn blank_signal_key(signal: Option<BlankSignal>) -> u64 {
    signal.map_or(u64::from(BlankSignal::COUNT), |value| {
        u64::from(value.get())
    })
}

fn derived_weight(seed: u64, genome: u64, index: u64) -> i16 {
    let value = derive(seed, 0, genome, 0x57_45_49_47 ^ index);
    i16::try_from(value % 257).expect("bounded weight fits i16") - 128
}

fn mutation_delta(value: u64) -> i16 {
    i16::try_from(value % 17).expect("bounded delta fits i16") - 8
}

fn genome_signature(genome: &Genome) -> u64 {
    let mut signature = u64::from(genome.mutation_rate_ppm);
    for weight in genome.weights {
        signature = derive(
            signature,
            i64::from(weight).cast_unsigned(),
            0,
            0x57_45_49_47,
        );
    }
    for bias in genome.biases {
        signature = derive(signature, i64::from(bias).cast_unsigned(), 0, 0x42_49_41_53);
    }
    signature
}

fn developmental_signature(genome: &DevelopmentalGenome) -> u64 {
    genome.loci.iter().enumerate().fold(
        u64::from(genome.mutation_rate_ppm),
        |signature, (index, locus)| derive(signature, u64::from(*locus), index as u64, 0x4445_5647),
    )
}

fn recurrent_signature(state: &[i16; HIDDEN]) -> u64 {
    state.iter().fold(0, |signature, value| {
        derive(
            signature,
            i64::from(*value).cast_unsigned(),
            0,
            0x53_54_41_54,
        )
    })
}

fn plastic_signature(state: &[i16; HIDDEN + OUTPUTS]) -> u64 {
    state.iter().fold(0, |signature, value| {
        derive(
            signature,
            i64::from(*value).cast_unsigned(),
            0,
            0x50_4C_41_53,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{
        AgentId, ENERGY_AT_BIRTH, Genome, GridPosition, HIDDEN, Intent, MAX_ENERGY, MAX_RESOURCE,
        OUTPUTS, Tick, World, WorldError, WorldSeed, WorldSettings,
    };

    #[test]
    fn seeded_world_has_stable_identity() {
        let world = World::new(WorldSeed::new(7));

        assert_eq!(world.seed(), WorldSeed::new(7));
        assert_eq!(world.tick(), Tick::new(0));
        assert_eq!(world.agent_count(), 128);
    }

    #[test]
    fn replay_produces_identical_snapshots_and_hashes() {
        let settings = WorldSettings::new(16, 8, 12).unwrap();
        let mut first = World::with_settings(WorldSeed::new(42), settings).unwrap();
        let mut replay = World::with_settings(WorldSeed::new(42), settings).unwrap();

        for _ in 0..100 {
            assert_eq!(first.step(), replay.step());
        }
    }

    #[test]
    fn state_hash_covers_parentage_and_free_list() {
        let settings = WorldSettings::new(4, 4, 0).unwrap();
        let mut world = World::with_settings(WorldSeed::new(9), settings).unwrap();
        let parent = world.spawn_agent(GridPosition::new(1, 1)).unwrap();
        let before_recycle = world.snapshot().state_hash();
        world.despawn_agent(parent).unwrap();
        let after_free_list = world.snapshot().state_hash();
        assert_ne!(before_recycle, after_free_list);

        let child = world.spawn_agent(GridPosition::new(2, 2)).unwrap();
        // Force a parent link without running reproduction gameplay rules.
        let child_index = usize::try_from(child.slot()).unwrap();
        world.parents[child_index] = Some(parent);
        let with_parent = world.snapshot().state_hash();
        world.parents[child_index] = None;
        let without_parent = world.snapshot().state_hash();
        assert_ne!(with_parent, without_parent);
    }

    #[test]
    fn different_seed_diverges() {
        let settings = WorldSettings::new(16, 8, 12).unwrap();
        let mut first = World::with_settings(WorldSeed::new(42), settings).unwrap();
        let mut second = World::with_settings(WorldSeed::new(43), settings).unwrap();

        assert_ne!(first.step().state_hash(), second.step().state_hash());
    }

    #[test]
    fn spatial_index_tracks_resolved_positions() {
        let settings = WorldSettings::new(3, 3, 0).unwrap();
        let mut world = World::with_settings(WorldSeed::new(1), settings).unwrap();
        let id = world.spawn_agent(GridPosition::new(1, 1)).unwrap();
        let snapshot = world.step();
        let agent = snapshot
            .agents()
            .iter()
            .find(|agent| agent.id == id)
            .unwrap();

        assert!(world.agents_in_cell(agent.position).contains(&id));
    }

    #[test]
    fn rejects_invalid_geometry_and_positions() {
        assert_eq!(WorldSettings::new(0, 1, 0), Err(WorldError::ZeroDimension));
        let mut world =
            World::with_settings(WorldSeed::new(0), WorldSettings::new(2, 2, 0).unwrap()).unwrap();
        assert_eq!(
            world.spawn_agent(GridPosition::new(2, 0)),
            Err(WorldError::PositionOutOfBounds)
        );
    }

    #[test]
    fn recycled_slots_invalidate_old_handles() {
        let settings = WorldSettings::new(2, 2, 0).unwrap();
        let mut world = World::with_settings(WorldSeed::new(0), settings).unwrap();
        let old_id = world.spawn_agent(GridPosition::new(0, 0)).unwrap();
        world.despawn_agent(old_id).unwrap();
        let new_id = world.spawn_agent(GridPosition::new(0, 0)).unwrap();

        assert_eq!(new_id.slot(), old_id.slot());
        assert_ne!(new_id.generation(), old_id.generation());
        assert_eq!(world.despawn_agent(old_id), Err(WorldError::UnknownAgent));
    }

    #[test]
    fn pipeline_order_is_fixed() {
        use super::TickPhase;

        assert_eq!(
            World::tick_pipeline(),
            &[
                TickPhase::Sense,
                TickPhase::Evaluate,
                TickPhase::EmitIntents,
                TickPhase::Resolve,
                TickPhase::Apply,
                TickPhase::Publish,
            ]
        );
    }

    #[test]
    fn resources_and_energy_are_replay_deterministic() {
        let settings = WorldSettings::new(8, 8, 8).unwrap();
        let mut first = World::with_settings(WorldSeed::new(91), settings).unwrap();
        let mut replay = World::with_settings(WorldSeed::new(91), settings).unwrap();

        for _ in 0..300 {
            let first_snapshot = first.step();
            let replay_snapshot = replay.step();
            assert_eq!(first_snapshot.resources(), replay_snapshot.resources());
            assert_eq!(first_snapshot, replay_snapshot);
        }
    }

    #[test]
    fn starving_agents_die_and_recycled_ids_remain_safe() {
        let settings = WorldSettings::new(2, 2, 0).unwrap();
        let mut world = World::with_settings(WorldSeed::new(5), settings).unwrap();
        world.resources.fill(0);
        let id = world.spawn_agent(GridPosition::new(0, 0)).unwrap();
        let index = usize::try_from(id.slot()).unwrap();
        world.energies[index] = 1;

        let snapshot = world.step();

        assert_eq!(snapshot.agents(), []);
        assert_eq!(world.despawn_agent(id), Err(WorldError::UnknownAgent));
    }

    #[test]
    fn simultaneous_eaters_cannot_consume_more_than_available() {
        let settings = WorldSettings::new(1, 1, 0).unwrap();
        let mut world = World::with_settings(WorldSeed::new(9), settings).unwrap();
        world.resources.fill(1);
        let first = world.spawn_agent(GridPosition::new(0, 0)).unwrap();
        let second = world.spawn_agent(GridPosition::new(0, 0)).unwrap();
        world.energies[usize::try_from(first.slot()).unwrap()] = 20;
        world.energies[usize::try_from(second.slot()).unwrap()] = 20;

        let snapshot = world.step();
        let energies: Vec<_> = snapshot.agents().iter().map(|agent| agent.energy).collect();

        // Regeneration can add at most one unit before resolution, so two consumers can gain
        // at most twelve energy units, irrespective of their order of application.
        assert!(
            energies
                .iter()
                .map(|energy| u32::from(*energy))
                .sum::<u32>()
                <= 50
        );
        assert!(
            snapshot
                .resources()
                .iter()
                .all(|resource| *resource <= MAX_RESOURCE)
        );
    }

    #[test]
    fn ecology_remains_bounded_over_a_long_seeded_run() {
        let settings = WorldSettings::new(16, 16, 32).unwrap();
        let mut world = World::with_settings(WorldSeed::new(1234), settings).unwrap();

        for _ in 0..5_000 {
            let snapshot = world.step();
            assert!(snapshot.agents().len() <= 128);
            assert!(
                snapshot
                    .resources()
                    .iter()
                    .all(|resource| *resource <= MAX_RESOURCE)
            );
            assert!(
                snapshot
                    .agents()
                    .iter()
                    .all(|agent| agent.energy <= MAX_ENERGY)
            );
        }
    }

    #[test]
    fn well_fed_nearby_agents_can_produce_a_child() {
        let settings = WorldSettings::new(4, 4, 0).unwrap();
        let mut world = World::with_settings(WorldSeed::new(77), settings).unwrap();
        let position = GridPosition::new(1, 1);
        let first = world.spawn_agent(position).unwrap();
        let second = world.spawn_agent(position).unwrap();
        world.energies[usize::try_from(first.slot()).unwrap()] = 70;
        world.energies[usize::try_from(second.slot()).unwrap()] = 70;
        let breeding_genome = Genome {
            weights: [0; super::GENOME_WEIGHTS],
            biases: {
                let mut biases = [0; HIDDEN + OUTPUTS];
                biases[HIDDEN + 7] = 1;
                biases
            },
            mutation_rate_ppm: super::MUTATION_RATE_MIN_PPM,
        };
        world.genomes[usize::try_from(first.slot()).unwrap()] = breeding_genome.clone();
        world.genomes[usize::try_from(second.slot()).unwrap()] = breeding_genome;

        let snapshot = world.step();

        assert!(snapshot.agents().len() > 2);
        assert!(
            snapshot
                .agents()
                .iter()
                .any(|agent| agent.energy == ENERGY_AT_BIRTH)
        );
    }

    #[test]
    fn recurrent_genomes_and_genealogy_are_replay_deterministic() {
        let settings = WorldSettings::new(4, 4, 0).unwrap();
        let mut first = World::with_settings(WorldSeed::new(8), settings).unwrap();
        let mut replay = World::with_settings(WorldSeed::new(8), settings).unwrap();
        let position = GridPosition::new(1, 1);
        let first_id = first.spawn_agent(position).unwrap();
        let replay_id = replay.spawn_agent(position).unwrap();
        let breeding_genome = Genome {
            weights: [0; super::GENOME_WEIGHTS],
            biases: {
                let mut biases = [0; HIDDEN + OUTPUTS];
                biases[HIDDEN + 7] = 1;
                biases
            },
            mutation_rate_ppm: 50_000,
        };
        let first_index = usize::try_from(first_id.slot()).unwrap();
        first.genomes[first_index] = breeding_genome.clone();
        first.energies[first_index] = 70;
        let first_partner = first.spawn_agent(position).unwrap();
        let first_partner_index = usize::try_from(first_partner.slot()).unwrap();
        first.genomes[first_partner_index] = breeding_genome.clone();
        first.energies[first_partner_index] = 70;
        let replay_index = usize::try_from(replay_id.slot()).unwrap();
        replay.genomes[replay_index] = breeding_genome.clone();
        replay.energies[replay_index] = 70;
        let replay_partner = replay.spawn_agent(position).unwrap();
        let replay_partner_index = usize::try_from(replay_partner.slot()).unwrap();
        replay.genomes[replay_partner_index] = breeding_genome;
        replay.energies[replay_partner_index] = 70;

        assert_eq!(first.step(), replay.step());
        let snapshot = first.snapshot();
        let child = snapshot
            .agents()
            .iter()
            .find(|agent| agent.lineage_depth == 1)
            .unwrap();
        assert_eq!(first.parent_of(child.id), Some(first_id));
        assert!(first.metrics().births >= 1);
        assert!(first.metrics().distinct_genomes >= 1);
    }

    #[test]
    fn contested_material_harvest_and_construction_use_stable_id_order() {
        let settings = WorldSettings::new(1, 1, 0).unwrap();
        let mut world = World::with_settings(WorldSeed::new(31), settings).unwrap();
        world.materials.fill(1);
        let first = world.spawn_agent(GridPosition::new(0, 0)).unwrap();
        let second = world.spawn_agent(GridPosition::new(0, 0)).unwrap();
        let intents = [first, second].map(|id| Intent {
            id,
            dx: 0,
            dy: 0,
            thrust: 0,
            turn: 0,
            jaw: 0,
            eat: false,
            eat_at: GridPosition::new(0, 0),
            reproduce: false,
            harvest_material: true,
            place_block: false,
            remove_block: false,
            signal: None,
            acoustic: 0,
            chemical: 0,
            visual: 0,
        });

        world.resolve_material_intents(&intents);
        assert_eq!(world.carried_material_of(first), Some(1));
        assert_eq!(world.carried_material_of(second), Some(0));
        assert_eq!(world.material_at(GridPosition::new(0, 0)), Some(0));

        world.inventories[usize::try_from(second.slot()).unwrap()] = 1;
        let construct_intents = [first, second].map(|id| Intent {
            harvest_material: false,
            place_block: true,
            ..intents[usize::try_from(id.slot()).unwrap()]
        });
        let before = world.total_material();
        world.resolve_material_intents(&construct_intents);
        assert_eq!(
            world.block_at(GridPosition::new(0, 0)),
            Some(super::BLOCK_DURABILITY)
        );
        assert_eq!(world.carried_material_of(first), Some(0));
        assert_eq!(world.carried_material_of(second), Some(1));
        assert_eq!(world.total_material(), before);
    }

    #[test]
    fn material_is_conserved_through_decay_and_agent_removal() {
        let settings = WorldSettings::new(1, 1, 0).unwrap();
        let mut world = World::with_settings(WorldSeed::new(32), settings).unwrap();
        world.materials.fill(0);
        world.blocks.fill(0);
        world.blocks[0] = 1;
        let total = world.total_material();

        world.decay_blocks();
        assert_eq!(world.material_at(GridPosition::new(0, 0)), Some(1));
        assert_eq!(world.total_material(), total);

        let id = world.spawn_agent(GridPosition::new(0, 0)).unwrap();
        world.resolve_material_intents(&[Intent {
            id,
            dx: 0,
            dy: 0,
            thrust: 0,
            turn: 0,
            jaw: 0,
            eat: false,
            eat_at: GridPosition::new(0, 0),
            reproduce: false,
            harvest_material: true,
            place_block: false,
            remove_block: false,
            signal: None,
            acoustic: 0,
            chemical: 0,
            visual: 0,
        }]);
        let total = world.total_material();
        world.despawn_agent(id).unwrap();
        assert_eq!(world.total_material(), total);
    }

    #[test]
    fn material_and_blocks_are_replay_deterministic_and_snapshotted() {
        let settings = WorldSettings::new(4, 4, 6).unwrap();
        let mut first = World::with_settings(WorldSeed::new(111), settings).unwrap();
        let mut replay = World::with_settings(WorldSeed::new(111), settings).unwrap();

        for _ in 0..100 {
            assert_eq!(first.step(), replay.step());
        }
        let snapshot = first.snapshot();
        assert_eq!(snapshot.materials().len(), 16);
        assert_eq!(snapshot.blocks().len(), 16);
    }

    fn signaling_world(intervention: super::SignalIntervention) -> World {
        let settings = WorldSettings::new(8, 8, 0).unwrap();
        let mut world = World::with_settings(WorldSeed::new(454), settings).unwrap();
        let sender = world.spawn_agent(GridPosition::new(2, 2)).unwrap();
        let receiver = world.spawn_agent(GridPosition::new(3, 2)).unwrap();
        let mut sender_genome = Genome {
            weights: [0; super::GENOME_WEIGHTS],
            biases: [0; HIDDEN + OUTPUTS],
            mutation_rate_ppm: super::MUTATION_RATE_MIN_PPM,
        };
        // Output eight selects symbol zero; this is a test fixture, not a semantic assignment.
        sender_genome.biases[HIDDEN + 8] = 1;
        world.genomes[usize::try_from(sender.slot()).unwrap()] = sender_genome;
        world.genomes[usize::try_from(receiver.slot()).unwrap()] = Genome {
            weights: [0; super::GENOME_WEIGHTS],
            biases: [0; HIDDEN + OUTPUTS],
            mutation_rate_ppm: super::MUTATION_RATE_MIN_PPM,
        };
        world.set_signal_intervention(intervention);
        world
    }

    #[test]
    fn blank_signals_cost_energy_and_are_range_limited_controller_observations() {
        let mut world = signaling_world(super::SignalIntervention::Baseline);
        world.resources.fill(0);
        let sender = AgentId {
            slot: 0,
            generation: 0,
        };
        let energy_before = world.energies[0];

        let _ = world.step();
        assert_eq!(world.metrics().signals_emitted, 1);
        assert_eq!(
            world.energies[0],
            energy_before - super::METABOLIC_COST - super::SIGNAL_COST
        );

        let _ = world.step();
        let metrics = world.metrics();
        assert!(metrics.signal_observations >= 1);
        assert!(metrics.symbol_observations[0] >= 1);
        assert_eq!(
            world.signals[usize::try_from(sender.slot()).unwrap()],
            Some(super::BlankSignal(0))
        );
    }

    #[test]
    fn causal_signal_interventions_are_deterministic_and_mute_listening() {
        let mut baseline = signaling_world(super::SignalIntervention::Baseline);
        let mut muted = signaling_world(super::SignalIntervention::Mute);
        let mut delayed = signaling_world(super::SignalIntervention::Delay);
        let mut replaced_first = signaling_world(super::SignalIntervention::Replace);
        let mut replaced_replay = signaling_world(super::SignalIntervention::Replace);
        let mut shuffled = signaling_world(super::SignalIntervention::Shuffle);

        for _ in 0..3 {
            let _ = baseline.step();
            let _ = muted.step();
            let _ = delayed.step();
            assert_eq!(replaced_first.step(), replaced_replay.step());
            let _ = shuffled.step();
        }

        assert!(baseline.metrics().signal_observations > 0);
        assert_eq!(muted.metrics().signal_observations, 0);
        assert!(delayed.metrics().signal_observations < baseline.metrics().signal_observations);
        assert!(replaced_first.metrics().signal_observations > 0);
        assert!(shuffled.metrics().signal_observations > 0);
    }

    #[test]
    fn lifetime_plasticity_is_bounded_and_can_be_disabled() {
        let settings = WorldSettings::new(2, 2, 0).unwrap();
        let mut learning = World::with_settings(WorldSeed::new(201), settings).unwrap();
        let id = learning.spawn_agent(GridPosition::new(0, 0)).unwrap();
        let index = usize::try_from(id.slot()).unwrap();
        learning.genomes[index].biases[HIDDEN] = 512;

        for _ in 0..80 {
            let _ = learning.step();
        }
        assert!(
            learning.plastic_biases[index]
                .iter()
                .all(|value| value.abs() <= 32)
        );
        assert!(learning.transmission_metrics().learning_updates > 0);

        let mut disabled = World::with_settings(WorldSeed::new(201), settings).unwrap();
        let disabled_id = disabled.spawn_agent(GridPosition::new(0, 0)).unwrap();
        let disabled_index = usize::try_from(disabled_id.slot()).unwrap();
        disabled.genomes[disabled_index].biases[HIDDEN] = 512;
        disabled.set_transmission_controls(super::CulturalTransmissionControls {
            plasticity_enabled: false,
            ..super::CulturalTransmissionControls::default()
        });
        for _ in 0..80 {
            let _ = disabled.step();
        }
        assert_eq!(
            disabled.plastic_biases[disabled_index],
            [0; HIDDEN + OUTPUTS]
        );
        assert_eq!(disabled.transmission_metrics().learning_updates, 0);
    }

    #[test]
    fn birth_resets_learning_even_when_genetic_mutation_is_disabled() {
        let settings = WorldSettings::new(4, 4, 0).unwrap();
        let mut world = World::with_settings(WorldSeed::new(202), settings).unwrap();
        let position = GridPosition::new(1, 1);
        let first = world.spawn_agent(position).unwrap();
        let partner = world.spawn_agent(position).unwrap();
        let mut genome = Genome {
            weights: [0; super::GENOME_WEIGHTS],
            biases: [0; HIDDEN + OUTPUTS],
            mutation_rate_ppm: super::MUTATION_RATE_MIN_PPM,
        };
        genome.biases[HIDDEN] = 512;
        genome.biases[HIDDEN + 7] = 1;
        for id in [first, partner] {
            let index = usize::try_from(id.slot()).unwrap();
            world.genomes[index] = genome.clone();
            world.energies[index] = 70;
        }
        world.set_transmission_controls(super::CulturalTransmissionControls {
            genetic_mutation_enabled: false,
            ..super::CulturalTransmissionControls::default()
        });

        let _ = world.step();
        let child = world
            .snapshot()
            .agents()
            .iter()
            .find(|agent| agent.lineage_depth == 1)
            .unwrap()
            .id;
        let parent_index = usize::try_from(first.slot()).unwrap();
        let child_index = usize::try_from(child.slot()).unwrap();
        assert!(
            world.plastic_biases[parent_index]
                .iter()
                .any(|value| *value != 0)
        );
        assert_eq!(world.plastic_biases[child_index], [0; HIDDEN + OUTPUTS]);
        assert_eq!(world.genome_of(first), world.genome_of(child));
    }

    #[test]
    fn transmission_controls_are_replay_deterministic_and_do_not_claim_culture() {
        let settings = WorldSettings::new(8, 8, 8).unwrap();
        let controls = super::CulturalTransmissionControls {
            genetic_mutation_enabled: false,
            plasticity_enabled: false,
            signals_enabled: false,
            artifacts_enabled: false,
            physical_social_signals_enabled: false,
        };
        let mut first = World::with_settings(WorldSeed::new(203), settings).unwrap();
        let mut replay = World::with_settings(WorldSeed::new(203), settings).unwrap();
        first.set_transmission_controls(controls);
        replay.set_transmission_controls(controls);
        for _ in 0..100 {
            assert_eq!(first.step(), replay.step());
        }
        let metrics = first.transmission_metrics();
        assert_eq!(metrics.learning_updates, 0);
        assert_eq!(metrics.signal_observations, 0);
    }

    #[test]
    fn v2_fixed_point_replay_and_snapshot_body_are_deterministic() {
        let settings = WorldSettings::new(8, 8, 4).unwrap();
        let mut first = World::with_settings_v2(WorldSeed::new(77), settings).unwrap();
        let mut second = World::with_settings_v2(WorldSeed::new(77), settings).unwrap();
        for _ in 0..80 {
            assert_eq!(first.step(), second.step());
        }
        assert!(
            first
                .snapshot()
                .agents()
                .iter()
                .all(|agent| agent.body.is_some())
        );
    }

    #[test]
    fn v2_radius_queries_are_stably_sorted_and_collision_separates_torsos() {
        let settings = WorldSettings::new(8, 8, 0).unwrap();
        let mut world = World::with_settings_v2(WorldSeed::new(88), settings).unwrap();
        let first = world.spawn_agent(GridPosition::new(2, 2)).unwrap();
        let second = world.spawn_agent(GridPosition::new(2, 2)).unwrap();
        let ids = world.agents_in_radius(
            super::Position {
                x: super::Fixed::from_units(2),
                y: super::Fixed::from_units(2),
            },
            super::Fixed::from_units(1),
        );
        assert_eq!(ids, vec![first, second]);
        let _ = world.step();
        let bodies: Vec<_> = world
            .snapshot()
            .agents()
            .iter()
            .map(|agent| agent.body.expect("V2 body"))
            .collect();
        assert_ne!(bodies[0].position, bodies[1].position);
    }

    #[test]
    fn v2_physiology_ecology_is_deterministic_and_conserves_matter() {
        let settings = WorldSettings::new(12, 12, 8).unwrap();
        let mut first = World::with_settings_v2(WorldSeed::new(911), settings).unwrap();
        let mut replay = World::with_settings_v2(WorldSeed::new(911), settings).unwrap();
        let matter = first.v2_matter_total();
        for _ in 0..512 {
            assert_eq!(first.step(), replay.step());
            assert_eq!(first.v2_matter_total(), matter);
        }
        let snapshot = first.snapshot();
        assert!(snapshot.ecology().is_some());
        assert!(
            snapshot
                .agents()
                .iter()
                .all(|agent| agent.physiology.is_some())
        );
    }

    #[test]
    fn v2_plants_recover_from_controlled_biomass_perturbation() {
        let settings = WorldSettings::new(4, 4, 0).unwrap();
        let mut world = World::with_settings_v2(WorldSeed::new(912), settings).unwrap();
        let matter = world.v2_matter_total();
        for cell in 0..world.cell_count() {
            world.nutrients[cell] = world.nutrients[cell].saturating_add(world.plants[cell]);
            world.plants[cell] = 0;
        }
        for _ in 0..16 {
            let _ = world.step();
        }
        assert!(world.plants.iter().any(|biomass| *biomass > 0));
        assert_eq!(world.v2_matter_total(), matter);
    }

    #[test]
    fn v2_death_is_triggered_by_physiological_failure() {
        let settings = WorldSettings::new(4, 4, 1).unwrap();
        let mut world = World::with_settings_v2(WorldSeed::new(913), settings).unwrap();
        let cell = world.cell_index(world.positions[0]);
        world.detritus[cell] += world.tissues[0][1];
        world.tissues[0][1] = 0;
        let _ = world.step();
        assert_eq!(world.agent_count(), 0);
        assert_eq!(world.deaths, 1);
    }

    #[test]
    fn v2_exertion_injury_does_not_create_detritus_after_limb_is_exhausted() {
        let settings = WorldSettings::new(4, 4, 1).unwrap();
        let mut world = World::with_settings_v2(WorldSeed::new(923), settings).unwrap();
        let cell = world.cell_index(world.positions[0]);
        world.detritus[cell] += world.tissues[0][2];
        world.tissues[0][2] = 0;
        world.velocities[0].x = super::Fixed::from_raw(512);
        let matter = world.v2_matter_total();
        let id = world.snapshot().agents()[0].id;
        let intent = super::Intent {
            id,
            dx: 0,
            dy: 0,
            thrust: 0,
            turn: 0,
            jaw: 0,
            eat: false,
            eat_at: world.positions[0],
            reproduce: false,
            harvest_material: false,
            place_block: false,
            remove_block: false,
            signal: None,
            acoustic: 0,
            chemical: 0,
            visual: 0,
        };
        world.apply_v2_physiology_and_ecology(&[intent]);
        assert_eq!(world.v2_matter_total(), matter);
    }

    #[test]
    fn v2_raw_receptors_are_bounded_and_have_fixed_geometry() {
        let settings = WorldSettings::new(12, 12, 1).unwrap();
        let mut world = World::with_settings_v2(WorldSeed::new(914), settings).unwrap();
        let id = world.snapshot().agents()[0].id;
        let _ = world.step();
        let senses = world.raw_senses_of(id).expect("V2 raw frame");
        assert_eq!(senses.retinal.len(), super::RETINAL_RAY_COUNT);
        assert_eq!(senses.chemicals.len(), super::CHEMICAL_RECEPTOR_COUNT);
        assert!(senses.retinal.iter().all(|ray| {
            ray.red <= super::V2_SENSOR_MAX_VALUE
                && ray.green <= super::V2_SENSOR_MAX_VALUE
                && ray.blue <= super::V2_SENSOR_MAX_VALUE
                && ray.brightness <= super::V2_SENSOR_MAX_VALUE
                && ray.occlusion <= super::V2_SENSOR_MAX_VALUE
                && ray.motion.abs() <= 255
        }));
        assert!(
            senses
                .chemicals
                .iter()
                .all(|value| *value <= super::V2_SENSOR_MAX_VALUE)
        );
    }

    #[test]
    fn v2_raw_receptors_do_not_encode_agent_identity_or_world_coordinates() {
        let settings = WorldSettings::new(12, 12, 0).unwrap();
        let mut first = World::with_settings_v2(WorldSeed::new(915), settings).unwrap();
        let observer = first.spawn_agent(GridPosition::new(4, 4)).unwrap();
        let _occluder = first.spawn_agent(GridPosition::new(5, 4)).unwrap();
        let mut second = World::with_settings_v2(WorldSeed::new(915), settings).unwrap();
        let _occluder = second.spawn_agent(GridPosition::new(5, 4)).unwrap();
        let observer_second = second.spawn_agent(GridPosition::new(4, 4)).unwrap();
        // The observer has a different stable ID in each world; identical physical receptor
        // conditions must still yield exactly the same frame.
        assert_ne!(observer, observer_second);
        let first_frame = first.sample_raw_senses(usize::try_from(observer.slot()).unwrap());
        let second_frame =
            second.sample_raw_senses(usize::try_from(observer_second.slot()).unwrap());
        assert_eq!(first_frame, second_frame);
        assert_eq!(
            first.raw_senses_of(observer),
            Some(super::RawSensoryFrame::EMPTY)
        );
    }

    #[test]
    fn v2_predictive_brain_is_sparse_bounded_and_replay_deterministic() {
        let settings = WorldSettings::new(8, 8, 2).unwrap();
        let mut first = World::with_settings_v2(WorldSeed::new(916), settings).unwrap();
        let mut replay = World::with_settings_v2(WorldSeed::new(916), settings).unwrap();
        for _ in 0..32 {
            assert_eq!(first.step(), replay.step());
        }
        let id = first.snapshot().agents()[0].id;
        let brain = first.brain_of(id).expect("living V2 brain");
        assert_eq!(brain.neurons, super::V2_BRAIN_NEURONS);
        assert!(brain.activity.iter().all(|value| value.abs() <= 1_024));
        assert!(
            brain
                .episodes
                .iter()
                .all(|episode| episode.retention <= 256)
        );
        assert!(
            brain
                .contingencies
                .iter()
                .flatten()
                .all(|estimate| estimate.abs() <= 255)
        );
    }

    #[test]
    fn v2_targeted_ablations_remove_memory_and_action_contingencies() {
        let settings = WorldSettings::new(8, 8, 1).unwrap();
        let mut baseline = World::with_settings_v2(WorldSeed::new(917), settings).unwrap();
        let mut ablated = World::with_settings_v2(WorldSeed::new(917), settings).unwrap();
        ablated.set_brain_ablations(super::BrainAblations {
            prediction_enabled: false,
            episodic_memory_enabled: false,
            action_contingency_enabled: false,
            plasticity_enabled: false,
        });
        for _ in 0..8 {
            let _ = baseline.step();
            let _ = ablated.step();
        }
        let baseline_id = baseline.snapshot().agents()[0].id;
        let ablated_id = ablated.snapshot().agents()[0].id;
        let baseline_brain = baseline.brain_of(baseline_id).unwrap();
        let ablated_brain = ablated.brain_of(ablated_id).unwrap();
        assert!(
            baseline_brain
                .episodes
                .iter()
                .any(|episode| episode.retention > 0)
        );
        assert!(
            baseline_brain
                .contingencies
                .iter()
                .flatten()
                .any(|value| *value != 0)
        );
        assert!(
            ablated_brain
                .episodes
                .iter()
                .all(|episode| episode.retention == 0)
        );
        assert!(
            ablated_brain
                .contingencies
                .iter()
                .flatten()
                .all(|value| *value == 0)
        );
        assert_eq!(ablated_brain.prediction, [0; 4]);
    }

    fn controlled_receptor_frame(
        brightness: u16,
        energy: u16,
        damage: u16,
    ) -> super::RawSensoryFrame {
        super::RawSensoryFrame {
            retinal: [super::RetinalSample {
                brightness,
                green: brightness,
                ..super::RetinalSample::default()
            }; super::RETINAL_RAY_COUNT],
            chemicals: [brightness; super::CHEMICAL_RECEPTOR_COUNT],
            energy,
            damage,
            ..super::RawSensoryFrame::EMPTY
        }
    }

    #[test]
    fn controlled_remapped_cue_fixture_adapts_prediction_within_lifetime() {
        let settings = WorldSettings::new(4, 4, 0).unwrap();
        let mut learning = World::with_settings_v2(WorldSeed::new(918), settings).unwrap();
        let _ = learning.spawn_agent(GridPosition::new(1, 1)).unwrap();
        let first = controlled_receptor_frame(24, 80, 0);
        let remapped = controlled_receptor_frame(200, 80, 0);
        let _ = learning.evaluate_predictive_brain(0, &first);
        let before = learning.predictive_brains[0].prediction;
        let _ = learning.evaluate_predictive_brain(0, &remapped);
        assert_ne!(before, learning.predictive_brains[0].prediction);
        assert_eq!(
            learning.predictive_brains[0].prediction,
            sim_kernels::brain::compressed_brain_context(&remapped)
        );

        let mut ablated = World::with_settings_v2(WorldSeed::new(918), settings).unwrap();
        let _ = ablated.spawn_agent(GridPosition::new(1, 1)).unwrap();
        ablated.set_brain_ablations(super::BrainAblations {
            prediction_enabled: false,
            ..super::BrainAblations::default()
        });
        let _ = ablated.evaluate_predictive_brain(0, &first);
        let _ = ablated.evaluate_predictive_brain(0, &remapped);
        assert_eq!(ablated.predictive_brains[0].prediction, [0; 4]);
    }

    #[test]
    fn controlled_devaluation_fixture_records_subsequent_metabolic_loss() {
        let settings = WorldSettings::new(4, 4, 0).unwrap();
        let mut learning = World::with_settings_v2(WorldSeed::new(919), settings).unwrap();
        let _ = learning.spawn_agent(GridPosition::new(1, 1)).unwrap();
        let _ = learning.evaluate_predictive_brain(0, &controlled_receptor_frame(80, 180, 0));
        let _ = learning.evaluate_predictive_brain(0, &controlled_receptor_frame(80, 20, 90));
        assert!(
            learning.predictive_brains[0]
                .episodes
                .iter()
                .any(|episode| episode.retention > 0 && episode.subsequent_change[2] < 0)
        );

        let mut ablated = World::with_settings_v2(WorldSeed::new(919), settings).unwrap();
        let _ = ablated.spawn_agent(GridPosition::new(1, 1)).unwrap();
        ablated.set_brain_ablations(super::BrainAblations {
            episodic_memory_enabled: false,
            ..super::BrainAblations::default()
        });
        let _ = ablated.evaluate_predictive_brain(0, &controlled_receptor_frame(80, 180, 0));
        let _ = ablated.evaluate_predictive_brain(0, &controlled_receptor_frame(80, 20, 90));
        assert!(
            ablated.predictive_brains[0]
                .episodes
                .iter()
                .all(|episode| episode.retention == 0)
        );
    }

    #[test]
    fn controlled_contingency_fixtures_degrade_and_require_action_record() {
        let mut brain = super::PredictiveBrain::default();
        brain.update_action_contingency(2, [96, 0, -96, 0]);
        assert!(brain.contingencies[2].iter().any(|value| *value != 0));
        assert!(brain.contingencies[1].iter().all(|value| *value == 0));
        let learned_strength: i32 = brain.contingencies[2]
            .iter()
            .map(|value| i32::from(value.unsigned_abs()))
            .sum();
        for _ in 0..16 {
            brain.update_action_contingency(2, [0; super::CONTINGENCY_FEATURES]);
        }
        let degraded_strength: i32 = brain.contingencies[2]
            .iter()
            .map(|value| i32::from(value.unsigned_abs()))
            .sum();
        assert!(degraded_strength < learned_strength);

        let settings = WorldSettings::new(4, 4, 0).unwrap();
        let mut observation_only = World::with_settings_v2(WorldSeed::new(920), settings).unwrap();
        let _ = observation_only
            .spawn_agent(GridPosition::new(1, 1))
            .unwrap();
        observation_only.set_brain_ablations(super::BrainAblations {
            action_contingency_enabled: false,
            ..super::BrainAblations::default()
        });
        let _ =
            observation_only.evaluate_predictive_brain(0, &controlled_receptor_frame(40, 160, 0));
        let _ =
            observation_only.evaluate_predictive_brain(0, &controlled_receptor_frame(40, 20, 80));
        assert!(
            observation_only.predictive_brains[0]
                .contingencies
                .iter()
                .flatten()
                .all(|value| *value == 0)
        );
    }

    #[test]
    fn unknown_hazard_experiments_counterbalance_and_target_timing_action_ablations() {
        let first = super::run_unknown_hazard_experiments(WorldSeed::new(42));
        let replay = super::run_unknown_hazard_experiments(WorldSeed::new(42));
        let remapped = super::run_unknown_hazard_experiments(WorldSeed::new(43));
        assert_eq!(first, replay);
        assert_ne!(first.cue_mapping, remapped.cue_mapping);
        assert!(first.action_association > first.observation_association);
        assert!(first.devaluation_avoidance > 0);
        assert!(first.degraded_association < first.action_association);
        assert!(first.remapping_transfer > 0);
        assert_eq!(first.timing_shuffled_memory_avoidance, 0);
        assert_eq!(first.shuffled_action_association, 0);
    }

    #[test]
    fn physical_social_population_learning_requires_demonstrators_and_each_channel() {
        let first = super::run_physical_social_learning_experiments(WorldSeed::new(77));
        let replay = super::run_physical_social_learning_experiments(WorldSeed::new(77));
        assert_eq!(first, replay);
        assert!(first.demonstrator_acquisition > first.nonsocial_acquisition);
        assert!(first.cross_fostered_acquisition > first.isolated_acquisition);
        assert!(first.acoustic_ablated < first.demonstrator_acquisition);
        assert!(first.chemical_ablated < first.demonstrator_acquisition);
        assert!(first.visual_ablated < first.demonstrator_acquisition);
    }

    #[test]
    fn cultural_replacement_cohorts_transmit_and_accumulate_without_inheritance_channels() {
        let first = super::run_cultural_evolution_experiments(WorldSeed::new(78));
        let replay = super::run_cultural_evolution_experiments(WorldSeed::new(78));
        assert_eq!(first, replay);
        // Three founder replacements retain each village's physical expression only with contact.
        assert!(first.first_tradition[3] > first.social_isolation[3]);
        assert!(first.second_tradition[3] > first.social_isolation[3]);
        assert_ne!(first.first_profile, first.second_profile);
        // Removing genetic or environmental inheritance leaves the social result intact.
        assert!(first.ecology_reset_control[3] > first.genetic_inheritance_control[3]);
        assert!(first.first_tradition[3] > first.individual_rediscovery_control[3]);
        // Later composite expressions outperform the founder and pass a one-source bottleneck.
        assert!(first.cumulative_practice[3] > first.cumulative_practice[0]);
        assert!(first.bottleneck_practice > first.cumulative_practice[0]);
    }

    #[test]
    fn v2_water_contamination_requires_wounds_before_infection() {
        let settings = WorldSettings::new(4, 4, 1).unwrap();
        let mut world = World::with_settings_v2(WorldSeed::new(923), settings).unwrap();
        let index = 0;
        let cell = world.cell_index(world.positions[index]);
        world.water_contamination[cell] = 24;
        let id = world.snapshot().agents()[0].id;
        let intent = super::Intent {
            id,
            dx: 0,
            dy: 0,
            thrust: 0,
            turn: 0,
            jaw: 0,
            eat: false,
            eat_at: world.positions[index],
            reproduce: false,
            harvest_material: false,
            place_block: false,
            remove_block: false,
            signal: None,
            acoustic: 0,
            chemical: 0,
            visual: 0,
        };
        world.apply_v2_physiology_and_ecology(&[intent]);
        assert_eq!(world.infections[index], 0);
        world.tissues[index][2] = 80;
        world.detritus[cell] = world.detritus[cell].saturating_add(20);
        world.apply_v2_physiology_and_ecology(&[intent]);
        assert!(world.infections[index] > 0);
        assert_eq!(
            world.snapshot().agents()[0].physiology.unwrap().infection,
            world.infections[index]
        );
    }

    #[test]
    fn v2_development_is_seeded_and_asexual_birth_resets_lifetime_state() {
        let settings = WorldSettings::new(6, 6, 0).unwrap();
        let mut first = World::with_settings_v2(WorldSeed::new(921), settings).unwrap();
        let mut replay = World::with_settings_v2(WorldSeed::new(921), settings).unwrap();
        let parent = first.spawn_agent(GridPosition::new(2, 2)).unwrap();
        let replay_parent = replay.spawn_agent(GridPosition::new(2, 2)).unwrap();
        for (world, id) in [(&mut first, parent), (&mut replay, replay_parent)] {
            let index = id.slot() as usize;
            world.life_stages[index] = super::LifeStage::Adult;
            world.growth[index] = 100;
            world.reproductive_reserves[index] = super::V2_OFFSPRING_INVESTMENT;
            world.energies[index] = 40;
            world.predictive_brains[index].learned_bias[0] = 77;
            world.resolve_v2_asexual_reproduction();
        }
        assert_eq!(first.snapshot(), replay.snapshot());
        let child = first
            .snapshot()
            .agents()
            .iter()
            .find(|agent| agent.lineage_depth == 1)
            .expect("adult invested in one offspring")
            .id;
        let child_index = child.slot() as usize;
        assert_eq!(first.parent_of(child), Some(parent));
        assert_eq!(
            first.development_of(child).unwrap().stage,
            super::LifeStage::Embryo
        );
        assert_eq!(
            first.predictive_brains[child_index].learned_bias,
            [0; super::V2_BRAIN_NEURONS]
        );
        assert!(
            first.developmental_genomes[child_index]
                .loci
                .iter()
                .all(|locus| *locus <= 64)
        );
    }

    #[test]
    fn v2_founder_feeding_budget_is_positive_and_toxin_is_detoxifiable() {
        let settings = WorldSettings::new(4, 4, 1).unwrap();
        let mut world = World::with_settings_v2(WorldSeed::new(922), settings).unwrap();
        let index = 0;
        world.tick = Tick::new(1);
        world.growth[index] = 0;
        let cell = (0..world.cell_count())
            .find(|cell| {
                !super::derive(world.seed.get(), 0, *cell as u64, 0x464F_4F44).is_multiple_of(5)
            })
            .expect("fixture has a non-toxic plant cell");
        world.positions[index] = super::GridPosition::new(
            u16::try_from(cell % usize::from(settings.width())).unwrap(),
            u16::try_from(cell / usize::from(settings.width())).unwrap(),
        );
        world.physics_positions[index] = super::Position {
            x: super::Fixed::from_units(i32::from(world.positions[index].x())),
            y: super::Fixed::from_units(i32::from(world.positions[index].y())),
        };
        let plants_before = world.plants[cell];
        world.energies[index] = 30;
        world.mouth_open[index] = super::Fixed::from_raw(super::FIXED_SCALE);
        let intent = super::Intent {
            id: world.snapshot().agents()[0].id,
            dx: 0,
            dy: 0,
            thrust: 0,
            turn: 0,
            jaw: 256,
            eat: false,
            eat_at: world.positions[index],
            reproduce: false,
            harvest_material: false,
            place_block: false,
            remove_block: false,
            signal: None,
            acoustic: 0,
            chemical: 0,
            visual: 0,
        };

        world.apply_v2_physiology_and_ecology(&[intent]);

        // An eight-unit mouthful pays two mandatory base/sensor units and one unit
        // of nutrition-dependent growth, leaving a +5 founder energy budget.
        assert_eq!(world.energies[index], 35);
        assert_eq!(world.guts[index], 0);
        // Per-tick nutrient turnover regrows six units before the eight-unit bite.
        assert_eq!(world.plants[cell], plants_before - 2);
        assert_eq!(world.toxins[index], 0);

        let toxic_cell = (0..world.cell_count())
            .find(|candidate| {
                super::derive(world.seed.get(), 0, *candidate as u64, 0x464F_4F44).is_multiple_of(5)
            })
            .expect("fixture has a toxic plant cell");
        world.positions[index] = super::GridPosition::new(
            u16::try_from(toxic_cell % usize::from(settings.width())).unwrap(),
            u16::try_from(toxic_cell / usize::from(settings.width())).unwrap(),
        );
        world.physics_positions[index] = super::Position {
            x: super::Fixed::from_units(i32::from(world.positions[index].x())),
            y: super::Fixed::from_units(i32::from(world.positions[index].y())),
        };
        let toxic_intent = super::Intent {
            eat_at: world.positions[index],
            ..intent
        };
        world.apply_v2_physiology_and_ecology(&[toxic_intent]);

        // The delayed toxic signal retains one unit after ordinary clearance rather
        // than imposing an immediate feeding pause or fabricating an energy penalty.
        assert_eq!(world.energies[index], 40);
        assert_eq!(world.toxins[index], 1);
    }

    #[test]
    fn v2_ancestral_replicates_reproduce_and_mutation_increases_diversity() {
        const REPLICATES: u64 = 8;
        const TICKS: u32 = 1_200;
        let settings = WorldSettings::new(20, 20, 24).unwrap();
        let mut mutated_diversity = 0_u64;
        let mut control_diversity = 0_u64;
        let mut mutated_births = 0_u64;
        let mut control_births = 0_u64;
        let mut mutated_surviving_descendants = 0_u64;
        let mut control_surviving_descendants = 0_u64;

        for seed in 0..REPLICATES {
            for mutation_enabled in [true, false] {
                let mut world =
                    World::with_settings_v2(WorldSeed::new(50_000 + seed), settings).unwrap();
                world.set_transmission_controls(super::CulturalTransmissionControls {
                    genetic_mutation_enabled: mutation_enabled,
                    ..super::CulturalTransmissionControls::default()
                });
                for _ in 0..TICKS {
                    let _ = world.step();
                }
                let metrics = world.metrics();
                if mutation_enabled {
                    mutated_births += metrics.births;
                    mutated_diversity += u64::from(metrics.diversity_ppm);
                    mutated_surviving_descendants += world
                        .snapshot()
                        .agents()
                        .iter()
                        .filter(|agent| agent.lineage_depth > 0)
                        .count() as u64;
                } else {
                    control_births += metrics.births;
                    control_diversity += u64::from(metrics.diversity_ppm);
                    control_surviving_descendants += world
                        .snapshot()
                        .agents()
                        .iter()
                        .filter(|agent| agent.lineage_depth > 0)
                        .count() as u64;
                }
            }
        }

        assert!(mutated_births > 0 && control_births > 0);
        assert!(mutated_surviving_descendants > 0 && control_surviving_descendants > 0);
        assert!(mutated_diversity > control_diversity);
    }
}
