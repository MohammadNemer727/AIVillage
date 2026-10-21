//! Deterministic foraging test: does a hungry founder learn to find and eat food?
//!
//! A flat clearing of forest floor holds mushrooms and tree trunks. The founder sees them on its
//! forward retina only as colours at a distance, smells the mushrooms through two nostrils as
//! the Luanti nose does, and eats a mushroom by opening its jaw within reach of it. An eaten
//! mushroom grows again elsewhere after a while. Nothing names the food or rewards the founder:
//! the homeostatic founder feels only its own energy, which falls while it lives and rises when
//! it swallows. Curiosity-only and random founders are run in the same clearing for comparison.
#![allow(
    missing_docs,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::format_push_string,
    clippy::missing_panics_doc,
    reason = "compact deterministic experiment harness"
)]

use core::f64::consts::PI;

use serde::Serialize;
use sim_core::{
    RawSensoryFrame, RetinalSample,
    environment::{
        BodyProprioception, EnvironmentCapabilities, EnvironmentObservation, PrimitiveAction,
        RETINA_COLUMNS, RETINA_POINTS, Retina, TransportStatus,
    },
    runtime::{
        AFFORDANCE_ACTIONS, AgentRuntime, ExperimentMode, JAW_EXPERIMENT, ROOTING_REST,
        ROOTING_SMELL,
    },
};
use sim_kernels::derive;

use crate::harness::{SEEDS, rate};
use sim_format::FounderSpec;

/// Side of the square clearing, in nodes.
const SIZE: f64 = 24.0;
const MUSHROOMS: usize = 8;
const TREES: usize = 10;
/// Decisions before an eaten mushroom grows again somewhere else.
const REGROW: u32 = 300;
/// Mouth reach, as in Luanti.
const REACH: f64 = 1.6;
const EYE_RANGE: f64 = 6.0;
const ODOUR_RADIUS: f64 = 12.0;
pub const DECISIONS: u32 = 24_000;
/// Decisions per reported phase.
pub const PHASE: u32 = 4_000;

const MUSHROOM: [u16; 3] = [106, 79, 72];
const TRUNK: [u16; 3] = [100, 80, 50];
const GRASS: [u16; 3] = [64, 111, 25];
const SKY: [u16; 3] = [120, 170, 255];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Forager {
    Random,
    /// Random, with the same rooting reflex as the homeostatic founder but no learning.
    Reflex,
    Curious,
    Homeostatic,
}

pub const FORAGERS: [Forager; 4] = [
    Forager::Random,
    Forager::Reflex,
    Forager::Curious,
    Forager::Homeostatic,
];

#[derive(Clone, Copy, Debug)]
struct Point {
    x: f64,
    z: f64,
}

struct Clearing {
    seed: u64,
    position: Point,
    heading: f64,
    mushrooms: Vec<(Point, u32)>,
    trees: Vec<Point>,
    sequence: u64,
    swallowed: u16,
    draws: u64,
}

impl Clearing {
    fn new(seed: u64) -> Self {
        let mut clearing = Self {
            seed,
            position: Point {
                x: SIZE / 2.0,
                z: SIZE / 2.0,
            },
            heading: 0.0,
            mushrooms: Vec::new(),
            trees: Vec::new(),
            sequence: 0,
            swallowed: 0,
            draws: 0,
        };
        clearing.trees = (0..TREES).map(|_| clearing.random_point()).collect();
        clearing.mushrooms = (0..MUSHROOMS)
            .map(|_| (clearing.random_point(), 0))
            .collect();
        clearing
    }

    fn random_point(&mut self) -> Point {
        self.draws += 1;
        let x = derive(self.seed, self.draws, 0, 0x464F_5241) % 2_200;
        let z = derive(self.seed, self.draws, 1, 0x464F_5241) % 2_200;
        Point {
            x: 1.0 + x as f64 / 100.0,
            z: 1.0 + z as f64 / 100.0,
        }
    }

