//! Population ownership, lifecycle, and independently durable runtime state.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sim_core::{
    DevelopmentalGenome,
    runtime::{AffordanceConfig, AgentRuntime, ExperimentMode},
};
use sim_format::{
    AgentCheckpoint, FounderSpec, POPULATION_MANIFEST_SCHEMA_VERSION, PopulationBlockReason,
    PopulationCommandResult, PopulationIndividual, PopulationManifest,
};

use crate::{
    BridgeError, BridgeRequest, BridgeResponse, ReproductionTransitionAudit, TraceRecord,
    response_for,
};

pub use sim_format::{
    PopulationBirthKind as BirthKind, PopulationLifecycleEvent as LifecycleEvent,
};

/// Founder ordinal whose genome develops in `slot` under a replicate's genome rotation.
#[must_use]
pub const fn founder_ordinal(slot: u32, founder_count: u32, genome_rotation: u32) -> u32 {
    (slot + genome_rotation % founder_count) % founder_count
}

/// Runtime eligibility and inheritance settings for natural reproduction.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LifecycleConfig {
    /// Decisions required before reproduction.
    pub maturity_decisions: u64,
    /// Reserve required before reproduction.
    pub minimum_reproductive_reserve: u16,
    /// Reserve charged atomically with a successful birth.
    pub reproductive_cost: u16,
    /// Enables deterministic mutation; false gives exact inheritance.
    pub mutation_enabled: bool,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            maturity_decisions: 100,
            minimum_reproductive_reserve: 160,
            reproductive_cost: 80,
            mutation_enabled: true,
        }
    }
}

/// Idempotent command sent by a physical body to the lifecycle authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LifecycleCommand {
    /// Durable event ID retained across retries and process crashes.
    pub event_id: String,
    /// Requested lifecycle operation.
    #[serde(flatten)]
    pub operation: LifecycleOperation,
}

/// Operations accepted by the lifecycle endpoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum LifecycleOperation {
    /// Close one life and free its slot. This never implies reproduction.
    Death {
        /// Individual reported dead by its body.
        individual_id: u64,
        /// Slot occupied at death.
        body_slot: u32,
    },
    /// Attempt reproduction from a living parent's explicit policy decision.
    Reproduce {
        /// Living parent making the decision.
        parent_id: u64,
        /// Parent's current body slot.
        body_slot: u32,
        /// Body-side delivery failures accumulated before this command reached Rust.
        #[serde(default)]
        infrastructure_failures: u32,
    },
    /// Record a body-side attempt that could not reach lifecycle infrastructure.
    InfrastructureBlocked {
        /// Parent whose attempt encountered infrastructure failure.
        parent_id: u64,
    },
}

/// Reusable Luanti embodiment identity, deliberately separate from an evolving individual.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct BodySlotId(pub u32);

/// Lifecycle operation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleError {
    /// No live individual has this identity.
    UnknownIndividual(u64),
    /// Every configured body slot is occupied.
    CapacityBlocked,
    /// The monotonically increasing identity counter overflowed.
    IdentityExhausted,
}

impl core::fmt::Display for LifecycleError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownIndividual(id) => write!(formatter, "unknown individual {id}"),
            Self::CapacityBlocked => formatter.write_str("population capacity is full"),
            Self::IdentityExhausted => formatter.write_str("individual identity space exhausted"),
        }
    }
}

impl Error for LifecycleError {}

#[derive(Clone, Debug)]
struct Individual {
    body_slot: BodySlotId,
    parent_id: Option<u64>,
    generation: u32,
    genome: DevelopmentalGenome,
    runtime: AgentRuntime,
    trace: Vec<TraceRecord>,
    reproductive_reserve: u16,
    reproduction_signal: i16,
}

/// Population-aware bridge coordinator keyed by durable individual identity.
#[derive(Clone, Debug)]
pub struct PopulationCoordinator {
    seed: u64,
    capacity: u32,
    next_individual_id: u64,
    individuals: BTreeMap<u64, Individual>,
    trace: Vec<TraceRecord>,
    lifecycle: Vec<LifecycleEvent>,
    lifecycle_config: LifecycleConfig,
    processed_events: BTreeMap<String, PopulationCommandResult>,
    storage_directory: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize)]
struct LifecycleWal {
    event_id: String,
    result: PopulationCommandResult,
    manifest: PopulationManifest,
    checkpoints: Vec<AgentCheckpoint>,
}

impl PopulationCoordinator {
    /// Applies independent episodic ablations to every live private runtime.
    pub fn set_episodic_config(&mut self, config: sim_core::runtime::EpisodicConfig) {
        for individual in self.individuals.values_mut() {
            individual.runtime.set_episodic_config(config);
        }
    }

    /// Creates a one-founder population while preserving the original bridge constructor.
    #[must_use]
    pub fn new(seed: u64, individual_id: u64) -> Self {
        Self::new_with_mode(seed, individual_id, ExperimentMode::PredictiveBaseline)
    }

    /// Creates a one-founder population with an explicit policy.
    #[must_use]
    pub fn new_with_mode(seed: u64, individual_id: u64, mode: ExperimentMode) -> Self {
        let founder = FounderSpec::new(seed, individual_id);
        let runtime = AgentRuntime::new_with_mode(
            seed,
            individual_id,
            founder.genome.default_brain_config(),
            mode,
        );
        Self::from_founder(seed, individual_id, founder.genome, runtime)
    }

