//! Genotype to phenotype expression.
//!
//! A [`DevelopmentalGenome`] is sixteen loci in `0..=64`. Expression turns each locus into one
//! bounded, interpretable developmental parameter, never into a behaviour: gains, thresholds,
//! rates, and life-history timings that the live runtime already has places to apply.
//!
//! Expression is versioned so that nothing changes silently. [`ExpressionVersion::Legacy`] is
//! the original behaviour (only loci 6 and 13 are read, everything else is a fixed constant) and
//! is an exact no-op on every action and fingerprint, so retained traces keep replaying.
//! [`ExpressionVersion::V2`] gives all sixteen loci a role.
//!
//! Every parameter is a monotone integer map of its locus over a fixed, documented range, with
//! enough resolution that a one- or two-step mutation moves it. The coarse existing maps for loci
//! 6 and 13 are kept exactly as they were.

use serde::{Deserialize, Serialize};

use crate::{
    DevelopmentalGenome,
    environment::{Interoception, PrimitiveAction},
};

/// Which genotype-to-phenotype map a runtime is built with.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ExpressionVersion {
    /// Only loci 6 and 13 are expressed; every other parameter is its legacy constant.
    #[default]
    Legacy,
    /// All sixteen loci are expressed as the parameters documented on [`LOCUS_ROLES`].
    V2,
}

/// Fixed-point scale of the gains: 64 is 1.0.
pub const GAIN_UNIT: u16 = 64;
/// Felt fullness (0..=255) below which the hunger drive acts.
pub const HUNGER_DRIVE_BELOW: u16 = 96;

/// One locus's role in [`ExpressionVersion::V2`].
#[derive(Clone, Copy, Debug)]
pub struct LocusRole {
    /// Locus index.
    pub locus: usize,
    /// Parameter name.
    pub parameter: &'static str,
    /// Plain-language meaning.
    pub meaning: &'static str,
    /// Inclusive range of the parameter, from locus 0 to locus 64.
    pub range: (i32, i32),
    /// Where the live runtime applies it.
    pub applied_in: &'static str,
}

/// The full v2 map, one row per locus, in locus order.
pub const LOCUS_ROLES: [LocusRole; DevelopmentalGenome::LOCUS_COUNT] = [
    role(
        0,
        "thrust_gain",
        "strength of forward and backward drive, in 1/64 (64 = 1x)",
        (32, 160),
        "action shaping",
    ),
    role(
        1,
        "turn_gain",
        "strength of turning, in 1/64 (64 = 1x)",
        (32, 160),
        "action shaping",
    ),
    role(
        2,
        "motor_threshold",
        "smallest thrust or turn that is carried out (movement initiation)",
        (0, 64),
        "action shaping",
    ),
    role(
        3,
        "hunger_drive",
        "forward drive while felt fullness is low",
        (0, 128),
        "action shaping",
    ),
    role(
        4,
        "pain_flight",
        "forward drive and turn while hurt",
        (0, 256),
        "action shaping",
    ),
    role(
        5,
        "jaw_gain",
        "strength of the jaw, in 1/64 (64 = 1x)",
        (32, 160),
        "action shaping",
    ),
    role(
        6,
        "connection_stride",
        "offset of the four sparse recurrent connections (1 + locus % 15)",
        (1, 15),
        "brain",
    ),
    role(
        7,
        "predictor_learning_divisor",
        "divisor of the predictor's gradient step; smaller learns faster",
        (4096, 256),
        "recurrent predictor",
    ),
    role(
        8,
        "predictor_update_period",
        "decisions between predictor updates; smaller updates more often",
        (4, 1),
        "recurrent predictor",
    ),
    role(
        9,
        "predictor_rehearsal_batch",
        "replay samples rehearsed per predictor update",
        (4, 68),
        "recurrent predictor",
    ),
    role(
        10,
        "episodic_surprise_write",
        "prediction surprise needed to store an unremarkable event; smaller stores more",
        (66, 2),
        "episodic memory",
    ),
    role(
        11,
        "episodic_retention",
        "starting retention of a stored event",
        (512, 2048),
        "episodic memory",
    ),
    role(
        12,
        "maturity_percent",
        "reproductive maturity as a percentage of the configured maturity",
        (50, 150),
        "lifecycle",
    ),
    role(
        13,
        "plasticity_scale",
        "multiplier on the learned-bias update (1 + locus / 16)",
        (1, 5),
        "brain",
    ),
    role(
        14,
        "reserve_threshold",
        "reproductive reserve needed before reproducing",
        (120, 220),
        "lifecycle",
    ),
    role(
        15,
        "reproductive_cost",
        "reproductive reserve spent on a birth",
        (40, 120),
        "lifecycle",
    ),
];

