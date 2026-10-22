//! Live energy and homeostatic reinforcement learning (Keramati and Gutkin, 2014).
//!
//! The founder's body burns energy while it lives and faster while it moves, and gains energy
//! when it swallows. Its drive is the distance of its energy from a set point; the only reward
//! it learns from is a fall in that drive, so eating is rewarding when hungry and not when full.
//! Nothing tells it what food is, where food is, or which action eats: it must learn which of
//! its sensations (colours in parts of its view, the smell in each nostril) and which actions
//! lead to drive reduction. While sated it explores by curiosity instead.
//!
//! Like a newborn's rooting and sucking reflexes, one innate reflex is given: a hungry founder
//! opens its jaw when the smell at its snout is very strong. It does not find, approach or
//! recognise food; those must be learned from the meals the reflex makes possible.

use serde::{Deserialize, Serialize};
use sim_kernels::derive;

use super::{AFFORDANCE_ACTIONS, LEGACY_AFFORDANCE_ACTIONS};
use crate::environment::{EnvironmentObservation, PrimitiveAction, RETINA_COLUMNS, RETINA_ROWS};

/// Largest energy store.
pub const ENERGY_MAX: u16 = 255;
/// Energy the body regulates toward; the drive is the distance from it.
pub const ENERGY_SET_POINT: u16 = 200;
/// Energy at birth: hungry enough that seeking food matters from the start.
pub const ENERGY_AT_BIRTH: u16 = 140;
/// Below this energy the founder is hungry and acts on learned value; above it, by curiosity.
const HUNGRY_BELOW: u16 = 190;
/// Burn units that cost one unit of energy.
const BURN_PER_ENERGY: u16 = 32;
/// Burn per decision of being alive.
const REST_BURN: u16 = 2;
/// Extra burn per decision of moving the body.
const MOVE_BURN: u16 = 2;
/// Energy gained by one full swallow (amplitude 255).
const MEAL_ENERGY: u32 = 48;
/// Smell in the stronger nostril at which a hungry founder's jaw opens by reflex.
pub const ROOTING_SMELL: u16 = 160;
/// Decisions the reflex rests after firing, so a strong smell with nothing in reach cannot hold
/// the founder biting in place.
pub const ROOTING_REST: u8 = 8;
/// Experiment index of opening the jaw.
pub const JAW_EXPERIMENT: usize = 10;

/// Column sectors the forward retina is pooled into, from the far right to the far left.
const SECTORS: usize = 6;
/// Levels per colour channel, so colours fall into 27 broad bins.
const COLOUR_LEVELS: usize = 3;
const COLOURS: usize = COLOUR_LEVELS * COLOUR_LEVELS * COLOUR_LEVELS;
/// Occlusion at or above which a retina point counts as near (within about two nodes).
const NEAR_OCCLUSION: u16 = 170;
const VISION_FEATURES: usize = SECTORS * RETINA_ROWS * COLOURS * 2;
/// Strength levels of the stronger nostril, and which nostril is stronger.
const SMELL_LEVELS: usize = 8;
const SMELL_SIDES: usize = 3;
/// Difference between the nostrils that counts as one side smelling stronger.
const SMELL_SIDE_MARGIN: u16 = 2;
const SMELL_FEATURES: usize = SMELL_LEVELS * SMELL_SIDES;
const STATE_FEATURES: usize = VISION_FEATURES + SMELL_FEATURES + 1;
/// Binary features: every state feature, separately while hungry and while sated.
pub const FEATURES: usize = 2 * STATE_FEATURES;

/// Recent decisions that share credit for a change in drive (an eligibility trace).
const TRACE_SPAN: usize = 24;
/// Credit kept per decision of age: three quarters.
const TRACE_KEEP: (i64, i64) = (7, 8);
/// Discount per decision: 31/32, a horizon of about 32 decisions.
const DISCOUNT: (i64, i64) = (31, 32);
/// Learning rate, shared out over a decision's active features.
const LEARNING_RATE: (i64, i64) = (1, 8);
/// Fixed-point scale of values, in reward units.
const VALUE_SCALE: i64 = 16;
/// Hungry decisions that explore at random: one in this many.
const EXPLORE_ONE_IN: u64 = 10;
const WEIGHT_LIMIT: i64 = (i32::MAX / 2) as i64;

