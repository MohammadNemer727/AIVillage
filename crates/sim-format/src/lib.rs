//! Versioned data contracts shared by the desktop application and headless tools.
//!
//! Format types remain independent of Bevy so saved experiments can be replayed without a window.
//!
//! # V1 freeze
//!
//! Schema version `1` is the frozen baseline for the current grid/dot simulator. Golden replay
//! fixtures under `tests/fixtures/` pin `seed → tick → state_hash` for this schema. Do not bump
//! these constants without regenerating the fixtures and documenting a migration.

#![forbid(unsafe_code)]

use std::{collections::BTreeMap, error::Error, fmt};

use serde::{Deserialize, Serialize};
use sim_core::{
    CheckpointError, DevelopmentalGenome, World, WorldCheckpoint,
    runtime::{AgentRuntime, AgentRuntimeState},
};

/// Frozen V1 configuration / interchange schema version.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

/// Frozen V1 snapshot header schema version.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Version for durable exact-world checkpoint envelopes.
pub const CHECKPOINT_SCHEMA_VERSION: u32 = 1;
/// Version for standalone one-agent runtime checkpoints. Version 4 adds private bounded episodic
/// memory; versions 1–3 migrate to the canonical empty newborn memory.
pub const AGENT_CHECKPOINT_SCHEMA_VERSION: u32 = 4;
const FIRST_AGENT_CHECKPOINT_SCHEMA_VERSION: u32 = 1;
/// Version for a live Luanti population manifest. Version 2 retains the exact pending
/// reproduction signal and explicit attempt snapshots; version 1 loads with no pending decision.
pub const POPULATION_MANIFEST_SCHEMA_VERSION: u32 = 2;
const LEGACY_POPULATION_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Version for append-only replay event logs.
pub const EVENT_LOG_SCHEMA_VERSION: u32 = 1;

/// Human-readable marker for the frozen V1 simulator baseline.
pub const V1_FREEZE_LABEL: &str = "v1";

/// Deterministic inherited specification for one standalone founder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FounderSpec {
    /// Run seed; keys the founder's genome and its receptor noise.
    pub seed: u64,
    /// Stable agent ID; the founder ordinal for the genome and a receptor-noise key.
    pub agent_id: u64,
    /// The program a seeded V2 world would give the founder with this ordinal.
    pub genome: DevelopmentalGenome,
}

impl FounderSpec {
    /// Derives the founder program for `agent_id` in a run seeded with `seed`.
    #[must_use]
    pub fn new(seed: u64, agent_id: u64) -> Self {
        Self {
            seed,
            agent_id,
            genome: DevelopmentalGenome::founder(seed, agent_id),
        }
    }

    /// Constructs a fresh standalone runtime with a blank lifetime brain.
    #[must_use]
    pub fn runtime(&self) -> AgentRuntime {
        AgentRuntime::new(self.seed, self.agent_id, self.genome.default_brain_config())
    }
}

/// Versioned checkpoint for one standalone runtime, deliberately separate from V1 worlds.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentCheckpoint {
    /// Envelope schema version.
    pub schema_version: u32,
    /// Run seed that keys receptor noise.
    pub seed: u64,
    /// Stable agent ID that keys receptor noise.
    pub agent_id: u64,
    /// Every mutable runtime field needed to resume exact action selection.
    pub runtime: AgentRuntimeState,
}

/// Durable identity and lineage metadata for one live individual.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PopulationIndividual {
    /// Evolving individual identity; unlike a body slot, this is never reused.
    pub individual_id: u64,
    /// Luanti body slot currently occupied by this individual.
    pub body_slot: u32,
    /// Asexual parent, absent only for founders.
    pub parent_id: Option<u64>,
    /// Founder generation is zero; every child is one greater than its parent.
    pub generation: u32,
    /// Inherited developmental program, excluding all acquired lifetime state.
    pub genome: DevelopmentalGenome,
    /// Relative path to this individual's exact runtime checkpoint.
    pub runtime_file: String,
    /// Durable reproductive reserve after all charged birth costs.
    #[serde(default = "full_reserve")]
    pub reproductive_reserve: u16,
    /// Next observation sequence accepted after body or server restart.
    #[serde(default)]
    pub next_sequence: u64,
    /// Most recent independent reproduction output, retained until consumed or replaced.
    #[serde(default)]
    pub reproduction_signal: i16,
}

