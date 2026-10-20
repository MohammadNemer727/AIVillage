//! Exact, renderer-independent persistence of authoritative world state.

use serde::{Deserialize, Serialize};

use crate::{
    AgentId, AgentSlot, BLANK_SIGNAL_COUNT, BlankSignal, CulturalTransmissionControls,
    GENOME_WEIGHTS, Genome, GridPosition, HIDDEN, OUTPUTS, SignalIntervention, Tick, World,
    WorldSeed, WorldSettings,
};

/// Exact V1 world-state payload used by versioned format envelopes.
///
/// This type is intentionally opaque: construct it with [`World::checkpoint`] and restore it
/// with [`World::from_checkpoint`] so derived spatial data is always rebuilt canonically.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorldCheckpoint {
    seed: u64,
    tick: u64,
    settings: SettingsCheckpoint,
    slots: Vec<SlotCheckpoint>,
    free_slots: Vec<u32>,
    positions: Vec<PositionCheckpoint>,
    ages: Vec<u64>,
    energies: Vec<u16>,
    genomes: Vec<GenomeCheckpoint>,
    recurrent_states: Vec<Vec<i16>>,
    plastic_biases: Vec<Vec<i16>>,
    parents: Vec<ParentCheckpoint>,
    lineage_depths: Vec<u32>,
    resources: Vec<u16>,
    materials: Vec<u16>,
    inventories: Vec<u8>,
    blocks: Vec<u16>,
    signals: Vec<u8>,
    previous_signals: Vec<u8>,
    signal_intervention: SignalIntervention,
    transmission_controls: CulturalTransmissionControls,
    births: u64,
    deaths: u64,
    signals_emitted: u64,
    signal_energy_spent: u64,
    signal_observations: u64,
    symbol_emissions: [u64; BLANK_SIGNAL_COUNT],
    symbol_observations: [u64; BLANK_SIGNAL_COUNT],
    genetic_births: u64,
    learning_updates: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct SettingsCheckpoint {
    width: u16,
    height: u16,
    initial_agents: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct SlotCheckpoint {
    generation: u32,
    alive: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct PositionCheckpoint {
    x: u16,
    y: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct GenomeCheckpoint {
    weights: Vec<i16>,
    biases: Vec<i16>,
    mutation_rate_ppm: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct ParentCheckpoint {
    has_parent: bool,
    slot: u32,
    generation: u32,
}

/// An invalid or incompatible authoritative checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckpointError {
    /// The persisted geometry is invalid.
    InvalidSettings,
    /// A slot-indexed field has a different length than the slot table.
    InconsistentSlots,
    /// A grid field has a different length than the world cell count.
    InconsistentGrid,
    /// A genome or controller-state vector has an unexpected fixed size.
    InconsistentController,
    /// A persisted position lies outside the saved geometry.
    PositionOutOfBounds,
    /// A free-list entry does not refer to a dead slot.
    InvalidFreeSlot,
    /// A parent reference cannot identify a slot in this history.
    InvalidParent,
    /// A saved signal is outside the blank alphabet.
    InvalidSignal,
}

impl core::fmt::Display for CheckpointError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSettings => "checkpoint has invalid world settings",
            Self::InconsistentSlots => "checkpoint slot-indexed fields have inconsistent lengths",
            Self::InconsistentGrid => "checkpoint grid fields have inconsistent lengths",
            Self::InconsistentController => {
                "checkpoint controller fields have inconsistent lengths"
            }
            Self::PositionOutOfBounds => "checkpoint contains an out-of-bounds position",
            Self::InvalidFreeSlot => "checkpoint free list contains a live or unknown slot",
            Self::InvalidParent => "checkpoint contains an unknown parent slot",
            Self::InvalidSignal => "checkpoint contains an invalid blank signal",
        })
    }
}

impl std::error::Error for CheckpointError {}

