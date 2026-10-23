//! A deliberately small, loopback-only bridge for the `ai_bridge` Luanti mod.
//!
//! The bridge owns no Luanti state. Lua sends an already bounded observation after an action
//! interval; Rust validates it, advances [`sim_core::runtime::AgentRuntime`], and returns the
//! next primitive action.  It is intentionally one request at a time: async holding and timeout
//! safety remain in the Lua side, where Luanti's server steps are known.

#![forbid(unsafe_code)]

pub mod ablation;
pub mod benchmark;
pub mod contingency;
pub mod episodic_eval;
pub mod foraging;
pub mod harness;
pub mod lineage;
pub mod population;
pub mod prediction_eval;
pub mod reaching;

pub use population::{
    BirthKind, BodySlotId, LifecycleCommand, LifecycleConfig, LifecycleError, LifecycleEvent,
    LifecycleOperation, PopulationCoordinator,
};
/// Backward-compatible name for the now population-aware coordinator.
pub type Bridge = PopulationCoordinator;

use std::{
    io::Read,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sim_core::{
    environment::{EnvironmentObservation, EnvironmentTransition, PrimitiveAction},
    runtime::{
        AffordanceConfig, AgentRuntime, EpisodicConfig, EpisodicPolicyDecision, EpisodicWrite,
        ExperimentMode, FrozenPhysiology, RuntimeError,
    },
};
use sim_format::FounderSpec;
use sim_kernels::brain::{MODULATORS, NEURONS};
use tiny_http::{Header, Method, Response, Server, StatusCode};

/// Wire protocol accepted by this crate. Version 5 adds recurrent-predictor diagnostics while
/// preserving the version-4 body observation contract.
pub const PROTOCOL_VERSION: u16 = 5;
/// Requests larger than this cannot be one body's receptor frame.
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;

/// Luanti timing evidence recorded with each request. It is not given to the brain.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApplicationTrace {
    /// Monotonic dedicated-server step at which the previous action began applying.
    pub applied_server_step: u64,
    /// Number of server steps for which that action was held.
    pub applied_steps: u32,
    /// Accumulated Luanti delta time for that interval, in microseconds.
    pub accumulated_dtime_us: u64,
    /// Motor command actually applied by Luanti, after any mode-specific fixture override.
    pub applied_action: PrimitiveAction,
    /// Founder pose at the end of the recorded interval, in millinodes/milliradians.
    pub pose: AppliedPose,
}

/// Quantised Luanti pose evidence, deliberately excluded from environment observations.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppliedPose {
    /// X position in millinodes.
    pub x_millinode: i32,
    /// Y position in millinodes.
    pub y_millinode: i32,
    /// Z position in millinodes.
    pub z_millinode: i32,
    /// Luanti yaw in milliradians.
    pub yaw_milliradian: i32,
}

/// Observer-only state, never accepted as a runtime input.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Diagnostics {
    /// Explicit policy that generated this response.
    pub experiment_mode: ExperimentMode,
    /// Affordance learner settings, so a trace replays with the settings it was recorded with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affordance_config: Option<AffordanceConfig>,
    /// Runtime decision counter after this response.
    pub decision_tick: u64,
    /// Fingerprint of the post-decision brain.
    pub brain_fingerprint: u64,
    /// Action emitted by the unchanged predictive founder before any intrinsic override.
    pub baseline_action: PrimitiveAction,
    /// Intrinsic action selected by the affordance learner, if enabled.
    pub intrinsic_action: PrimitiveAction,
    /// Intrinsic information score; zero for the predictive baseline.
    pub intrinsic_score: u32,
    /// Fingerprint of checkpointed affordance state.
    pub affordance_fingerprint: u64,
    /// Fingerprint of the checkpointed recurrent representation and acquired decoder state.
    #[serde(default)]
    pub recurrent_fingerprint: u64,
    /// Fingerprint of private checkpointed episodic records.
    #[serde(default)]
    pub episodic_fingerprint: u64,
    /// Number of private events currently retained.
    #[serde(default)]
    pub episodic_events: usize,
    /// Independent episodic prediction and action-selection ablations.
    #[serde(default)]
    pub episodic_config: EpisodicConfig,
    /// Episode write → retrieval → score delta → selected-action causal evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episodic_mediation: Option<EpisodicMediation>,
    /// Whether any lifetime brain parameter has departed from its blank initial state.
    pub learned: bool,
    /// Mean absolute activity of the modulatory population.
    pub mean_modulator_activity: u16,
    /// Stores fixed by the runtime rather than supplied by Luanti.
    pub frozen_physiology: FrozenPhysiology,
}