const fn full_reserve() -> u16 {
    255
}

/// Why a population individual was born.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PopulationBirthKind {
    /// Capacity and parent state permitted ordinary asexual reproduction.
    Asexual,
}

/// Durable population lifecycle audit record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PopulationLifecycleEvent {
    /// A body slot was released.
    Death {
        /// Durable idempotency key supplied by the body.
        event_id: String,
        /// Individual that died.
        individual_id: u64,
        /// Slot made available.
        body_slot: u32,
    },
    /// A newborn received a fresh runtime.
    Birth {
        /// Durable idempotency key supplied by the living parent body.
        event_id: String,
        /// Never-reused newborn identity.
        individual_id: u64,
        /// Reusable slot assigned to the newborn.
        body_slot: u32,
        /// Asexual parent.
        parent_id: u64,
        /// Newborn lineage generation.
        generation: u32,
        /// Reproduction mechanism.
        kind: PopulationBirthKind,
        /// Explicit reserve charged atomically with the birth.
        reproductive_cost: u16,
    },
    /// A queued lifecycle request and its eligibility snapshot before resolution.
    ReproductionAttempt {
        /// Durable idempotency key supplied by the body.
        event_id: String,
        /// Living individual requesting reproduction.
        parent_id: u64,
        /// Body slot claimed by the request.
        body_slot: u32,
        /// Decision counter at lifecycle evaluation.
        decision_tick: u64,
        /// Independent controller output retained from the triggering transition.
        signal_0: i16,
        /// Configured maturity threshold.
        maturity_decisions: u64,
        /// Reserve before any successful cost.
        reproductive_reserve: u16,
        /// Whether a body slot was free before resolution.
        slot_available: bool,
    },
    /// A reproduction attempt was denied with an auditable reason.
    ReproductionBlocked {
        /// Durable idempotency key supplied by the body.
        event_id: String,
        /// Individual whose reproduction request was denied.
        parent_id: u64,
        /// Configured number of body slots.
        capacity: u32,
        /// Why the attempt could not become a birth.
        reason: PopulationBlockReason,
    },
}

/// Stable categories for denied reproductive attempts.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PopulationBlockReason {
    /// No reusable body slot was empty.
    NoFreeSlot,
    /// The parent was dead, immature, depleted, or did not decide to reproduce.
    BiologicalEligibility,
    /// The body could not deliver or complete the lifecycle request.
    Infrastructure,
}

/// Durable result returned for an idempotent lifecycle command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum PopulationCommandResult {
    /// Death was committed and the slot became empty.
    Died {
        /// Closed individual identity.
        individual_id: u64,
        /// Newly empty reusable slot.
        body_slot: u32,
    },
    /// Birth, lineage, cost, and slot assignment were committed together.
    Born {
        /// Never-reused newborn identity.
        individual_id: u64,
        /// Living parent charged for this birth.
        parent_id: u64,
        /// Slot atomically assigned to the child.
        body_slot: u32,
        /// Child lineage generation.
        generation: u32,
        /// Parent reserve after the explicit cost.
        reserve_after: u16,
    },
    /// The command was accepted and durably recorded but no birth occurred.
    Blocked {
        /// Stable denial category.
        reason: PopulationBlockReason,
    },
}

