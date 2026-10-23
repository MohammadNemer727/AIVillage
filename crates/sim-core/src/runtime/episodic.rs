//! Private bounded lifetime episodic memory over body-accessible experience.
//!
//! Records contain no labels, names, coordinates, task reward, or inherited acquired state.
//! Compact source features are retained beside versioned recurrent keys so representation drift
//! can be rejected or compared without silently reinterpreting an old latent.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::needless_range_loop,
    reason = "all compact values are explicitly bounded before fixed-width casts"
)]

use serde::{Deserialize, Serialize};
use sim_kernels::derive;

use super::{
    LATENT_FEATURES, SENSE_FEATURES,
    semantic::{GraphWrite, SemanticGraph},
};
use crate::environment::{EnvironmentObservation, PrimitiveAction};

/// Current serialized event-record schema.
pub const EPISODIC_SCHEMA_VERSION: u16 = 1;
/// Version of the fixed recurrent representation used as an event key.
pub const EPISODIC_ENCODER_VERSION: u16 = 1;
/// Maximum private events retained by one individual.
pub const EPISODIC_CAPACITY: usize = 64;
/// Fixed number of summaries returned by every retrieval.
pub const EPISODIC_TOP_K: usize = 4;
const INITIAL_RETENTION: u16 = 1_024;
const NOVEL_DISTANCE: u32 = 192;
const SURPRISE_WRITE: u16 = 6;

/// Independent ablations for episodic effects on prediction and action selection.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct EpisodicConfig {
    /// Permit retrieved residuals to amend the recurrent next-sense prediction.
    pub prediction_enabled: bool,
    /// Permit retrieved body outcomes to bias embodied action selection.
    pub policy_enabled: bool,
    /// Consolidate episodes into a bounded semantic graph and let it, not raw events, supply
    /// the retrieval summaries the policy sees.
    pub graph_enabled: bool,
}

/// One experience-grounded event belonging to exactly one lifetime.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EpisodicEvent {
    /// Event-record schema at write time.
    pub schema_version: u16,
    /// Recurrent encoder version that produced both keys.
    pub encoder_version: u16,
    /// Monotonic private event identity.
    pub event_id: u64,
    /// Recurrent representation before the observed consequence.
    pub latent_before: Vec<i16>,
    /// Recurrent representation after the observed consequence.
    pub latent_after: Vec<i16>,
    /// Stable compact body-accessible source features before the consequence.
    pub source_before: Vec<i16>,
    /// Stable compact body-accessible source features after the consequence.
    pub source_after: Vec<i16>,
    /// Primitive action that preceded the consequence.
    pub action: PrimitiveAction,
    /// Predictor uncertainty before observing this consequence.
    pub uncertainty: u16,
    /// Mean absolute prediction surprise at the consequence.
    pub surprise: u16,
    /// Signed health, fullness, and pain changes.
    pub body_delta: [i16; 3],
    /// Fall in aggregate felt health/fullness/pain drive; positive is relief.
    #[serde(default)]
    pub drive_delta: i16,
    /// Acceleration, vertical motion, and mean retinal motion available to the body.
    pub self_motion: [i16; 3],
    /// Lifetime decision tick at which the consequence was felt.
    pub tick: u64,
    /// Deterministically decaying retention strength.
    pub retention: u16,
    /// Number of matching observations supporting this exact event.
    pub evidence: u16,
    /// Conflicting outcomes observed under a nearby key and the same action.
    pub contradictions: u16,
    /// Stable fingerprint of immutable event evidence.
    pub fingerprint: u64,
}

