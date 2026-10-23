//! Private, bounded spatial memory grown from self-motion.
//!
//! The body keeps a dead-reckoned estimate of where it is relative to where it was born, from its
//! own felt heading (the `orientation` sense), the thrust and strafe it commanded, how long it
//! held them (its own clock), and the felt `touch` sense, so a body pushing against something does
//! not count as moving. Heading is felt rather than integrated from turn commands because the
//! real body does not always carry a commanded turn out. No world
//! coordinate, node name, map, or designer label is read. Places are remembered where the body
//! felt relief or harm, merged within a radius, and decay. When hungry, the body can steer back
//! toward a place that once relieved it; it steers away from places that once hurt it.
//!
//! The motion gain is a developmental body prior, like an insect's path integrator: full thrust
//! walks this body 3.8 blocks per second (measured over 69 straight runs in development pilots). Distance is speed times the time the action was held, which the body senses (`held_ms`): real decisions
//! last longer than the nominal 100 ms whenever the bridge is slow, while the held action keeps
//! being executed.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "all compact values are explicitly bounded before fixed-width casts"
)]

use serde::{Deserialize, Serialize};
use sim_kernels::derive;

use crate::environment::PrimitiveAction;

/// Current serialized spatial-memory schema.
pub const SPATIAL_SCHEMA_VERSION: u16 = 1;
/// Maximum private places retained by one individual.
pub const SPATIAL_CAPACITY: usize = 16;
/// Distance in milliblocks within which two experiences are the same place.
const MERGE_RADIUS: i64 = 4_000;
/// Distance in milliblocks at which a target counts as reached.
const ARRIVE_RADIUS: i64 = 1_200;
/// Distance in milliblocks within which a remembered harm turns the body away.
const HARM_RADIUS: i64 = 3_500;
/// Milliblocks the body travels in one second at full thrust.
const FULL_THRUST_SPEED: i64 = 3_800;
/// Milliblocks moved by full thrust over a 100 ms decision; used to look a few steps ahead.
const FULL_THRUST_STEP: i64 = FULL_THRUST_SPEED / 10;
/// The hold length assumed when the body does not report how long it held the action.
const NOMINAL_HOLD_MS: i64 = 100;
/// Decisions spent searching around a remembered place that did not deliver, before steering
/// back to it again.
const SEARCH_DECISIONS: u16 = 60;
/// Felt fullness below which the body goes looking for a relieving place.
const HUNGRY_BELOW: u16 = 128;
const DECAY_TICKS: u64 = 256;
/// Consecutive decisions of pushing against something before the body sidesteps.
const BLOCKED_BEFORE_DETOUR: u16 = 3;
/// Decisions of sidestepping (8 blocks at full thrust) once blocked.
const DETOUR_DECISIONS: u16 = 20;

/// Independent spatial-memory ablations.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct SpatialConfig {
    /// Let remembered places steer the body.
    pub enabled: bool,
    /// Sham control: places are remembered at mirrored coordinates, so the map is wrong while
    /// every other cost and mechanism is identical.
    pub scrambled: bool,
}

/// One remembered place.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Place {
    /// Stable private identity.
    pub id: u64,
    /// Estimated east-west position in milliblocks from birth.
    pub x: i64,
    /// Estimated north-south position in milliblocks from birth.
    pub y: i64,
    /// Net felt utility here: positive relieved, negative hurt.
    pub valence: i64,
    /// Times the body felt something notable here.
    pub visits: u32,
    /// Decision tick of the latest visit.
    pub last_tick: u64,
}

/// Checkpointable private spatial memory.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct SpatialMemory {
    /// Serialized schema; zero means never written.
    pub schema_version: u16,
    /// Independent ablation switches.
    pub config: SpatialConfig,
    /// Dead-reckoned east-west position in milliblocks.
    pub x: i64,
    /// Dead-reckoned north-south position in milliblocks.
    pub y: i64,
    /// Heading in 1/256 turns, counterclockwise, relative to the felt heading at birth.
    pub heading: i32,
    /// Felt heading at the first observation, which defines the body's own frame.
    pub origin_heading: i32,
    /// Whether the first felt heading has been taken.
    pub oriented: bool,
    /// Bounded places.
    pub places: Vec<Place>,
    /// Identity assigned to the next place.
    pub next_place_id: u64,
    /// Places forgotten for capacity.
    pub evicted: u64,
    /// Consecutive decisions spent pushing against something that did not give.
    pub blocked: u16,
    /// Decisions of sidestepping still to go.
    pub detour: u16,
    /// Decisions of searching around a place that was reached without relief still to go.
    pub search: u16,
}