/// Versioned index of independently checkpointed live individuals.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PopulationManifest {
    /// Envelope schema version.
    pub schema_version: u32,
    /// Population seed used for founders, mutation, and runtime receptor noise.
    pub seed: u64,
    /// Next identity to issue; IDs below this value are never reused.
    pub next_individual_id: u64,
    /// Maximum simultaneously occupied body slots.
    pub capacity: u32,
    /// Whether deterministic mutation is enabled for natural births.
    #[serde(default = "mutation_enabled")]
    pub mutation_enabled: bool,
    /// Minimum accepted decisions before a parent is mature.
    #[serde(default)]
    pub maturity_decisions: u64,
    /// Reserve required before paying the reproductive cost.
    #[serde(default)]
    pub minimum_reproductive_reserve: u16,
    /// Reserve removed in the same transaction as a successful birth.
    #[serde(default)]
    pub reproductive_cost: u16,
    /// Live individuals in canonical identity order.
    pub individuals: Vec<PopulationIndividual>,
    /// Ordered lifecycle audit, including denied births.
    #[serde(default)]
    pub lifecycle: Vec<PopulationLifecycleEvent>,
    /// Completed command results keyed by durable event ID for exactly-once retries.
    #[serde(default)]
    pub processed_events: BTreeMap<String, PopulationCommandResult>,
}

const fn mutation_enabled() -> bool {
    true
}

impl PopulationManifest {
    /// Rejects manifests whose format cannot be interpreted by this build.
    ///
    /// # Errors
    ///
    /// Returns an unsupported-version error for manifests newer than this build.
    pub fn validate_version(&self) -> Result<(), PersistenceError> {
        if !matches!(
            self.schema_version,
            LEGACY_POPULATION_MANIFEST_SCHEMA_VERSION | POPULATION_MANIFEST_SCHEMA_VERSION
        ) {
            return Err(PersistenceError::UnsupportedPopulationManifestVersion(
                self.schema_version,
            ));
        }
        Ok(())
    }
}

impl AgentCheckpoint {
    /// Captures an exact standalone runtime checkpoint in the current schema.
    #[must_use]
    pub fn current(runtime: &AgentRuntime) -> Self {
        Self {
            schema_version: AGENT_CHECKPOINT_SCHEMA_VERSION,
            seed: runtime.seed(),
            agent_id: runtime.agent_id(),
            runtime: runtime.state().clone(),
        }
    }

    /// Restores a standalone runtime.
    ///
    /// # Errors
    ///
    /// Returns an error when the agent checkpoint schema is unsupported.
    pub fn restore(self) -> Result<AgentRuntime, PersistenceError> {
        if !(FIRST_AGENT_CHECKPOINT_SCHEMA_VERSION..=AGENT_CHECKPOINT_SCHEMA_VERSION)
            .contains(&self.schema_version)
        {
            return Err(PersistenceError::UnsupportedAgentCheckpointVersion(
                self.schema_version,
            ));
        }
        Ok(AgentRuntime::from_state(
            self.seed,
            self.agent_id,
            self.runtime,
        ))
    }

    /// Encodes this checkpoint as TOML.
    ///
    /// # Errors
    ///
    /// Returns an error if the value cannot be represented as TOML.
    pub fn to_toml(&self) -> Result<String, PersistenceError> {
        toml::to_string(self).map_err(PersistenceError::Encode)
    }

    /// Decodes a checkpoint from TOML, rejecting malformed brain state.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid TOML, out-of-range state, or unsupported format versions.
    pub fn from_toml(text: &str) -> Result<Self, PersistenceError> {
        let checkpoint: Self = toml::from_str(text).map_err(PersistenceError::Decode)?;
        if !(FIRST_AGENT_CHECKPOINT_SCHEMA_VERSION..=AGENT_CHECKPOINT_SCHEMA_VERSION)
            .contains(&checkpoint.schema_version)
        {
            return Err(PersistenceError::UnsupportedAgentCheckpointVersion(
                checkpoint.schema_version,
            ));
        }
        Ok(checkpoint)
    }
}

/// Reproducible channel-removal controls for a transmission experiment.
///
/// A result from this configuration must be described as evidence from controlled transmission
/// channels, not as culture, unless the experiment separately demonstrates the required effect.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Each serialized flag independently controls one experimental channel."
)]
pub struct TransmissionExperimentConfig {
    /// Allow inherited genome mutation.
    pub genetic_mutation_enabled: bool,
    /// Allow bounded, lifetime-only learning updates.
    pub plasticity_enabled: bool,
    /// Allow blank signal emission and observation.
    pub signals_enabled: bool,
    /// Allow persistent environmental artifacts.
    pub artifacts_enabled: bool,
    /// Allow V2 physical acoustic, chemical, and visual social outputs/receptors.
    pub physical_social_signals_enabled: bool,
}