/// Fixed-size nonsemantic retrieval result.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EpisodicSummary {
    /// False for padding entries when fewer than [`EPISODIC_TOP_K`] records match.
    pub valid: bool,
    /// Source event identity.
    pub event_id: u64,
    /// Action whose consequence was retained.
    pub action: PrimitiveAction,
    /// Observed compact sensory change.
    pub sensory_delta: [i16; SENSE_FEATURES],
    /// Signed health, fullness, and pain changes.
    pub body_delta: [i16; 3],
    /// Fall in aggregate felt drive; positive is relief.
    pub drive_delta: i16,
    /// Query distance, with smaller values more similar.
    pub distance: u32,
    /// Retained evidence count.
    pub evidence: u16,
    /// Surprise that made the event salient.
    pub surprise: u16,
    /// Nearby same-action outcomes that disagreed with this record.
    pub contradictions: u16,
}

/// Machine-readable causal evidence from completing one pending episode.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EpisodicWrite {
    /// Retained records before this observation.
    pub events_before: u16,
    /// Retained records after this observation.
    pub events_after: u16,
    /// Whether this observation created a distinct retained event.
    pub inserted: bool,
    /// Whether it reinforced an exactly matching retained event.
    pub reinforced: bool,
    /// Prediction error that made the transition eligible for a surprise write.
    pub surprise: u16,
    /// Signed body utility of the observed consequence.
    pub outcome_utility: i32,
    /// Decisions between the action and this observed consequence.
    pub eligibility_delay: u16,
    /// What the same consequence did to the semantic graph, when it is enabled.
    #[serde(default)]
    pub graph: GraphWrite,
}

/// Machine-readable retrieval-to-action mediation for one decision.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EpisodicPolicyDecision {
    /// Number of valid body-consequence summaries in the bounded retrieval.
    pub retrieved: u16,
    /// Distance of the nearest valid body-consequence summary.
    pub nearest_distance: Option<u32>,
    /// Evidence-weighted remembered utility for the fallback, when represented.
    pub fallback_utility: Option<i64>,
    /// Remembered utility of the selected episodic alternative, when one qualified.
    pub selected_utility: Option<i64>,
    /// Selected remembered utility minus remembered fallback utility.
    pub score_delta: i64,
    /// Whether retrieval replaced the policy's fallback action.
    pub action_changed: bool,
}

/// Checkpointable private episodic store.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct EpisodicMemory {
    /// Serialized memory schema.
    pub schema_version: u16,
    /// Representation version accepted for latent comparisons.
    pub encoder_version: u16,
    /// Independent prediction and policy ablations.
    pub config: EpisodicConfig,
    /// Bounded event records.
    pub events: Vec<EpisodicEvent>,
    /// Identity assigned to the next distinct event.
    pub next_event_id: u64,
    /// Number of records deterministically forgotten.
    pub forgotten: u64,
    /// Bounded semantic consolidation of the same experience; empty unless enabled.
    pub graph: SemanticGraph,
}

impl Default for EpisodicMemory {
    fn default() -> Self {
        Self::new(EpisodicConfig::default())
    }
}

impl EpisodicMemory {
    /// Canonical empty newborn memory.
    #[must_use]
    pub const fn new(config: EpisodicConfig) -> Self {
        Self {
            schema_version: EPISODIC_SCHEMA_VERSION,
            encoder_version: EPISODIC_ENCODER_VERSION,
            config,
            events: Vec::new(),
            next_event_id: 0,
            forgotten: 0,
            graph: SemanticGraph::empty(),
        }
    }

    /// Migrates supported legacy state and validates all bounds and fingerprints.
    pub(crate) fn migrate_and_validate(&mut self) -> Result<(), String> {
        if self.schema_version == 0 && self.events.is_empty() {
            self.schema_version = EPISODIC_SCHEMA_VERSION;
            self.encoder_version = EPISODIC_ENCODER_VERSION;
        }
        self.graph.migrate_and_validate()?;
        if self.schema_version != EPISODIC_SCHEMA_VERSION
            || self.encoder_version != EPISODIC_ENCODER_VERSION
            || self.events.len() > EPISODIC_CAPACITY
            || self.events.iter().any(|event| {
                event.schema_version != EPISODIC_SCHEMA_VERSION
                    || event.encoder_version != EPISODIC_ENCODER_VERSION
                    || event.latent_before.len() != LATENT_FEATURES
                    || event.latent_after.len() != LATENT_FEATURES
                    || event.source_before.len() != SENSE_FEATURES
                    || event.source_after.len() != SENSE_FEATURES
                    || event.retention == 0
                    || event.evidence == 0
                    || event.fingerprint != event_fingerprint(event)
            })
        {
            return Err("episodic memory state is out of range or incompatible".to_owned());
        }
        if self
            .events
            .iter()
            .any(|event| event.event_id >= self.next_event_id)
        {
            return Err("episodic event identity is out of range".to_owned());
        }
        Ok(())
    }