/// Observer-only account of one spatial decision.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpatialDecision {
    /// Places retained.
    pub places: u16,
    /// Dead-reckoned position in milliblocks.
    pub x: i64,
    /// Dead-reckoned position in milliblocks.
    pub y: i64,
    /// Heading in 1/256 turns.
    pub heading: i32,
    /// Milliblocks to the target place when steering toward relief.
    pub target_distance: Option<i64>,
    /// Whether spatial memory replaced the policy's action.
    pub steering: bool,
    /// Whether a remembered harm turned the body away.
    pub avoiding: bool,
    /// Whether the body is searching around a remembered place it has reached.
    pub searching: bool,
}

impl SpatialMemory {
    /// Canonical empty newborn spatial memory.
    #[must_use]
    pub const fn new(config: SpatialConfig) -> Self {
        Self {
            schema_version: 0,
            config,
            x: 0,
            y: 0,
            heading: 0,
            origin_heading: 0,
            oriented: false,
            places: Vec::new(),
            next_place_id: 0,
            evicted: 0,
            blocked: 0,
            detour: 0,
            search: 0,
        }
    }

    /// Accepts a never-written memory and rejects state that is out of range.
    pub(crate) fn migrate_and_validate(&mut self) -> Result<(), String> {
        if *self == Self::new(self.config) {
            return Ok(());
        }
        if self.schema_version != SPATIAL_SCHEMA_VERSION
            || self.places.len() > SPATIAL_CAPACITY
            || !(0..256).contains(&self.heading)
            || self
                .places
                .iter()
                .any(|place| place.id >= self.next_place_id)
        {
            return Err("spatial memory state is out of range or incompatible".to_owned());
        }
        Ok(())
    }

    /// Integrates one executed action, then remembers a notable consequence here.
    ///
    /// `felt_heading` is the body's own sense of the way it faces, in 1/256 turns. `outcome_utility` is the signed felt utility of what just happened (zero when nothing
    /// notable did). The body integrates only when it is not pushing against something.
    pub fn observe(
        &mut self,
        executed: PrimitiveAction,
        held_ms: u16,
        felt_heading: u16,
        touching: bool,
        outcome_utility: i32,
        tick: u64,
    ) {
        if !self.config.enabled && self.places.is_empty() && self.schema_version == 0 {
            return;
        }
        self.schema_version = SPATIAL_SCHEMA_VERSION;
        let pushing = executed.thrust != 0 || executed.strafe != 0;
        self.blocked = if touching && pushing {
            self.blocked.saturating_add(1)
        } else {
            0
        };
        self.detour = self.detour.saturating_sub(1);
        self.search = self.search.saturating_sub(1);
        if self.blocked >= BLOCKED_BEFORE_DETOUR && self.detour == 0 {
            self.detour = DETOUR_DECISIONS;
            self.blocked = 0;
        }
        if !self.oriented {
            self.origin_heading = i32::from(felt_heading);
            self.oriented = true;
        }
        self.heading = (i32::from(felt_heading) - self.origin_heading).rem_euclid(256);
        if !touching {
            let (forward_x, forward_y) = direction(self.heading + 64);
            let (right_x, right_y) = (-forward_y, forward_x);
            let thrust = i64::from(executed.thrust.clamp(-256, 256));
            let strafe = i64::from(executed.strafe.clamp(-256, 256));
            // Distance is speed times the time held: `thrust / 256 * speed * held_ms / 1000`.
            let held = if held_ms == 0 {
                NOMINAL_HOLD_MS
            } else {
                i64::from(held_ms).clamp(20, 2_000)
            };
            let scale = FULL_THRUST_SPEED * held;
            self.x += (forward_x * thrust + right_x * strafe) * scale / (256 * 1_000 * 1_024);
            self.y += (forward_y * thrust + right_y * strafe) * scale / (256 * 1_000 * 1_024);
        }
        if outcome_utility != 0 {
            self.remember(outcome_utility, tick);
        }
        self.decay(tick);
    }