/// Observer-only causal evidence for the episodic policy path.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EpisodicMediation {
    /// Consequence of the preceding action completed by this observation.
    pub write: EpisodicWrite,
    /// Retrieval and remembered score comparison for the current decision.
    pub retrieval: EpisodicPolicyDecision,
    /// Active policy action before episodic retrieval.
    pub pre_episodic_action: PrimitiveAction,
    /// Action emitted after the episodic policy switch.
    pub selected_action: PrimitiveAction,
    /// Curiosity or homeostatic policy score before episodic mediation.
    pub baseline_policy_score: u32,
}

/// A request from one body using the permitted Lua bridge endpoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BridgeRequest {
    /// Versioned contract discriminator.
    pub protocol: u16,
    /// Stable, never-reused evolving individual identity.
    pub agent_id: u64,
    /// Reusable body slot occupied by this individual. Omitted by protocol-v3 legacy clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_slot: Option<u32>,
    /// Complete post-action external observation.
    pub observation: EnvironmentObservation,
    /// Evidence of the preceding action's actual application.
    pub trace: ApplicationTrace,
}

/// The next action and its causally associated transition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BridgeResponse {
    /// Echoed wire protocol version.
    pub protocol: u16,
    /// Echoed stable founder identity.
    pub agent_id: u64,
    /// Current reusable Luanti body slot, distinct from individual identity.
    pub body_slot: u32,
    /// Observation sequence for which this action was generated.
    pub generated_sequence: u64,
    /// The sequence the bridge will accept next.
    pub expected_sequence: u64,
    /// The primitive action Lua may hold until its next completed observation interval.
    pub action: PrimitiveAction,
    /// Data intended exclusively for observer diagnostics, never an action or observation.
    pub diagnostics: Diagnostics,
}

/// One replayable post-action bridge exchange, suitable for NDJSON output.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TraceRecord {
    /// The complete validated request, including Luanti application timing.
    pub request: BridgeRequest,
    /// The action generated from its observation.
    pub response: BridgeResponse,
    /// Population eligibility state retained at this transition boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reproduction: Option<ReproductionTransitionAudit>,
}

/// Observer-only reproduction evidence retained with an accepted transition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReproductionTransitionAudit {
    /// Independent controller output decoded as blank signal zero.
    pub signal_0: i16,
    /// Decision tick that emitted `signal_0`.
    pub decision_tick: u64,
    /// Configured maturity threshold.
    pub maturity_decisions: u64,
    /// Whether this decision met the maturity threshold.
    pub mature: bool,
    /// Durable reserve before any lifecycle command caused by this transition.
    pub reproductive_reserve: u16,
    /// Body fullness observed at this transition, when available.
    pub fullness: Option<u16>,
    /// Whether some configured body slot was unoccupied at this transition.
    pub slot_available: bool,
}

/// Rejected transport input, before any runtime mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BridgeError {
    /// JSON did not meet the protocol's bounded wire contract.
    InvalidPayload(String),
    /// Another protocol version was sent.
    Protocol {
        /// Version supplied by the endpoint.
        received: u16,
    },
    /// A request tried to address a different agent.
    Agent {
        /// Stable identity configured for this bridge.
        expected: u64,
        /// Identity supplied by the endpoint.
        received: u64,
    },
    /// No live individual has the supplied evolving identity.
    UnknownIndividual(u64),
    /// The individual exists but does not occupy the claimed body slot.
    BodySlot {
        /// Evolving individual identity supplied by the request.
        individual_id: u64,
        /// Slot currently occupied by the individual.
        expected: u32,
        /// Slot claimed by the request.
        received: u32,
    },
    /// The observation did not carry the expected sequence.
    Sequence(RuntimeError),
    /// A recorded response does not reproduce from the Milestone A runtime.
    Replay,
}