    /// Completes the preceding action event, writes it only when experience is salient, and
    /// consolidates the consequence into the semantic graph when that is enabled.
    #[allow(
        clippy::too_many_arguments,
        reason = "event construction keeps the complete bounded evidence contract in one causal write"
    )]
    pub fn observe(
        &mut self,
        latent_before: &[i16],
        latent_after: &[i16],
        source_before: &[i16],
        source_after: &[i16; SENSE_FEATURES],
        action: PrimitiveAction,
        prediction: &[i16],
        uncertainty: u16,
        observation: &EnvironmentObservation,
        tick: u64,
    ) -> EpisodicWrite {
        let graph = if self.config.graph_enabled && source_before.len() == SENSE_FEATURES {
            let (body_delta, drive_delta) = outcome(source_before, source_after);
            self.graph.observe(
                latent_before,
                source_before,
                action,
                body_delta,
                drive_delta,
                tick,
            )
        } else {
            GraphWrite::default()
        };
        let mut write = self.observe_event(
            latent_before,
            latent_after,
            source_before,
            source_after,
            action,
            prediction,
            uncertainty,
            observation,
            tick,
        );
        write.graph = graph;
        write
    }

    #[allow(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "event construction keeps the complete bounded evidence contract in one causal write"
    )]
    fn observe_event(
        &mut self,
        latent_before: &[i16],
        latent_after: &[i16],
        source_before: &[i16],
        source_after: &[i16; SENSE_FEATURES],
        action: PrimitiveAction,
        prediction: &[i16],
        uncertainty: u16,
        observation: &EnvironmentObservation,
        tick: u64,
    ) -> EpisodicWrite {
        let events_before = u16::try_from(self.events.len()).unwrap_or(u16::MAX);
        if latent_before.len() != LATENT_FEATURES
            || latent_after.len() != LATENT_FEATURES
            || source_before.len() != SENSE_FEATURES
            || prediction.len() != SENSE_FEATURES
        {
            return EpisodicWrite {
                events_before,
                events_after: events_before,
                ..EpisodicWrite::default()
            };
        }
        self.decay(tick);
        let surprise = mean_difference(prediction, source_after);
        let (body_delta, drive_delta) = outcome(source_before, source_after);
        let outcome_utility = body_utility(drive_delta, body_delta);
        let mean_motion = observation
            .senses
            .retinal
            .iter()
            .map(|sample| i32::from(sample.motion))
            .sum::<i32>()
            / i32::try_from(observation.senses.retinal.len()).unwrap_or(1);
        let self_motion = [
            observation.senses.acceleration,
            observation.body.vertical_motion,
            mean_motion.clamp(-255, 255) as i16,
        ];
        let nearest = self
            .events
            .iter()
            .map(|event| key_distance(event, latent_before, source_before))
            .min()
            .unwrap_or(u32::MAX);
        let harmful = body_delta[0] < 0 || body_delta[2] > 0;
        let relieving = body_delta[0] > 0 || body_delta[1] > 0 || body_delta[2] < 0;
        let novel = nearest >= NOVEL_DISTANCE;
        if surprise < SURPRISE_WRITE && !harmful && !relieving && !novel {
            return EpisodicWrite {
                events_before,
                events_after: events_before,
                surprise,
                outcome_utility,
                eligibility_delay: 1,
                ..EpisodicWrite::default()
            };
        }

        let mut candidate = EpisodicEvent {
            schema_version: EPISODIC_SCHEMA_VERSION,
            encoder_version: EPISODIC_ENCODER_VERSION,
            event_id: self.next_event_id,
            latent_before: latent_before.to_vec(),
            latent_after: latent_after.to_vec(),
            source_before: source_before.to_vec(),
            source_after: source_after.to_vec(),
            action,
            uncertainty,
            surprise,
            body_delta,
            drive_delta,
            self_motion,
            tick,
            retention: INITIAL_RETENTION,
            evidence: 1,
            contradictions: 0,
            fingerprint: 0,
        };
        candidate.fingerprint = event_fingerprint(&candidate);
        if let Some(existing) = self.events.iter_mut().find(|event| {
            event.action == candidate.action
                && event.latent_before == candidate.latent_before
                && event.latent_after == candidate.latent_after
                && event.source_before == candidate.source_before
                && event.source_after == candidate.source_after
                && event.body_delta == candidate.body_delta
                && event.self_motion == candidate.self_motion
        }) {
            existing.evidence = existing.evidence.saturating_add(1);
            existing.retention = INITIAL_RETENTION;
            return EpisodicWrite {
                events_before,
                events_after: events_before,
                reinforced: true,
                surprise,
                outcome_utility,
                eligibility_delay: 1,
                ..EpisodicWrite::default()
            };
        }
        for existing in &mut self.events {
            if existing.action == action
                && key_distance(existing, latent_before, source_before) < NOVEL_DISTANCE
                && outcome_distance(existing, &candidate) > NOVEL_DISTANCE
            {
                existing.contradictions = existing.contradictions.saturating_add(1);
                candidate.contradictions = candidate.contradictions.saturating_add(1);
            }
        }
        self.next_event_id = self.next_event_id.saturating_add(1);
        if self.events.len() == EPISODIC_CAPACITY {
            let evict = self
                .events
                .iter()
                .enumerate()
                .min_by_key(|(_, event)| {
                    (
                        u32::from(event.retention)
                            + u32::from(event.evidence) * 16
                            + u32::from(event.surprise),
                        event.tick,
                        event.event_id,
                    )
                })
                .map(|(index, _)| index)
                .unwrap_or_default();
            self.events.remove(evict);
            self.forgotten = self.forgotten.saturating_add(1);
        }
        self.events.push(candidate);
        EpisodicWrite {
            events_before,
            events_after: u16::try_from(self.events.len()).unwrap_or(u16::MAX),
            inserted: true,
            surprise,
            outcome_utility,
            eligibility_delay: 1,
            ..EpisodicWrite::default()
        }
    }

    /// Retrieves a fixed-size nearest set using current latent and body drive only.
    #[must_use]
    pub fn retrieve(
        &self,
        latent: &[i16],
        source: &[i16; SENSE_FEATURES],
    ) -> [EpisodicSummary; EPISODIC_TOP_K] {
        let mut ranked: Vec<_> = self
            .events
            .iter()
            .filter(|event| {
                event.encoder_version == self.encoder_version
                    && body_utility(event.drive_delta, event.body_delta) != 0
            })
            .map(|event| (query_distance(event, latent, source), event))
            .collect();
        ranked.sort_unstable_by_key(|(distance, event)| (*distance, event.event_id));
        // Preserve action alternatives before filling spare slots. Otherwise repeated records for
        // one action can occupy the whole fixed top-k and make policy retrieval unable to compare
        // a remembered fallback with a better action.
        let mut selected: Vec<(u32, &EpisodicEvent)> = Vec::with_capacity(EPISODIC_TOP_K);
        for candidate in &ranked {
            if selected
                .iter()
                .all(|(_, event)| event.action != candidate.1.action)
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
                .all(|(_, event)| event.event_id != candidate.1.event_id)
            {
                selected.push(candidate);
            }
        }
        summaries(&selected)
    }

    /// Summaries the policy consults: the semantic graph's when it is enabled, otherwise the
    /// raw events'. Both are fixed-size and bounded.
    #[must_use]
    pub fn policy_summaries(
        &self,
        latent: &[i16],
        source: &[i16; SENSE_FEATURES],
    ) -> [EpisodicSummary; EPISODIC_TOP_K] {
        if self.config.graph_enabled {
            self.graph.retrieve(latent, source)
        } else {
            self.retrieve(latent, source)
        }
    }

    /// Retrieves nearest consequences only for the action whose outcome is being predicted.
    ///
    /// Filtering before truncation prevents unrelated actions from consuming the fixed top-k.
    #[must_use]
    pub fn retrieve_for_action(
        &self,
        latent: &[i16],
        source: &[i16; SENSE_FEATURES],
        action: PrimitiveAction,
    ) -> [EpisodicSummary; EPISODIC_TOP_K] {
        let mut ranked: Vec<_> = self
            .events
            .iter()
            .filter(|event| event.encoder_version == self.encoder_version && event.action == action)
            .map(|event| (query_distance(event, latent, source), event))
            .collect();
        ranked.sort_unstable_by_key(|(distance, event)| (*distance, event.event_id));
        ranked.truncate(EPISODIC_TOP_K);
        summaries(&ranked)
    }

    /// Chooses a retrieved action only when its evidence-weighted local outcome consensus is
    /// positive and better than the fallback's consensus. Positive and negative support must not
    /// tie, so ambiguous neighborhoods abstain. Historical pairwise contradiction counters remain
    /// audit evidence but are not treated as independent observations: doing so would count one
    /// conflicting event repeatedly while exact-context reinforcement remains sparse.
    #[must_use]
    pub fn recommend_action(
        summaries: &[EpisodicSummary],
        fallback: PrimitiveAction,
    ) -> PrimitiveAction {
        Self::policy_decision(summaries, fallback).0
    }

    /// Returns both the selected action and the complete bounded mediation evidence.
    #[must_use]
    pub fn policy_decision(
        summaries: &[EpisodicSummary],
        fallback: PrimitiveAction,
    ) -> (PrimitiveAction, EpisodicPolicyDecision) {
        let mut actions: Vec<(PrimitiveAction, i64, i64, i64, i64, u32, u64)> = Vec::new();
        for summary in summaries.iter().filter(|summary| summary.valid) {
            let weight = i64::from(summary.evidence);
            let utility = i64::from(body_utility(summary.drive_delta, summary.body_delta));
            if let Some(entry) = actions
                .iter_mut()
                .find(|(action, ..)| *action == summary.action)
            {
                entry.1 += utility * weight;
                entry.2 += weight;
                entry.3 += i64::from(utility > 0) * weight;
                entry.4 += i64::from(utility < 0) * weight;
                entry.5 = entry.5.min(summary.distance);
                entry.6 = entry.6.min(summary.event_id);
            } else {
                actions.push((
                    summary.action,
                    utility * weight,
                    weight,
                    i64::from(utility > 0) * weight,
                    i64::from(utility < 0) * weight,
                    summary.distance,
                    summary.event_id,
                ));
            }
        }
        let fallback_utility = actions
            .iter()
            .find(|(action, ..)| *action == fallback)
            .and_then(|(_, total, weight, positive, negative, ..)| {
                (positive > negative).then_some(total / weight)
            });
        let selected = actions
            .iter()
            .filter_map(
                |(action, total, weight, positive, negative, distance, event_id)| {
                    let mean = total / weight;
                    (*positive > *negative && mean > 0 && mean > fallback_utility.unwrap_or(0))
                        .then_some((
                            mean,
                            core::cmp::Reverse(*distance),
                            core::cmp::Reverse(*event_id),
                            *action,
                        ))
                },
            )
            .max_by_key(|candidate| (candidate.0, candidate.1, candidate.2));
        let action = selected.map_or(fallback, |candidate| candidate.3);
        let selected_utility = selected.map(|candidate| candidate.0);
        (
            action,
            EpisodicPolicyDecision {
                retrieved: u16::try_from(summaries.iter().filter(|summary| summary.valid).count())
                    .unwrap_or(u16::MAX),
                nearest_distance: summaries
                    .iter()
                    .filter(|summary| summary.valid)
                    .map(|summary| summary.distance)
                    .min(),
                fallback_utility,
                selected_utility,
                score_delta: selected_utility.unwrap_or_else(|| fallback_utility.unwrap_or(0))
                    - fallback_utility.unwrap_or(0),
                action_changed: action != fallback,
            },
        )
    }

    fn decay(&mut self, tick: u64) {
        for event in &mut self.events {
            let age = tick.saturating_sub(event.tick);
            let decay = u16::try_from(age / 64).unwrap_or(u16::MAX);
            event.retention = event.retention.saturating_sub(decay).max(1);
        }
    }

    /// Stable fingerprint of all private acquired state and ablation settings.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut result = derive(
            u64::from(self.schema_version),
            self.next_event_id,
            self.forgotten,
            0x4550_4D45,
        );
        result = derive(
            result,
            u64::from(self.config.prediction_enabled),
            u64::from(self.config.policy_enabled),
            0x4550_4346,
        );
        if self.config.graph_enabled || self.graph.schema_version != 0 {
            result = derive(
                result,
                u64::from(self.config.graph_enabled),
                self.graph.fingerprint(),
                0x4550_4752,
            );
        }
        for event in &self.events {
            result = derive(
                result,
                event.fingerprint,
                (u64::from(event.retention) << 32) | u64::from(event.evidence),
                0x4550_4556,
            );
        }
        result
    }
}