    /// Creates an initial cohort whose identities and body slots are deliberately distinct.
    ///
    /// Founder identities are consecutive from `first_individual_id`; slots are zero-based.
    /// Capacity may exceed the initial cohort to leave room for births.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty cohort, insufficient capacity, or identity overflow.
    pub fn new_population_with_mode(
        seed: u64,
        first_individual_id: u64,
        founder_count: u32,
        capacity: u32,
        mode: ExperimentMode,
    ) -> Result<Self, &'static str> {
        Self::new_rotated_population_with_mode(
            seed,
            first_individual_id,
            founder_count,
            capacity,
            mode,
            0,
        )
    }

    /// Creates a cohort in which slot `s` develops from the genome of founder ordinal
    /// `(s + genome_rotation) % founder_count`.
    ///
    /// Identities and body slots stay fixed, so rotating across replicate populations separates
    /// a founder genotype from its body slot and from identity-keyed receptor noise.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty cohort, insufficient capacity, or identity overflow.
    pub fn new_rotated_population_with_mode(
        seed: u64,
        first_individual_id: u64,
        founder_count: u32,
        capacity: u32,
        mode: ExperimentMode,
        genome_rotation: u32,
    ) -> Result<Self, &'static str> {
        Self::new_population(
            seed,
            first_individual_id,
            founder_count,
            capacity,
            genome_rotation,
            |id, genome| AgentRuntime::new_with_mode(seed, id, genome.default_brain_config(), mode),
        )
    }

    /// Creates a one-founder affordance population with custom learner settings.
    ///
    /// # Errors
    ///
    /// Returns a description when an affordance setting is out of range.
    pub fn new_with_affordance(
        seed: u64,
        individual_id: u64,
        affordance: AffordanceConfig,
    ) -> Result<Self, String> {
        let founder = FounderSpec::new(seed, individual_id);
        let runtime = AgentRuntime::new_with_affordance(
            seed,
            individual_id,
            founder.genome.default_brain_config(),
            affordance,
        )?;
        Ok(Self::from_founder(
            seed,
            individual_id,
            founder.genome,
            runtime,
        ))
    }

    /// Creates an initial affordance-learning cohort under one coordinator.
    ///
    /// # Errors
    ///
    /// Returns a description when cohort bounds or learner settings are invalid.
    ///
    /// # Panics
    ///
    /// Panics only if the already validated learner settings are rejected inconsistently while
    /// constructing one of the otherwise identical founder runtimes.
    pub fn new_population_with_affordance(
        seed: u64,
        first_individual_id: u64,
        founder_count: u32,
        capacity: u32,
        genome_rotation: u32,
        affordance: AffordanceConfig,
    ) -> Result<Self, String> {
        affordance.validate()?;
        Self::new_population(
            seed,
            first_individual_id,
            founder_count,
            capacity,
            genome_rotation,
            |id, genome| {
                AgentRuntime::new_with_affordance(
                    seed,
                    id,
                    genome.default_brain_config(),
                    affordance,
                )
                .expect("validated affordance settings remain valid")
            },
        )
        .map_err(str::to_owned)
    }

    fn new_population(
        seed: u64,
        first_individual_id: u64,
        founder_count: u32,
        capacity: u32,
        genome_rotation: u32,
        mut runtime_for: impl FnMut(u64, &DevelopmentalGenome) -> AgentRuntime,
    ) -> Result<Self, &'static str> {
        if founder_count == 0 {
            return Err("founder count must be non-zero");
        }
        if capacity < founder_count {
            return Err("population capacity cannot be smaller than the founder cohort");
        }
        let next_individual_id = first_individual_id
            .checked_add(u64::from(founder_count))
            .ok_or("founder identity range overflowed")?;
        let mut individuals = BTreeMap::new();
        for slot in 0..founder_count {
            let individual_id = first_individual_id + u64::from(slot);
            let ordinal = founder_ordinal(slot, founder_count, genome_rotation);
            let genome = FounderSpec::new(seed, first_individual_id + u64::from(ordinal)).genome;
            let runtime = runtime_for(individual_id, &genome);
            individuals.insert(
                individual_id,
                Individual {
                    body_slot: BodySlotId(slot),
                    parent_id: None,
                    generation: 0,
                    genome,
                    runtime,
                    trace: Vec::new(),
                    reproductive_reserve: 255,
                    reproduction_signal: 0,
                },
            );
        }
        Ok(Self {
            seed,
            capacity,
            next_individual_id,
            individuals,
            trace: Vec::new(),
            lifecycle: Vec::new(),
            lifecycle_config: LifecycleConfig::default(),
            processed_events: BTreeMap::new(),
            storage_directory: None,
        })
    }

    /// Continues a legacy one-founder checkpoint inside a population coordinator.
    #[must_use]
    pub fn from_runtime(runtime: AgentRuntime) -> Self {
        let seed = runtime.seed();
        let individual_id = runtime.agent_id();
        let genome = FounderSpec::new(seed, individual_id).genome;
        Self::from_founder(seed, individual_id, genome, runtime)
    }

    fn from_founder(
        seed: u64,
        individual_id: u64,
        genome: DevelopmentalGenome,
        runtime: AgentRuntime,
    ) -> Self {
        let individual = Individual {
            body_slot: BodySlotId(0),
            parent_id: None,
            generation: 0,
            genome,
            runtime,
            trace: Vec::new(),
            reproductive_reserve: 255,
            reproduction_signal: 0,
        };
        Self {
            seed,
            // One founder has one body: a birth would have no body to occupy.
            capacity: 1,
            next_individual_id: individual_id.saturating_add(1),
            individuals: BTreeMap::from([(individual_id, individual)]),
            trace: Vec::new(),
            lifecycle: Vec::new(),
            lifecycle_config: LifecycleConfig {
                maturity_decisions: 0,
                minimum_reproductive_reserve: 0,
                reproductive_cost: 0,
                mutation_enabled: true,
            },
            processed_events: BTreeMap::new(),
            storage_directory: None,
        }
    }

    /// Applies lifecycle eligibility and mutation settings.
    ///
    /// # Errors
    ///
    /// Rejects a cost greater than the required reserve.
    pub fn set_lifecycle_config(&mut self, config: LifecycleConfig) -> Result<(), &'static str> {
        if config.reproductive_cost > config.minimum_reproductive_reserve {
            return Err("reproductive cost cannot exceed minimum reserve");
        }
        self.lifecycle_config = config;
        Ok(())
    }

    /// Binds lifecycle commands to a durable population directory and writes the initial state.
    ///
    /// # Errors
    ///
    /// Propagates directory creation, serialization, and durable-write failures.
    pub fn bind_storage(&mut self, directory: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.storage_directory = Some(directory.to_path_buf());
        self.save(directory)
    }

    /// Limits simultaneous bodies. Existing occupants must fit.
    ///
    /// # Errors
    ///
    /// Returns an error for zero capacity or when an occupied slot would be excluded.
    pub fn set_capacity(&mut self, capacity: u32) -> Result<(), &'static str> {
        if capacity == 0
            || self
                .individuals
                .values()
                .any(|individual| individual.body_slot.0 >= capacity)
        {
            return Err("capacity does not contain all occupied body slots");
        }
        self.capacity = capacity;
        Ok(())
    }

    /// Restores compatibility with the old single-runtime accessor.
    ///
    /// # Panics
    ///
    /// Panics when called after every individual has died. Population-aware callers should use
    /// [`Self::runtime_for`].
    #[must_use]
    pub fn runtime(&self) -> &AgentRuntime {
        &self
            .individuals
            .first_key_value()
            .expect("coordinator always has a founder when using runtime()")
            .1
            .runtime
    }

    /// Returns one individual's independently evolving runtime.
    #[must_use]
    pub fn runtime_for(&self, individual_id: u64) -> Option<&AgentRuntime> {
        self.individuals
            .get(&individual_id)
            .map(|individual| &individual.runtime)
    }

    /// Sum of live decision counters, suitable for periodic population checkpointing.
    #[must_use]
    pub fn total_decisions(&self) -> u64 {
        self.individuals
            .values()
            .map(|individual| individual.runtime.state().decision_tick)
            .sum()
    }

    /// Returns the body slot currently occupied by an individual.
    #[must_use]
    pub fn body_slot_of(&self, individual_id: u64) -> Option<BodySlotId> {
        self.individuals
            .get(&individual_id)
            .map(|individual| individual.body_slot)
    }

    /// Returns lineage metadata and inherited program.
    #[must_use]
    pub fn manifest(&self) -> PopulationManifest {
        PopulationManifest {
            schema_version: POPULATION_MANIFEST_SCHEMA_VERSION,
            seed: self.seed,
            next_individual_id: self.next_individual_id,
            capacity: self.capacity,
            mutation_enabled: self.lifecycle_config.mutation_enabled,
            maturity_decisions: self.lifecycle_config.maturity_decisions,
            minimum_reproductive_reserve: self.lifecycle_config.minimum_reproductive_reserve,
            reproductive_cost: self.lifecycle_config.reproductive_cost,
            individuals: self
                .individuals
                .iter()
                .map(|(&individual_id, individual)| PopulationIndividual {
                    individual_id,
                    body_slot: individual.body_slot.0,
                    parent_id: individual.parent_id,
                    generation: individual.generation,
                    genome: individual.genome.clone(),
                    runtime_file: runtime_file(individual_id).to_string_lossy().into_owned(),
                    reproductive_reserve: individual.reproductive_reserve,
                    next_sequence: individual.runtime.state().next_sequence,
                    reproduction_signal: individual.reproduction_signal,
                })
                .collect(),
            lifecycle: self.lifecycle.clone(),
            processed_events: self.processed_events.clone(),
        }
    }

    /// Routes one request without coupling its sequence or learned state to any other individual.
    ///
    /// # Errors
    ///
    /// Rejects unsupported protocol versions, unknown identities, mismatched body slots, and
    /// out-of-order observations without mutating that runtime.
    pub fn handle(&mut self, request: BridgeRequest) -> Result<BridgeResponse, BridgeError> {
        if request.protocol != crate::PROTOCOL_VERSION {
            return Err(BridgeError::Protocol {
                received: request.protocol,
            });
        }
        let slot_available =
            self.individuals.len() < usize::try_from(self.capacity).unwrap_or(usize::MAX);
        let Some(individual) = self.individuals.get_mut(&request.agent_id) else {
            return Err(BridgeError::UnknownIndividual(request.agent_id));
        };
        if let Some(received) = request.body_slot
            && received != individual.body_slot.0
        {
            return Err(BridgeError::BodySlot {
                individual_id: request.agent_id,
                expected: individual.body_slot.0,
                received,
            });
        }
        let observed_reserve = request.observation.interoception.map(|body| body.fullness);
        let transition = individual
            .runtime
            .step(request.observation)
            .map_err(BridgeError::Sequence)?;
        if let Some(observed) = observed_reserve {
            let recovery = observed / 64;
            individual.reproductive_reserve = individual
                .reproductive_reserve
                .saturating_add(recovery)
                .min(observed);
        }
        individual.reproduction_signal = transition.action.signals[0];
        let response = response_for(
            request.agent_id,
            individual.body_slot.0,
            &transition,
            &individual.runtime,
        );
        let record = TraceRecord {
            request,
            response: response.clone(),
            reproduction: Some(ReproductionTransitionAudit {
                signal_0: transition.action.signals[0],
                decision_tick: transition.decision_tick,
                maturity_decisions: self.lifecycle_config.maturity_decisions,
                mature: individual.runtime.state().decision_tick
                    >= self.lifecycle_config.maturity_decisions,
                reproductive_reserve: individual.reproductive_reserve,
                fullness: transition
                    .observation
                    .interoception
                    .map(|body| body.fullness),
                slot_available,
            }),
        };
        individual.trace.push(record.clone());
        self.trace.push(record);
        Ok(response)
    }

    /// Removes an individual and immediately frees its body slot.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError::UnknownIndividual`] if the identity is not live.
    pub fn die(&mut self, individual_id: u64) -> Result<BodySlotId, LifecycleError> {
        let Some(individual) = self.individuals.remove(&individual_id) else {
            return Err(LifecycleError::UnknownIndividual(individual_id));
        };
        self.lifecycle.push(LifecycleEvent::Death {
            event_id: format!("legacy-death-{individual_id}"),
            individual_id,
            body_slot: individual.body_slot.0,
        });
        Ok(individual.body_slot)
    }

    /// Creates a naturally reproduced, fresh-runtime asexual child in the lowest free body slot.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown parent, full capacity, or exhausted identity space.
    pub fn reproduce(&mut self, parent_id: u64) -> Result<u64, LifecycleError> {
        self.birth(parent_id, BirthKind::Asexual)
    }

    fn birth(&mut self, parent_id: u64, kind: BirthKind) -> Result<u64, LifecycleError> {
        let Some(parent) = self.individuals.get(&parent_id) else {
            return Err(LifecycleError::UnknownIndividual(parent_id));
        };
        let occupied: BTreeSet<_> = self
            .individuals
            .values()
            .map(|individual| individual.body_slot.0)
            .collect();
        let Some(slot) = (0..self.capacity).find(|slot| !occupied.contains(slot)) else {
            self.lifecycle.push(LifecycleEvent::ReproductionBlocked {
                event_id: format!("legacy-blocked-{}", self.lifecycle.len()),
                parent_id,
                capacity: self.capacity,
                reason: PopulationBlockReason::NoFreeSlot,
            });
            return Err(LifecycleError::CapacityBlocked);
        };
        let child_id = self.next_individual_id;
        self.next_individual_id = self
            .next_individual_id
            .checked_add(1)
            .ok_or(LifecycleError::IdentityExhausted)?;
        let generation = parent.generation.saturating_add(1);
        let genome = parent.genome.asexual_offspring_with_mutation(
            self.seed,
            parent_id,
            child_id,
            generation,
            self.lifecycle_config.mutation_enabled,
        );
        let runtime = newborn_runtime(self.seed, child_id, &genome, &parent.runtime);
        self.individuals.insert(
            child_id,
            Individual {
                body_slot: BodySlotId(slot),
                parent_id: Some(parent_id),
                generation,
                genome,
                runtime,
                trace: Vec::new(),
                reproductive_reserve: 255,
                reproduction_signal: 0,
            },
        );
        self.lifecycle.push(LifecycleEvent::Birth {
            event_id: format!("legacy-birth-{child_id}"),
            individual_id: child_id,
            body_slot: slot,
            parent_id,
            generation,
            kind,
            reproductive_cost: 0,
        });
        Ok(child_id)
    }

    /// Applies one durable lifecycle command exactly once.
    ///
    /// A successful return means the command result, lineage, reserve cost, runtime, and slot
    /// assignment are recoverable from the write-ahead transaction before the caller may act on
    /// the response.
    ///
    /// # Errors
    ///
    /// Rejects malformed IDs and invalid identities or slots, and propagates durable transaction
    /// write failures.
    pub fn process_lifecycle(
        &mut self,
        command: LifecycleCommand,
    ) -> Result<PopulationCommandResult, Box<dyn Error + Send + Sync>> {
        if command.event_id.is_empty() || command.event_id.len() > 128 {
            return Err("event_id must contain 1..=128 bytes".into());
        }
        if let Some(result) = self.processed_events.get(&command.event_id) {
            return Ok(result.clone());
        }
        let mut next = self.clone();
        let result = next.apply_lifecycle(&command)?;
        let event_id = command.event_id;
        next.processed_events
            .insert(event_id.clone(), result.clone());
        if let Some(directory) = &self.storage_directory {
            next.save_transaction(directory, &event_id, &result)?;
            next.storage_directory = Some(directory.clone());
        }
        *self = next;
        Ok(result)
    }

    fn apply_lifecycle(
        &mut self,
        command: &LifecycleCommand,
    ) -> Result<PopulationCommandResult, Box<dyn Error + Send + Sync>> {
        match command.operation {
            LifecycleOperation::Death {
                individual_id,
                body_slot,
            } => {
                let individual = self
                    .individuals
                    .get(&individual_id)
                    .ok_or(LifecycleError::UnknownIndividual(individual_id))?;
                if individual.body_slot.0 != body_slot {
                    return Err("death command body slot mismatch".into());
                }
                self.individuals.remove(&individual_id);
                self.lifecycle.push(LifecycleEvent::Death {
                    event_id: command.event_id.clone(),
                    individual_id,
                    body_slot,
                });
                Ok(PopulationCommandResult::Died {
                    individual_id,
                    body_slot,
                })
            }
            LifecycleOperation::InfrastructureBlocked { parent_id } => {
                self.lifecycle.push(LifecycleEvent::ReproductionBlocked {
                    event_id: command.event_id.clone(),
                    parent_id,
                    capacity: self.capacity,
                    reason: PopulationBlockReason::Infrastructure,
                });
                Ok(PopulationCommandResult::Blocked {
                    reason: PopulationBlockReason::Infrastructure,
                })
            }
            LifecycleOperation::Reproduce {
                parent_id,
                body_slot,
                infrastructure_failures,
            } => {
                for failure in 0..infrastructure_failures {
                    self.lifecycle.push(LifecycleEvent::ReproductionBlocked {
                        event_id: format!("{}:infrastructure:{}", command.event_id, failure + 1),
                        parent_id,
                        capacity: self.capacity,
                        reason: PopulationBlockReason::Infrastructure,
                    });
                }
                self.apply_reproduction(command, parent_id, body_slot)
            }
        }
    }

    fn apply_reproduction(
        &mut self,
        command: &LifecycleCommand,
        parent_id: u64,
        body_slot: u32,
    ) -> Result<PopulationCommandResult, Box<dyn Error + Send + Sync>> {
        let slot_available =
            self.individuals.len() < usize::try_from(self.capacity).unwrap_or(usize::MAX);
        let Some(snapshot) = self.individuals.get(&parent_id) else {
            // A body may ask to reproduce just before its death is processed. A parent that is no
            // longer alive is a biological block, recorded once, so the request is not retried.
            return Ok(self.blocked(
                &command.event_id,
                parent_id,
                PopulationBlockReason::BiologicalEligibility,
            ));
        };
        self.lifecycle.push(LifecycleEvent::ReproductionAttempt {
            event_id: command.event_id.clone(),
            parent_id,
            body_slot,
            decision_tick: snapshot.runtime.state().decision_tick,
            signal_0: snapshot.reproduction_signal,
            maturity_decisions: self.lifecycle_config.maturity_decisions,
            reproductive_reserve: snapshot.reproductive_reserve,
            slot_available,
        });
        let eligible = self.individuals.get(&parent_id).is_some_and(|parent| {
            parent.body_slot.0 == body_slot
                && parent.runtime.state().decision_tick >= self.lifecycle_config.maturity_decisions
                && parent.reproductive_reserve >= self.lifecycle_config.minimum_reproductive_reserve
                && parent.reproductive_reserve >= self.lifecycle_config.reproductive_cost
                && parent.reproduction_signal > 0
        });
        if !eligible {
            return Ok(self.blocked(
                &command.event_id,
                parent_id,
                PopulationBlockReason::BiologicalEligibility,
            ));
        }
        let occupied: BTreeSet<_> = self
            .individuals
            .values()
            .map(|individual| individual.body_slot.0)
            .collect();
        let Some(slot) = (0..self.capacity).find(|slot| !occupied.contains(slot)) else {
            return Ok(self.blocked(
                &command.event_id,
                parent_id,
                PopulationBlockReason::NoFreeSlot,
            ));
        };
        let child_id = self.next_individual_id;
        self.next_individual_id = child_id
            .checked_add(1)
            .ok_or(LifecycleError::IdentityExhausted)?;
        let parent = self
            .individuals
            .get_mut(&parent_id)
            .expect("eligibility established a living parent");
        let generation = parent.generation.saturating_add(1);
        let genome = parent.genome.asexual_offspring_with_mutation(
            self.seed,
            parent_id,
            child_id,
            generation,
            self.lifecycle_config.mutation_enabled,
        );
        let runtime = newborn_runtime(self.seed, child_id, &genome, &parent.runtime);
        parent.reproductive_reserve = parent
            .reproductive_reserve
            .saturating_sub(self.lifecycle_config.reproductive_cost);
        parent.reproduction_signal = 0;
        let reserve_after = parent.reproductive_reserve;
        self.individuals.insert(
            child_id,
            Individual {
                body_slot: BodySlotId(slot),
                parent_id: Some(parent_id),
                generation,
                genome,
                runtime,
                trace: Vec::new(),
                reproductive_reserve: 255,
                reproduction_signal: 0,
            },
        );
        self.lifecycle.push(LifecycleEvent::Birth {
            event_id: command.event_id.clone(),
            individual_id: child_id,
            body_slot: slot,
            parent_id,
            generation,
            kind: BirthKind::Asexual,
            reproductive_cost: self.lifecycle_config.reproductive_cost,
        });
        Ok(PopulationCommandResult::Born {
            individual_id: child_id,
            parent_id,
            body_slot: slot,
            generation,
            reserve_after,
        })
    }

    fn blocked(
        &mut self,
        event_id: &str,
        parent_id: u64,
        reason: PopulationBlockReason,
    ) -> PopulationCommandResult {
        self.lifecycle.push(LifecycleEvent::ReproductionBlocked {
            event_id: event_id.to_owned(),
            parent_id,
            capacity: self.capacity,
            reason,
        });
        PopulationCommandResult::Blocked { reason }
    }

    /// Returns lifecycle events, including blocked reproduction attempts.
    #[must_use]
    pub fn lifecycle_events(&self) -> &[LifecycleEvent] {
        &self.lifecycle
    }

    /// Drains all accepted transitions in arrival order.
    #[must_use]
    pub fn drain_trace(&mut self) -> Vec<TraceRecord> {
        for individual in self.individuals.values_mut() {
            individual.trace.clear();
        }
        core::mem::take(&mut self.trace)
    }

    /// Drains one individual's trace without disturbing another's trace state.
    #[must_use]
    pub fn drain_trace_for(&mut self, individual_id: u64) -> Option<Vec<TraceRecord>> {
        self.individuals
            .get_mut(&individual_id)
            .map(|individual| core::mem::take(&mut individual.trace))
    }

    /// Atomically replaces every live runtime checkpoint, then the manifest index.
    ///
    /// # Errors
    ///
    /// Propagates checkpoint serialization and file-system errors.
    pub fn save(&self, directory: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
        std::fs::create_dir_all(directory.join("individuals"))?;
        for (&individual_id, individual) in &self.individuals {
            atomic_write(
                &directory.join(runtime_file(individual_id)),
                &serde_json::to_vec(&AgentCheckpoint::current(&individual.runtime))?,
            )?;
        }
        atomic_write(
            &directory.join("population.json"),
            &serde_json::to_vec_pretty(&self.manifest())?,
        )
    }

    fn save_transaction(
        &self,
        directory: &Path,
        event_id: &str,
        result: &PopulationCommandResult,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        std::fs::create_dir_all(directory.join("individuals"))?;
        let wal = LifecycleWal {
            event_id: event_id.to_owned(),
            result: result.clone(),
            manifest: self.manifest(),
            checkpoints: self
                .individuals
                .values()
                .map(|individual| AgentCheckpoint::current(&individual.runtime))
                .collect(),
        };
        atomic_write(&directory.join("lifecycle.wal"), &serde_json::to_vec(&wal)?)?;
        materialize_wal(directory, &wal)?;
        std::fs::remove_file(directory.join("lifecycle.wal"))?;
        Ok(())
    }

    /// Loads the manifest and each individual checkpoint independently.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported or inconsistent manifests, invalid checkpoints, or
    /// file-system failures.
    pub fn load(directory: &Path) -> Result<Self, Box<dyn Error + Send + Sync>> {
        recover_wal(directory)?;
        let manifest: PopulationManifest =
            serde_json::from_slice(&std::fs::read(directory.join("population.json"))?)?;
        manifest.validate_version()?;
        if manifest.capacity == 0 {
            return Err("population capacity must be non-zero".into());
        }
        let mut ids = BTreeSet::new();
        let mut slots = BTreeSet::new();
        let mut individuals = BTreeMap::new();
        for entry in &manifest.individuals {
            if !ids.insert(entry.individual_id)
                || !slots.insert(entry.body_slot)
                || entry.body_slot >= manifest.capacity
                || entry.individual_id >= manifest.next_individual_id
                || entry.runtime_file != runtime_file(entry.individual_id).to_string_lossy()
            {
                return Err("population manifest identity invariant failed".into());
            }
            let checkpoint: AgentCheckpoint =
                serde_json::from_slice(&std::fs::read(directory.join(&entry.runtime_file))?)?;
            if checkpoint.seed != manifest.seed || checkpoint.agent_id != entry.individual_id {
                return Err("runtime checkpoint does not match population manifest".into());
            }
            individuals.insert(
                entry.individual_id,
                Individual {
                    body_slot: BodySlotId(entry.body_slot),
                    parent_id: entry.parent_id,
                    generation: entry.generation,
                    genome: entry.genome.clone(),
                    runtime: checkpoint.restore()?,
                    trace: Vec::new(),
                    reproductive_reserve: entry.reproductive_reserve,
                    reproduction_signal: entry.reproduction_signal,
                },
            );
        }
        Ok(Self {
            seed: manifest.seed,
            capacity: manifest.capacity,
            next_individual_id: manifest.next_individual_id,
            individuals,
            trace: Vec::new(),
            lifecycle: manifest.lifecycle,
            lifecycle_config: LifecycleConfig {
                maturity_decisions: manifest.maturity_decisions,
                minimum_reproductive_reserve: manifest.minimum_reproductive_reserve,
                reproductive_cost: manifest.reproductive_cost,
                mutation_enabled: manifest.mutation_enabled,
            },
            processed_events: manifest.processed_events,
            storage_directory: Some(directory.to_path_buf()),
        })
    }
}

