//! Bounded private semantic graph consolidated from episodic experience.
//!
//! A node stands for "this action, in contexts like this, had this kind of bodily outcome". Nodes
//! are merged by predictive, action-conditioned equivalence (same exact action, same signed
//! health/fullness/pain outcome class, nearby recurrent and drive context), never by visual
//! similarity. Support is the evidence weight of a node. A new observation of the same action in
//! a nearby context with a different outcome class contradicts the older node and halves its
//! support, so the graph follows a changed world instead of averaging two worlds together.
//!
//! Graph summaries use the same fixed-size summary type as raw episodic retrieval, so the policy
//! consensus rule is shared and bounded work per decision is preserved.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "all compact values are explicitly bounded before fixed-width casts"
)]

use serde::{Deserialize, Serialize};
use sim_kernels::derive;

use super::{
    LATENT_FEATURES, SENSE_FEATURES,
    episodic::{EPISODIC_ENCODER_VERSION, EPISODIC_TOP_K, EpisodicSummary, action_fingerprint},
};
use crate::environment::PrimitiveAction;

/// Current serialized graph schema.
pub const GRAPH_SCHEMA_VERSION: u16 = 1;
/// Maximum private nodes retained by one individual.
pub const GRAPH_CAPACITY: usize = 16;
const MERGE_DISTANCE: u32 = 192;
const SUPPORT_PER_OBSERVATION: u16 = 2;
const MAX_SUPPORT: u16 = 1_024;
const DECAY_TICKS: u64 = 128;

/// Checkpointable private semantic graph.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct SemanticGraph {
    /// Serialized graph schema; zero means never written.
    pub schema_version: u16,
    /// Representation version accepted for latent comparisons.
    pub encoder_version: u16,
    /// Bounded nodes.
    pub nodes: Vec<SemanticNode>,
    /// Identity assigned to the next node.
    pub next_node_id: u64,
    /// Nodes ever created; with `evicted` this measures node churn.
    pub created: u64,
    /// Nodes forgotten for capacity.
    pub evicted: u64,
    /// Contradictions applied to older nodes.
    pub contradictions_applied: u64,
}

/// One consolidated action-outcome equivalence class in one kind of context.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SemanticNode {
    /// Stable private identity.
    pub node_id: u64,
    /// Exact primitive action the node is conditioned on.
    pub action: PrimitiveAction,
    /// Signs of the health, fullness, and pain changes.
    pub outcome_class: [i8; 3],
    /// Running centroid of the recurrent state before the action.
    pub latent: Vec<i16>,
    /// Running centroid of the felt health, fullness, and pain drives before the action.
    pub drive: [i16; 3],
    /// Running mean signed health, fullness, and pain changes.
    pub body_delta: [i16; 3],
    /// Running mean fall in felt drive; positive is relief.
    pub drive_delta: i16,
    /// Evidence weight; contradicted nodes lose half of it.
    pub support: u16,
    /// Observations merged into this node.
    pub observations: u32,
    /// Times a conflicting observation halved this node's support.
    pub contradictions: u16,
    /// Decision tick of the latest supporting observation.
    pub last_tick: u64,
}

/// What one observation did to the graph, recorded for mediation and churn metrics.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GraphWrite {
    /// An existing node absorbed the observation.
    pub merged: bool,
    /// A new node was created.
    pub created: bool,
    /// A node was forgotten to make room.
    pub evicted: bool,
    /// Older nodes whose support was halved.
    pub contradicted: u16,
    /// Nodes retained afterwards.
    pub nodes: u16,
}