/// Signed health, fullness, and pain changes, and the fall in felt drive, between two felt states.
fn outcome(source_before: &[i16], source_after: &[i16]) -> ([i16; 3], i16) {
    let body_delta = [
        source_after[12].saturating_sub(source_before[12]),
        source_after[13].saturating_sub(source_before[13]),
        source_after[15].saturating_sub(source_before[15]),
    ];
    let drive_delta = body_drive(source_before)
        .saturating_sub(body_drive(source_after))
        .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    (body_delta, drive_delta)
}

/// Signed felt utility of the change between two felt states, for memories other than events.
pub(super) fn felt_utility(source_before: &[i16], source_after: &[i16]) -> i32 {
    let (body_delta, drive_delta) = outcome(source_before, source_after);
    body_utility(drive_delta, body_delta)
}

fn body_utility(drive_delta: i16, body_delta: [i16; 3]) -> i32 {
    i32::from(drive_delta) * 2 + i32::from(body_delta[0]) * 4 + i32::from(body_delta[1]) * 2
        - i32::from(body_delta[2]) * 4
}

fn summaries(ranked: &[(u32, &EpisodicEvent)]) -> [EpisodicSummary; EPISODIC_TOP_K] {
    let mut result = [EpisodicSummary::default(); EPISODIC_TOP_K];
    for (slot, (distance, event)) in result.iter_mut().zip(ranked) {
        *slot = EpisodicSummary {
            valid: true,
            event_id: event.event_id,
            action: event.action,
            sensory_delta: core::array::from_fn(|feature| {
                event.source_after[feature].saturating_sub(event.source_before[feature])
            }),
            body_delta: event.body_delta,
            drive_delta: event.drive_delta,
            distance: *distance,
            evidence: event.evidence,
            surprise: event.surprise,
            contradictions: event.contradictions,
        };
    }
    result
}

