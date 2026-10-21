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

use std::{error::Error, fmt};

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
/// Version for standalone one-agent runtime checkpoints.
pub const AGENT_CHECKPOINT_SCHEMA_VERSION: u32 = 1;

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

impl AgentCheckpoint {
    /// Captures an exact standalone runtime checkpoint.
    #[must_use]
    pub fn v1(runtime: &AgentRuntime) -> Self {
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
        if self.schema_version != AGENT_CHECKPOINT_SCHEMA_VERSION {
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
        if checkpoint.schema_version != AGENT_CHECKPOINT_SCHEMA_VERSION {
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
    use sim_core::{World, WorldSeed, WorldSettings};

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
        let mut checkpoint = AgentCheckpoint::v1(&runtime);
        checkpoint.schema_version += 1;
        let encoded = checkpoint.to_toml().unwrap();
        assert!(matches!(
            AgentCheckpoint::from_toml(&encoded),
            Err(PersistenceError::UnsupportedAgentCheckpointVersion(_))
        ));
        assert!(checkpoint.restore().is_err());
    }
}