    /// Steers toward a remembered relief when hungry, and away from a remembered harm, or
    /// returns the policy's own action.
    #[must_use]
    pub fn steer(
        &mut self,
        fallback: PrimitiveAction,
        fullness: u16,
    ) -> (PrimitiveAction, SpatialDecision) {
        let mut decision = SpatialDecision {
            places: u16::try_from(self.places.len()).unwrap_or(u16::MAX),
            x: self.x,
            y: self.y,
            heading: self.heading,
            ..SpatialDecision::default()
        };
        if !self.config.enabled || self.places.is_empty() {
            return (fallback, decision);
        }
        let (forward_x, forward_y) = direction(self.heading + 64);
        let ahead = (
            self.x + forward_x * FULL_THRUST_STEP * 4 / 1_024,
            self.y + forward_y * FULL_THRUST_STEP * 4 / 1_024,
        );
        if let Some(harm) = self
            .places
            .iter()
            .filter(|place| place.valence < 0)
            .find(|place| squared_distance(ahead, (place.x, place.y)) < HARM_RADIUS * HARM_RADIUS)
        {
            decision.avoiding = true;
            let error = bearing_error(self.heading, (self.x, self.y), (harm.x, harm.y));
            let inside =
                squared_distance((self.x, self.y), (harm.x, harm.y)) < HARM_RADIUS * HARM_RADIUS;
            // Turn away from the harm; once facing away from it, walk out of it.
            let facing_away = error.abs() >= 96;
            let away = if error >= 0 { -8 } else { 8 };
            return (
                PrimitiveAction {
                    thrust: if inside && facing_away { 256 } else { 0 },
                    turn: if facing_away { 0 } else { away * 32 },
                    ..fallback
                },
                decision,
            );
        }
        if fullness >= HUNGRY_BELOW {
            return (fallback, decision);
        }
        let Some(target) = self
            .places
            .iter()
            .filter(|place| place.valence > 0)
            .min_by_key(|place| {
                (
                    squared_distance((self.x, self.y), (place.x, place.y)),
                    place.id,
                )
            })
        else {
            return (fallback, decision);
        };
        let (target_x, target_y) = (target.x, target.y);
        let distance = isqrt(squared_distance((self.x, self.y), (target_x, target_y)));
        decision.target_distance = Some(distance);
        if self.search > 0 {
            // Reached it and still hungry: look around while the estimate of the place settles.
            decision.searching = true;
            return (fallback, decision);
        }
        if distance <= ARRIVE_RADIUS {
            self.search = SEARCH_DECISIONS;
            decision.searching = true;
            return (fallback, decision);
        }
        decision.steering = true;
        if self.detour > 0 {
            // Something in the way did not give: slide sideways, then try the line again.
            return (
                PrimitiveAction {
                    thrust: 0,
                    turn: 0,
                    strafe: 256,
                    ..fallback
                },
                decision,
            );
        }
        let error = bearing_error(self.heading, (self.x, self.y), (target_x, target_y));
        (
            PrimitiveAction {
                thrust: if error.abs() <= 16 { 256 } else { 0 },
                turn: error.clamp(-8, 8) as i16 * 32,
                strafe: 0,
                ..fallback
            },
            decision,
        )
    }