fn newborn_runtime(
    seed: u64,
    child_id: u64,
    genome: &DevelopmentalGenome,
    parent: &AgentRuntime,
) -> AgentRuntime {
    let mode = parent.state().experiment_mode;
    let mut newborn = if mode == ExperimentMode::AffordanceDiscovery {
        AgentRuntime::new_with_affordance(
            seed,
            child_id,
            genome.default_brain_config(),
            parent.state().affordance.config,
        )
        .expect("a live parent's affordance configuration is valid")
    } else {
        AgentRuntime::new_with_mode(seed, child_id, genome.default_brain_config(), mode)
    };
    newborn.set_episodic_config(parent.state().episodic_memory.config);
    newborn
}

fn runtime_file(individual_id: u64) -> PathBuf {
    PathBuf::from(format!("individuals/{individual_id}.json"))
}

/// Replaces `path` only after the new bytes are durably on disk.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn Error + Send + Sync>> {
    let partial = path.with_extension("partial");
    let mut file = std::fs::File::create(&partial)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(partial, path)?;
    Ok(())
}

fn materialize_wal(
    directory: &Path,
    wal: &LifecycleWal,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    std::fs::create_dir_all(directory.join("individuals"))?;
    for checkpoint in &wal.checkpoints {
        atomic_write(
            &directory.join(runtime_file(checkpoint.agent_id)),
            &serde_json::to_vec(checkpoint)?,
        )?;
    }
    atomic_write(
        &directory.join("population.json"),
        &serde_json::to_vec_pretty(&wal.manifest)?,
    )
}