const fn role(
    locus: usize,
    parameter: &'static str,
    meaning: &'static str,
    range: (i32, i32),
    applied_in: &'static str,
) -> LocusRole {
    LocusRole {
        locus,
        parameter,
        meaning,
        range,
        applied_in,
    }
}

/// The bounded developmental parameters of one individual.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DevelopmentalPhenotype {
    /// Which map produced it.
    pub version: ExpressionVersion,
    /// Thrust gain, in 1/64.
    pub thrust_gain: u16,
    /// Turn gain, in 1/64.
    pub turn_gain: u16,
    /// Deadzone below which thrust and turn are not carried out.
    pub motor_threshold: u16,
    /// Forward drive while hungry.
    pub hunger_drive: u16,
    /// Forward drive and turn while hurt.
    pub pain_flight: u16,
    /// Jaw gain, in 1/64.
    pub jaw_gain: u16,
    /// Recurrent connection stride.
    pub connection_stride: usize,
    /// Predictor gradient divisor.
    pub predictor_learning_divisor: u16,
    /// Decisions between predictor updates.
    pub predictor_update_period: u8,
    /// Replay samples per predictor update.
    pub predictor_rehearsal_batch: u8,
    /// Surprise needed to store an unremarkable event.
    pub episodic_surprise_write: u16,
    /// Starting retention of a stored event.
    pub episodic_retention: u16,
    /// Reproductive maturity as a percentage of the configured maturity.
    pub maturity_percent: u16,
    /// Plasticity multiplier.
    pub plasticity_scale: i32,
    /// Reserve needed to reproduce.
    pub reserve_threshold: u16,
    /// Reserve spent on a birth.
    pub reproductive_cost: u16,
}

impl Default for DevelopmentalPhenotype {
    fn default() -> Self {
        Self::legacy_constants(1, 1)
    }
}

/// Linear integer map of a locus onto `range` (either direction), exact at both ends.
#[must_use]
pub fn lerp(locus: u8, range: (i32, i32)) -> i32 {
    let value = i32::from(locus.min(DevelopmentalGenome::LOCUS_MAX));
    let span = i32::from(DevelopmentalGenome::LOCUS_MAX);
    range.0 + (range.1 - range.0) * value / span
}

impl DevelopmentalPhenotype {
    /// Every legacy constant, with the two legacy-expressed brain parameters supplied.
    #[must_use]
    pub const fn legacy_constants(connection_stride: usize, plasticity_scale: i32) -> Self {
        Self {
            version: ExpressionVersion::Legacy,
            thrust_gain: GAIN_UNIT,
            turn_gain: GAIN_UNIT,
            motor_threshold: 0,
            hunger_drive: 0,
            pain_flight: 0,
            jaw_gain: GAIN_UNIT,
            connection_stride,
            predictor_learning_divisor: 1_024,
            predictor_update_period: 2,
            predictor_rehearsal_batch: 8,
            episodic_surprise_write: 6,
            episodic_retention: 1_024,
            maturity_percent: 100,
            plasticity_scale,
            reserve_threshold: 160,
            reproductive_cost: 80,
        }
    }

    /// Whether every parameter is inside the range its locus can produce, so checkpoints that
    /// carry a phenotype cannot smuggle in an unbounded value.
    #[must_use]
    pub fn is_in_range(&self) -> bool {
        if self.version == ExpressionVersion::Legacy {
            return self.connection_stride >= 1 && self.plasticity_scale >= 1;
        }
        let within = |value: i32, range: (i32, i32)| {
            value >= range.0.min(range.1) && value <= range.0.max(range.1)
        };
        let values = [
            i32::from(self.thrust_gain),
            i32::from(self.turn_gain),
            i32::from(self.motor_threshold),
            i32::from(self.hunger_drive),
            i32::from(self.pain_flight),
            i32::from(self.jaw_gain),
            i32::try_from(self.connection_stride).unwrap_or(i32::MAX),
            i32::from(self.predictor_learning_divisor),
            i32::from(self.predictor_update_period),
            i32::from(self.predictor_rehearsal_batch),
            i32::from(self.episodic_surprise_write),
            i32::from(self.episodic_retention),
            i32::from(self.maturity_percent),
            self.plasticity_scale,
            i32::from(self.reserve_threshold),
            i32::from(self.reproductive_cost),
        ];
        values
            .iter()
            .zip(LOCUS_ROLES.iter())
            .all(|(value, role)| within(*value, role.range))
    }