    /// Stable fingerprint; zero for a never-written memory so older traces stay valid.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        if *self == Self::new(self.config) && !self.config.enabled {
            return 0;
        }
        let mut result = derive(
            u64::from(self.schema_version),
            u64::from(self.config.enabled) | (u64::from(self.config.scrambled) << 1),
            self.next_place_id
                ^ (self.evicted << 32)
                ^ (u64::from(self.detour) << 48)
                ^ (u64::from(self.search) << 56),
            0x5350_4154,
        );
        result = derive(
            result,
            self.x.cast_unsigned(),
            self.y.cast_unsigned(),
            u64::from(self.heading.cast_unsigned())
                | (u64::from(self.origin_heading.cast_unsigned()) << 16),
        );
        for place in &self.places {
            result = derive(
                result,
                place.id,
                (place.x.cast_unsigned()).wrapping_mul(31) ^ place.y.cast_unsigned(),
                place.valence.cast_unsigned() ^ (u64::from(place.visits) << 48),
            );
        }
        result
    }

    fn remember(&mut self, utility: i32, tick: u64) {
        let (x, y) = if self.config.scrambled {
            (-self.x, -self.y)
        } else {
            (self.x, self.y)
        };
        if let Some(place) = self.places.iter_mut().find(|place| {
            squared_distance((x, y), (place.x, place.y)) <= MERGE_RADIUS * MERGE_RADIUS
        }) {
            let weight = i64::from(place.visits);
            place.x = (place.x * weight + x) / (weight + 1);
            place.y = (place.y * weight + y) / (weight + 1);
            place.valence = (place.valence / 2).saturating_add(i64::from(utility));
            place.visits = place.visits.saturating_add(1);
            place.last_tick = tick;
            return;
        }
        if self.places.len() == SPATIAL_CAPACITY {
            let weakest = self
                .places
                .iter()
                .enumerate()
                .min_by_key(|(_, place)| (place.valence.abs(), place.last_tick, place.id))
                .map(|(index, _)| index)
                .unwrap_or_default();
            self.places.remove(weakest);
            self.evicted += 1;
        }
        self.places.push(Place {
            id: self.next_place_id,
            x,
            y,
            valence: i64::from(utility),
            visits: 1,
            last_tick: tick,
        });
        self.next_place_id += 1;
    }

    fn decay(&mut self, tick: u64) {
        self.places.retain_mut(|place| {
            let lost = i64::try_from(tick.saturating_sub(place.last_tick) / DECAY_TICKS)
                .unwrap_or(i64::MAX);
            if lost > 0 {
                place.valence -= place.valence.signum() * lost.min(place.valence.abs());
                place.last_tick += lost.cast_unsigned() * DECAY_TICKS;
            }
            place.valence != 0
        });
    }
}

/// Unit vector scaled by 1024 for an angle in 1/256 turns.
fn direction(units: i32) -> (i64, i64) {
    let angle = f64::from(units.rem_euclid(256)) * core::f64::consts::TAU / 256.0;
    (
        (angle.cos() * 1_024.0).round() as i64,
        (angle.sin() * 1_024.0).round() as i64,
    )
}

/// Signed error in 1/256 turns from the body's forward direction to the bearing of `target`.
fn bearing_error(heading: i32, from: (i64, i64), target: (i64, i64)) -> i32 {
    let bearing = ((target.1 - from.1) as f64).atan2((target.0 - from.0) as f64);
    let units = (bearing * 256.0 / core::f64::consts::TAU).round() as i32;
    let difference = (units - (heading + 64)).rem_euclid(256);
    if difference >= 128 {
        difference - 256
    } else {
        difference
    }
}

const fn squared_distance(left: (i64, i64), right: (i64, i64)) -> i64 {
    (left.0 - right.0) * (left.0 - right.0) + (left.1 - right.1) * (left.1 - right.1)
}