    fn eye_ray(&self, angle: f64, row: usize) -> RetinalSample {
        let direction = (angle.cos(), angle.sin());
        let mut nearest: Option<(f64, [u16; 3])> = None;
        let mut consider = |target: Point, radius: f64, colour: [u16; 3], rows: &[usize]| {
            let (dx, dz) = (target.x - self.position.x, target.z - self.position.z);
            let along = dx * direction.0 + dz * direction.1;
            let across = (dz * direction.0 - dx * direction.1).abs();
            if along > 0.0 && along <= EYE_RANGE && across <= radius + along * 0.065 {
                // Small things on the ground are seen on the low rows; trunks on every row.
                let low_row = usize::from(along >= 2.0);
                if rows.contains(&row)
                    && (rows.len() > 2 || row == low_row)
                    && nearest.is_none_or(|(distance, _)| along < distance)
                {
                    nearest = Some((along, colour));
                }
            }
        };
        for (mushroom, growing) in &self.mushrooms {
            if *growing == 0 {
                consider(*mushroom, 0.3, MUSHROOM, &[0, 1]);
            }
        }
        for tree in &self.trees {
            consider(*tree, 0.4, TRUNK, &[0, 1, 2, 3, 4]);
        }
        let (distance, colour) = nearest.unwrap_or(match row {
            0 => (1.2, GRASS),
            1 => (2.7, GRASS),
            _ => (EYE_RANGE, SKY),
        });
        RetinalSample {
            red: colour[0],
            green: colour[1],
            blue: colour[2],
            brightness: 200,
            motion: 0,
            occlusion: (255.0 * (1.0 - distance.min(EYE_RANGE) / EYE_RANGE)).round() as u16,
        }
    }

    fn sniff(&self) -> [u16; 2] {
        [1.0, -1.0].map(|side: f64| {
            let nostril = Point {
                x: self.position.x
                    + 0.3 * self.heading.cos()
                    + side * 0.3 * (self.heading + PI / 2.0).cos(),
                z: self.position.z
                    + 0.3 * self.heading.sin()
                    + side * 0.3 * (self.heading + PI / 2.0).sin(),
            };
            let odour: f64 = self
                .mushrooms
                .iter()
                .filter(|(_, growing)| *growing == 0)
                .map(|(mushroom, _)| {
                    let distance = (mushroom.x - nostril.x).hypot(mushroom.z - nostril.z);
                    if distance <= ODOUR_RADIUS {
                        1.0 / (1.0 + distance * distance)
                    } else {
                        0.0
                    }
                })
                .sum();
            (32.0 * (64.0f64.mul_add(odour, 1.0)).log2())
                .round()
                .clamp(0.0, 255.0) as u16
        })
    }

    fn observe(&mut self) -> EnvironmentObservation {
        let mut senses = RawSensoryFrame::EMPTY;
        let smell = self.sniff();
        senses.chemicals[0] = smell[0];
        senses.chemicals[1] = smell[1];
        let mut retina = Retina {
            points: [RetinalSample::default(); RETINA_POINTS],
        };
        for (index, point) in retina.points.iter_mut().enumerate() {
            let (row, column) = (index / RETINA_COLUMNS, index % RETINA_COLUMNS);
            let angle = self.heading + (column as f64 - 11.5) * 7.5_f64.to_radians();
            *point = self.eye_ray(angle, row);
        }
        let mut capabilities = EnvironmentCapabilities {
            body_contract_version: 5,
            ..EnvironmentCapabilities::default()
        };
        capabilities.dead.chemicals = [false, false, true, true];
        capabilities.dead.vibration = true;
        let observation = EnvironmentObservation {
            sequence: self.sequence,
            senses,
            body: BodyProprioception {
                eye_pitch: 128,
                grounded: 1,
                swallowed: self.swallowed,
                ..BodyProprioception::default()
            },
            capabilities,
            transport: TransportStatus::Healthy,
            retina: Some(retina),
        };
        self.sequence += 1;
        self.swallowed = 0;
        observation
    }