impl Default for TransmissionExperimentConfig {
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

/// Reproducible top-level configuration for a world run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldConfig {
    /// Schema version used to decode this configuration.
    pub schema_version: u32,
    /// User-provided seed used to derive deterministic random streams.
    pub seed: u64,
    /// Intended authoritative ticks per second.
    pub ticks_per_second: u16,
    /// Channel controls recorded with the run for replayable experiments.
    #[serde(default)]
    pub transmission: TransmissionExperimentConfig,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            seed: 0,
            ticks_per_second: 20,
            transmission: TransmissionExperimentConfig::default(),
        }
    }
}

/// Identifies the snapshot schema without prescribing its future payload.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SnapshotHeader {
    /// Schema version used to encode the snapshot.
    pub schema_version: u32,
    /// Authoritative tick represented by the snapshot.
    pub tick: u64,
}

impl SnapshotHeader {
    /// Creates a V1 snapshot header for the given tick.
    #[must_use]
    pub const fn v1(tick: u64) -> Self {
        Self {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            tick,
        }
    }
}

/// A versioned, exact checkpoint that can resume a deterministic world history.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CheckpointFile {
    /// Format version for this envelope.
    pub schema_version: u32,
    /// Tick and format identity for quick inspection without decoding the payload.
    pub header: SnapshotHeader,
    /// Canonical state hash at the saved tick.
    pub state_hash: u64,
    /// Every future-relevant authoritative world field.
    pub world: WorldCheckpoint,
}

impl CheckpointFile {
    /// Captures a resumable version-one checkpoint from a world.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError::Checkpoint`] for worlds the format cannot represent, such as
    /// V2 worlds.
    pub fn v1(world: &World) -> Result<Self, PersistenceError> {
        let snapshot = world.snapshot();
        Ok(Self {
            schema_version: CHECKPOINT_SCHEMA_VERSION,
            header: SnapshotHeader::v1(snapshot.tick().get()),
            state_hash: snapshot.state_hash(),
            world: world.checkpoint().map_err(PersistenceError::Checkpoint)?,
        })
    }

    /// Validates and restores an exact world history.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported versions, malformed state, or a mismatched canonical hash.
    pub fn restore(self) -> Result<World, PersistenceError> {
        if self.schema_version != CHECKPOINT_SCHEMA_VERSION {
            return Err(PersistenceError::UnsupportedCheckpointVersion(
                self.schema_version,
            ));
        }
        if self.header.schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(PersistenceError::UnsupportedSnapshotVersion(
                self.header.schema_version,
            ));
        }
        let world = World::from_checkpoint(self.world).map_err(PersistenceError::Checkpoint)?;
        let snapshot = world.snapshot();
        if snapshot.tick().get() != self.header.tick || snapshot.state_hash() != self.state_hash {
            return Err(PersistenceError::IntegrityMismatch);
        }
        Ok(world)
    }

    /// Encodes this portable checkpoint as TOML.
    ///
    /// # Errors
    ///
    /// Returns an error if the value cannot be represented as TOML.
    pub fn to_toml(&self) -> Result<String, PersistenceError> {
        toml::to_string(self).map_err(PersistenceError::Encode)
    }

    /// Decodes a portable checkpoint from TOML.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid TOML or unsupported format versions.
    pub fn from_toml(text: &str) -> Result<Self, PersistenceError> {
        let checkpoint: Self = toml::from_str(text).map_err(PersistenceError::Decode)?;
        if checkpoint.schema_version != CHECKPOINT_SCHEMA_VERSION {
            return Err(PersistenceError::UnsupportedCheckpointVersion(
                checkpoint.schema_version,
            ));
        }
        Ok(checkpoint)
    }
}