fn recover_wal(directory: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
    let path = directory.join("lifecycle.wal");
    if !path.exists() {
        return Ok(());
    }
    let wal: LifecycleWal = serde_json::from_slice(&std::fs::read(&path)?)?;
    materialize_wal(directory, &wal)?;
    std::fs::remove_file(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use sim_core::{
        RawSensoryFrame,
        environment::{
            BodyProprioception, EnvironmentCapabilities, EnvironmentObservation, PrimitiveAction,
            TransportStatus,
        },
    };

    use super::*;
    use crate::{ApplicationTrace, AppliedPose, PROTOCOL_VERSION};

    fn request(individual_id: u64, slot: u32, sequence: u64) -> BridgeRequest {
        BridgeRequest {
            protocol: PROTOCOL_VERSION,
            agent_id: individual_id,
            body_slot: Some(slot),
            observation: EnvironmentObservation {
                sequence,
                senses: RawSensoryFrame::EMPTY,
                body: BodyProprioception::default(),
                capabilities: EnvironmentCapabilities::default(),
                transport: TransportStatus::Healthy,
                retina: None,
                interoception: None,
                held_item: None,
            },
            trace: ApplicationTrace {
                applied_server_step: sequence,
                applied_steps: 1,
                accumulated_dtime_us: 100_000,
                applied_action: PrimitiveAction::default(),
                pose: AppliedPose {
                    x_millinode: 0,
                    y_millinode: 1_000,
                    z_millinode: 0,
                    yaw_milliradian: 0,
                },
            },
        }
    }

    fn two_individuals() -> (PopulationCoordinator, u64) {
        let mut population = PopulationCoordinator::new(17, 10);
        population.set_capacity(2).unwrap();
        let child = population.reproduce(10).unwrap();
        (population, child)
    }

    #[test]
    fn initial_cohort_routes_identity_separately_from_body_slot() {
        let mut population = PopulationCoordinator::new_population_with_mode(
            17,
            1,
            4,
            4,
            ExperimentMode::PredictiveBaseline,
        )
        .unwrap();
        for individual_id in 1..=4 {
            let slot = u32::try_from(individual_id - 1).unwrap();
            assert_eq!(
                population.body_slot_of(individual_id),
                Some(BodySlotId(slot))
            );
            population.handle(request(individual_id, slot, 0)).unwrap();
        }
        let before = population.runtime_for(2).unwrap().clone();
        assert!(matches!(
            population.handle(request(2, 0, 1)),
            Err(BridgeError::BodySlot {
                individual_id: 2,
                expected: 1,
                received: 0
            })
        ));
        assert_eq!(population.runtime_for(2), Some(&before));
        assert!(population.handle(request(1, 0, 1)).is_ok());
        assert!(population.handle(request(2, 1, 1)).is_ok());
    }

    #[test]
    fn interleaved_and_isolated_stepping_match() {
        let (mut interleaved, child) = two_individuals();
        let mut isolated = interleaved.clone();
        for sequence in 0..8 {
            interleaved.handle(request(10, 0, sequence)).unwrap();
            interleaved.handle(request(child, 1, sequence)).unwrap();
        }
        for individual_id in [10, child] {
            let slot = isolated.body_slot_of(individual_id).unwrap().0;
            for sequence in 0..8 {
                isolated
                    .handle(request(individual_id, slot, sequence))
                    .unwrap();
            }
            assert_eq!(
                interleaved.runtime_for(individual_id),
                isolated.runtime_for(individual_id)
            );
        }
        assert_eq!(interleaved.drain_trace_for(10).unwrap().len(), 8);
        assert_eq!(interleaved.drain_trace_for(child).unwrap().len(), 8);
    }

    #[test]
    fn individuals_restart_independently() {
        let (mut population, child) = two_individuals();
        population.handle(request(10, 0, 0)).unwrap();
        population.handle(request(child, 1, 0)).unwrap();
        population.handle(request(child, 1, 1)).unwrap();
        let directory = std::env::temp_dir().join(format!(
            "sim-luanti-population-{}-restart",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        population.save(&directory).unwrap();
        let mut loaded = PopulationCoordinator::load(&directory).unwrap();
        std::fs::remove_dir_all(directory).unwrap();
        assert_eq!(loaded.runtime_for(10), population.runtime_for(10));
        assert_eq!(loaded.runtime_for(child), population.runtime_for(child));
        assert!(loaded.handle(request(10, 0, 1)).is_ok());
        assert!(loaded.handle(request(child, 1, 2)).is_ok());
    }

    #[test]
    fn multi_founder_predictors_restart_with_exact_next_decisions() {
        let mut population = PopulationCoordinator::new_population_with_mode(
            29,
            41,
            4,
            4,
            ExperimentMode::PredictiveBaseline,
        )
        .unwrap();
        for sequence in 0..20 {
            for slot in 0..4 {
                let individual_id = 41 + u64::from(slot);
                let mut input = request(individual_id, slot, sequence);
                let amplitude = u16::try_from(sequence * 7 + u64::from(slot)).unwrap();
                input.observation.senses.touch = amplitude;
                input.observation.senses.energy = 255 - amplitude;
                input.observation.senses.acceleration =
                    i16::try_from(sequence).unwrap() - i16::try_from(slot).unwrap();
                population.handle(input).unwrap();
            }
        }
        let directory = std::env::temp_dir().join(format!(
            "sim-luanti-multi-predictor-{}-restart",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        population.save(&directory).unwrap();
        let mut loaded = PopulationCoordinator::load(&directory).unwrap();
        std::fs::remove_dir_all(directory).unwrap();

        let expected_manifest = population.manifest();
        let loaded_manifest = loaded.manifest();
        assert_eq!(loaded_manifest.individuals, expected_manifest.individuals);
        for entry in &expected_manifest.individuals {
            assert_eq!(
                loaded.body_slot_of(entry.individual_id),
                Some(BodySlotId(entry.body_slot))
            );
            let before = population.runtime_for(entry.individual_id).unwrap().state();
            let after = loaded.runtime_for(entry.individual_id).unwrap().state();
            assert!(
                before
                    .recurrent_predictor
                    .hidden
                    .iter()
                    .any(|value| *value != 0)
            );
            assert!(before.recurrent_predictor.updates > 0);
            assert_ne!(before.recurrent_predictor.replay.len(), 0);
            assert_eq!(after.next_sequence, entry.next_sequence);
            assert_eq!(
                after.recurrent_predictor.fingerprint(),
                before.recurrent_predictor.fingerprint()
            );
            assert_eq!(
                after.recurrent_predictor.hidden,
                before.recurrent_predictor.hidden
            );
            assert_eq!(
                after.recurrent_predictor.decoder_delta,
                before.recurrent_predictor.decoder_delta
            );
            assert_eq!(
                after.recurrent_predictor.momentum,
                before.recurrent_predictor.momentum
            );
            assert_eq!(
                after.recurrent_predictor.replay,
                before.recurrent_predictor.replay
            );
            assert_eq!(
                after.recurrent_predictor.replay_cursor,
                before.recurrent_predictor.replay_cursor
            );
            assert_eq!(
                after.recurrent_predictor.sequence_step,
                before.recurrent_predictor.sequence_step
            );
            assert_eq!(
                after.recurrent_predictor.updates,
                before.recurrent_predictor.updates
            );
            assert_ne!(before.episodic_memory.events.len(), 0);
            assert_eq!(after.episodic_memory, before.episodic_memory);
            assert_eq!(
                after.episodic_memory.fingerprint(),
                before.episodic_memory.fingerprint()
            );

            let mut next = request(entry.individual_id, entry.body_slot, entry.next_sequence);
            next.observation.senses.touch = 211;
            next.observation.senses.energy = 44;
            next.observation.senses.acceleration = -17;
            assert_eq!(
                loaded.handle(next.clone()).unwrap(),
                population.handle(next).unwrap()
            );
            assert_eq!(
                loaded.runtime_for(entry.individual_id),
                population.runtime_for(entry.individual_id)
            );
        }
    }

    #[test]
    fn newborn_inherits_genome_but_no_acquired_state() {
        let mut population =
            PopulationCoordinator::new_with_mode(8, 20, ExperimentMode::Homeostatic);
        population.set_capacity(2).unwrap();
        for sequence in 0..5 {
            let mut input = request(20, 0, sequence);
            input.observation.interoception = Some(sim_core::environment::Interoception {
                health: if sequence == 2 { 37 } else { 240 },
                fullness: 40 + u16::try_from(sequence * 23).unwrap(),
                air: 255,
                pain: if sequence == 2 { 211 } else { 0 },
            });
            population.handle(input).unwrap();
        }
        let parent_memory = population
            .runtime_for(20)
            .unwrap()
            .state()
            .episodic_memory
            .clone();
        assert_ne!(parent_memory.events.len(), 0);
        let child = population.reproduce(20).unwrap();
        let expected = newborn_runtime(
            8,
            child,
            &population.individuals[&child].genome,
            population.runtime_for(20).unwrap(),
        );
        let child_runtime = population.runtime_for(child).unwrap();
        let child_state = child_runtime.state();
        assert_eq!(child_state.decision_tick, 0);
        assert_eq!(child_state.next_sequence, 0);
        assert_eq!(child_runtime, &expected);
        assert_eq!(child_state.episodic_memory.events.len(), 0);
        assert_eq!(child_state.episodic_memory.next_event_id, 0);
        assert_ne!(
            child_state.episodic_memory.fingerprint(),
            parent_memory.fingerprint()
        );
        assert_eq!(
            child_state.developmental_config,
            population.individuals[&child].genome.default_brain_config()
        );
        assert!(
            child_state
                .recurrent_predictor
                .hidden
                .iter()
                .all(|value| *value == 0)
        );
        assert!(
            child_state
                .recurrent_predictor
                .decoder_delta
                .iter()
                .all(|value| *value == 0)
        );
        assert!(
            child_state
                .recurrent_predictor
                .momentum
                .iter()
                .all(|value| *value == 0)
        );
        assert_eq!(child_state.recurrent_predictor.replay.len(), 0);
        assert_eq!(child_state.recurrent_predictor.updates, 0);
        assert_ne!(
            child_state.recurrent_predictor.fingerprint(),
            population
                .runtime_for(20)
                .unwrap()
                .state()
                .recurrent_predictor
                .fingerprint()
        );
    }

    #[test]
    fn lineage_mutation_is_deterministic() {
        let parent = DevelopmentalGenome::founder(3, 7);
        let first = parent.asexual_offspring(3, 7, 8, 1);
        let repeated = parent.asexual_offspring(3, 7, 8, 1);
        assert_eq!(first, repeated);

        let mut population = PopulationCoordinator::new(3, 7);
        population.set_capacity(2).unwrap();
        let child = population.reproduce(7).unwrap();
        let entry = population
            .manifest()
            .individuals
            .into_iter()
            .find(|entry| entry.individual_id == child)
            .unwrap();
        assert_eq!(entry.parent_id, Some(7));
        assert_eq!(entry.generation, 1);
        assert_eq!(entry.genome, first);
    }

    #[test]
    fn reproduction_requested_by_a_dead_parent_is_blocked_once_not_rejected() {
        let mut population = PopulationCoordinator::new_population_with_mode(
            5,
            1,
            2,
            2,
            ExperimentMode::PredictiveBaseline,
        )
        .unwrap();
        let death = LifecycleCommand {
            event_id: "death-1".into(),
            operation: LifecycleOperation::Death {
                individual_id: 1,
                body_slot: 0,
            },
        };
        population.process_lifecycle(death).unwrap();
        let late = LifecycleCommand {
            event_id: "late-reproduce".into(),
            operation: LifecycleOperation::Reproduce {
                parent_id: 1,
                body_slot: 0,
                infrastructure_failures: 0,
            },
        };
        let blocked = PopulationCommandResult::Blocked {
            reason: PopulationBlockReason::BiologicalEligibility,
        };
        assert_eq!(population.process_lifecycle(late.clone()).unwrap(), blocked);
        let events = population.lifecycle_events().len();
        assert_eq!(population.process_lifecycle(late).unwrap(), blocked);
        assert_eq!(population.lifecycle_events().len(), events);
    }

    #[test]
    fn founder_genome_rotation_moves_genotypes_but_not_identities_or_slots() {
        let mode = ExperimentMode::PredictiveBaseline;
        let plain = PopulationCoordinator::new_population_with_mode(5, 1, 4, 4, mode).unwrap();
        let rotated =
            PopulationCoordinator::new_rotated_population_with_mode(5, 1, 4, 4, mode, 1).unwrap();
        let wrapped =
            PopulationCoordinator::new_rotated_population_with_mode(5, 1, 4, 4, mode, 5).unwrap();
        let (plain, rotated) = (plain.manifest().individuals, rotated.manifest().individuals);
        for (slot, entry) in rotated.iter().enumerate() {
            assert_eq!(entry.individual_id, plain[slot].individual_id);
            assert_eq!(entry.body_slot, plain[slot].body_slot);
            assert_eq!(entry.genome, plain[(slot + 1) % 4].genome);
        }
        assert_eq!(wrapped.manifest().individuals, rotated);
    }

    #[test]
    fn death_reuses_slot_but_never_individual_identity() {
        let (mut population, first_child) = two_individuals();
        assert_eq!(
            population.reproduce(10),
            Err(LifecycleError::CapacityBlocked)
        );
        assert!(matches!(
            population.lifecycle_events().last(),
            Some(LifecycleEvent::ReproductionBlocked {
                reason: PopulationBlockReason::NoFreeSlot,
                ..
            })
        ));
        assert_eq!(population.die(first_child).unwrap(), BodySlotId(1));
        let reborn = population.reproduce(10).unwrap();
        assert!(reborn > first_child);
        assert_eq!(population.body_slot_of(reborn), Some(BodySlotId(1)));
        assert!(matches!(
            population.lifecycle_events().last(),
            Some(LifecycleEvent::Birth {
                kind: BirthKind::Asexual,
                ..
            })
        ));
    }

    fn eligible(population: &mut PopulationCoordinator, parent_id: u64) {
        let parent = population.individuals.get_mut(&parent_id).unwrap();
        parent.reproduction_signal = 1;
        parent.reproductive_reserve = 255;
    }

    #[test]
    fn death_never_reproduces_and_extinction_is_valid() {
        let mut population = PopulationCoordinator::new_population_with_mode(
            9,
            1,
            4,
            4,
            ExperimentMode::Homeostatic,
        )
        .unwrap();
        for id in 1..=4 {
            let result = population
                .process_lifecycle(LifecycleCommand {
                    event_id: format!("death-{id}"),
                    operation: LifecycleOperation::Death {
                        individual_id: id,
                        body_slot: u32::try_from(id - 1).unwrap(),
                    },
                })
                .unwrap();
            assert!(matches!(result, PopulationCommandResult::Died { .. }));
        }
        assert!(population.individuals.is_empty());
        assert_eq!(
            population
                .lifecycle
                .iter()
                .filter(|event| matches!(event, LifecycleEvent::Birth { .. }))
                .count(),
            0
        );
    }

    #[test]
    fn reproduction_checks_decision_maturity_reserve_and_capacity() {
        let mut population = PopulationCoordinator::new_population_with_mode(
            4,
            1,
            2,
            2,
            ExperimentMode::Homeostatic,
        )
        .unwrap();
        population
            .set_lifecycle_config(LifecycleConfig {
                maturity_decisions: 1,
                minimum_reproductive_reserve: 200,
                reproductive_cost: 80,
                mutation_enabled: false,
            })
            .unwrap();
        let blocked = population
            .process_lifecycle(LifecycleCommand {
                event_id: "immature".into(),
                operation: LifecycleOperation::Reproduce {
                    parent_id: 1,
                    body_slot: 0,
                    infrastructure_failures: 2,
                },
            })
            .unwrap();
        assert_eq!(
            blocked,
            PopulationCommandResult::Blocked {
                reason: PopulationBlockReason::BiologicalEligibility
            }
        );
        population.handle(request(1, 0, 0)).unwrap();
        eligible(&mut population, 1);
        let full = population
            .process_lifecycle(LifecycleCommand {
                event_id: "full".into(),
                operation: LifecycleOperation::Reproduce {
                    parent_id: 1,
                    body_slot: 0,
                    infrastructure_failures: 0,
                },
            })
            .unwrap();
        assert_eq!(
            full,
            PopulationCommandResult::Blocked {
                reason: PopulationBlockReason::NoFreeSlot
            }
        );
        assert_eq!(
            population
                .lifecycle
                .iter()
                .filter(|event| matches!(
                    event,
                    LifecycleEvent::ReproductionBlocked {
                        reason: PopulationBlockReason::Infrastructure,
                        ..
                    }
                ))
                .count(),
            2
        );
    }

    #[test]
    fn natural_controller_signal_reaches_audited_lifecycle_birth() {
        let mut population =
            PopulationCoordinator::new_with_mode(4, 1, ExperimentMode::PredictiveBaseline);
        population.set_capacity(2).unwrap();
        population
            .set_lifecycle_config(LifecycleConfig {
                maturity_decisions: 1,
                minimum_reproductive_reserve: 160,
                reproductive_cost: 80,
                mutation_enabled: false,
            })
            .unwrap();
        let mut damaged = request(1, 0, 0);
        damaged.observation.interoception = Some(sim_core::environment::Interoception {
            health: 0,
            fullness: 160,
            air: 255,
            pain: 255,
        });
        let response = population.handle(damaged).unwrap();
        assert!(response.action.signals[0] > 0);
        let trace = population.drain_trace();
        let audit = trace[0].reproduction.expect("retained transition audit");
        assert_eq!(audit.signal_0, response.action.signals[0]);
        assert!(audit.mature);
        assert_eq!(audit.reproductive_reserve, 160);
        assert_eq!(audit.fullness, Some(160));
        assert!(audit.slot_available);

        let result = population
            .process_lifecycle(LifecycleCommand {
                event_id: "natural-controller-birth".into(),
                operation: LifecycleOperation::Reproduce {
                    parent_id: 1,
                    body_slot: 0,
                    infrastructure_failures: 0,
                },
            })
            .unwrap();
        assert!(matches!(
            result,
            PopulationCommandResult::Born {
                parent_id: 1,
                body_slot: 1,
                reserve_after: 80,
                ..
            }
        ));
        assert!(matches!(
            &population.lifecycle[0],
            LifecycleEvent::ReproductionAttempt {
                signal_0,
                decision_tick: 1,
                maturity_decisions: 1,
                reproductive_reserve: 160,
                slot_available: true,
                ..
            } if *signal_0 > 0
        ));
        let child = population
            .manifest()
            .individuals
            .into_iter()
            .find(|entry| entry.parent_id == Some(1))
            .unwrap();
        assert_eq!(child.body_slot, 1);
        assert_eq!(child.generation, 1);
        assert_eq!(
            population
                .runtime_for(child.individual_id)
                .unwrap()
                .state()
                .decision_tick,
            0
        );
    }

    #[test]
    fn lifecycle_retry_and_wal_recovery_are_exactly_once() {
        let directory =
            std::env::temp_dir().join(format!("sim-lifecycle-wal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let mut population = PopulationCoordinator::new(12, 1);
        population.set_capacity(2).unwrap();
        population.bind_storage(&directory).unwrap();
        eligible(&mut population, 1);
        let before = population.clone();
        let command = LifecycleCommand {
            event_id: "durable-birth".into(),
            operation: LifecycleOperation::Reproduce {
                parent_id: 1,
                body_slot: 0,
                infrastructure_failures: 0,
            },
        };
        let first = population.process_lifecycle(command.clone()).unwrap();
        let repeated = population.process_lifecycle(command).unwrap();
        assert_eq!(first, repeated);
        assert_eq!(
            population
                .lifecycle
                .iter()
                .filter(|event| matches!(event, LifecycleEvent::Birth { .. }))
                .count(),
            1
        );

        let wal = LifecycleWal {
            event_id: "durable-birth".into(),
            result: first.clone(),
            manifest: population.manifest(),
            checkpoints: population
                .individuals
                .values()
                .map(|individual| AgentCheckpoint::current(&individual.runtime))
                .collect(),
        };
        // Crash after WAL publication, after one checkpoint replacement, and after manifest
        // replacement all roll forward to the same exactly-once state.
        for stage in 0..3 {
            let crash_directory = directory.join(format!("crash-{stage}"));
            before.save(&crash_directory).unwrap();
            atomic_write(
                &crash_directory.join("lifecycle.wal"),
                &serde_json::to_vec(&wal).unwrap(),
            )
            .unwrap();
            if stage >= 1 {
                let checkpoint = wal.checkpoints.last().unwrap();
                atomic_write(
                    &crash_directory.join(runtime_file(checkpoint.agent_id)),
                    &serde_json::to_vec(checkpoint).unwrap(),
                )
                .unwrap();
            }
            if stage >= 2 {
                materialize_wal(&crash_directory, &wal).unwrap();
            }
            let loaded = PopulationCoordinator::load(&crash_directory).unwrap();
            assert_eq!(loaded.processed_events["durable-birth"], first);
            assert_eq!(loaded.individuals.len(), 2);
            assert!(!crash_directory.join("lifecycle.wal").exists());
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn mutation_off_is_exact_and_mutation_on_is_deterministic() {
        let mut population = PopulationCoordinator::new(44, 1);
        population.set_capacity(2).unwrap();
        population.lifecycle_config.mutation_enabled = false;
        eligible(&mut population, 1);
        let parent = population.individuals[&1].genome.clone();
        let child = population.reproduce(1).unwrap();
        assert_eq!(population.individuals[&child].genome, parent);

        let genome = DevelopmentalGenome::founder(44, 1);
        assert_eq!(
            genome.asexual_offspring_with_mutation(44, 1, 2, 1, true),
            genome.asexual_offspring_with_mutation(44, 1, 2, 1, true)
        );
    }

    #[test]
    fn experiment_001_bounded_turnover() {
        for mutation_enabled in [false, true] {
            let mut population = PopulationCoordinator::new_population_with_mode(
                101,
                1,
                4,
                4,
                ExperimentMode::Homeostatic,
            )
            .unwrap();
            population
                .set_lifecycle_config(LifecycleConfig {
                    maturity_decisions: 0,
                    minimum_reproductive_reserve: 1,
                    reproductive_cost: 1,
                    mutation_enabled,
                })
                .unwrap();
            for birth in 0..16 {
                let parent_id = *population.individuals.keys().next().unwrap();
                let victim_id = *population
                    .individuals
                    .keys()
                    .find(|&&id| id != parent_id)
                    .unwrap();
                let victim_slot = population.individuals[&victim_id].body_slot.0;
                population
                    .process_lifecycle(LifecycleCommand {
                        event_id: format!("{mutation_enabled}-death-{birth}"),
                        operation: LifecycleOperation::Death {
                            individual_id: victim_id,
                            body_slot: victim_slot,
                        },
                    })
                    .unwrap();
                eligible(&mut population, parent_id);
                let parent_genome = population.individuals[&parent_id].genome.clone();
                let result = population
                    .process_lifecycle(LifecycleCommand {
                        event_id: format!("{mutation_enabled}-birth-{birth}"),
                        operation: LifecycleOperation::Reproduce {
                            parent_id,
                            body_slot: population.individuals[&parent_id].body_slot.0,
                            infrastructure_failures: 0,
                        },
                    })
                    .unwrap();
                let PopulationCommandResult::Born { individual_id, .. } = result else {
                    panic!("eligible bounded birth was blocked");
                };
                if !mutation_enabled {
                    assert_eq!(population.individuals[&individual_id].genome, parent_genome);
                }
                assert_eq!(
                    population.individuals[&individual_id]
                        .runtime
                        .state()
                        .decision_tick,
                    0
                );
            }
            assert_eq!(population.individuals.len(), 4);
            assert_eq!(
                population
                    .lifecycle
                    .iter()
                    .filter(|event| matches!(event, LifecycleEvent::Birth { .. }))
                    .count(),
                16
            );
        }
    }
}