/// One remembered decision: the action taken and the features active when it was taken.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Decision {
    /// Experiment index of the action.
    pub action: u8,
    /// Sorted active feature indices.
    pub features: Vec<u16>,
}

/// Live energy store and the value the founder has learned for its actions.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Homeostasis {
    /// Energy store, at most [`ENERGY_MAX`].
    pub energy: u16,
    /// Burn accumulated toward the next unit of energy spent.
    pub burn: u16,
    /// Swallows so far.
    pub meals: u32,
    /// Action values: `weights[action * FEATURES + feature]`. Empty while the learner is off.
    pub weights: Vec<i32>,
    /// Recent decisions, most recent first.
    pub recent: Vec<Decision>,
    /// Decisions until the rooting reflex can fire again.
    #[serde(default)]
    pub reflex_rest: u8,
    /// Drive felt at the previous observation, when the body reports what it feels.
    #[serde(default)]
    pub felt_drive: Option<i64>,
}

impl Homeostasis {
    /// A newborn body with blank values.
    #[must_use]
    pub fn newborn() -> Self {
        Self {
            energy: ENERGY_AT_BIRTH,
            burn: 0,
            meals: 0,
            weights: vec![0; AFFORDANCE_ACTIONS * FEATURES],
            recent: Vec::new(),
            reflex_rest: 0,
            felt_drive: None,
        }
    }

    /// Whether the learner is in use.
    #[must_use]
    pub fn active(&self) -> bool {
        !self.weights.is_empty()
    }

    /// Whether the founder is hungry enough to act on learned value.
    #[must_use]
    pub const fn hungry(&self) -> bool {
        self.energy < HUNGRY_BELOW
    }

    pub(super) fn migrate_and_validate(&mut self) -> Result<(), String> {
        if !self.active() {
            return Ok(());
        }
        if self.weights.len() == LEGACY_AFFORDANCE_ACTIONS * FEATURES {
            self.weights.resize(AFFORDANCE_ACTIONS * FEATURES, 0);
        }
        if self.weights.len() != AFFORDANCE_ACTIONS * FEATURES
            || self.energy > ENERGY_MAX
            || self.burn >= BURN_PER_ENERGY
            || self.recent.len() > TRACE_SPAN
            || self.reflex_rest > ROOTING_REST
            || self.recent.iter().any(|decision| {
                usize::from(decision.action) >= AFFORDANCE_ACTIONS
                    || decision
                        .features
                        .iter()
                        .any(|feature| usize::from(*feature) >= FEATURES)
            })
        {
            return Err("homeostatic state is out of range".to_owned());
        }
        Ok(())
    }

    /// Lives through the interval that `observation` reports: burns energy for the action that
    /// was held and gains what was swallowed. Returns the fall in drive, the reward.
    pub(super) fn live(&mut self, observation: &EnvironmentObservation) -> i64 {
        if let Some(felt) = observation.interoception {
            // A body that feels itself: its energy is how fed it feels, and being hurt adds to
            // its drive, so pain is felt as a rise in drive and healing as a fall.
            if observation.body.swallowed > 0 {
                self.meals = self.meals.saturating_add(1);
            }
            self.energy = felt.fullness.min(ENERGY_MAX);
            let now = drive(self.energy) + injury(felt.health);
            let reward = self.felt_drive.map_or(0, |before| before - now);
            self.felt_drive = Some(now);
            return reward;
        }
        let before = drive(self.energy);
        let moved = self
            .recent
            .first()
            .is_some_and(|decision| moves(super::affordance_action(usize::from(decision.action))));
        self.burn += REST_BURN + if moved { MOVE_BURN } else { 0 };
        if self.burn >= BURN_PER_ENERGY {
            self.burn -= BURN_PER_ENERGY;
            self.energy = self.energy.saturating_sub(1);
        }
        let swallowed = observation.body.swallowed.min(255);
        if swallowed > 0 {
            self.meals = self.meals.saturating_add(1);
            let gain = u16::try_from(u32::from(swallowed) * MEAL_ENERGY / 255).unwrap_or_default();
            self.energy = self.energy.saturating_add(gain).min(ENERGY_MAX);
        }
        before - drive(self.energy)
    }