impl core::fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidPayload(error) => write!(formatter, "invalid bridge payload: {error}"),
            Self::Protocol { received } => write!(formatter, "unsupported protocol {received}"),
            Self::Agent { expected, received } => {
                write!(formatter, "expected agent {expected}, received {received}")
            }
            Self::UnknownIndividual(id) => write!(formatter, "unknown individual {id}"),
            Self::BodySlot {
                individual_id,
                expected,
                received,
            } => write!(
                formatter,
                "individual {individual_id} occupies body slot {expected}, received {received}"
            ),
            Self::Sequence(error) => error.fmt(formatter),
            Self::Replay => formatter.write_str("recorded bridge transition did not replay"),
        }
    }
}

impl std::error::Error for BridgeError {}

fn response_for(
    agent_id: u64,
    body_slot: u32,
    transition: &EnvironmentTransition,
    runtime: &AgentRuntime,
) -> BridgeResponse {
    BridgeResponse {
        protocol: PROTOCOL_VERSION,
        agent_id,
        body_slot,
        generated_sequence: transition.observation.sequence,
        expected_sequence: transition.observation.sequence + 1,
        action: transition.action,
        diagnostics: Diagnostics {
            decision_tick: transition.decision_tick,
            experiment_mode: runtime.state().experiment_mode,
            affordance_config: (runtime.state().experiment_mode
                == ExperimentMode::AffordanceDiscovery)
                .then_some(runtime.state().affordance.config),
            brain_fingerprint: transition.brain_fingerprint,
            baseline_action: if runtime.state().experiment_mode
                == ExperimentMode::PredictiveBaseline
            {
                transition.action
            } else {
                transition.baseline_action
            },
            intrinsic_action: transition.action,
            intrinsic_score: transition.intrinsic_score,
            affordance_fingerprint: transition.affordance_fingerprint,
            recurrent_fingerprint: runtime.state().recurrent_predictor.fingerprint(),
            episodic_fingerprint: runtime.state().episodic_memory.fingerprint(),
            episodic_events: runtime.state().episodic_memory.events.len(),
            episodic_config: runtime.state().episodic_memory.config,
            episodic_mediation: Some(EpisodicMediation {
                write: transition.episodic_write,
                retrieval: transition.episodic_policy,
                pre_episodic_action: transition.pre_episodic_action,
                selected_action: transition.action,
                baseline_policy_score: transition.intrinsic_score,
            }),
            learned: runtime
                .state()
                .brain
                .learned_bias
                .iter()
                .any(|value| *value != 0)
                || runtime
                    .state()
                    .recurrent_predictor
                    .decoder_delta
                    .iter()
                    .any(|value| *value != 0),
            mean_modulator_activity: mean_modulator_activity(runtime),
            frozen_physiology: runtime.state().physiology,
        },
    }
}

fn mean_modulator_activity(runtime: &AgentRuntime) -> u16 {
    let activity = &runtime.state().brain.activity;
    let total: i32 = activity[NEURONS - MODULATORS..]
        .iter()
        .map(|value| i32::from(value.abs()))
        .sum();
    u16::try_from(total / i32::try_from(MODULATORS).expect("small constant"))
        .expect("activity fits")
}

/// Replays accepted bridge records through the Milestone A standalone runtime.
///
/// # Errors
///
/// Returns an error if any recorded response cannot be reproduced exactly.
pub fn replay_trace(
    seed: u64,
    agent_id: u64,
    records: &[TraceRecord],
) -> Result<AgentRuntime, BridgeError> {
    if let Some(record) = records
        .iter()
        .find(|record| record.request.protocol != PROTOCOL_VERSION)
    {
        return Err(BridgeError::Protocol {
            received: record.request.protocol,
        });
    }
    let (mode, affordance, episodic) = records.first().map_or(
        (
            ExperimentMode::PredictiveBaseline,
            None,
            EpisodicConfig::default(),
        ),
        |record| {
            let diagnostics = &record.response.diagnostics;
            (
                diagnostics.experiment_mode,
                diagnostics.affordance_config,
                diagnostics.episodic_config,
            )
        },
    );
    if records.iter().any(|record| {
        record.response.diagnostics.experiment_mode != mode
            || record.response.diagnostics.affordance_config != affordance
            || record.response.diagnostics.episodic_config != episodic
    }) {
        return Err(BridgeError::Replay);
    }
    let founder = FounderSpec::new(seed, agent_id);
    let brain = founder.genome.default_brain_config();
    let mut runtime = match (mode, affordance) {
        (ExperimentMode::AffordanceDiscovery, Some(settings)) => {
            AgentRuntime::new_with_affordance(seed, agent_id, brain, settings)
                .map_err(|_| BridgeError::Replay)?
        }
        _ => AgentRuntime::new_with_mode(seed, agent_id, brain, mode),
    };
    runtime.set_episodic_config(episodic);
    for record in records {
        let transition = runtime
            .step(record.request.observation)
            .map_err(BridgeError::Sequence)?;
        let mut replayed = response_for(
            record.request.agent_id,
            record.response.body_slot,
            &transition,
            &runtime,
        );
        if record.response.diagnostics.episodic_mediation.is_none() {
            replayed.diagnostics.episodic_mediation = None;
        }
        if replayed != record.response {
            return Err(BridgeError::Replay);
        }
    }
    Ok(runtime)
}

