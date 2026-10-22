//! One-agent runtime golden trace: a seeded founder driven by the deterministic fake adapter.
//!
//! Fixture source of truth: `tests/fixtures/agent_runtime_golden.ndjson`, one
//! [`EnvironmentTransition`] per line. Regenerate after an intentional behaviour change with
//! `UPDATE_AGENT_GOLDEN=1 cargo test -p sim-format --test agent_runtime_golden`.
//! The 2026-10-22 refresh records controller outputs 8–14, which had previously been unwritten
//! zeros. No checkpoint schema changed: the new values are readouts of existing, already
//! checkpointed neurons, so brain state and GPU layouts retain their exact interpretation.

#![allow(missing_docs, reason = "Integration test crate.")]

use std::{collections::HashSet, fs, path::PathBuf};

use sim_core::{
    environment::{EnvironmentAdapter, EnvironmentTransition, PrimitiveAction, TransportStatus},
    runtime::FakeEnvironment,
};
use sim_format::{AgentCheckpoint, FounderSpec};

const RUN_SEED: u64 = 32;
const AGENT_ID: u64 = 0;
const ENVIRONMENT_SEED: u64 = 9;
const STEPS: usize = 48;
/// Transport faults injected into the live run, on both sides of the checkpoint midpoint.
const FAULTS: [(usize, TransportStatus); 3] = [
    (12, TransportStatus::Timeout),
    (30, TransportStatus::Rejected),
    (31, TransportStatus::Disconnected),
];

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/agent_runtime_golden.ndjson")
}

fn live_run() -> Vec<EnvironmentTransition> {
    let mut runtime = FounderSpec::new(RUN_SEED, AGENT_ID).runtime();
    let mut environment = FakeEnvironment::new(ENVIRONMENT_SEED);
    (0..STEPS)
        .map(|step| {
            if let Some((_, fault)) = FAULTS.iter().find(|(at, _)| *at == step) {
                environment.fail_next(*fault);
            }
            let transition = runtime.step(environment.observe()).unwrap();
            environment.act(&transition.action);
            transition
        })
        .collect()
}

fn to_ndjson(transitions: &[EnvironmentTransition]) -> String {
    transitions
        .iter()
        .map(|transition| serde_json::to_string(transition).unwrap() + "\n")
        .collect()
}

fn recorded() -> Vec<EnvironmentTransition> {
    let raw = fs::read_to_string(fixture_path()).unwrap();
    raw.lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn live_run_matches_golden_trace() {
    let live = to_ndjson(&live_run());
    if std::env::var_os("UPDATE_AGENT_GOLDEN").is_some() {
        fs::write(fixture_path(), &live).unwrap();
    }
    assert_eq!(fs::read_to_string(fixture_path()).unwrap(), live);
}

#[test]
fn golden_trace_exercises_varied_behaviour() {
    let trace = recorded();
    assert_eq!(trace.len(), STEPS);
    let actions: HashSet<_> = trace
        .iter()
        .map(|transition| (transition.action.thrust, transition.action.turn))
        .collect();
    let brains: HashSet<_> = trace
        .iter()
        .map(|transition| transition.brain_fingerprint)
        .collect();
    assert!(actions.len() > 1, "the founder never varied its movement");
    assert!(brains.len() > STEPS / 2, "the brain state barely changed");
    for (at, fault) in FAULTS {
        let faulted = &trace[at];
        assert_eq!(faulted.observation.transport, fault);
        assert_eq!(faulted.action, PrimitiveAction::default());
        assert_eq!(faulted.brain_fingerprint, trace[at - 1].brain_fingerprint);
    }
    let healthy = trace
        .iter()
        .filter(|transition| transition.observation.transport == TransportStatus::Healthy)
        .count();
    assert_eq!(healthy, STEPS - FAULTS.len());
}

#[test]
fn replaying_the_recorded_trace_reproduces_every_decision() {
    let mut runtime = FounderSpec::new(RUN_SEED, AGENT_ID).runtime();
    for expected in recorded() {
        assert_eq!(runtime.step(expected.observation).unwrap(), expected);
    }
}

#[test]
fn resuming_from_a_toml_checkpoint_reproduces_the_rest_of_the_trace() {
    let trace = recorded();
    let (before, after) = trace.split_at(STEPS / 2);
    let mut runtime = FounderSpec::new(RUN_SEED, AGENT_ID).runtime();
    for expected in before {
        assert_eq!(runtime.step(expected.observation).unwrap(), *expected);
    }
    let encoded = AgentCheckpoint::current(&runtime).to_toml().unwrap();
    let mut resumed = AgentCheckpoint::from_toml(&encoded)
        .unwrap()
        .restore()
        .unwrap();
    assert_eq!(resumed, runtime);
    for expected in after {
        assert_eq!(resumed.step(expected.observation).unwrap(), *expected);
    }
}