impl World {
    /// Captures every future-relevant field of this authoritative world.
    #[must_use]
    pub fn checkpoint(&self) -> WorldCheckpoint {
        WorldCheckpoint {
            seed: self.seed.get(),
            tick: self.tick.get(),
            settings: SettingsCheckpoint {
                width: self.settings.width,
                height: self.settings.height,
                initial_agents: self.settings.initial_agents,
            },
            slots: self
                .slots
                .iter()
                .map(|slot| SlotCheckpoint {
                    generation: slot.generation,
                    alive: slot.alive,
                })
                .collect(),
            free_slots: self.free_slots.clone(),
            positions: self
                .positions
                .iter()
                .map(|position| PositionCheckpoint {
                    x: position.x,
                    y: position.y,
                })
                .collect(),
            ages: self.ages.clone(),
            energies: self.energies.clone(),
            genomes: self
                .genomes
                .iter()
                .map(|genome| GenomeCheckpoint {
                    weights: genome.weights.to_vec(),
                    biases: genome.biases.to_vec(),
                    mutation_rate_ppm: genome.mutation_rate_ppm,
                })
                .collect(),
            recurrent_states: self
                .recurrent_states
                .iter()
                .map(|state| state.to_vec())
                .collect(),
            plastic_biases: self
                .plastic_biases
                .iter()
                .map(|state| state.to_vec())
                .collect(),
            parents: self
                .parents
                .iter()
                .map(|parent| ParentCheckpoint {
                    has_parent: parent.is_some(),
                    slot: parent.map_or(0, AgentId::slot),
                    generation: parent.map_or(0, AgentId::generation),
                })
                .collect(),
            lineage_depths: self.lineage_depths.clone(),
            resources: self.resources.clone(),
            materials: self.materials.clone(),
            inventories: self.inventories.clone(),
            blocks: self.blocks.clone(),
            signals: self
                .signals
                .iter()
                .copied()
                .map(signal_checkpoint_value)
                .collect(),
            previous_signals: self
                .previous_signals
                .iter()
                .copied()
                .map(signal_checkpoint_value)
                .collect(),
            signal_intervention: self.signal_intervention,
            transmission_controls: self.transmission_controls,
            births: self.births,
            deaths: self.deaths,
            signals_emitted: self.signals_emitted,
            signal_energy_spent: self.signal_energy_spent,
            signal_observations: self.signal_observations,
            symbol_emissions: self.symbol_emissions,
            symbol_observations: self.symbol_observations,
            genetic_births: self.genetic_births,
            learning_updates: self.learning_updates,
        }
    }