/// One observable boundary in an append-only deterministic replay log.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReplayEvent {
    /// Authoritative tick at which this event was recorded.
    pub tick: u64,
    /// Canonical state hash after that tick.
    pub state_hash: u64,
}

/// Versioned append-only state-hash checkpoints for replay verification.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventLog {
    /// Format version for this log.
    pub schema_version: u32,
    /// Ordered events; ticks must increase strictly.
    pub events: Vec<ReplayEvent>,
}

impl Default for EventLog {
    fn default() -> Self {
        Self::v1()
    }
}

impl EventLog {
    /// Creates an empty V1 event log.
    #[must_use]
    pub const fn v1() -> Self {
        Self {
            schema_version: EVENT_LOG_SCHEMA_VERSION,
            events: Vec::new(),
        }
    }

    /// Records a snapshot, rejecting duplicate or out-of-order ticks.
    ///
    /// # Errors
    ///
    /// Returns an error when the event ordering is not append-only.
    pub fn record(&mut self, world: &World) -> Result<(), PersistenceError> {
        if self.schema_version != EVENT_LOG_SCHEMA_VERSION {
            return Err(PersistenceError::UnsupportedEventLogVersion(
                self.schema_version,
            ));
        }
        let snapshot = world.snapshot();
        let event = ReplayEvent {
            tick: snapshot.tick().get(),
            state_hash: snapshot.state_hash(),
        };
        if self
            .events
            .last()
            .is_some_and(|previous| previous.tick >= event.tick)
        {
            return Err(PersistenceError::NonMonotonicEvent);
        }
        self.events.push(event);
        Ok(())
    }

    /// Verifies that a world matches the log entry at its current tick.
    #[must_use]
    pub fn matches(&self, world: &World) -> bool {
        let snapshot = world.snapshot();
        self.events.iter().any(|event| {
            event.tick == snapshot.tick().get() && event.state_hash == snapshot.state_hash()
        })
    }
}

/// Errors produced by durable simulation formats.
#[derive(Debug)]
pub enum PersistenceError {
    /// The checkpoint envelope version is not supported.
    UnsupportedCheckpointVersion(u32),
    /// The standalone agent checkpoint envelope version is not supported.
    UnsupportedAgentCheckpointVersion(u32),
    /// The population-manifest envelope version is not supported.
    UnsupportedPopulationManifestVersion(u32),
    /// The snapshot-header version is not supported.
    UnsupportedSnapshotVersion(u32),
    /// The event-log version is not supported.
    UnsupportedEventLogVersion(u32),
    /// The decoded state violates core invariants.
    Checkpoint(CheckpointError),
    /// The restored state does not match its recorded tick or canonical hash.
    IntegrityMismatch,
    /// Events were not appended in strictly increasing tick order.
    NonMonotonicEvent,
    /// Serialization failed.
    Encode(toml::ser::Error),
    /// Deserialization failed.
    Decode(toml::de::Error),
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCheckpointVersion(version) => {
                write!(formatter, "unsupported checkpoint schema version {version}")
            }
            Self::UnsupportedAgentCheckpointVersion(version) => {
                write!(
                    formatter,
                    "unsupported agent checkpoint schema version {version}"
                )
            }
            Self::UnsupportedPopulationManifestVersion(version) => {
                write!(
                    formatter,
                    "unsupported population manifest schema version {version}"
                )
            }
            Self::UnsupportedSnapshotVersion(version) => {
                write!(formatter, "unsupported snapshot schema version {version}")
            }
            Self::UnsupportedEventLogVersion(version) => {
                write!(formatter, "unsupported event-log schema version {version}")
            }
            Self::Checkpoint(error) => write!(formatter, "invalid checkpoint: {error}"),
            Self::IntegrityMismatch => formatter.write_str("checkpoint integrity mismatch"),
            Self::NonMonotonicEvent => formatter.write_str("event log is not append-only"),
            Self::Encode(error) => write!(formatter, "encode checkpoint: {error}"),
            Self::Decode(error) => write!(formatter, "decode checkpoint: {error}"),
        }
    }
}