fn isqrt(value: i64) -> i64 {
    (value.max(0) as f64).sqrt() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> SpatialConfig {
        SpatialConfig {
            enabled: true,
            scrambled: false,
        }
    }

    /// The felt heading of an ideal body that carries out every commanded turn exactly.
    fn felt(memory: &SpatialMemory, action: PrimitiveAction) -> u16 {
        let base = if memory.oriented {
            memory.origin_heading + memory.heading
        } else {
            64
        };
        let steps = i32::from(action.turn / 32).clamp(-8, 8);
        u16::try_from((base + steps).rem_euclid(256)).unwrap()
    }

    fn walk(memory: &mut SpatialMemory, thrust: i16, turn: i16, decisions: u32, tick: &mut u64) {
        for _ in 0..decisions {
            *tick += 1;
            let action = PrimitiveAction {
                thrust,
                turn,
                ..PrimitiveAction::default()
            };
            memory.observe(action, 0, felt(memory, action), false, 0, *tick);
        }
    }

    #[test]
    fn dead_reckoning_matches_the_body_motion_rule() {
        let mut memory = SpatialMemory::new(config());
        let mut tick = 0;
        // 25 decisions at full thrust is 2.5 s at 3.8 blocks/s: 9.5 blocks along the heading.
        walk(&mut memory, 256, 0, 25, &mut tick);
        let (forward_x, forward_y) = direction(64);
        assert!((memory.x - forward_x * 9_500 / 1_024).abs() < 60);
        assert!((memory.y - forward_y * 9_500 / 1_024).abs() < 60);
        // Eight turn steps for 8 decisions is 64/256 of a turn: a quarter turn counterclockwise.
        walk(&mut memory, 0, 256, 8, &mut tick);
        assert_eq!(memory.heading, 64);
    }

    #[test]
    fn touching_something_blocks_integration() {
        let mut memory = SpatialMemory::new(config());
        for tick in 0..10 {
            memory.observe(
                PrimitiveAction {
                    thrust: 256,
                    ..PrimitiveAction::default()
                },
                0,
                64,
                true,
                0,
                tick,
            );
        }
        assert_eq!((memory.x, memory.y), (0, 0));
    }

    #[test]
    fn a_hungry_body_returns_to_the_place_that_relieved_it() {
        let mut memory = SpatialMemory::new(config());
        let mut tick = 0;
        walk(&mut memory, 256, 0, 25, &mut tick);
        memory.observe(
            PrimitiveAction::default(),
            0,
            felt(&memory, PrimitiveAction::default()),
            false,
            500,
            tick + 1,
        );
        tick += 1;
        // Wander away: turn about and walk further.
        walk(&mut memory, 0, 256, 16, &mut tick);
        walk(&mut memory, 256, 0, 40, &mut tick);
        let start = (memory.x, memory.y);
        let target = (memory.places[0].x, memory.places[0].y);
        let before = isqrt(squared_distance(start, target));
        let fed = memory.steer(PrimitiveAction::default(), 255);
        assert!(!fed.1.steering, "a fed body is left alone");
        // Follow the steering with the same motion rule until it arrives.
        let mut arrived = false;
        for _ in 0..400 {
            let (action, decision) = memory.steer(PrimitiveAction::default(), 20);
            if decision.target_distance.is_some_and(|d| d <= ARRIVE_RADIUS) {
                arrived = true;
                break;
            }
            tick += 1;
            memory.observe(action, 0, felt(&memory, action), false, 0, tick);
        }
        assert!(before > 10_000 && arrived);
    }

    #[test]
    fn a_remembered_harm_turns_the_body_away() {
        let mut memory = SpatialMemory::new(config());
        let mut tick = 0;
        walk(&mut memory, 256, 0, 12, &mut tick);
        memory.observe(
            PrimitiveAction::default(),
            0,
            felt(&memory, PrimitiveAction::default()),
            false,
            -500,
            tick + 1,
        );
        tick += 1;
        walk(&mut memory, 0, 256, 16, &mut tick);
        walk(&mut memory, 256, 0, 6, &mut tick);
        walk(&mut memory, 0, 256, 16, &mut tick);
        let (action, decision) = memory.steer(
            PrimitiveAction {
                thrust: 256,
                ..PrimitiveAction::default()
            },
            255,
        );
        assert!(decision.avoiding && action.thrust == 0 && action.turn != 0);
    }

    #[test]
    fn the_scrambled_control_remembers_a_mirrored_map() {
        let mut real = SpatialMemory::new(config());
        let mut sham = SpatialMemory::new(SpatialConfig {
            enabled: true,
            scrambled: true,
        });
        let mut tick = 0;
        for memory in [&mut real, &mut sham] {
            tick = 0;
            walk(memory, 256, 0, 25, &mut tick);
            memory.observe(
                PrimitiveAction::default(),
                0,
                felt(memory, PrimitiveAction::default()),
                false,
                500,
                tick + 1,
            );
        }
        assert_eq!(
            (sham.places[0].x, sham.places[0].y),
            (-real.places[0].x, -real.places[0].y)
        );
        let _ = tick;
    }

    #[test]
    fn capacity_is_bounded_and_round_trips_through_a_checkpoint() {
        let mut memory = SpatialMemory::new(config());
        let mut tick = 0;
        for _ in 0..(SPATIAL_CAPACITY + 5) {
            walk(&mut memory, 256, 0, 20, &mut tick);
            tick += 1;
            memory.observe(
                PrimitiveAction::default(),
                0,
                felt(&memory, PrimitiveAction::default()),
                false,
                300,
                tick,
            );
        }
        assert_eq!(memory.places.len(), SPATIAL_CAPACITY);
        let mut loaded: SpatialMemory =
            serde_json::from_str(&serde_json::to_string(&memory).unwrap()).unwrap();
        loaded.migrate_and_validate().unwrap();
        assert_eq!(loaded.fingerprint(), memory.fingerprint());
        loaded.heading = 300;
        assert!(loaded.migrate_and_validate().is_err());
        assert_eq!(
            SpatialMemory::new(SpatialConfig::default()).fingerprint(),
            0
        );
    }

    #[test]
    fn a_body_that_keeps_pushing_against_something_sidesteps_then_resumes() {
        let mut memory = SpatialMemory::new(config());
        let mut tick = 0;
        walk(&mut memory, 256, 0, 25, &mut tick);
        memory.observe(
            PrimitiveAction::default(),
            0,
            felt(&memory, PrimitiveAction::default()),
            false,
            500,
            tick + 1,
        );
        tick += 1;
        walk(&mut memory, 0, 256, 16, &mut tick);
        walk(&mut memory, 256, 0, 30, &mut tick);
        let push = PrimitiveAction {
            thrust: 256,
            ..PrimitiveAction::default()
        };
        for _ in 0..3 {
            tick += 1;
            memory.observe(push, 0, felt(&memory, push), true, 0, tick);
        }
        let (action, decision) = memory.steer(PrimitiveAction::default(), 20);
        assert!(decision.steering && action.strafe == 256 && action.thrust == 0);
        for _ in 0..DETOUR_DECISIONS {
            tick += 1;
            let (action, _) = memory.steer(PrimitiveAction::default(), 20);
            memory.observe(action, 0, felt(&memory, action), false, 0, tick);
        }
        let (action, _) = memory.steer(PrimitiveAction::default(), 20);
        assert_eq!(
            action.strafe, 0,
            "the detour ends and the body steers again"
        );
    }

    #[test]
    fn distance_scales_with_how_long_the_action_was_held() {
        let moved = |held_ms: u16| {
            let mut memory = SpatialMemory::new(config());
            memory.observe(
                PrimitiveAction {
                    thrust: 256,
                    ..PrimitiveAction::default()
                },
                held_ms,
                64,
                false,
                0,
                1,
            );
            (memory.x, memory.y)
        };
        let (short_x, short_y) = moved(100);
        let (long_x, long_y) = moved(200);
        assert!((long_x - 2 * short_x).abs() <= 2 && (long_y - 2 * short_y).abs() <= 2);
        assert_eq!(
            moved(0),
            moved(100),
            "an unreported hold is the nominal 100 ms"
        );
    }

    #[test]
    fn a_body_standing_in_a_remembered_harm_turns_away_then_walks_out() {
        let mut memory = SpatialMemory::new(config());
        memory.observe(PrimitiveAction::default(), 0, 64, false, 0, 1);
        memory.observe(PrimitiveAction::default(), 0, 64, false, -500, 2);
        let mut tick = 2;
        let mut left = false;
        for _ in 0..120 {
            let (action, decision) = memory.steer(PrimitiveAction::default(), 255);
            if !decision.avoiding {
                left = true;
                break;
            }
            tick += 1;
            let felt = felt(&memory, action);
            memory.observe(action, 0, felt, false, 0, tick);
        }
        assert!(
            left,
            "the body gets out of the harm instead of turning on the spot"
        );
    }

    #[test]
    fn arriving_without_relief_searches_then_steers_back() {
        let mut memory = SpatialMemory::new(config());
        let mut tick = 0;
        walk(&mut memory, 256, 0, 25, &mut tick);
        memory.observe(
            PrimitiveAction::default(),
            0,
            felt(&memory, PrimitiveAction::default()),
            false,
            500,
            tick + 1,
        );
        tick += 1;
        walk(&mut memory, 0, 256, 16, &mut tick);
        walk(&mut memory, 256, 0, 40, &mut tick);
        let fallback = PrimitiveAction {
            thrust: 256,
            turn: 64,
            ..PrimitiveAction::default()
        };
        let mut searched = 0;
        let mut resumed = false;
        for _ in 0..400 {
            let (action, decision) = memory.steer(fallback, 20);
            if decision.searching {
                assert_eq!(action, fallback, "the policy acts while searching");
                searched += 1;
            } else if searched > 0 && decision.steering {
                resumed = true;
                break;
            }
            tick += 1;
            let felt = felt(&memory, action);
            memory.observe(action, 0, felt, false, 0, tick);
        }
        assert!(searched >= usize::from(SEARCH_DECISIONS) - 1);
        assert!(
            resumed,
            "after searching the body steers back toward the place"
        );
    }
}