    /// Applies the motor parameters to the action the body is about to carry out.
    ///
    /// Gain scales thrust, turn and jaw; the threshold removes movements too small to start;
    /// hunger and pain then add their drives. [`ExpressionVersion::Legacy`] returns the action
    /// untouched.
    #[must_use]
    pub fn shape_action(
        &self,
        action: PrimitiveAction,
        felt: Option<Interoception>,
    ) -> PrimitiveAction {
        if self.version == ExpressionVersion::Legacy {
            return action;
        }
        let scale = |amplitude: i16, gain: u16| -> i16 {
            let scaled = i32::from(amplitude) * i32::from(gain) / i32::from(GAIN_UNIT);
            i16::try_from(scaled.clamp(-256, 256)).unwrap_or_default()
        };
        let threshold = i16::try_from(self.motor_threshold).unwrap_or(i16::MAX);
        let initiate = |amplitude: i16| {
            if amplitude.abs() < threshold {
                0
            } else {
                amplitude
            }
        };
        let mut shaped = action;
        shaped.thrust = initiate(scale(action.thrust, self.thrust_gain));
        shaped.turn = initiate(scale(action.turn, self.turn_gain));
        let jaw = u32::from(action.jaw) * u32::from(self.jaw_gain) / u32::from(GAIN_UNIT);
        shaped.jaw = u16::try_from(jaw.min(256)).unwrap_or(256);
        if let Some(felt) = felt {
            if felt.fullness < HUNGER_DRIVE_BELOW {
                shaped.thrust = shaped
                    .thrust
                    .max(i16::try_from(self.hunger_drive).unwrap_or(i16::MAX));
            }
            if felt.pain > 0 {
                let flight = i16::try_from(self.pain_flight).unwrap_or(i16::MAX);
                shaped.thrust = shaped.thrust.max(flight);
                if shaped.turn == 0 {
                    shaped.turn = flight / 2;
                }
            }
        }
        shaped
    }
}

impl DevelopmentalGenome {
    /// Expresses this genome under `version`.
    #[must_use]
    pub fn phenotype(&self, version: ExpressionVersion) -> DevelopmentalPhenotype {
        let config = self.default_brain_config();
        let legacy = DevelopmentalPhenotype::legacy_constants(
            config.connection_stride,
            config.plasticity_scale,
        );
        if version == ExpressionVersion::Legacy {
            return legacy;
        }
        let at = |locus: usize| lerp(self.loci()[locus], LOCUS_ROLES[locus].range);
        DevelopmentalPhenotype {
            version,
            thrust_gain: u16::try_from(at(0)).unwrap_or(GAIN_UNIT),
            turn_gain: u16::try_from(at(1)).unwrap_or(GAIN_UNIT),
            motor_threshold: u16::try_from(at(2)).unwrap_or_default(),
            hunger_drive: u16::try_from(at(3)).unwrap_or_default(),
            pain_flight: u16::try_from(at(4)).unwrap_or_default(),
            jaw_gain: u16::try_from(at(5)).unwrap_or(GAIN_UNIT),
            connection_stride: config.connection_stride,
            predictor_learning_divisor: u16::try_from(at(7)).unwrap_or(1_024),
            predictor_update_period: u8::try_from(at(8)).unwrap_or(2),
            predictor_rehearsal_batch: u8::try_from(at(9)).unwrap_or(8),
            episodic_surprise_write: u16::try_from(at(10)).unwrap_or(6),
            episodic_retention: u16::try_from(at(11)).unwrap_or(1_024),
            maturity_percent: u16::try_from(at(12)).unwrap_or(100),
            plasticity_scale: config.plasticity_scale,
            reserve_threshold: u16::try_from(at(14)).unwrap_or(160),
            reproductive_cost: u16::try_from(at(15)).unwrap_or(80),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn felt(fullness: u16, pain: u16) -> Interoception {
        Interoception {
            health: 255,
            fullness,
            air: 255,
            pain,
        }
    }

    #[test]
    fn the_map_has_one_row_per_locus_in_order_and_covers_all_sixteen() {
        for (index, role) in LOCUS_ROLES.iter().enumerate() {
            assert_eq!(role.locus, index);
        }
        let mut names: Vec<_> = LOCUS_ROLES.iter().map(|role| role.parameter).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), DevelopmentalGenome::LOCUS_COUNT);
    }

