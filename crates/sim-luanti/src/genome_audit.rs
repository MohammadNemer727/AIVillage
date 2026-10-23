//! Experiment 004 audit: which loci reach the live runtime and how birth mutation actually
//! samples them.
//!
//! The mutation draw is replicated here only to attribute each outcome (draw hit, zero step,
//! clamped no-op). The replica must reproduce the real
//! [`DevelopmentalGenome::asexual_offspring_with_mutation`] exactly; the audit fails otherwise,
//! so it cannot drift from the code it reports on.

#![allow(
    clippy::cast_precision_loss,
    reason = "counts are far below 2^52, and the ratios are reported, not decided on"
)]

use serde::Serialize;
use sim_core::DevelopmentalGenome;
use sim_format::FounderSpec;
use sim_kernels::derive;

const MUTATION_SALT: u64 = 0x504F_5000;
const FOUNDERS_PER_SEED: u64 = 4;

/// Where one locus is read.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct LocusUse {
    /// Locus index.
    pub locus: usize,
    /// Parameter the live runtime derives from it, if any.
    pub live: Option<&'static str>,
    /// Role in the legacy simulated world, if any. Unrelated to the live runtime.
    pub legacy_world: Option<&'static str>,
}

/// Source audit of every read of every locus, 2026-10-23 (`DevelopmentalGenome::locus` callers).
pub const LOCUS_USES: [LocusUse; DevelopmentalGenome::LOCUS_COUNT] = [
    legacy(0, "growth target: 64 + locus"),
    legacy(1, "head radius: (32 + locus / 2) / 192 of a body unit"),
    unused(2),
    unused(3),
    unused(4),
    unused(5),
    live(6, "connection_stride = 1 + locus % 15"),
    unused(7),
    unused(8),
    unused(9),
    unused(10),
    unused(11),
    legacy(
        12,
        "metabolic cost: locus / 128, which is 0 for every value up to 64",
    ),
    live(13, "plasticity_scale = 1 + locus / 16"),
    unused(14),
    unused(15),
];

const fn live(locus: usize, role: &'static str) -> LocusUse {
    LocusUse {
        locus,
        live: Some(role),
        legacy_world: None,
    }
}

const fn legacy(locus: usize, role: &'static str) -> LocusUse {
    LocusUse {
        locus,
        live: None,
        legacy_world: Some(role),
    }
}

const fn unused(locus: usize) -> LocusUse {
    LocusUse {
        locus,
        live: None,
        legacy_world: None,
    }
}

/// How births sample one locus.
#[derive(Clone, Debug, Default, Serialize)]
pub struct LocusMutation {
    /// Locus index.
    pub locus: usize,
    /// Births in which the per-locus draw selected this locus for mutation.
    pub draws_hit: u64,
    /// Hits whose step was zero (no change).
    pub zero_steps: u64,
    /// Hits with a nonzero step that was clamped back to the same value.
    pub clamped_no_ops: u64,
    /// Births in which the stored value changed.
    pub changed: u64,
    /// Births in which a value the live runtime reads changed its derived parameter.
    pub live_parameter_changed: u64,
    /// Applied steps after clamping, indexed by step + 2 (-2, -1, 0, +1, +2).
    pub applied_steps: [u64; 5],
}

/// Whole-population mutation audit.
#[derive(Clone, Debug, Serialize)]
pub struct MutationAudit {
    /// Births simulated.
    pub births: u64,
    /// Per-locus mutation probability in parts per million.
    pub mutation_rate_ppm: u16,
    /// Source audit of locus reads.
    pub uses: Vec<LocusUse>,
    /// Per-locus outcomes.
    pub loci: Vec<LocusMutation>,
    /// Births by number of loci that changed, index 0..=16.
    pub loci_changed_per_birth: [u64; DevelopmentalGenome::LOCUS_COUNT + 1],
    /// Fraction of births with at least one changed locus.
    pub fraction_any_locus_changed: f64,
    /// Fraction of births in which the live runtime's derived parameters changed.
    pub fraction_live_parameter_changed: f64,
    /// Expected births until one changes a live parameter.
    pub expected_births_per_live_change: f64,
    /// Expected live-parameter changes in a 54-birth experiment (Experiment 002's size).
    pub expected_live_changes_in_54_births: f64,
    /// Probability of seeing none in 54 births if births are independent.
    pub probability_none_in_54_births: f64,
    /// Fraction of founder locus values sitting at the clamp limits 0 or 64.
    pub founder_values_at_clamp_limit: f64,
    /// Whether the replica reproduced the real mutation exactly on every birth.
    pub replica_matches_real_mutation: bool,
}

fn live_parameters(genome: &DevelopmentalGenome) -> (usize, i32) {
    let config = genome.default_brain_config();
    (config.connection_stride, config.plasticity_scale)
}