/// Saves a founder's exact runtime state (everything it has learned) to `path`, replacing the
/// file only once the new state is completely written.
///
/// # Errors
///
/// Propagates serialization and file-system errors.
pub fn save_state(
    path: &std::path::Path,
    runtime: &AgentRuntime,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let partial = path.with_extension("partial");
    std::fs::write(&partial, serde_json::to_vec(runtime.state())?)?;
    std::fs::rename(&partial, path)?;
    Ok(())
}

/// Loads a founder saved by [`save_state`], or `None` if nothing has been saved at `path`.
///
/// # Errors
///
/// Returns an error for an unreadable file or a state that fails validation.
pub fn load_state(
    path: &std::path::Path,
    seed: u64,
    agent_id: u64,
) -> Result<Option<AgentRuntime>, Box<dyn std::error::Error + Send + Sync>> {
    if !path.exists() {
        return Ok(None);
    }
    let state = serde_json::from_slice(&std::fs::read(path)?)?;
    Ok(Some(AgentRuntime::from_state(seed, agent_id, state)))
}

/// Binds an HTTP server exclusively to `127.0.0.1`.
///
/// The returned server has no routing other than `POST /v1/transition`.  Call
/// [`serve_once`] in a loop controlled by the embedding process.
///
/// # Errors
///
/// Propagates an operating-system bind error.
pub fn bind_loopback(port: u16) -> Result<Server, Box<dyn std::error::Error + Send + Sync>> {
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    Server::http(address)
}

/// Handles at most one request, waiting no longer than `wait`.
///
/// This function never turns a missing request into an action. The Lua mod owns timeout behaviour
/// and applies an explicit zero action on disconnect or timeout.
///
/// # Errors
///
/// Returns I/O failures from the HTTP server.
pub fn serve_once(
    server: &Server,
    bridge: &mut Bridge,
    wait: Duration,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    serve_once_with_stall(server, bridge, wait, None, &mut false)
}

