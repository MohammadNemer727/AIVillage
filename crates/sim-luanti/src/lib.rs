//! A deliberately small, loopback-only bridge for the `ai_bridge` Luanti mod.
//!
//! The bridge owns no Luanti state. Lua sends an already bounded observation after an action
//! interval; Rust validates it, advances [`sim_core::runtime::AgentRuntime`], and returns the
//! next primitive action.  It is intentionally one request at a time: async holding and timeout
//! safety remain in the Lua side, where Luanti's server steps are known.

#![forbid(unsafe_code)]

pub mod benchmark;

use std::{
    io::Read,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sim_core::{
    environment::{EnvironmentObservation, EnvironmentTransition, PrimitiveAction},
    runtime::{AgentRuntime, ExperimentMode, FrozenPhysiology, RuntimeError},
};
use sim_format::FounderSpec;
use sim_kernels::brain::{MODULATORS, NEURONS};
use tiny_http::{Header, Method, Response, Server, StatusCode};

/// Wire protocol accepted by this crate.
pub const PROTOCOL_VERSION: u16 = 3;
/// Requests larger than this cannot be receptor frames from the one-agent protocol.
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
    /// Whether any lifetime brain parameter has departed from its blank initial state.
    pub learned: bool,
    /// Mean absolute activity of the modulatory population.
    pub mean_modulator_activity: u16,
    /// Stores fixed by the runtime rather than supplied by Luanti.
    pub frozen_physiology: FrozenPhysiology,
}

/// A request from the one permitted Lua bridge endpoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BridgeRequest {
    /// Versioned contract discriminator.
    pub protocol: u16,
    /// Stable founder identity; exactly one ID is accepted by a bridge instance.
    pub agent_id: u64,
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
            Self::Sequence(error) => error.fmt(formatter),
            Self::Replay => formatter.write_str("recorded bridge transition did not replay"),
        }
    }
}

impl std::error::Error for BridgeError {}

/// Stateful protocol handler, separately testable from the HTTP server.
#[derive(Clone, Debug)]
pub struct Bridge {
    runtime: AgentRuntime,
    trace: Vec<TraceRecord>,
}

impl Bridge {
    /// Constructs a bridge for one deterministic founder.
    #[must_use]
    pub fn new(seed: u64, agent_id: u64) -> Self {
        Self::new_with_mode(seed, agent_id, ExperimentMode::PredictiveBaseline)
    }

    /// Constructs a bridge with an explicit baseline or affordance-discovery policy.
    #[must_use]
    pub fn new_with_mode(seed: u64, agent_id: u64, mode: ExperimentMode) -> Self {
        let founder = FounderSpec::new(seed, agent_id);
        Self {
            runtime: AgentRuntime::new_with_mode(
                seed,
                agent_id,
                founder.genome.default_brain_config(),
                mode,
            ),
            trace: Vec::new(),
        }
    }

    /// Validates and processes one complete Luanti observation interval.
    ///
    /// # Errors
    ///
    /// Returns an error without mutating the runtime when protocol, agent, or sequence mismatch.
    pub fn handle(&mut self, request: BridgeRequest) -> Result<BridgeResponse, BridgeError> {
        if request.protocol != PROTOCOL_VERSION {
            return Err(BridgeError::Protocol {
                received: request.protocol,
            });
        }
        if request.agent_id != self.runtime.agent_id() {
            return Err(BridgeError::Agent {
                expected: self.runtime.agent_id(),
                received: request.agent_id,
            });
        }
        let transition = self
            .runtime
            .step(request.observation)
            .map_err(BridgeError::Sequence)?;
        let response = response_for(request.agent_id, &transition, &self.runtime);
        self.trace.push(TraceRecord {
            request,
            response: response.clone(),
        });
        Ok(response)
    }

    /// Removes all accepted exchanges recorded since the previous drain.
    #[must_use]
    pub fn drain_trace(&mut self) -> Vec<TraceRecord> {
        core::mem::take(&mut self.trace)
    }
}

fn response_for(
    agent_id: u64,
    transition: &EnvironmentTransition,
    runtime: &AgentRuntime,
) -> BridgeResponse {
    BridgeResponse {
        protocol: PROTOCOL_VERSION,
        agent_id,
        generated_sequence: transition.observation.sequence,
        expected_sequence: transition.observation.sequence + 1,
        action: transition.action,
        diagnostics: Diagnostics {
            decision_tick: transition.decision_tick,
            experiment_mode: runtime.state().experiment_mode,
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
            learned: runtime
                .state()
                .brain
                .learned_bias
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
pub fn replay_trace(seed: u64, agent_id: u64, records: &[TraceRecord]) -> Result<(), BridgeError> {
    let mode = records
        .first()
        .map_or(ExperimentMode::PredictiveBaseline, |record| {
            record.response.diagnostics.experiment_mode
        });
    if records
        .iter()
        .any(|record| record.response.diagnostics.experiment_mode != mode)
    {
        return Err(BridgeError::Replay);
    }
    let founder = FounderSpec::new(seed, agent_id);
    let mut runtime =
        AgentRuntime::new_with_mode(seed, agent_id, founder.genome.default_brain_config(), mode);
    for record in records {
        let transition = runtime
            .step(record.request.observation)
            .map_err(BridgeError::Sequence)?;
        if response_for(record.request.agent_id, &transition, &runtime) != record.response {
            return Err(BridgeError::Replay);
        }
    }
    Ok(())
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
    if request.method() != &Method::Post || request.url() != "/v1/transition" {
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
    use sim_core::{
        RawSensoryFrame,
        environment::{BodyProprioception, EnvironmentCapabilities, TransportStatus},
    };

    use super::*;

    fn request(sequence: u64) -> BridgeRequest {
        BridgeRequest {
            protocol: PROTOCOL_VERSION,
            agent_id: 9,
            observation: EnvironmentObservation {
                sequence,
                senses: RawSensoryFrame::EMPTY,
                body: BodyProprioception::default(),
                capabilities: EnvironmentCapabilities::default(),
                transport: TransportStatus::Healthy,
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

    #[test]
    fn rejects_protocol_agent_and_sequence_without_advancing() {
        let mut bridge = Bridge::new(4, 9);
        let mut wrong_protocol = request(0);
        wrong_protocol.protocol = 4;
        assert!(matches!(
            bridge.handle(wrong_protocol),
            Err(BridgeError::Protocol { .. })
        ));
        let mut wrong_agent = request(0);
        wrong_agent.agent_id = 10;
        assert!(matches!(
            bridge.handle(wrong_agent),
            Err(BridgeError::Agent { .. })
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
    fn rejects_oversized_payload_before_json_decode() {
        assert!(matches!(
            decode_request(&vec![b' '; MAX_PAYLOAD_BYTES + 1]),
            Err(BridgeError::InvalidPayload(_))
        ));
    }
}