    /// Active features of what the founder senses now, given its hunger.
    pub(super) fn features(
        &self,
        observation: &EnvironmentObservation,
        smell: [u16; 2],
    ) -> Vec<u16> {
        let offset = if self.hungry() { 0 } else { STATE_FEATURES };
        let mut features = Vec::new();
        if let Some(retina) = &observation.retina {
            for row in 0..RETINA_ROWS {
                for column in 0..RETINA_COLUMNS {
                    let point = retina.points[row * RETINA_COLUMNS + column];
                    let level = |value: u16| usize::from(value.min(255)) * COLOUR_LEVELS / 256;
                    let colour = (level(point.red) * COLOUR_LEVELS + level(point.green))
                        * COLOUR_LEVELS
                        + level(point.blue);
                    let near = usize::from(point.occlusion >= NEAR_OCCLUSION);
                    let sector = column * SECTORS / RETINA_COLUMNS;
                    features.push(((sector * RETINA_ROWS + row) * COLOURS + colour) * 2 + near);
                }
            }
        }
        let [left, right] = smell;
        let level = usize::from(left.max(right).min(255)) * SMELL_LEVELS / 256;
        let side = if left > right.saturating_add(SMELL_SIDE_MARGIN) {
            0
        } else if right > left.saturating_add(SMELL_SIDE_MARGIN) {
            2
        } else {
            1
        };
        features.push(VISION_FEATURES + level * SMELL_SIDES + side);
        features.push(STATE_FEATURES - 1);
        let mut features: Vec<u16> = features
            .into_iter()
            .map(|feature| u16::try_from(offset + feature).unwrap_or_default())
            .collect();
        features.sort_unstable();
        features.dedup();
        features
    }

    fn value(&self, action: usize, features: &[u16]) -> i64 {
        let row = &self.weights[action * FEATURES..(action + 1) * FEATURES];
        features
            .iter()
            .map(|feature| i64::from(row[usize::from(*feature)]))
            .sum()
    }

    fn best_value(&self, features: &[u16]) -> i64 {
        (0..AFFORDANCE_ACTIONS)
            .map(|action| self.value(action, features))
            .max()
            .unwrap_or_default()
    }

    /// Temporal-difference update: the reward that followed the previous decision, plus the
    /// discounted best value now, corrects the values of the recent decisions, older ones less.
    pub(super) fn learn(&mut self, reward: i64, features: &[u16]) {
        let Some(previous) = self.recent.first() else {
            return;
        };
        let target = reward * VALUE_SCALE + self.best_value(features) * DISCOUNT.0 / DISCOUNT.1;
        let error = target - self.value(usize::from(previous.action), &previous.features);
        let mut credit = error * LEARNING_RATE.0 / LEARNING_RATE.1;
        for decision in &self.recent {
            if credit == 0 {
                break;
            }
            // Each sense (vision, smell, and the baseline) takes an equal share of the credit,
            // split among its own active features, so many uninformative colours cannot drown
            // one informative smell.
            let mut counts = [0_i64; 3];
            for feature in &decision.features {
                counts[sense(*feature)] += 1;
            }
            let base = usize::from(decision.action) * FEATURES;
            for feature in &decision.features {
                let share = credit / (SENSES * counts[sense(*feature)]);
                let weight = &mut self.weights[base + usize::from(*feature)];
                *weight =
                    i32::try_from((i64::from(*weight) + share).clamp(-WEIGHT_LIMIT, WEIGHT_LIMIT))
                        .unwrap_or_default();
            }
            credit = credit * TRACE_KEEP.0 / TRACE_KEEP.1;
        }
    }