fn mean_difference(left: &[i16], right: &[i16]) -> u16 {
    let total: u32 = left
        .iter()
        .zip(right)
        .map(|(left, right)| u32::from(left.abs_diff(*right)))
        .sum();
    u16::try_from(total / u32::try_from(SENSE_FEATURES).unwrap_or(1)).unwrap_or(u16::MAX)
}

fn drive(source: &[i16]) -> [i16; 3] {
    [
        255_i16.saturating_sub(source[12]),
        255_i16.saturating_sub(source[13]),
        source[15],
    ]
}

fn body_drive(source: &[i16]) -> i32 {
    i32::from(255_i16.saturating_sub(source[12]))
        + i32::from(255_i16.saturating_sub(source[13]))
        + i32::from(source[15])
}

fn query_distance(event: &EpisodicEvent, latent: &[i16], source: &[i16]) -> u32 {
    let latent_distance: u32 = event
        .latent_before
        .iter()
        .zip(latent)
        .map(|(left, right)| u32::from(left.abs_diff(*right)))
        .sum();
    let drive_distance: u32 = drive(&event.source_before)
        .iter()
        .zip(drive(source))
        .map(|(left, right)| u32::from(left.abs_diff(right)) * 4)
        .sum();
    latent_distance.saturating_add(drive_distance)
}