    /// Applies one held action; returns whether the founder ate.
    fn act(&mut self, action: PrimitiveAction) -> bool {
        let turn = f64::from(action.turn.signum()) * 22.5_f64.to_radians();
        self.heading = (self.heading + turn).rem_euclid(2.0 * PI);
        let forward = f64::from(action.thrust.signum()) * if action.thrust > 0 { 0.4 } else { 0.2 };
        let sideways = f64::from(action.strafe.signum()) * 0.3;
        let (cos, sin) = (self.heading.cos(), self.heading.sin());
        let moved = Point {
            x: (self.position.x + forward * cos - sideways * sin).clamp(0.5, SIZE - 0.5),
            z: (self.position.z + forward * sin + sideways * cos).clamp(0.5, SIZE - 0.5),
        };
        if !self
            .trees
            .iter()
            .any(|tree| (tree.x - moved.x).hypot(tree.z - moved.z) < 0.7)
        {
            self.position = moved;
        }
        for (_, growing) in &mut self.mushrooms {
            *growing = growing.saturating_sub(1);
        }
        if action.jaw == 0 {
            return false;
        }
        let eaten = self.mushrooms.iter().position(|(mushroom, growing)| {
            let (dx, dz) = (mushroom.x - self.position.x, mushroom.z - self.position.z);
            let along = dx * cos + dz * sin;
            *growing == 0
                && along > 0.0
                && dx.hypot(dz) <= REACH
                && (dz * cos - dx * sin).abs() <= 0.5
        });
        let Some(index) = eaten else {
            return false;
        };
        let elsewhere = self.random_point();
        self.mushrooms[index] = (elsewhere, REGROW);
        self.swallowed = 255;
        true
    }
}

/// Meals and energy of one founder over the whole run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Life {
    /// Meals in each phase.
    pub meals: Vec<u32>,
    /// Mean energy in each phase, for the homeostatic founder (zero otherwise).
    pub energy: Vec<u32>,
}

fn live(forager: Forager, seed: u64) -> Life {
    let founder = FounderSpec::new(seed, 1);
    let mode = match forager {
        Forager::Random | Forager::Reflex | Forager::Curious => ExperimentMode::AffordanceDiscovery,
        Forager::Homeostatic => ExperimentMode::Homeostatic,
    };
    let mut runtime =
        AgentRuntime::new_with_mode(seed, 1, founder.genome.default_brain_config(), mode);
    let mut clearing = Clearing::new(seed);
    let phases = (DECISIONS / PHASE) as usize;
    let mut life = Life {
        meals: vec![0; phases],
        energy: vec![0; phases],
    };
    let mut reflex_rest = 0;
    for decision in 0..DECISIONS {
        let observation = clearing.observe();
        let smell = (
            observation.senses.chemicals[0],
            observation.senses.chemicals[1],
        );
        let transition = runtime.step(observation).expect("sequential observations");
        // The same reflex as the runtime's, always hungry: fire, then rest.
        let reflex = if reflex_rest > 0 {
            reflex_rest -= 1;
            false
        } else {
            smell.0.max(smell.1) >= ROOTING_SMELL
        };
        if reflex {
            reflex_rest = ROOTING_REST;
        }
        let action = match forager {
            Forager::Reflex if reflex => random_action(JAW_EXPERIMENT),
            Forager::Random | Forager::Reflex => {
                let index =
                    derive(seed, u64::from(decision), 0, 0x524E_4430) % AFFORDANCE_ACTIONS as u64;
                random_action(index as usize)
            }
            Forager::Curious | Forager::Homeostatic => transition.action,
        };
        let phase = (decision / PHASE) as usize;
        if clearing.act(action) {
            life.meals[phase] += 1;
        }
        life.energy[phase] += u32::from(runtime.state().homeostasis.energy);
    }
    for energy in &mut life.energy {
        *energy /= PHASE;
    }
    life
}

