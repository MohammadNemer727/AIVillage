//! Genotype to phenotype expression.
//!
//! A [`DevelopmentalGenome`] is sixteen loci in `0..=64`. Expression turns each locus into one
//! bounded, interpretable developmental parameter, never into a behaviour: gains, thresholds,
//! tones, and life-history timings that the live runtime already has places to apply. Every
//! parameter has a direct behavioural or life-history consequence in a baseline founder.
//!
//! Expression is versioned so that nothing changes silently. [`ExpressionVersion::Legacy`] is
//! the original behaviour (only loci 6 and 13 are read, everything else is a fixed constant) and
//! is an exact no-op on every action and fingerprint, so retained traces keep replaying.
//! [`ExpressionVersion::V2`] gives all sixteen loci a role.
//!
//! Every parameter is a monotone integer map of its locus over a fixed, documented range, with
//! enough resolution that a one- or two-step mutation moves it. The coarse existing maps for loci
//! 6 and 13 are kept exactly as they were.
//!
//! Predictor and episodic-memory tuning is deliberately not a gene here: in a baseline founder
//! nothing downstream acts on what those tune, so such a gene would change internal state and no
//! behaviour. They return as genes when the evolving population uses policy retrieval.

use serde::{Deserialize, Serialize};

use crate::{
    DevelopmentalGenome, RawSensoryFrame,
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
/// Largest value of a bounded receptor amplitude.
const RECEPTOR_MAX: i32 = 255;

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
        "smallest thrust or turn that is carried out (movement initiation), out of 256",
        (0, 192),
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
        "visual_gain",
        "sensitivity of the retina to colour and brightness, in 1/64 (64 = 1x)",
        (32, 128),
        "senses",
    ),
    role(
        8,
        "chemical_gain",
        "sensitivity of the chemical receptors, in 1/64 (64 = 1x)",
        (32, 128),
        "senses",
    ),
    role(
        9,
        "locomotor_tone",
        "resting forward drive while fed and unhurt",
        (0, 96),
        "action shaping",
    ),
    role(
        10,
        "interoceptive_gain",
        "how strongly hunger, pain and toxin are felt, in 1/64 (64 = 1x)",
        (16, 128),
        "senses",
    ),
    role(
        11,
        "reserve_recovery",
        "speed at which reproductive reserve refills from being fed, in 1/64 (64 = 1x)",
        (32, 128),
        "lifecycle",
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
    /// Retinal colour and brightness gain, in 1/64.
    pub visual_gain: u16,
    /// Chemical receptor gain, in 1/64.
    pub chemical_gain: u16,
    /// Resting forward drive while fed and unhurt.
    pub locomotor_tone: u16,
    /// Interoceptive (hunger, pain, toxin) gain, in 1/64.
    pub interoceptive_gain: u16,
    /// Reproductive reserve refill speed, in 1/64.
    pub reserve_recovery: u16,
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
            visual_gain: GAIN_UNIT,
            chemical_gain: GAIN_UNIT,
            locomotor_tone: 0,
            interoceptive_gain: GAIN_UNIT,
            reserve_recovery: GAIN_UNIT,
            maturity_percent: 100,
            plasticity_scale,
            reserve_threshold: 160,
            reproductive_cost: 80,
        }
    }

    /// Every parameter as an integer, in locus order, so the value at index `i` is the one locus
    /// `i` expresses.
    #[must_use]
    pub fn parameters(&self) -> [i32; DevelopmentalGenome::LOCUS_COUNT] {
        [
            i32::from(self.thrust_gain),
            i32::from(self.turn_gain),
            i32::from(self.motor_threshold),
            i32::from(self.hunger_drive),
            i32::from(self.pain_flight),
            i32::from(self.jaw_gain),
            i32::try_from(self.connection_stride).unwrap_or(i32::MAX),
            i32::from(self.visual_gain),
            i32::from(self.chemical_gain),
            i32::from(self.locomotor_tone),
            i32::from(self.interoceptive_gain),
            i32::from(self.reserve_recovery),
            i32::from(self.maturity_percent),
            self.plasticity_scale,
            i32::from(self.reserve_threshold),
            i32::from(self.reproductive_cost),
        ]
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
        let values = self.parameters();
        values
            .iter()
            .zip(LOCUS_ROLES.iter())
            .all(|(value, role)| within(*value, role.range))
    }

    /// Scales the external receptor amplitudes the brain receives by this individual's sensitivities.
    /// [`ExpressionVersion::Legacy`] leaves the frame untouched.
    pub fn shape_senses(&self, frame: &mut RawSensoryFrame) {
        if self.version == ExpressionVersion::Legacy {
            return;
        }
        let amplify = |value: u16, gain: u16| -> u16 {
            let scaled = i32::from(value) * i32::from(gain) / i32::from(GAIN_UNIT);
            u16::try_from(scaled.clamp(0, RECEPTOR_MAX)).unwrap_or_default()
        };
        for ray in &mut frame.retinal {
            ray.red = amplify(ray.red, self.visual_gain);
            ray.green = amplify(ray.green, self.visual_gain);
            ray.blue = amplify(ray.blue, self.visual_gain);
            ray.brightness = amplify(ray.brightness, self.visual_gain);
        }
        for chemical in &mut frame.chemicals {
            *chemical = amplify(*chemical, self.chemical_gain);
        }
    }

    /// Scales how strongly the body feels its own hunger, gut, toxin and damage, in the frame the
    /// brain receives. [`ExpressionVersion::Legacy`] leaves the frame untouched.
    pub fn shape_interoception(&self, frame: &mut RawSensoryFrame) {
        if self.version == ExpressionVersion::Legacy {
            return;
        }
        for channel in [
            &mut frame.energy,
            &mut frame.gut_pressure,
            &mut frame.toxin_stress,
            &mut frame.damage,
        ] {
            let scaled =
                i32::from(*channel) * i32::from(self.interoceptive_gain) / i32::from(GAIN_UNIT);
            *channel = u16::try_from(scaled.clamp(0, RECEPTOR_MAX)).unwrap_or_default();
        }
    }

    /// Applies the motor parameters to the action the body is about to carry out.
    ///
    /// Gain scales thrust, turn and jaw; the threshold removes movements too small to start; the
    /// drives and tone then raise forward thrust where they apply. A drive or tone of zero
    /// changes nothing. [`ExpressionVersion::Legacy`] returns the action untouched.
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
        let raise = |thrust: i16, drive: u16| thrust.max(i16::try_from(drive).unwrap_or(i16::MAX));
        if let Some(felt) = felt {
            let hungry = felt.fullness < HUNGER_DRIVE_BELOW;
            let hurt = felt.pain > 0;
            if hungry && self.hunger_drive > 0 {
                shaped.thrust = raise(shaped.thrust, self.hunger_drive);
            }
            if hurt && self.pain_flight > 0 {
                shaped.thrust = raise(shaped.thrust, self.pain_flight);
                if shaped.turn == 0 {
                    shaped.turn = i16::try_from(self.pain_flight / 2).unwrap_or(i16::MAX);
                }
            }
            if !hungry && !hurt && self.locomotor_tone > 0 {
                shaped.thrust = raise(shaped.thrust, self.locomotor_tone);
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
        let unsigned = |locus: usize| u16::try_from(at(locus)).unwrap_or_default();
        DevelopmentalPhenotype {
            version,
            thrust_gain: unsigned(0),
            turn_gain: unsigned(1),
            motor_threshold: unsigned(2),
            hunger_drive: unsigned(3),
            pain_flight: unsigned(4),
            jaw_gain: unsigned(5),
            connection_stride: config.connection_stride,
            visual_gain: unsigned(7),
            chemical_gain: unsigned(8),
            locomotor_tone: unsigned(9),
            interoceptive_gain: unsigned(10),
            reserve_recovery: unsigned(11),
            maturity_percent: unsigned(12),
            plasticity_scale: config.plasticity_scale,
            reserve_threshold: unsigned(14),
            reproductive_cost: unsigned(15),
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
    fn legacy_matches_the_original_brain_config_and_never_touches_anything() {
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
        let mut frame = RawSensoryFrame::EMPTY;
        frame.retinal[0].brightness = 90;
        frame.chemicals[1] = 40;
        let before = frame;
        legacy.shape_senses(&mut frame);
        legacy.shape_interoception(&mut frame);
        assert_eq!(frame, before);
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
    fn gain_threshold_drives_and_tone_shape_the_action() {
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
        let idle = PrimitiveAction::default();
        let driven = base
            .with_locus(3, 64)
            .with_locus(9, 0)
            .phenotype(ExpressionVersion::V2);
        assert_eq!(driven.shape_action(idle, Some(felt(255, 0))).thrust, 0);
        assert_eq!(driven.shape_action(idle, Some(felt(10, 0))).thrust, 128);
        let flighty = base
            .with_locus(4, 64)
            .with_locus(9, 0)
            .phenotype(ExpressionVersion::V2);
        let hurt = flighty.shape_action(idle, Some(felt(255, 30)));
        assert!(hurt.thrust >= 256 && hurt.turn > 0);
        let toned = base.with_locus(9, 64).phenotype(ExpressionVersion::V2);
        assert_eq!(toned.shape_action(idle, Some(felt(255, 0))).thrust, 96);
    }

    #[test]
    fn sensitivities_scale_the_receptor_amplitudes_and_stay_in_range() {
        let base = DevelopmentalGenome::uniform(32, 12_000);
        let mut frame = RawSensoryFrame::EMPTY;
        frame.retinal[3].red = 100;
        frame.chemicals[0] = 100;
        frame.energy = 100;
        frame.damage = 40;
        let dim = base
            .with_locus(7, 0)
            .with_locus(8, 0)
            .with_locus(10, 0)
            .phenotype(ExpressionVersion::V2);
        let keen = base
            .with_locus(7, 64)
            .with_locus(8, 64)
            .with_locus(10, 64)
            .phenotype(ExpressionVersion::V2);
        let (mut low, mut high) = (frame, frame);
        dim.shape_senses(&mut low);
        dim.shape_interoception(&mut low);
        keen.shape_senses(&mut high);
        keen.shape_interoception(&mut high);
        assert!(low.retinal[3].red < frame.retinal[3].red && high.retinal[3].red == 200);
        assert!(low.chemicals[0] < high.chemicals[0]);
        assert!(low.energy < frame.energy && high.energy == 200);
        assert!(low.damage < high.damage);
        frame.retinal[3].red = 250;
        keen.shape_senses(&mut frame);
        assert_eq!(
            frame.retinal[3].red, 255,
            "amplitudes saturate at the receptor bound"
        );
    }

    #[test]
    fn zero_drives_and_tone_never_remove_a_backward_movement() {
        let neutral = DevelopmentalGenome::uniform(32, 12_000)
            .with_locus(3, 0)
            .with_locus(4, 0)
            .with_locus(9, 0)
            .phenotype(ExpressionVersion::V2);
        assert_eq!(
            (
                neutral.hunger_drive,
                neutral.pain_flight,
                neutral.locomotor_tone
            ),
            (0, 0, 0)
        );
        let backward = PrimitiveAction {
            thrust: -256,
            ..PrimitiveAction::default()
        };
        for fullness in [0, 255] {
            for pain in [0, 90] {
                assert_eq!(
                    neutral
                        .shape_action(backward, Some(felt(fullness, pain)))
                        .thrust,
                    neutral.shape_action(backward, None).thrust
                );
            }
        }
    }

    #[test]
    fn parameters_are_listed_in_locus_order_and_follow_their_locus() {
        let base = DevelopmentalGenome::uniform(32, 12_000);
        for locus in 0..DevelopmentalGenome::LOCUS_COUNT {
            let low = base
                .with_locus(locus, 0)
                .phenotype(ExpressionVersion::V2)
                .parameters();
            let high = base
                .with_locus(locus, 64)
                .phenotype(ExpressionVersion::V2)
                .parameters();
            let changed: Vec<_> = (0..DevelopmentalGenome::LOCUS_COUNT)
                .filter(|index| low[*index] != high[*index])
                .collect();
            assert_eq!(
                changed,
                [locus],
                "locus {locus} must move exactly its own parameter"
            );
        }
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