fn key_distance(event: &EpisodicEvent, latent: &[i16], source: &[i16]) -> u32 {
    query_distance(event, latent, source)
}

fn outcome_distance(left: &EpisodicEvent, right: &EpisodicEvent) -> u32 {
    left.source_after
        .iter()
        .zip(&right.source_after)
        .map(|(left, right)| u32::from(left.abs_diff(*right)))
        .sum()
}

fn event_fingerprint(event: &EpisodicEvent) -> u64 {
    let mut result = derive(
        u64::from(event.schema_version),
        u64::from(event.encoder_version),
        event.tick,
        0x4550_4650,
    );
    for (index, value) in event
        .latent_before
        .iter()
        .chain(&event.latent_after)
        .chain(&event.source_before)
        .chain(&event.source_after)
        .enumerate()
    {
        result = derive(
            result,
            i64::from(*value).cast_unsigned(),
            index as u64,
            0x4550_4441,
        );
    }
    result = derive(
        result,
        i64::from(event.drive_delta).cast_unsigned(),
        0,
        0x4550_4452,
    );
    for (index, value) in event
        .body_delta
        .iter()
        .chain(&event.self_motion)
        .enumerate()
    {
        result = derive(
            result,
            i64::from(*value).cast_unsigned(),
            index as u64,
            0x4550_424F,
        );
    }
    derive(
        result,
        u64::from(event.surprise),
        u64::from(event.uncertainty),
        action_fingerprint(event.action),
    )
}

