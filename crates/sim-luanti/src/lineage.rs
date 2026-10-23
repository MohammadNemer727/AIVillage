//! Exact reconstruction of every individual's inherited genome from a population audit.
//!
//! Live manifests keep genomes only for the living. Founders and births are deterministic in
//! the population seed, durable identities, and generation, so the audit alone recovers the
//! genome of every individual that ever lived, including the dead.

use std::collections::BTreeMap;

use serde::Serialize;
use sim_core::DevelopmentalGenome;
use sim_format::{FounderSpec, PopulationLifecycleEvent, PopulationManifest};

use crate::population::founder_ordinal;

/// One individual's reconstructed inheritance.
#[derive(Clone, Debug, Serialize)]
pub struct LineageEntry {
    /// Never-reused identity.
    pub individual_id: u64,
    /// Asexual parent, absent for founders.
    pub parent_id: Option<u64>,
    /// Founders are generation zero.
    pub generation: u32,
    /// Founder ordinal whose genotype this lineage descends from.
    pub founder_ordinal: u32,
    /// Exact inherited developmental program.
    pub genome: DevelopmentalGenome,
    /// Expressed recurrent connection stride.
    pub connection_stride: usize,
    /// Expressed plasticity gain.
    pub plasticity_scale: i32,
    /// Whether mutation changed an expressed parameter relative to the parent.
    pub expressed_mutation: bool,
}

/// Rebuilds every founder and birth in audit order.
///
/// # Errors
///
/// Fails when a birth names an unknown parent or when a reconstructed genome disagrees with a
/// genome the manifest still holds for a living individual.
pub fn reconstruct(
    manifest: &PopulationManifest,
    first_individual_id: u64,
    founder_count: u32,
    genome_rotation: u32,
) -> Result<Vec<LineageEntry>, String> {
    if founder_count == 0 {
        return Err("founder count must be non-zero".to_owned());
    }
    let mut entries: BTreeMap<u64, LineageEntry> = BTreeMap::new();
    for slot in 0..founder_count {
        let ordinal = founder_ordinal(slot, founder_count, genome_rotation);
        let genome =
            FounderSpec::new(manifest.seed, first_individual_id + u64::from(ordinal)).genome;
        let individual_id = first_individual_id + u64::from(slot);
        entries.insert(
            individual_id,
            entry(individual_id, None, 0, ordinal, genome, false),
        );
    }
    for event in &manifest.lifecycle {
        let PopulationLifecycleEvent::Birth {
            individual_id,
            parent_id,
            generation,
            ..
        } = event
        else {
            continue;
        };
        let parent = entries
            .get(parent_id)
            .ok_or_else(|| format!("birth of {individual_id} names unknown parent {parent_id}"))?;
        let genome = parent.genome.asexual_offspring_with_mutation(
            manifest.seed,
            *parent_id,
            *individual_id,
            *generation,
            manifest.mutation_enabled,
        );
        let config = genome.default_brain_config();
        let expressed_mutation = config.connection_stride != parent.connection_stride
            || config.plasticity_scale != parent.plasticity_scale;
        let child = entry(
            *individual_id,
            Some(*parent_id),
            *generation,
            parent.founder_ordinal,
            genome,
            expressed_mutation,
        );
        entries.insert(*individual_id, child);
    }
    for living in &manifest.individuals {
        if entries
            .get(&living.individual_id)
            .is_none_or(|entry| entry.genome != living.genome)
        {
            return Err(format!(
                "reconstructed genome of {} disagrees with the manifest",
                living.individual_id
            ));
        }
    }
    Ok(entries.into_values().collect())
}

fn entry(
    individual_id: u64,
    parent_id: Option<u64>,
    generation: u32,
    founder_ordinal: u32,
    genome: DevelopmentalGenome,
    expressed_mutation: bool,
) -> LineageEntry {
    let config = genome.default_brain_config();
    LineageEntry {
        individual_id,
        parent_id,
        generation,
        founder_ordinal,
        connection_stride: config.connection_stride,
        plasticity_scale: config.plasticity_scale,
        genome,
        expressed_mutation,
    }
}

#[cfg(test)]
mod tests {
    use sim_core::runtime::ExperimentMode;

    use super::*;
    use crate::{LifecycleConfig, PopulationCoordinator};

    #[test]
    fn reconstruction_matches_every_live_genome_after_rotated_births() {
        let mut population = PopulationCoordinator::new_rotated_population_with_mode(
            8,
            1,
            2,
            4,
            ExperimentMode::PredictiveBaseline,
            1,
        )
        .unwrap();
        population
            .set_lifecycle_config(LifecycleConfig {
                maturity_decisions: 0,
                ..LifecycleConfig::default()
            })
            .unwrap();
        let first = population.reproduce(1).unwrap();
        let second = population.reproduce(first).unwrap();
        let manifest = population.manifest();
        let lineage = reconstruct(&manifest, 1, 2, 1).unwrap();
        assert_eq!(lineage.len(), 4);
        assert_eq!(lineage[0].founder_ordinal, 1);
        assert_eq!(lineage[1].founder_ordinal, 0);
        let grandchild = lineage
            .iter()
            .find(|entry| entry.individual_id == second)
            .unwrap();
        assert_eq!(grandchild.generation, 2);
        assert_eq!(grandchild.founder_ordinal, 1);
        assert!(reconstruct(&manifest, 1, 2, 0).is_err());
    }
}