impl SemanticGraph {
    /// The never-written newborn graph.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            schema_version: 0,
            encoder_version: 0,
            nodes: Vec::new(),
            next_node_id: 0,
            created: 0,
            evicted: 0,
            contradictions_applied: 0,
        }
    }

    /// Creates a graph already stamped with the current schema.
    #[must_use]
    pub fn current() -> Self {
        Self {
            schema_version: GRAPH_SCHEMA_VERSION,
            encoder_version: EPISODIC_ENCODER_VERSION,
            ..Self::default()
        }
    }

    /// Accepts a never-written graph and rejects anything else that is out of range.
    pub(crate) fn migrate_and_validate(&mut self) -> Result<(), String> {
        if *self == Self::empty() {
            // Never written: stays canonical, so checkpoints that predate the graph round-trip.
            return Ok(());
        }
        if self.schema_version != GRAPH_SCHEMA_VERSION
            || self.encoder_version != EPISODIC_ENCODER_VERSION
            || self.nodes.len() > GRAPH_CAPACITY
            || self.nodes.iter().any(|node| {
                node.latent.len() != LATENT_FEATURES
                    || node.support == 0
                    || node.support > MAX_SUPPORT
                    || node.observations == 0
                    || node.node_id >= self.next_node_id
            })
        {
            return Err("semantic graph state is out of range or incompatible".to_owned());
        }
        Ok(())
    }

    /// Consolidates one completed action consequence.
    #[allow(
        clippy::too_many_arguments,
        reason = "one causal write has this many inputs"
    )]
    pub fn observe(
        &mut self,
        latent_before: &[i16],
        source_before: &[i16],
        action: PrimitiveAction,
        body_delta: [i16; 3],
        drive_delta: i16,
        tick: u64,
    ) -> GraphWrite {
        if self.schema_version == 0 {
            *self = Self::current();
        }
        if latent_before.len() != LATENT_FEATURES || source_before.len() != SENSE_FEATURES {
            return self.write(GraphWrite::default());
        }
        let class = outcome_class(body_delta);
        if class == [0; 3] {
            return self.write(GraphWrite::default());
        }
        self.decay(tick);
        let drive = drive_of(source_before);
        let mut write = GraphWrite::default();
        for node in &mut self.nodes {
            if node.action == action
                && node.outcome_class != class
                && distance(node, latent_before, drive) < MERGE_DISTANCE
            {
                node.support = (node.support / 2).max(1);
                node.contradictions = node.contradictions.saturating_add(1);
                write.contradicted += 1;
                self.contradictions_applied += 1;
            }
        }
        if let Some(node) = self.nodes.iter_mut().find(|node| {
            node.action == action
                && node.outcome_class == class
                && distance(node, latent_before, drive) < MERGE_DISTANCE
        }) {
            let seen = i64::from(node.observations);
            for (centroid, value) in node.latent.iter_mut().zip(latent_before) {
                *centroid = running_mean(*centroid, *value, seen);
            }
            for index in 0..3 {
                node.drive[index] = running_mean(node.drive[index], drive[index], seen);
                node.body_delta[index] =
                    running_mean(node.body_delta[index], body_delta[index], seen);
            }
            node.drive_delta = running_mean(node.drive_delta, drive_delta, seen);
            node.observations = node.observations.saturating_add(1);
            node.support = node
                .support
                .saturating_add(SUPPORT_PER_OBSERVATION)
                .min(MAX_SUPPORT);
            node.last_tick = tick;
            write.merged = true;
            return self.write(write);
        }
        if self.nodes.len() == GRAPH_CAPACITY {
            let weakest = self
                .nodes
                .iter()
                .enumerate()
                .min_by_key(|(_, node)| (node.support, node.last_tick, node.node_id))
                .map(|(index, _)| index)
                .unwrap_or_default();
            self.nodes.remove(weakest);
            self.evicted += 1;
            write.evicted = true;
        }
        self.nodes.push(SemanticNode {
            node_id: self.next_node_id,
            action,
            outcome_class: class,
            latent: latent_before.to_vec(),
            drive,
            body_delta,
            drive_delta,
            support: SUPPORT_PER_OBSERVATION,
            observations: 1,
            contradictions: 0,
            last_tick: tick,
        });
        self.next_node_id += 1;
        self.created += 1;
        write.created = true;
        self.write(write)
    }

    /// Nearest nodes for the current context in the same fixed-size shape as raw retrieval,
    /// keeping one node per distinct action before filling spare slots.
    #[must_use]
    pub fn retrieve(
        &self,
        latent: &[i16],
        source: &[i16; SENSE_FEATURES],
    ) -> [EpisodicSummary; EPISODIC_TOP_K] {
        let drive = drive_of(source);
        let mut ranked: Vec<_> = self
            .nodes
            .iter()
            .map(|node| (distance(node, latent, drive), node))
            .collect();
        ranked.sort_unstable_by_key(|(distance, node)| (*distance, node.node_id));
        let mut selected: Vec<(u32, &SemanticNode)> = Vec::with_capacity(EPISODIC_TOP_K);
        for candidate in &ranked {
            if selected
                .iter()
                .all(|(_, node)| node.action != candidate.1.action)
            {
                selected.push(*candidate);
                if selected.len() == EPISODIC_TOP_K {
                    break;
                }
            }
        }
        for candidate in ranked {
            if selected.len() == EPISODIC_TOP_K {
                break;
            }
            if selected
                .iter()
                .all(|(_, node)| node.node_id != candidate.1.node_id)
            {
                selected.push(candidate);
            }
        }
        let mut result = [EpisodicSummary::default(); EPISODIC_TOP_K];
        for (slot, (distance, node)) in result.iter_mut().zip(&selected) {
            *slot = EpisodicSummary {
                valid: true,
                event_id: node.node_id,
                action: node.action,
                sensory_delta: [0; SENSE_FEATURES],
                body_delta: node.body_delta,
                drive_delta: node.drive_delta,
                distance: *distance,
                evidence: node.support,
                surprise: 0,
                contradictions: node.contradictions,
            };
        }
        result
    }

    /// Stable fingerprint of the graph, including its churn counters.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut result = derive(
            u64::from(self.schema_version),
            self.next_node_id,
            self.created ^ (self.evicted << 20) ^ (self.contradictions_applied << 40),
            0x5347_5248,
        );
        for node in &self.nodes {
            result = derive(
                result,
                node.node_id,
                (u64::from(node.support) << 32) | u64::from(node.contradictions),
                action_fingerprint(node.action),
            );
            result = derive(
                result,
                node.last_tick,
                node.latent.iter().fold(0_u64, |hash, value| {
                    hash.wrapping_mul(31)
                        .wrapping_add(u64::from(value.cast_unsigned()))
                }),
                u64::from(node.outcome_class[0].cast_unsigned())
                    | u64::from(node.outcome_class[1].cast_unsigned()) << 8
                    | u64::from(node.outcome_class[2].cast_unsigned()) << 16,
            );
        }
        result
    }

    fn write(&self, mut write: GraphWrite) -> GraphWrite {
        write.nodes = u16::try_from(self.nodes.len()).unwrap_or(u16::MAX);
        write
    }

    fn decay(&mut self, tick: u64) {
        for node in &mut self.nodes {
            let lost = u16::try_from(tick.saturating_sub(node.last_tick) / DECAY_TICKS)
                .unwrap_or(u16::MAX);
            node.support = node.support.saturating_sub(lost).max(1);
        }
    }
}