/// Handles one request, optionally injecting one pre-runtime stall for smoke testing.
///
/// A stalled request receives no runtime decision. The next Lua request therefore preserves its
/// sequence, reports `Timeout`, and can be accepted as the recovery boundary.
///
/// Failures confined to one request, such as a client that disconnects before its reply, are
/// logged and never end the serving loop.
///
/// # Errors
///
/// Returns failures of the HTTP server itself.
pub fn serve_once_with_stall(
    server: &Server,
    bridge: &mut Bridge,
    wait: Duration,
    stall_sequence: Option<(u64, Duration)>,
    stalled: &mut bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some(mut request) = server.recv_timeout(wait)? else {
        return Ok(());
    };
    if request.method() == &Method::Get && request.url() == "/v1/population" {
        let manifest = bridge.manifest();
        respond_or_log(request, StatusCode(200), &manifest);
        return Ok(());
    }
    if request.method() != &Method::Post
        || (request.url() != "/v1/transition" && request.url() != "/v1/lifecycle")
    {
        if let Err(error) = request.respond(Response::empty(StatusCode(404))) {
            eprintln!("sim-luanti: failed to respond to invalid route: {error}");
        }
        return Ok(());
    }
    let mut body = Vec::new();
    if let Err(error) = request
        .as_reader()
        .take((MAX_PAYLOAD_BYTES + 1) as u64)
        .read_to_end(&mut body)
    {
        eprintln!("sim-luanti: failed to read request: {error}");
        return Ok(());
    }
    if request.url() == "/v1/lifecycle" {
        let command: LifecycleCommand = match serde_json::from_slice(&body) {
            Ok(command) => command,
            Err(error) => {
                respond_or_log(
                    request,
                    StatusCode(400),
                    &serde_json::json!({"error": format!("invalid lifecycle command: {error}")}),
                );
                return Ok(());
            }
        };
        let result = bridge.process_lifecycle(command);
        match result {
            Ok(response) => respond_or_log(request, StatusCode(200), &response),
            Err(error) => respond_or_log(
                request,
                StatusCode(400),
                &serde_json::json!({"error": error.to_string()}),
            ),
        }
        return Ok(());
    }
    let wire = match decode_request(&body) {
        Ok(wire) => wire,
        Err(error) => {
            eprintln!("sim-luanti: rejected request: {error}");
            respond_or_log(
                request,
                StatusCode(400),
                &serde_json::json!({"error": error.to_string(), "expected_sequence": null}),
            );
            return Ok(());
        }
    };
    if let Some((sequence, delay)) = stall_sequence
        && !*stalled
        && wire.observation.sequence == sequence
    {
        *stalled = true;
        std::thread::sleep(delay);
        respond_or_log(
            request,
            StatusCode(503),
            &serde_json::json!({"error": "injected smoke stall"}),
        );
        return Ok(());
    }
    match bridge.handle(wire) {
        Ok(response) => respond_or_log(request, StatusCode(200), &response),
        Err(error) => {
            eprintln!("sim-luanti: rejected request: {error}");
            let expected_sequence = match &error {
                BridgeError::Sequence(RuntimeError::UnexpectedSequence { expected, .. }) => {
                    Some(*expected)
                }
                _ => None,
            };
            respond_or_log(
                request,
                StatusCode(400),
                &serde_json::json!({
                    "error": error.to_string(),
                    "expected_sequence": expected_sequence,
                }),
            );
        }
    }
    Ok(())
}

/// Accepts one request and handles its runtime work on an independent worker.
///
/// This is the population-serving path: a deliberately delayed request from one body does not
/// prevent another body from reaching the coordinator. Runtime mutation remains serialized by
/// the coordinator lock, while network delay and response delivery happen outside it.
///
/// # Errors
///
/// Returns failures of the HTTP server itself.
#[allow(clippy::too_many_lines)]
pub fn serve_once_concurrent(
    server: &Server,
    bridge: &Arc<Mutex<Bridge>>,
    wait: Duration,
    stall_sequence: Option<(u64, Duration)>,
    stalled: &Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some(mut request) = server.recv_timeout(wait)? else {
        return Ok(());
    };
    if request.method() == &Method::Get && request.url() == "/v1/population" {
        let manifest = bridge
            .lock()
            .map_err(|_| "population coordinator lock was poisoned")?
            .manifest();
        respond_or_log(request, StatusCode(200), &manifest);
        return Ok(());
    }
    if request.method() != &Method::Post
        || (request.url() != "/v1/transition" && request.url() != "/v1/lifecycle")
    {
        respond_or_log(
            request,
            StatusCode(404),
            &serde_json::json!({"error": "not found"}),
        );
        return Ok(());
    }
    let mut body = Vec::new();
    if let Err(error) = request
        .as_reader()
        .take((MAX_PAYLOAD_BYTES + 1) as u64)
        .read_to_end(&mut body)
    {
        eprintln!("sim-luanti: failed to read request: {error}");
        return Ok(());
    }
    if request.url() == "/v1/lifecycle" {
        let command: LifecycleCommand = match serde_json::from_slice(&body) {
            Ok(command) => command,
            Err(error) => {
                respond_or_log(
                    request,
                    StatusCode(400),
                    &serde_json::json!({"error": format!("invalid lifecycle command: {error}")}),
                );
                return Ok(());
            }
        };
        let result = bridge
            .lock()
            .map_err(|_| "population coordinator lock was poisoned")?
            .process_lifecycle(command);
        match result {
            Ok(response) => respond_or_log(request, StatusCode(200), &response),
            Err(error) => respond_or_log(
                request,
                StatusCode(400),
                &serde_json::json!({"error": error.to_string()}),
            ),
        }
        return Ok(());
    }
    let wire = match decode_request(&body) {
        Ok(wire) => wire,
        Err(error) => {
            eprintln!("sim-luanti: rejected request: {error}");
            respond_or_log(
                request,
                StatusCode(400),
                &serde_json::json!({"error": error.to_string(), "expected_sequence": null}),
            );
            return Ok(());
        }
    };
    let stall_delay = stall_sequence.and_then(|(sequence, delay)| {
        (wire.observation.sequence == sequence
            && stalled
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok())
        .then_some(delay)
    });
    let bridge = Arc::clone(bridge);
    std::thread::spawn(move || {
        if let Some(delay) = stall_delay {
            std::thread::sleep(delay);
            respond_or_log(
                request,
                StatusCode(503),
                &serde_json::json!({"error": "injected smoke stall"}),
            );
            return;
        }
        let result = match bridge.lock() {
            Ok(mut coordinator) => coordinator.handle(wire),
            Err(error) => {
                eprintln!("sim-luanti: population coordinator lock poisoned: {error}");
                respond_or_log(
                    request,
                    StatusCode(500),
                    &serde_json::json!({"error": "population coordinator unavailable"}),
                );
                return;
            }
        };
        match result {
            Ok(response) => respond_or_log(request, StatusCode(200), &response),
            Err(error) => {
                eprintln!("sim-luanti: rejected request: {error}");
                let expected_sequence = match &error {
                    BridgeError::Sequence(RuntimeError::UnexpectedSequence {
                        expected, ..
                    }) => Some(*expected),
                    _ => None,
                };
                respond_or_log(
                    request,
                    StatusCode(400),
                    &serde_json::json!({
                        "error": error.to_string(),
                        "expected_sequence": expected_sequence,
                    }),
                );
            }
        }
    });
    Ok(())
}