impl Error for PersistenceError {}

#[cfg(test)]
mod tests {
    use sim_core::{World, WorldSeed, WorldSettings, runtime::AFFORDANCE_ACTIONS};

    use super::{AgentCheckpoint, CheckpointFile, EventLog, FounderSpec, PersistenceError};

    #[test]
    fn checkpoint_round_trip_resumes_identical_history() {
        let settings = WorldSettings::new(12, 10, 16).unwrap();
        let mut uninterrupted = World::with_settings(WorldSeed::new(77), settings).unwrap();
        for _ in 0..500 {
            let _ = uninterrupted.step();
        }
        let encoded = CheckpointFile::v1(&uninterrupted)
            .unwrap()
            .to_toml()
            .unwrap();
        let mut resumed = CheckpointFile::from_toml(&encoded)
            .unwrap()
            .restore()
            .unwrap();
        assert_eq!(uninterrupted.snapshot(), resumed.snapshot());

        for _ in 0..500 {
            assert_eq!(uninterrupted.step(), resumed.step());
        }
    }

    #[test]
    fn event_log_is_ordered_and_matches_recorded_snapshot() {
        let mut world = World::new(WorldSeed::new(8));
        let mut log = EventLog::v1();
        assert!(log.record(&world).is_ok());
        assert!(log.matches(&world));
        assert!(log.record(&world).is_err());
        let _ = world.step();
        assert!(log.record(&world).is_ok());
        assert!(log.matches(&world));
    }

    #[test]
    fn founder_spec_matches_seeded_world_founders() {
        let world = World::new_v2(WorldSeed::new(32));
        let snapshot = world.snapshot();
        assert_ne!(snapshot.agents(), []);
        for agent in snapshot.agents() {
            let founder = FounderSpec::new(32, u64::from(agent.id.slot()));
            assert_eq!(
                Some(&founder.genome),
                world.developmental_genome_of(agent.id)
            );
        }
    }

    #[test]
    fn agent_checkpoint_rejects_unsupported_versions() {
        let runtime = FounderSpec::new(32, 0).runtime();
        let mut checkpoint = AgentCheckpoint::current(&runtime);
        checkpoint.schema_version += 1;
        let encoded = checkpoint.to_toml().unwrap();
        assert!(matches!(
            AgentCheckpoint::from_toml(&encoded),
            Err(PersistenceError::UnsupportedAgentCheckpointVersion(_))
        ));
        assert!(checkpoint.restore().is_err());
    }

    #[test]
    fn version_one_agent_checkpoint_migrates_twelve_action_tables() {
        let runtime = FounderSpec::new(32, 0).runtime();
        let mut wire = serde_json::to_value(AgentCheckpoint::current(&runtime)).unwrap();
        wire["schema_version"] = 1.into();
        for field in ["visits", "after", "after_spread", "last_tried"] {
            wire["runtime"]["affordance"][field]
                .as_array_mut()
                .unwrap()
                .truncate(12);
        }
        let checkpoint: AgentCheckpoint = serde_json::from_value(wire).unwrap();
        let restored = checkpoint.restore().unwrap();
        assert_eq!(restored.state().affordance.visits.len(), AFFORDANCE_ACTIONS);
        assert!(
            restored.state().affordance.visits[12..]
                .iter()
                .all(|count| *count == 0)
        );
    }

    #[test]
    fn version_three_checkpoint_migrates_to_empty_episodic_memory() {
        let runtime = FounderSpec::new(32, 0).runtime();
        let mut wire = serde_json::to_value(AgentCheckpoint::current(&runtime)).unwrap();
        wire["schema_version"] = 3.into();
        wire["runtime"]
            .as_object_mut()
            .unwrap()
            .remove("episodic_memory");
        let checkpoint: AgentCheckpoint = serde_json::from_value(wire).unwrap();
        let restored = checkpoint.restore().unwrap();
        assert_eq!(restored.state().episodic_memory.events.len(), 0);
        assert_eq!(restored.state().episodic_memory.next_event_id, 0);
    }
}