    /// The action with the highest learned value, or now and then a random one. Returns the
    /// experiment index and its value.
    pub(super) fn choose(&self, features: &[u16], seed: u64, tick: u64) -> (usize, i64) {
        if derive(seed, tick, 0, 0x484F_4D45).is_multiple_of(EXPLORE_ONE_IN) {
            let action =
                usize::try_from(derive(seed, tick, 1, 0x484F_4D45) % AFFORDANCE_ACTIONS as u64)
                    .unwrap_or_default();
            return (action, self.value(action, features));
        }
        let mut best = (0, i64::MIN, 0);
        for action in 0..AFFORDANCE_ACTIONS {
            let value = self.value(action, features);
            let tie = derive(seed, tick, action as u64 + 2, 0x484F_4D45) & 0xff;
            if (value, tie) > (best.1, best.2) {
                best = (action, value, tie);
            }
        }
        (best.0, best.1)
    }

    /// Whether the rooting reflex fires this decision: hungry, with food smelt right at the
    /// snout, and rested since it last fired.
    pub(super) fn rooting(&mut self, smell: [u16; 2]) -> bool {
        if self.reflex_rest > 0 {
            self.reflex_rest -= 1;
            return false;
        }
        let fires = self.hungry() && smell[0].max(smell[1]) >= ROOTING_SMELL;
        if fires {
            self.reflex_rest = ROOTING_REST;
        }
        fires
    }

    /// Remembers a decision for later credit.
    pub(super) fn remember(&mut self, action: usize, features: Vec<u16>) {
        self.recent.insert(
            0,
            Decision {
                action: u8::try_from(action).unwrap_or_default(),
                features,
            },
        );
        self.recent.truncate(TRACE_SPAN);
    }
}

/// Senses that share each update equally: vision, smell, and the baseline.
const SENSES: i64 = 3;

/// Which sense a feature belongs to.
fn sense(feature: u16) -> usize {
    match usize::from(feature) % STATE_FEATURES {
        index if index < VISION_FEATURES => 0,
        index if index < VISION_FEATURES + SMELL_FEATURES => 1,
        _ => 2,
    }
}

/// Distance of energy from the set point, squared so the same meal matters more when hungrier.
fn drive(energy: u16) -> i64 {
    let distance = i64::from(ENERGY_SET_POINT) - i64::from(energy);
    distance * distance
}

/// Drive from injury: the distance of health from full, squared like hunger's.
fn injury(health: u16) -> i64 {
    let missing = 255 - i64::from(health.min(255));
    missing * missing
}

const fn moves(action: PrimitiveAction) -> bool {
    action.thrust != 0 || action.strafe != 0 || action.jump != 0 || action.turn != 0
}

/// Stable fingerprint of the energy store and every learned value.
#[must_use]
pub fn homeostasis_fingerprint(homeostasis: &Homeostasis) -> u64 {
    let mut fingerprint = derive(
        u64::from(homeostasis.energy),
        u64::from(homeostasis.burn),
        u64::from(homeostasis.meals),
        0x484F_4650,
    );
    fingerprint = derive(
        fingerprint,
        u64::from(homeostasis.reflex_rest),
        0,
        0x484F_5246,
    );
    if let Some(felt) = homeostasis.felt_drive {
        fingerprint = derive(fingerprint, felt.cast_unsigned(), 1, 0x484F_5246);
    }
    for (index, weight) in homeostasis.weights.iter().enumerate() {
        if *weight != 0 {
            fingerprint = derive(
                fingerprint,
                i64::from(*weight).cast_unsigned(),
                index as u64,
                0x484F_5754,
            );
        }
    }
    for (age, decision) in homeostasis.recent.iter().enumerate() {
        fingerprint = derive(
            fingerprint,
            u64::from(decision.action),
            age as u64,
            0x484F_5243,
        );
        for feature in &decision.features {
            fingerprint = derive(fingerprint, u64::from(*feature), age as u64, 0x484F_4645);
        }
    }
    fingerprint
}