fn respond_or_log<T: Serialize>(request: tiny_http::Request, status: StatusCode, value: &T) {
    if let Err(error) = respond_json(request, status, value) {
        eprintln!("sim-luanti: failed to respond to request: {error}");
    }
}

fn decode_request(body: &[u8]) -> Result<BridgeRequest, BridgeError> {
    if body.len() > MAX_PAYLOAD_BYTES {
        return Err(BridgeError::InvalidPayload(
            "payload exceeds 64 KiB".to_owned(),
        ));
    }
    serde_json::from_slice(body).map_err(|error| BridgeError::InvalidPayload(error.to_string()))
}

fn respond_json<T: Serialize>(
    request: tiny_http::Request,
    status: StatusCode,
    value: &T,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let body = serde_json::to_vec(value)?;
    let response = Response::from_data(body)
        .with_status_code(status)
        .with_header(
            Header::from_bytes("Content-Type", "application/json").expect("static header is valid"),
        );
    request.respond(response)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpStream,
    };

    use sim_core::{
        RawSensoryFrame,
        environment::{
            BodyProprioception, EnvironmentCapabilities, HeldItemSense, TransportStatus,
        },
    };

    use super::*;

    fn request(sequence: u64) -> BridgeRequest {
        BridgeRequest {
            protocol: PROTOCOL_VERSION,
            agent_id: 9,
            body_slot: None,
            observation: EnvironmentObservation {
                sequence,
                senses: RawSensoryFrame::EMPTY,
                body: BodyProprioception::default(),
                capabilities: EnvironmentCapabilities::default(),
                transport: TransportStatus::Healthy,
                retina: None,
                interoception: None,
                held_item: None,
            },
            trace: ApplicationTrace {
                applied_server_step: 10,
                applied_steps: 1,
                accumulated_dtime_us: 100_000,
                applied_action: PrimitiveAction::default(),
                pose: AppliedPose {
                    x_millinode: 0,
                    y_millinode: 1_000,
                    z_millinode: 0,
                    yaw_milliradian: -1_571,
                },
            },
        }
    }

    fn felt(sequence: u64) -> BridgeRequest {
        let mut felt = request(sequence);
        felt.observation.interoception = Some(sim_core::environment::Interoception {
            health: 200,
            fullness: 120,
            air: 255,
            pain: u16::try_from(sequence % 2 * 40).expect("small"),
        });
        felt
    }

    #[test]
    fn felt_body_replaces_the_fixed_internal_stores() {
        let mut bridge = Bridge::new(4, 9);
        let response = bridge.handle(felt(0)).expect("accepted");
        assert_eq!(response.diagnostics.frozen_physiology.energy, 120);
        assert_eq!(response.diagnostics.frozen_physiology.damage, 55);
        let mut unfelt = Bridge::new(4, 9);
        let response = unfelt.handle(request(0)).expect("accepted");
        assert_eq!(
            response.diagnostics.frozen_physiology,
            FrozenPhysiology::default()
        );
    }

    #[test]
    fn independent_reproduction_output_traverses_http_json() {
        let server = bind_loopback(0).expect("bind ephemeral loopback port");
        let address = server.server_addr().to_ip().expect("IP listener");
        let mut damaged = felt(0);
        damaged.observation.interoception.as_mut().unwrap().health = 0;
        let body = serde_json::to_vec(&damaged).unwrap();
        let worker = std::thread::spawn(move || {
            let mut bridge = Bridge::new(4, 9);
            serve_once(&server, &mut bridge, Duration::from_secs(2)).unwrap();
        });

        let mut stream = TcpStream::connect(address).unwrap();
        write!(
            stream,
            "POST /v1/transition HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(&body).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        worker.join().unwrap();
        let payload = response.split("\r\n\r\n").nth(1).expect("HTTP body");
        let decoded: BridgeResponse = serde_json::from_str(payload).unwrap();
        assert!(decoded.action.signals[0] > 0);
        assert_eq!(
            serde_json::to_value(&decoded).unwrap()["action"]["signals"][0],
            decoded.action.signals[0]
        );
    }

    #[test]
    fn a_saved_founder_continues_exactly_where_it_stopped() {
        let path =
            std::env::temp_dir().join(format!("sim-luanti-state-{}.json", std::process::id()));
        let mut lived = Bridge::new_with_mode(4, 9, ExperimentMode::Homeostatic);
        for sequence in 0..3 {
            lived.handle(felt(sequence)).expect("accepted");
        }
        save_state(&path, lived.runtime()).expect("saved");
        let mut resumed =
            Bridge::from_runtime(load_state(&path, 4, 9).expect("loaded").expect("present"));
        std::fs::remove_file(&path).expect("removed");
        assert_eq!(resumed.runtime(), lived.runtime());
        // A restarted game begins counting again; it is told where the founder left off.
        assert!(matches!(
            resumed.handle(felt(0)),
            Err(BridgeError::Sequence(RuntimeError::UnexpectedSequence {
                expected: 3,
                ..
            }))
        ));
        for sequence in 3..6 {
            assert_eq!(
                resumed.handle(felt(sequence)).expect("resumed"),
                lived.handle(felt(sequence)).expect("lived")
            );
        }
        assert!(load_state(&path, 4, 9).expect("readable").is_none());
    }

    #[test]
    fn rejects_protocol_agent_and_sequence_without_advancing() {
        let mut bridge = Bridge::new(4, 9);
        let mut wrong_protocol = request(0);
        wrong_protocol.protocol = PROTOCOL_VERSION + 1;
        assert!(matches!(
            bridge.handle(wrong_protocol),
            Err(BridgeError::Protocol { .. })
        ));
        let mut wrong_agent = request(0);
        wrong_agent.agent_id = 10;
        assert!(matches!(
            bridge.handle(wrong_agent),
            Err(BridgeError::UnknownIndividual(10))
        ));
        assert_eq!(
            bridge.handle(request(0)).expect("first").generated_sequence,
            0
        );
        assert!(matches!(
            bridge.handle(request(0)),
            Err(BridgeError::Sequence(_))
        ));
        assert_eq!(
            bridge
                .handle(request(1))
                .expect("second")
                .generated_sequence,
            1
        );
    }

    #[test]
    fn trace_replays_through_founder_runtime_and_faults_echo_sequences() {
        let mut bridge = Bridge::new(4, 9);
        let first = bridge.handle(request(0)).expect("first");
        assert_eq!(first.expected_sequence, 1);
        let mut fault = request(1);
        fault.observation.transport = TransportStatus::Rejected;
        let response = bridge.handle(fault).expect("fault");
        assert_eq!(response.generated_sequence, 1);
        assert_eq!(response.expected_sequence, 2);
        let records = bridge.drain_trace();
        replay_trace(4, 9, &records).expect("replay");
    }

    #[test]
    fn replay_rejects_a_tampered_brain_fingerprint() {
        let mut bridge = Bridge::new(4, 9);
        bridge.handle(request(0)).expect("first");
        let mut records = bridge.drain_trace();
        records[0].response.diagnostics.brain_fingerprint ^= 1;
        assert_eq!(replay_trace(4, 9, &records), Err(BridgeError::Replay));
    }

    #[test]
    fn replay_rejects_a_trace_from_an_older_body_protocol() {
        let mut bridge = Bridge::new(4, 9);
        bridge.handle(request(0)).expect("first");
        let mut records = bridge.drain_trace();
        records[0].request.protocol = PROTOCOL_VERSION - 1;
        assert!(matches!(
            replay_trace(4, 9, &records),
            Err(BridgeError::Protocol { .. })
        ));
    }

    #[test]
    fn affordance_trace_replays_and_rejects_tampering() {
        let mut bridge = Bridge::new_with_mode(4, 9, ExperimentMode::AffordanceDiscovery);
        bridge.handle(request(0)).expect("first");
        bridge.handle(request(1)).expect("second");
        let records = bridge.drain_trace();
        replay_trace(4, 9, &records).expect("replay");
        let mut tampered = records;
        tampered[1].request.observation.body.hand_contact = 255;
        assert_eq!(replay_trace(4, 9, &tampered), Err(BridgeError::Replay));
    }

    #[test]
    fn custom_learner_settings_are_recorded_and_replayed() {
        let settings = AffordanceConfig {
            action_horizon: 16,
            ..AffordanceConfig::default()
        };
        let mut bridge = Bridge::new_with_affordance(4, 9, settings).expect("valid settings");
        for sequence in 0..6 {
            bridge.handle(request(sequence)).expect("in sequence");
        }
        let records = bridge.drain_trace();
        assert_eq!(
            records[0].response.diagnostics.affordance_config,
            Some(settings)
        );
        replay_trace(4, 9, &records).expect("replay with recorded settings");
        let mut tampered = records;
        tampered[3].response.diagnostics.affordance_config = Some(AffordanceConfig::default());
        assert_eq!(replay_trace(4, 9, &tampered), Err(BridgeError::Replay));
    }

    #[test]
    fn rejects_oversized_payload_before_json_decode() {
        assert!(matches!(
            decode_request(&vec![b' '; MAX_PAYLOAD_BYTES + 1]),
            Err(BridgeError::InvalidPayload(_))
        ));
    }

    #[test]
    fn held_item_physics_reaches_rust_without_semantic_fields() {
        let mut empty = request(0);
        empty.observation.capabilities.body_contract_version = 7;
        empty.observation.held_item = Some(HeldItemSense::default());
        let mut selected = request(0);
        selected.observation.capabilities.body_contract_version = 7;
        selected.observation.held_item = Some(HeldItemSense {
            occupied: 255,
            red: 61,
            green: 142,
            blue: 39,
            amount: 2,
            capacity: 64,
            durability: 255,
        });
        let empty = decode_request(&serde_json::to_vec(&empty).expect("encode")).expect("decode");
        let selected =
            decode_request(&serde_json::to_vec(&selected).expect("encode")).expect("decode");
        assert_ne!(empty.observation.held_item, selected.observation.held_item);
        let wire = serde_json::to_value(selected.observation.held_item.expect("held")).unwrap();
        assert_eq!(
            wire.as_object().expect("object").keys().collect::<Vec<_>>(),
            [
                "amount",
                "blue",
                "capacity",
                "durability",
                "green",
                "occupied",
                "red"
            ]
        );
        assert!(
            wire.as_object()
                .unwrap()
                .values()
                .all(serde_json::Value::is_number)
        );
        let mut invalid = serde_json::to_value(request(0)).unwrap();
        invalid["observation"]["held_item"] = serde_json::json!({
            "occupied": 256, "red": 0, "green": 0, "blue": 0,
            "amount": 0, "capacity": 0, "durability": 0
        });
        assert!(decode_request(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
}