fn random_action(index: usize) -> PrimitiveAction {
    let mut action = PrimitiveAction::default();
    match index {
        0 => action.thrust = 256,
        1 => action.thrust = -256,
        2 => action.turn = 256,
        3 => action.turn = -256,
        4 => action.strafe = 256,
        5 => action.strafe = -256,
        10 => action.jaw = 256,
        _ => {}
    }
    action
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Row {
    pub forager: Forager,
    /// One life per seed.
    pub lives: Vec<Life>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ForagingReport {
    pub schema_version: u16,
    pub seeds: [u64; 8],
    pub decisions: u32,
    pub phase: u32,
    pub rows: Vec<Row>,
}

impl ForagingReport {
    #[must_use]
    pub fn row(&self, forager: Forager) -> &Row {
        self.rows
            .iter()
            .find(|row| row.forager == forager)
            .expect("every forager is run")
    }
}

/// Meals in `phase`, summed over seeds.
#[must_use]
pub fn meals(row: &Row, phase: usize) -> u32 {
    row.lives.iter().map(|life| life.meals[phase]).sum()
}

/// Runs every forager over all seeds.
#[must_use]
pub fn run() -> ForagingReport {
    let rows = FORAGERS
        .iter()
        .map(|forager| Row {
            forager: *forager,
            lives: SEEDS.iter().map(|seed| live(*forager, *seed)).collect(),
        })
        .collect();
    ForagingReport {
        schema_version: 1,
        seeds: SEEDS,
        decisions: DECISIONS,
        phase: PHASE,
        rows,
    }
}

/// Compact truthful Markdown rendering for checked-in experiment evidence.
#[must_use]
pub fn markdown(report: &ForagingReport) -> String {
    let phases = (report.decisions / report.phase) as usize;
    let mut text = format!(
        "# Foraging test\n\n\
         A {SIZE}-node clearing holds {MUSHROOMS} mushrooms and {TREES} tree trunks. The founder \
         sees colours at a distance on its forward retina, smells mushrooms through two \
         nostrils, and eats one by opening its jaw within {REACH} nodes of it; an eaten \
         mushroom grows again elsewhere after {REGROW} decisions. Only the homeostatic founder \
         has live energy, and its only reward is the fall of its own hunger. A hungry founder opens \
         its jaw by reflex when the smell at its snout reaches {ROOTING_SMELL}; the reflex-only \
         founder has that reflex but acts at random otherwise, so it shows what the reflex alone \
         achieves. {} seeds, {} \
         decisions each.\n\n\
         *Meals per 1000 decisions*, pooled over seeds, in each {}-decision phase:\n\n\
         | founder |",
        report.seeds.len(),
        report.decisions,
        report.phase
    );
    for phase in 0..phases {
        text.push_str(&format!(" phase {} |", phase + 1));
    }
    text.push_str(" late meals per seed |\n|---|");
    text.push_str(&"---:|".repeat(phases));
    text.push_str("---|\n");
    let decisions = report.phase * report.seeds.len() as u32;
    for row in &report.rows {
        text.push_str(&format!("| {:?} |", row.forager));
        for phase in 0..phases {
            text.push_str(&format!(" {} |", rate(meals(row, phase) * 10, decisions)));
        }
        let late: Vec<_> = row
            .lives
            .iter()
            .map(|life| life.meals[phases - 1].to_string())
            .collect();
        text.push_str(&format!(" {} |\n", late.join(" · ")));
    }
    let homeostatic = report.row(Forager::Homeostatic);
    text.push_str(
        "\nMean energy of the homeostatic founder per phase (set point 200, birth 140): ",
    );
    let energy: Vec<_> = (0..phases)
        .map(|phase| {
            let total: u32 = homeostatic
                .lives
                .iter()
                .map(|life| life.energy[phase])
                .sum();
            (total / report.seeds.len() as u32).to_string()
        })
        .collect();
    text.push_str(&energy.join(" → "));
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_life_is_deterministic() {
        assert_eq!(live(Forager::Homeostatic, 7), live(Forager::Homeostatic, 7));
    }

    #[test]
    fn eating_needs_the_jaw_near_a_mushroom() {
        let mut clearing = Clearing::new(7);
        let ahead = Point {
            x: clearing.position.x + 1.0,
            z: clearing.position.z,
        };
        clearing.mushrooms[0] = (ahead, 0);
        clearing.trees.clear();
        assert!(!clearing.act(PrimitiveAction::default()));
        assert!(clearing.act(PrimitiveAction {
            jaw: 256,
            ..PrimitiveAction::default()
        }));
        assert_eq!(clearing.observe().body.swallowed, 255);
    }
}