pub(super) fn action_fingerprint(action: PrimitiveAction) -> u64 {
    let mut result = 0;
    let values = [
        i64::from(action.thrust).cast_unsigned(),
        i64::from(action.turn).cast_unsigned(),
        i64::from(action.strafe).cast_unsigned(),
        i64::from(action.look_pitch).cast_unsigned(),
        u64::from(action.jump),
        u64::from(action.jaw),
        u64::from(action.hand_pulse),
        u64::from(action.acoustic),
        u64::from(action.chemical),
        u64::from(action.visual),
        u64::from(action.use_pulse),
        u64::from(action.next_slot),
    ];
    for (index, value) in values.into_iter().enumerate() {
        result = derive(result, value, index as u64, 0x4550_4143);
    }
    for (index, signal) in action.signals.into_iter().enumerate() {
        result = derive(
            result,
            i64::from(signal).cast_unsigned(),
            index as u64,
            0x4550_5347,
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RawSensoryFrame,
        environment::{BodyProprioception, EnvironmentCapabilities, TransportStatus},
    };

    fn observation(sequence: u64, health: u16, fullness: u16, pain: u16) -> EnvironmentObservation {
        EnvironmentObservation {
            sequence,
            senses: RawSensoryFrame::EMPTY,
            body: BodyProprioception::default(),
            capabilities: EnvironmentCapabilities::default(),
            transport: TransportStatus::Healthy,
            retina: None,
            interoception: Some(crate::environment::Interoception {
                health,
                fullness,
                air: 255,
                pain,
            }),
            held_item: None,
        }
    }

    fn write(memory: &mut EpisodicMemory, tick: u64, after: i16) {
        let mut source_before = [0; SENSE_FEATURES];
        source_before[12] = 255;
        source_before[13] = 80;
        let mut source_after = source_before;
        source_after[0] = after;
        source_after[12] = if after < 0 { 180 } else { 255 };
        source_after[15] = if after < 0 { 80 } else { 0 };
        memory.observe(
            &[0; LATENT_FEATURES],
            &[1; LATENT_FEATURES],
            &source_before,
            &source_after,
            PrimitiveAction {
                thrust: i16::try_from(tick).unwrap_or_default(),
                ..PrimitiveAction::default()
            },
            &[0; SENSE_FEATURES],
            0,
            &observation(
                tick,
                source_after[12].cast_unsigned(),
                source_after[13].cast_unsigned(),
                source_after[15].cast_unsigned(),
            ),
            tick,
        );
    }

    #[test]
    fn newborn_is_canonical_and_empty() {
        let memory = EpisodicMemory::default();
        assert_eq!(memory, EpisodicMemory::new(EpisodicConfig::default()));
        assert_eq!(memory.events.len(), 0);
        assert_eq!(memory.next_event_id, 0);
    }

    #[test]
    fn capacity_eviction_and_retrieval_are_deterministic() {
        let run = || {
            let mut memory = EpisodicMemory::new(EpisodicConfig {
                prediction_enabled: true,
                policy_enabled: true,
                graph_enabled: false,
            });
            for tick in 0..EPISODIC_CAPACITY as u64 + 12 {
                write(&mut memory, tick, if tick % 2 == 0 { 40 } else { -40 });
            }
            memory
        };
        let memory = run();
        assert_eq!(memory, run());
        assert_eq!(memory.events.len(), EPISODIC_CAPACITY);
        assert_eq!(memory.forgotten, 12);
        let summaries = memory.retrieve(&[0; LATENT_FEATURES], &[0; SENSE_FEATURES]);
        assert!(summaries.iter().all(|summary| summary.valid));
    }

    #[test]
    fn contradictory_outcomes_are_retained_and_marked() {
        let mut memory = EpisodicMemory::default();
        write(&mut memory, 1, 80);
        write(&mut memory, 1, -80);
        assert_eq!(memory.events.len(), 2);
        assert!(memory.events.iter().all(|event| event.contradictions == 1));
    }

    #[test]
    fn policy_uses_local_outcomes_without_double_counting_contradiction_edges() {
        let fallback = PrimitiveAction {
            thrust: 256,
            ..PrimitiveAction::default()
        };
        let alternative = PrimitiveAction {
            thrust: -256,
            ..PrimitiveAction::default()
        };
        let positive = EpisodicSummary {
            valid: true,
            action: alternative,
            body_delta: [0, 40, 0],
            evidence: 1,
            contradictions: 3,
            ..EpisodicSummary::default()
        };
        assert_eq!(
            EpisodicMemory::recommend_action(&[positive], fallback),
            alternative
        );

        let negative = EpisodicSummary {
            valid: true,
            action: alternative,
            body_delta: [0, -40, 0],
            evidence: 1,
            contradictions: 3,
            ..EpisodicSummary::default()
        };
        assert_eq!(
            EpisodicMemory::recommend_action(&[positive, negative], fallback),
            fallback
        );
    }

    #[test]
    fn checkpoint_round_trip_and_incompatible_encoder_rejection() {
        let mut memory = EpisodicMemory::default();
        write(&mut memory, 1, -80);
        let encoded = serde_json::to_string(&memory).unwrap();
        let restored: EpisodicMemory = serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored, memory);
        let mut incompatible = restored;
        incompatible.encoder_version += 1;
        assert!(incompatible.migrate_and_validate().is_err());
    }
}