fn outcome_class(body_delta: [i16; 3]) -> [i8; 3] {
    [
        body_delta[0].signum() as i8,
        body_delta[1].signum() as i8,
        body_delta[2].signum() as i8,
    ]
}

fn drive_of(source: &[i16]) -> [i16; 3] {
    [
        255_i16.saturating_sub(source[12]),
        255_i16.saturating_sub(source[13]),
        source[15],
    ]
}

fn distance(node: &SemanticNode, latent: &[i16], drive: [i16; 3]) -> u32 {
    let latent_distance: u32 = node
        .latent
        .iter()
        .zip(latent)
        .map(|(left, right)| u32::from(left.abs_diff(*right)))
        .sum();
    let drive_distance: u32 = node
        .drive
        .iter()
        .zip(drive)
        .map(|(left, right)| u32::from(left.abs_diff(right)) * 4)
        .sum();
    latent_distance.saturating_add(drive_distance)
}

fn running_mean(mean: i16, value: i16, seen: i64) -> i16 {
    ((i64::from(mean) * seen + i64::from(value)) / (seen + 1)) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(thrust: i16) -> PrimitiveAction {
        PrimitiveAction {
            thrust,
            ..PrimitiveAction::default()
        }
    }

    fn observe(graph: &mut SemanticGraph, thrust: i16, fullness: i16, tick: u64) -> GraphWrite {
        let latent = [0_i16; LATENT_FEATURES];
        let mut source = [0_i16; SENSE_FEATURES];
        source[12] = 200;
        source[13] = 100;
        graph.observe(
            &latent,
            &source,
            action(thrust),
            [0, fullness, 0],
            fullness,
            tick,
        )
    }

    #[test]
    fn equivalent_observations_merge_and_a_contradiction_halves_the_old_support() {
        let mut graph = SemanticGraph::current();
        for tick in 0..5 {
            observe(&mut graph, -256, 4, tick);
        }
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].observations, 5);
        let before = graph.nodes[0].support;
        let write = observe(&mut graph, -256, -4, 5);
        assert!(write.created);
        assert_eq!(write.contradicted, 1);
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.nodes[0].support, before / 2);
        assert_eq!(graph.nodes[0].contradictions, 1);
    }

    #[test]
    fn retrieval_keeps_one_node_per_action_and_follows_a_reversed_world() {
        let mut graph = SemanticGraph::current();
        for tick in 0..6 {
            observe(&mut graph, -256, 4, tick);
            observe(&mut graph, 256, -4, tick);
        }
        for tick in 6..9 {
            observe(&mut graph, -256, -4, tick);
            observe(&mut graph, 256, 4, tick);
        }
        let latent = [0_i16; LATENT_FEATURES];
        let mut source = [0_i16; SENSE_FEATURES];
        source[12] = 200;
        source[13] = 100;
        let summaries = graph.retrieve(&latent, &source);
        let (chosen, decision) =
            super::super::episodic::EpisodicMemory::policy_decision(&summaries, action(-256));
        assert_eq!(chosen, action(256));
        assert!(decision.action_changed);
    }

    #[test]
    fn capacity_is_bounded_and_churn_is_counted() {
        let mut graph = SemanticGraph::current();
        for index in 0..(GRAPH_CAPACITY as i16 + 6) {
            observe(&mut graph, index * 3, 4, u64::try_from(index).unwrap());
        }
        assert_eq!(graph.nodes.len(), GRAPH_CAPACITY);
        assert_eq!(graph.evicted, 6);
        assert_eq!(graph.created, GRAPH_CAPACITY as u64 + 6);
    }

    #[test]
    fn checkpoint_round_trip_and_rejection_of_out_of_range_state() {
        let mut graph = SemanticGraph::current();
        observe(&mut graph, -256, 4, 0);
        let mut loaded: SemanticGraph =
            serde_json::from_str(&serde_json::to_string(&graph).unwrap()).unwrap();
        loaded.migrate_and_validate().unwrap();
        assert_eq!(loaded.fingerprint(), graph.fingerprint());
        loaded.nodes[0].support = 0;
        assert!(loaded.migrate_and_validate().is_err());
        let mut legacy: SemanticGraph = serde_json::from_str("{}").unwrap();
        legacy.migrate_and_validate().unwrap();
        assert_eq!(legacy.nodes.len(), 0);
    }

    /// Development check recorded in the 8B preregistration: on a synthetic reversal, how many
    /// choices after the world flips does each memory need before it recommends the new best
    /// action? Not a result, only evidence that the experiment has headroom to find a difference.
    #[test]
    fn graph_follows_a_reversal_no_later_than_raw_episodes() {
        use super::super::episodic::{EpisodicConfig, EpisodicMemory};
        use crate::{
            RawSensoryFrame,
            environment::{
                BodyProprioception, EnvironmentCapabilities, EnvironmentObservation, Interoception,
                TransportStatus,
            },
        };

        let observation = |fullness: u16| EnvironmentObservation {
            sequence: 0,
            senses: RawSensoryFrame::EMPTY,
            body: BodyProprioception::default(),
            capabilities: EnvironmentCapabilities::default(),
            transport: TransportStatus::Healthy,
            retina: None,
            interoception: Some(Interoception {
                health: 200,
                fullness,
                air: 255,
                pain: 0,
            }),
            held_item: None,
        };
        let latent = [0_i16; LATENT_FEATURES];
        let mut raw = EpisodicMemory::new(EpisodicConfig {
            policy_enabled: true,
            ..EpisodicConfig::default()
        });
        let mut graph = EpisodicMemory::new(EpisodicConfig {
            policy_enabled: true,
            graph_enabled: true,
            ..EpisodicConfig::default()
        });
        let mut tick = 0_u64;
        let mut experience = |memory: &mut EpisodicMemory, thrust: i16, gain: bool| {
            let mut before = [0_i16; SENSE_FEATURES];
            before[12] = 200;
            before[13] = 100;
            let mut after = before;
            after[13] = if gain { 140 } else { 60 };
            tick += 1;
            memory.observe(
                &latent,
                &latent,
                &before,
                &after,
                action(thrust),
                &[0; SENSE_FEATURES],
                0,
                &observation(after[13].cast_unsigned()),
                tick,
            );
        };
        let mut source = [0_i16; SENSE_FEATURES];
        source[12] = 200;
        source[13] = 100;
        let recommends = |memory: &EpisodicMemory, best: i16, fallback: i16| {
            let summaries = memory.policy_summaries(&latent, &source);
            EpisodicMemory::policy_decision(&summaries, action(fallback)).0 == action(best)
        };
        for round in 0..20 {
            for memory in [&mut raw, &mut graph] {
                experience(memory, -256, true);
                experience(memory, 256, false);
            }
            let _ = round;
        }
        assert!(recommends(&raw, -256, 256) && recommends(&graph, -256, 256));
        let mut switched = [None, None];
        for round in 1..=40_u32 {
            for memory in [&mut raw, &mut graph] {
                experience(memory, -256, false);
                experience(memory, 256, true);
            }
            for (slot, memory) in [&raw, &graph].into_iter().enumerate() {
                if switched[slot].is_none() && recommends(memory, 256, -256) {
                    switched[slot] = Some(round);
                }
            }
        }
        eprintln!(
            "reversal switch rounds: raw {:?}, graph {:?}",
            switched[0], switched[1]
        );
        let graph_round = switched[1].expect("the graph switches after a reversal");
        assert!(
            graph_round <= 6,
            "graph should adapt within a few observations"
        );
        assert!(switched[0].is_none_or(|raw_round| graph_round <= raw_round));
    }

    #[test]
    fn enabling_the_graph_cannot_change_any_prediction() {
        use super::super::{
            RecurrentPredictor,
            episodic::{EpisodicConfig, EpisodicMemory},
            learn_transition,
        };
        use crate::{
            RawSensoryFrame,
            environment::{
                BodyProprioception, EnvironmentCapabilities, EnvironmentObservation, Interoception,
                TransportStatus,
            },
        };

        let observation = |sequence: u64, fullness: u16| EnvironmentObservation {
            sequence,
            senses: RawSensoryFrame::EMPTY,
            body: BodyProprioception::default(),
            capabilities: EnvironmentCapabilities::default(),
            transport: TransportStatus::Healthy,
            retina: None,
            interoception: Some(Interoception {
                health: 200,
                fullness,
                air: 255,
                pain: 0,
            }),
            held_item: None,
        };
        let run = |graph_enabled: bool| {
            let mut predictor = RecurrentPredictor::new(7);
            let mut memory = EpisodicMemory::new(EpisodicConfig {
                policy_enabled: true,
                graph_enabled,
                ..EpisodicConfig::default()
            });
            (0..80_u64)
                .map(|tick| {
                    let fullness = 40 + u16::try_from(tick % 8).unwrap_or_default() * 25;
                    let thrust = if tick % 2 == 0 { 256 } else { -256 };
                    learn_transition(
                        &mut predictor,
                        &mut memory,
                        &observation(tick, fullness),
                        action(thrust),
                        tick,
                    );
                    predictor.prediction.clone()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run(false), run(true));
    }
}