    /// Restores an exact authoritative world from a validated checkpoint.
    ///
    /// Derived spatial bins are rebuilt rather than trusted from storage.
    ///
    /// # Errors
    ///
    /// Returns an error when the persisted world violates an authoritative-state invariant.
    #[allow(
        clippy::too_many_lines,
        reason = "Exact restoration keeps all invariant validation adjacent to field assignment."
    )]
    pub fn from_checkpoint(checkpoint: WorldCheckpoint) -> Result<Self, CheckpointError> {
        let settings = WorldSettings::new(
            checkpoint.settings.width,
            checkpoint.settings.height,
            checkpoint.settings.initial_agents,
        )
        .map_err(|_| CheckpointError::InvalidSettings)?;
        let slot_count = checkpoint.slots.len();
        let slot_fields = [
            checkpoint.positions.len(),
            checkpoint.ages.len(),
            checkpoint.energies.len(),
            checkpoint.genomes.len(),
            checkpoint.recurrent_states.len(),
            checkpoint.plastic_biases.len(),
            checkpoint.parents.len(),
            checkpoint.lineage_depths.len(),
            checkpoint.inventories.len(),
            checkpoint.signals.len(),
            checkpoint.previous_signals.len(),
        ];
        if slot_fields.iter().any(|length| *length != slot_count) {
            return Err(CheckpointError::InconsistentSlots);
        }
        let cell_count = usize::from(settings.width) * usize::from(settings.height);
        if checkpoint.resources.len() != cell_count
            || checkpoint.materials.len() != cell_count
            || checkpoint.blocks.len() != cell_count
        {
            return Err(CheckpointError::InconsistentGrid);
        }
        if checkpoint.free_slots.iter().any(|slot| {
            usize::try_from(*slot)
                .ok()
                .and_then(|index| checkpoint.slots.get(index))
                .is_none_or(|entry| entry.alive)
        }) {
            return Err(CheckpointError::InvalidFreeSlot);
        }
        let mut world = Self::with_settings_unchecked(WorldSeed::new(checkpoint.seed), settings);
        world.tick = Tick::new(checkpoint.tick);
        world.slots = checkpoint
            .slots
            .into_iter()
            .map(|slot| AgentSlot {
                generation: slot.generation,
                alive: slot.alive,
            })
            .collect();
        world.free_slots = checkpoint.free_slots;
        world.positions = checkpoint
            .positions
            .into_iter()
            .map(|position| GridPosition::new(position.x, position.y))
            .collect();
        world.physics_positions = world
            .positions
            .iter()
            .map(|position| crate::Position {
                x: crate::Fixed::from_units(i32::from(position.x())),
                y: crate::Fixed::from_units(i32::from(position.y())),
            })
            .collect();
        world.velocities = vec![crate::Velocity::default(); slot_count];
        world.orientations = vec![crate::Angle::default(); slot_count];
        world.mouth_open = vec![crate::Fixed::default(); slot_count];
        // V1 checkpoints intentionally do not serialize V2 ecology. Keep the parallel buffers
        // well-formed so a restored V1 world can still allocate/recycle slots safely.
        world.fats = vec![30; slot_count];
        world.guts = vec![0; slot_count];
        world.hydrations = vec![90; slot_count];
        world.fatigues = vec![0; slot_count];
        world.reproductive_reserves = vec![0; slot_count];
        world.toxins = vec![0; slot_count];
        world.secondary_toxins = vec![0; slot_count];
        world.infections = vec![0; slot_count];
        world.tissues = vec![[100; 3]; slot_count];
        world.chemical_adaptation = vec![[0; crate::CHEMICAL_RECEPTOR_COUNT]; slot_count];
        world.acoustic_emissions = vec![0; slot_count];
        world.chemical_emissions = vec![0; slot_count];
        world.visual_displays = vec![0; slot_count];
        world.last_raw_senses = vec![crate::empty_raw_senses(); slot_count];
        world.predictive_brains = vec![crate::PredictiveBrain::default(); slot_count];
        world.developmental_genomes = (0..slot_count)
            .map(|index| {
                world.seeded_developmental_genome(u64::try_from(index).unwrap_or_default())
            })
            .collect();
        world.life_stages = vec![crate::LifeStage::Adult; slot_count];
        world.growth = vec![100; slot_count];
        if world
            .positions
            .iter()
            .any(|position| !world.contains(*position))
        {
            return Err(CheckpointError::PositionOutOfBounds);
        }
        world.ages = checkpoint.ages;
        world.energies = checkpoint.energies;
        world.genomes = checkpoint
            .genomes
            .into_iter()
            .map(|genome| {
                if genome.weights.len() != GENOME_WEIGHTS || genome.biases.len() != HIDDEN + OUTPUTS
                {
                    return Err(CheckpointError::InconsistentController);
                }
                let weights: [i16; GENOME_WEIGHTS] = genome
                    .weights
                    .try_into()
                    .map_err(|_| CheckpointError::InconsistentController)?;
                let biases: [i16; HIDDEN + OUTPUTS] = genome
                    .biases
                    .try_into()
                    .map_err(|_| CheckpointError::InconsistentController)?;
                Ok(Genome {
                    weights,
                    biases,
                    mutation_rate_ppm: genome.mutation_rate_ppm,
                })
            })
            .collect::<Result<_, _>>()?;
        world.recurrent_states = checkpoint
            .recurrent_states
            .into_iter()
            .map(|state| {
                state
                    .try_into()
                    .map_err(|_| CheckpointError::InconsistentController)
            })
            .collect::<Result<_, _>>()?;
        world.plastic_biases = checkpoint
            .plastic_biases
            .into_iter()
            .map(|state| {
                state
                    .try_into()
                    .map_err(|_| CheckpointError::InconsistentController)
            })
            .collect::<Result<_, _>>()?;
        if checkpoint
            .parents
            .iter()
            .filter(|parent| parent.has_parent)
            .any(|parent| {
                usize::try_from(parent.slot)
                    .ok()
                    .and_then(|index| world.slots.get(index))
                    .is_none()
            })
        {
            return Err(CheckpointError::InvalidParent);
        }
        world.parents = checkpoint
            .parents
            .into_iter()
            .map(|parent| {
                parent.has_parent.then_some(AgentId {
                    slot: parent.slot,
                    generation: parent.generation,
                })
            })
            .collect();
        world.lineage_depths = checkpoint.lineage_depths;
        world.resources = checkpoint.resources;
        world.materials = checkpoint.materials;
        world.inventories = checkpoint.inventories;
        world.blocks = checkpoint.blocks;
        world.signals = checkpoint
            .signals
            .into_iter()
            .map(checkpoint_signal)
            .collect::<Result<_, _>>()?;
        world.previous_signals = checkpoint
            .previous_signals
            .into_iter()
            .map(checkpoint_signal)
            .collect::<Result<_, _>>()?;
        world.signal_intervention = checkpoint.signal_intervention;
        world.transmission_controls = checkpoint.transmission_controls;
        world.births = checkpoint.births;
        world.deaths = checkpoint.deaths;
        world.signals_emitted = checkpoint.signals_emitted;
        world.signal_energy_spent = checkpoint.signal_energy_spent;
        world.signal_observations = checkpoint.signal_observations;
        world.symbol_emissions = checkpoint.symbol_emissions;
        world.symbol_observations = checkpoint.symbol_observations;
        world.genetic_births = checkpoint.genetic_births;
        world.learning_updates = checkpoint.learning_updates;
        world.rebuild_spatial_index();
        Ok(world)
    }
}

fn signal_checkpoint_value(signal: Option<BlankSignal>) -> u8 {
    signal.map_or(BlankSignal::COUNT, BlankSignal::get)
}

fn checkpoint_signal(value: u8) -> Result<Option<BlankSignal>, CheckpointError> {
    if value == BlankSignal::COUNT {
        Ok(None)
    } else {
        BlankSignal::new(value)
            .map(Some)
            .ok_or(CheckpointError::InvalidSignal)
    }
}