/// Simulates `births` single births from founder genomes across many seeds.
///
/// # Panics
///
/// Panics if `births` is zero.
#[must_use]
pub fn audit(births: u64) -> MutationAudit {
    assert!(births > 0, "an audit needs at least one birth");
    let mut loci: Vec<LocusMutation> = (0..DevelopmentalGenome::LOCUS_COUNT)
        .map(|locus| LocusMutation {
            locus,
            ..LocusMutation::default()
        })
        .collect();
    let mut per_birth = [0_u64; DevelopmentalGenome::LOCUS_COUNT + 1];
    let mut live_changed = 0_u64;
    let mut at_limit = 0_u64;
    let mut founder_values = 0_u64;
    let mut rate = 0;
    let mut replica_matches = true;
    for birth in 0..births {
        let seed = 1 + birth / FOUNDERS_PER_SEED;
        let parent_id = 1 + birth % FOUNDERS_PER_SEED;
        let child_id = 1_000_000 + birth;
        let parent = FounderSpec::new(seed, parent_id).genome;
        rate = parent.mutation_rate_ppm();
        let child = parent.asexual_offspring_with_mutation(seed, parent_id, child_id, 1, true);
        let mut replica = *parent.loci();
        let mut changed = 0;
        for (locus, value) in replica.iter_mut().enumerate() {
            let stats = &mut loci[locus];
            founder_values += 1;
            at_limit += u64::from(*value == 0 || *value == DevelopmentalGenome::LOCUS_MAX);
            let random = derive(seed, parent_id, child_id ^ 1, MUTATION_SALT + locus as u64);
            if random % 1_000_000 >= u64::from(rate) {
                continue;
            }
            stats.draws_hit += 1;
            let step = i16::try_from(random % 5).unwrap_or_default() - 2;
            let applied =
                (i16::from(*value) + step).clamp(0, i16::from(DevelopmentalGenome::LOCUS_MAX));
            stats.zero_steps += u64::from(step == 0);
            stats.clamped_no_ops += u64::from(step != 0 && applied == i16::from(*value));
            stats.applied_steps[usize::try_from(applied - i16::from(*value) + 2).unwrap_or(2)] += 1;
            *value = u8::try_from(applied).unwrap_or_default();
            if *value != parent.loci()[locus] {
                stats.changed += 1;
                changed += 1;
            }
        }
        replica_matches &= replica == *child.loci();
        per_birth[changed] += 1;
        let (parent_stride, parent_scale) = live_parameters(&parent);
        let (child_stride, child_scale) = live_parameters(&child);
        if parent_stride != child_stride {
            loci[6].live_parameter_changed += 1;
        }
        if parent_scale != child_scale {
            loci[13].live_parameter_changed += 1;
        }
        live_changed += u64::from(parent_stride != child_stride || parent_scale != child_scale);
    }
    let births_f = births as f64;
    let fraction_live = live_changed as f64 / births_f;
    let any_changed = births - per_birth[0];
    MutationAudit {
        births,
        mutation_rate_ppm: rate,
        uses: LOCUS_USES.to_vec(),
        loci,
        loci_changed_per_birth: per_birth,
        fraction_any_locus_changed: any_changed as f64 / births_f,
        fraction_live_parameter_changed: fraction_live,
        expected_births_per_live_change: if fraction_live > 0.0 {
            1.0 / fraction_live
        } else {
            f64::INFINITY
        },
        expected_live_changes_in_54_births: 54.0 * fraction_live,
        probability_none_in_54_births: (1.0 - fraction_live).powi(54),
        founder_values_at_clamp_limit: at_limit as f64 / founder_values as f64,
        replica_matches_real_mutation: replica_matches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_replica_reproduces_the_real_mutation_on_every_birth() {
        let report = audit(20_000);
        assert!(report.replica_matches_real_mutation);
        assert_eq!(report.loci.len(), DevelopmentalGenome::LOCUS_COUNT);
        let accounted: u64 = report.loci_changed_per_birth.iter().sum();
        assert_eq!(accounted, report.births);
    }

    #[test]
    fn the_source_audit_names_each_live_locus_once() {
        let live: Vec<_> = LOCUS_USES
            .iter()
            .filter(|use_| use_.live.is_some())
            .collect();
        assert_eq!(
            live.iter().map(|use_| use_.locus).collect::<Vec<_>>(),
            [6, 13]
        );
        for (index, locus_use) in LOCUS_USES.iter().enumerate() {
            assert_eq!(locus_use.locus, index);
        }
    }

    #[test]
    fn with_locus_changes_exactly_one_locus_and_clamps() {
        let base = DevelopmentalGenome::uniform(32, 12_000);
        let changed = base.with_locus(5, 200);
        assert_eq!(changed.loci()[5], DevelopmentalGenome::LOCUS_MAX);
        assert_eq!(
            base.loci()
                .iter()
                .zip(changed.loci())
                .filter(|(left, right)| left != right)
                .count(),
            1
        );
    }
}
