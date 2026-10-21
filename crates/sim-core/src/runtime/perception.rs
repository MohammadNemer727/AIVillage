//! Scenes the founder learns from its raw senses.
//!
//! Instead of designer-chosen sense facts, the founder groups what it senses into a few kinds of
//! scene by online prototype learning: every eye ray's colour, brightness, distance and motion and
//! its body senses form one sense vector; each dimension is judged in units of its own running
//! spread; a sense vector joins its nearest prototype, nudging it closer, unless it is unlike
//! every prototype, in which case it founds a new one. Scenes carry no name: they are whatever
//! the founder has met.

use serde::{Deserialize, Serialize};

use super::{EnvironmentObservation, RETINAL_RAY_COUNT};

/// Channels per eye ray: red, green, blue, brightness, distance (occlusion) and motion.
const RAY_CHANNELS: usize = 6;
/// Body channels: touch, hand contact, eye pitch, grounded, vertical motion, acceleration.
const BODY_CHANNELS: usize = 6;
/// Length of a sense vector.
pub const SENSE_DIMENSIONS: usize = RETINAL_RAY_COUNT * RAY_CHANNELS + BODY_CHANNELS;
/// Most scenes the founder distinguishes.
pub const MAX_SCENES: usize = 8;
/// Sense vectors seen before scenes are formed, so the running spreads have settled.
const WARM_UP: u32 = 64;
/// Units per running spread in prototype space.
const SPREAD_UNIT: i32 = 16;
/// Smallest running spread a dimension is judged against, in receptor units.
const SPREAD_FLOOR: i32 = 8;
/// Horizon of the running means and spreads.
const STATS_HORIZON: i32 = 64;
/// Prototypes settle at this many members, so they keep tracking slowly.
const PROTOTYPE_HORIZON: u32 = 32;
/// Weight of a sense dimension under foveated vision: eye rays count more the nearer they look
/// to straight ahead (four times at the centre, three, two, then once in the periphery), as a
/// fovea sees the centre of the visual field most sharply; body senses count twice.
fn foveal_weight(dimension: usize) -> i32 {
    if dimension >= RETINAL_RAY_COUNT * RAY_CHANNELS {
        return 2;
    }
    let ray = dimension / RAY_CHANNELS;
    match ray.min(RETINAL_RAY_COUNT - ray) {
        0 => 4,
        1 => 3,
        2 => 2,
        _ => 1,
    }
}

/// A sense vector founds a new scene when it lies this many quarters of the typical distance
/// from every prototype.
const NOVEL_QUARTERS: i64 = 6;

/// Online scene prototypes over the founder's raw senses.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScenePerception {
    /// Sense vectors seen so far, saturating.
    pub seen: u32,
    /// Running mean of each dimension, in receptor units scaled by 16.
    pub mean: Vec<i32>,
    /// Running mean absolute deviation of each dimension, in receptor units scaled by 16.
    pub spread: Vec<i32>,
    /// Scene prototypes in spread units (16 per running spread).
    pub prototypes: Vec<Vec<i16>>,
    /// Members of each prototype, saturating.
    pub members: Vec<u32>,
    /// Running average distance from a sense vector to its nearest prototype.
    pub typical_distance: i64,
    /// The scene of the latest sense vector.
    pub current: usize,
}

/// The raw sense vector of an observation, in receptor units.
fn sense_vector(observation: &EnvironmentObservation) -> Vec<i32> {
    let mut vector = Vec::with_capacity(SENSE_DIMENSIONS);
    for sample in &observation.senses.retinal {
        vector.extend([
            i32::from(sample.red),
            i32::from(sample.green),
            i32::from(sample.blue),
            i32::from(sample.brightness),
            i32::from(sample.occlusion),
            i32::from(sample.motion),
        ]);
    }
    let body = observation.body;
    vector.extend([
        i32::from(observation.senses.touch),
        i32::from(body.hand_contact),
        i32::from(body.eye_pitch) - 128,
        i32::from(body.grounded),
        i32::from(body.vertical_motion),
        i32::from(observation.senses.acceleration),
    ]);
    vector
}

impl ScenePerception {
    pub(super) fn is_valid(&self) -> bool {
        let dimensions = |values: usize| values == 0 || values == SENSE_DIMENSIONS;
        dimensions(self.mean.len())
            && self.mean.len() == self.spread.len()
            && self.prototypes.len() <= MAX_SCENES
            && self.prototypes.len() == self.members.len()
            && self
                .prototypes
                .iter()
                .all(|prototype| prototype.len() == SENSE_DIMENSIONS)
            && (self.prototypes.is_empty() || self.current < self.prototypes.len())
    }

    /// Learns from an observation and returns its scene, or `None` while warming up.
    pub(super) fn observe(
        &mut self,
        observation: &EnvironmentObservation,
        foveated: bool,
    ) -> Option<usize> {
        let raw = sense_vector(observation);
        if self.mean.is_empty() {
            self.mean = raw.iter().map(|value| value * 16).collect();
            self.spread = vec![SPREAD_FLOOR * 16; SENSE_DIMENSIONS];
        }
        for ((value, mean), spread) in raw.iter().zip(&mut self.mean).zip(&mut self.spread) {
            let scaled = value * 16;
            *spread += ((scaled - *mean).abs() - *spread) / STATS_HORIZON;
            *mean += (scaled - *mean) / STATS_HORIZON;
        }
        self.seen = self.seen.saturating_add(1);
        if self.seen < WARM_UP {
            return None;
        }
        let point: Vec<i16> = raw
            .iter()
            .zip(&self.mean)
            .zip(&self.spread)
            .enumerate()
            .map(|(dimension, ((value, mean), spread))| {
                let units = (value * 16 - mean) * SPREAD_UNIT / (*spread).max(SPREAD_FLOOR * 16);
                let weight = if foveated {
                    foveal_weight(dimension)
                } else {
                    1
                };
                i16::try_from(units.clamp(-1_024, 1_024) * weight).unwrap_or_default()
            })
            .collect();
        let nearest = self
            .prototypes
            .iter()
            .map(|prototype| distance(prototype, &point))
            .enumerate()
            .min_by_key(|(_, distance)| *distance);
        let novel = nearest.is_none_or(|(_, distance)| {
            self.typical_distance > 0 && distance * 4 > self.typical_distance * NOVEL_QUARTERS
        });
        let scene = if novel && self.prototypes.len() < MAX_SCENES {
            self.prototypes.push(point);
            self.members.push(1);
            self.prototypes.len() - 1
        } else {
            let (scene, distance) = nearest.unwrap_or((0, 0));
            self.typical_distance += (distance - self.typical_distance) / 32;
            let members = self.members[scene].saturating_add(1);
            self.members[scene] = members;
            let horizon = i32::try_from(members.min(PROTOTYPE_HORIZON)).unwrap_or(1);
            for (centre, value) in self.prototypes[scene].iter_mut().zip(&point) {
                let moved = i32::from(*centre) + (i32::from(*value) - i32::from(*centre)) / horizon;
                *centre = i16::try_from(moved).unwrap_or_default();
            }
            scene
        };
        if self.typical_distance == 0 {
            // Before any comparison, judge novelty against the first scene's spread of values.
            self.typical_distance =
                i64::try_from(SENSE_DIMENSIONS).unwrap_or(1) * i64::from(SPREAD_UNIT);
        }
        self.current = scene;
        Some(scene)
    }
}

fn distance(prototype: &[i16], point: &[i16]) -> i64 {
    prototype
        .iter()
        .zip(point)
        .map(|(centre, value)| i64::from((i32::from(*centre) - i32::from(*value)).abs()))
        .sum()
}