    #[test]
    fn lerp_is_exact_at_both_ends_and_monotone_in_either_direction() {
        for role in LOCUS_ROLES {
            assert_eq!(lerp(0, role.range), role.range.0);
            assert_eq!(
                lerp(DevelopmentalGenome::LOCUS_MAX, role.range),
                role.range.1
            );
            let values: Vec<_> = (0..=64).map(|locus| lerp(locus, role.range)).collect();
            let increasing = role.range.1 >= role.range.0;
            assert!(values.windows(2).all(|pair| if increasing {
                pair[1] >= pair[0]
            } else {
                pair[1] <= pair[0]
            }));
        }
    }

    #[test]
    fn every_locus_produces_a_value_in_range_at_every_setting() {
        for locus in 0..DevelopmentalGenome::LOCUS_COUNT {
            for value in 0..=DevelopmentalGenome::LOCUS_MAX {
                let genome = DevelopmentalGenome::uniform(32, 12_000).with_locus(locus, value);
                assert!(
                    genome.phenotype(ExpressionVersion::V2).is_in_range(),
                    "{locus}={value}"
                );
            }
        }
    }

    #[test]
    fn legacy_matches_the_original_brain_config_and_never_touches_an_action() {
        let genome = DevelopmentalGenome::founder(9, 3);
        let config = genome.default_brain_config();
        let legacy = genome.phenotype(ExpressionVersion::Legacy);
        assert_eq!(legacy.connection_stride, config.connection_stride);
        assert_eq!(legacy.plasticity_scale, config.plasticity_scale);
        let action = PrimitiveAction {
            thrust: -77,
            turn: 31,
            jaw: 200,
            ..PrimitiveAction::default()
        };
        assert_eq!(legacy.shape_action(action, Some(felt(0, 255))), action);
    }

    #[test]
    fn v2_keeps_the_two_original_brain_roles_exactly() {
        for seed in 1..40 {
            let genome = DevelopmentalGenome::founder(seed, 1);
            let config = genome.default_brain_config();
            let expressed = genome.phenotype(ExpressionVersion::V2);
            assert_eq!(expressed.connection_stride, config.connection_stride);
            assert_eq!(expressed.plasticity_scale, config.plasticity_scale);
        }
    }

    #[test]
    fn gain_threshold_and_drives_shape_the_action() {
        let base = DevelopmentalGenome::uniform(32, 12_000);
        let walk = PrimitiveAction {
            thrust: 200,
            turn: 40,
            ..PrimitiveAction::default()
        };
        let weak = base.with_locus(0, 0).phenotype(ExpressionVersion::V2);
        let strong = base.with_locus(0, 64).phenotype(ExpressionVersion::V2);
        assert!(weak.shape_action(walk, None).thrust < strong.shape_action(walk, None).thrust);
        let dull = base.with_locus(2, 64).phenotype(ExpressionVersion::V2);
        assert_eq!(
            dull.shape_action(walk, None).turn,
            0,
            "a small turn is below the threshold"
        );
        let driven = base.with_locus(3, 64).phenotype(ExpressionVersion::V2);
        let idle = PrimitiveAction::default();
        assert_eq!(driven.shape_action(idle, Some(felt(255, 0))).thrust, 0);
        assert!(driven.shape_action(idle, Some(felt(10, 0))).thrust > 0);
        let flighty = base.with_locus(4, 64).phenotype(ExpressionVersion::V2);
        assert_eq!(flighty.shape_action(idle, Some(felt(255, 0))).thrust, 0);
        let hurt = flighty.shape_action(idle, Some(felt(255, 30)));
        assert!(hurt.thrust > 0 && hurt.turn > 0);
    }

    #[test]
    fn the_checkpointed_phenotype_rejects_an_out_of_range_value() {
        let mut phenotype =
            DevelopmentalGenome::uniform(32, 12_000).phenotype(ExpressionVersion::V2);
        assert!(phenotype.is_in_range());
        phenotype.thrust_gain = 9_999;
        assert!(!phenotype.is_in_range());
    }
}
