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
use crate::environment::{RETINA_COLUMNS, RETINA_POINTS};

/// Channels per eye ray: red, green, blue, brightness, distance (occlusion) and motion.
const RAY_CHANNELS: usize = 6;
/// Body channels: touch, hand contact, eye pitch, grounded, vertical motion, acceleration.
const BODY_CHANNELS: usize = 6;
/// Length of a sense vector from the legacy eye rays.
pub const SENSE_DIMENSIONS: usize = RETINAL_RAY_COUNT * RAY_CHANNELS + BODY_CHANNELS;
/// Length of a sense vector from the forward retina.
pub const RETINA_SENSE_DIMENSIONS: usize = RETINA_POINTS * RAY_CHANNELS + BODY_CHANNELS;
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
/// fovea sees the centre of the visual field most sharply; body senses count twice. With a
/// forward retina, nearness is measured in columns from the centre of gaze, three columns per
/// step.
fn foveal_weight(dimension: usize, dimensions: usize) -> i32 {
    if dimension >= dimensions - BODY_CHANNELS {
        return 2;
    }
    let point = dimension / RAY_CHANNELS;
    let steps = if dimensions == RETINA_SENSE_DIMENSIONS {
        let column = point % RETINA_COLUMNS;
        let from_edge = column.min(RETINA_COLUMNS - 1 - column);
        (RETINA_COLUMNS / 2 - 1 - from_edge) / 3
    } else {
        point.min(RETINAL_RAY_COUNT - point)
    };
    match steps {
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
    /// With salience, how often each coarse colour (3 bits per channel) has been seen on any eye
    /// point.
    #[serde(default)]
    pub colour_counts: Vec<u32>,
    /// Total of `colour_counts`.
    #[serde(default)]
    pub colour_total: u32,
}

/// Coarse colour bins for salience: 8 levels per channel.
pub const COLOUR_BINS: usize = 512;
/// Colour counts halve once their total exceeds this, so salience tracks recent experience.
const COLOUR_MEMORY: u32 = 1 << 20;

fn colour_bin(red: i32, green: i32, blue: i32) -> usize {
    let level = |value: i32| usize::try_from(value.clamp(0, 255) >> 5).unwrap_or_default();
    level(red) << 6 | level(green) << 3 | level(blue)
}

/// The raw sense vector of an observation, in receptor units: the forward retina when the body
/// has one, otherwise the legacy eye rays, then the body senses.
fn sense_vector(observation: &EnvironmentObservation) -> Vec<i32> {
    let eye: &[_] = observation
        .retina
        .as_ref()
        .map_or(&observation.senses.retinal[..], |retina| &retina.points[..]);
    let mut vector = Vec::with_capacity(eye.len() * RAY_CHANNELS + BODY_CHANNELS);
    for sample in eye {
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
        let dimensions = self.mean.len();
        matches!(self.colour_counts.len(), 0 | COLOUR_BINS)
            && matches!(dimensions, 0 | SENSE_DIMENSIONS | RETINA_SENSE_DIMENSIONS)
            && self.spread.len() == dimensions
            && self.prototypes.len() <= MAX_SCENES
            && self.prototypes.len() == self.members.len()
            && self
                .prototypes
                .iter()
                .all(|prototype| prototype.len() == dimensions)
            && (self.prototypes.is_empty() || self.current < self.prototypes.len())
    }

    /// Learns from an observation and returns its scene, or `None` while warming up.
    pub(super) fn observe(
        &mut self,
        observation: &EnvironmentObservation,
        foveated: bool,
        salient: bool,
    ) -> Option<usize> {
        let raw = sense_vector(observation);
        if self.mean.is_empty() {
            self.mean = raw.iter().map(|value| value * 16).collect();
            self.spread = vec![SPREAD_FLOOR * 16; raw.len()];
        }
        if raw.len() != self.mean.len() {
            // The body's eyes changed kind mid-life; scenes learned with the other eyes cannot
            // place this view.
            return None;
        }
        for ((value, mean), spread) in raw.iter().zip(&mut self.mean).zip(&mut self.spread) {
            let scaled = value * 16;
            *spread += ((scaled - *mean).abs() - *spread) / STATS_HORIZON;
            *mean += (scaled - *mean) / STATS_HORIZON;
        }
        let points = (raw.len() - BODY_CHANNELS) / RAY_CHANNELS;
        let bins: Vec<usize> = (0..points)
            .map(|point| {
                let channels = &raw[point * RAY_CHANNELS..];
                colour_bin(channels[0], channels[1], channels[2])
            })
            .collect();
        if salient {
            self.count_colours(&bins);
        }
        self.seen = self.seen.saturating_add(1);
        if self.seen < WARM_UP {
            return None;
        }
        // Attention as a spotlight (winner-take-all, after Itti & Koch): when something stands out,
        // only the most salient eye points shape the scene; when nothing does, the whole view does.
        let saliences: Vec<i32> = if salient {
            bins.iter().map(|bin| self.salience(*bin)).collect()
        } else {
            Vec::new()
        };
        let spotlight = saliences.iter().copied().max().unwrap_or(1);
        // Attention has a fixed budget: the points in the spotlight carry the whole view's
        // weight, so a focused view counts as much as an unfocused one.
        let attended = saliences
            .iter()
            .filter(|salience| **salience == spotlight)
            .count()
            .max(1);
        let budget = i32::try_from(points / attended).unwrap_or(1).max(1);
        let point: Vec<i16> = raw
            .iter()
            .zip(&self.mean)
            .zip(&self.spread)
            .enumerate()
            .map(|(dimension, ((value, mean), spread))| {
                let units = (value * 16 - mean) * SPREAD_UNIT / (*spread).max(SPREAD_FLOOR * 16);
                let mut weight = if foveated {
                    foveal_weight(dimension, raw.len())
                } else {
                    1
                };
                if salient && dimension < points * RAY_CHANNELS {
                    let salience = saliences[dimension / RAY_CHANNELS];
                    weight *= if salience == spotlight { budget } else { 0 };
                }
                let weighted = units.clamp(-1_024, 1_024) * weight;
                i16::try_from(weighted.clamp(i32::from(i16::MIN), i32::from(i16::MAX)))
                    .unwrap_or_default()
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
            self.typical_distance = i64::try_from(raw.len()).unwrap_or(1) * i64::from(SPREAD_UNIT);
        }
        self.current = scene;
        Some(scene)
    }
}

impl ScenePerception {
    /// Counts the colours of one view; counts halve as they accumulate, so salience tracks
    /// recent experience.
    fn count_colours(&mut self, bins: &[usize]) {
        if self.colour_counts.is_empty() {
            self.colour_counts = vec![0; COLOUR_BINS];
        }
        for bin in bins {
            self.colour_counts[*bin] = self.colour_counts[*bin].saturating_add(1);
        }
        self.colour_total = self
            .colour_total
            .saturating_add(u32::try_from(bins.len()).unwrap_or(u32::MAX));
        if self.colour_total > COLOUR_MEMORY {
            for count in &mut self.colour_counts {
                *count /= 2;
            }
            self.colour_total = self.colour_counts.iter().sum();
        }
    }

    /// Bottom-up salience of a colour: its self-information, how many halvings rarer it is than
    /// certain, less two, between one and eight. Colours seen everywhere weigh once; a colour
    /// seen on one point in a thousand weighs eight. Learned from experience, after
    /// information-maximisation models of attention (Bruce & Tsotsos).
    fn salience(&self, bin: usize) -> i32 {
        let count = self.colour_counts[bin].max(1);
        let rarity = (self.colour_total / count).max(1).ilog2();
        i32::try_from(rarity.saturating_sub(2).clamp(1, 8)).unwrap_or(1)
    }
}

fn distance(prototype: &[i16], point: &[i16]) -> i64 {
    prototype
        .iter()
        .zip(point)
        .map(|(centre, value)| i64::from((i32::from(*centre) - i32::from(*value)).abs()))
        .sum()
}
