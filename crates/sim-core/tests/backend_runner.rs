//! `BackendRunner` contract: every preference reproduces `World::step` exactly, `auto` never
//! needs a GPU, and an explicit `gpu` request never silently falls back to the CPU.

#![allow(
    clippy::unnecessary_literal_bound,
    clippy::unnecessary_wraps,
    reason = "Test doubles must match the `BrainBackend` and `OpenGpu` signatures exactly."
)]

use sim_core::{
    Backend, BackendPreference, BackendRunner, BackendStatus, BoxedBrainBackend, BrainBackend,
    BrainBackendError, BrainConfig, BrainStep, PredictiveBrain, RawSensoryFrame, World, WorldSeed,
    WorldSettings, gpu_not_built,
};

const TICKS: u32 = 120;

/// Stands in for a GPU: runs the CPU kernel serially, so it is exact but slower than rayon.
struct SerialBrains;

impl BrainBackend for SerialBrains {
    fn step_brains(
        &mut self,
        brains: &mut [PredictiveBrain],
        frames: &[RawSensoryFrame],
        configs: &[BrainConfig],
    ) -> Result<Vec<BrainStep>, BrainBackendError> {
        Ok(brains
            .iter_mut()
            .zip(frames.iter().zip(configs))
            .map(|(brain, (frame, config))| brain.step(config, frame))
            .collect())
    }

    fn device_name(&self) -> &str {
        "serial test device"
    }
}

/// A device that opens but loses every dispatch.
struct LostBrains;

impl BrainBackend for LostBrains {
    fn step_brains(
        &mut self,
        _: &mut [PredictiveBrain],
        _: &[RawSensoryFrame],
        _: &[BrainConfig],
    ) -> Result<Vec<BrainStep>, BrainBackendError> {
        Err(BrainBackendError::new("device lost"))
    }
}

fn open_serial() -> Result<BoxedBrainBackend, BrainBackendError> {
    Ok(Box::new(SerialBrains))
}

fn open_lost() -> Result<BoxedBrainBackend, BrainBackendError> {
    Ok(Box::new(LostBrains))
}

fn open_never() -> Result<BoxedBrainBackend, BrainBackendError> {
    panic!("the GPU must not be opened here")
}

fn v2_world() -> World {
    let settings = WorldSettings::new(24, 24, 64).expect("valid settings");
    World::with_settings_v2(WorldSeed::new(77), settings).expect("valid V2 world")
}

/// Steps a fresh V2 world on `runner` and asserts every tick matches `World::step`.
fn assert_matches_builtin(runner: &mut BackendRunner) {
    let mut expected = v2_world();
    let mut actual = v2_world();
    for tick in 1..=TICKS {
        let snapshot = runner.step(&mut actual).expect("tick succeeds");
        assert_eq!(
            snapshot.state_hash(),
            expected.step().state_hash(),
            "diverged at tick {tick}"
        );
    }
}

#[test]
fn cpu_preference_never_opens_the_gpu() {
    let mut runner = BackendRunner::new(BackendPreference::Cpu, open_never).expect("cpu opens");
    assert_eq!(
        runner.take_status_change(),
        Some(&BackendStatus::CpuRequested)
    );
    assert_matches_builtin(&mut runner);
    assert_eq!(runner.take_status_change(), None);
}

#[test]
fn explicit_gpu_fails_up_front_when_unavailable() {
    let error = BackendRunner::new(BackendPreference::Gpu, gpu_not_built)
        .expect_err("explicit gpu cannot fall back");
    assert!(error.to_string().contains("--features gpu"), "{error}");
}

#[test]
fn explicit_gpu_runs_every_tick_on_the_gpu_exactly() {
    let mut runner = BackendRunner::new(BackendPreference::Gpu, open_serial).expect("gpu opens");
    assert_eq!(
        runner.status(),
        &BackendStatus::GpuRequested {
            device: "serial test device".to_owned()
        }
    );
    assert_matches_builtin(&mut runner);
}

#[test]
fn explicit_gpu_reports_dispatch_failure_without_changing_the_world() {
    let mut runner = BackendRunner::new(BackendPreference::Gpu, open_lost).expect("gpu opens");
    let mut world = v2_world();
    let before = world.snapshot();
    let error = runner
        .step(&mut world)
        .expect_err("explicit gpu cannot fall back");
    assert!(error.to_string().contains("device lost"), "{error}");
    assert_eq!(world.snapshot(), before);
}

#[test]
fn auto_waits_for_a_v2_world_before_opening_the_gpu() {
    let mut runner = BackendRunner::new(BackendPreference::Auto, open_never).expect("auto opens");
    assert_eq!(runner.status(), &BackendStatus::AutoPending);
    assert_eq!(runner.take_status_change(), None);
    let v1_world =
        || World::with_settings(WorldSeed::new(77), WorldSettings::default()).expect("valid V1");
    let (mut v1, mut reference) = (v1_world(), v1_world());
    for _ in 0..TICKS {
        let snapshot = runner.step(&mut v1).expect("V1 never fails");
        assert_eq!(snapshot.state_hash(), reference.step().state_hash());
    }
    assert_eq!(runner.status(), &BackendStatus::AutoPending);
}

#[test]
fn auto_without_a_gpu_runs_on_the_cpu_and_says_why() {
    let mut runner =
        BackendRunner::new(BackendPreference::Auto, gpu_not_built).expect("auto opens");
    assert_matches_builtin(&mut runner);
    let status = runner.take_status_change().expect("decision is reported");
    assert!(
        matches!(status, BackendStatus::AutoGpuUnavailable { error } if error.contains("--features gpu")),
        "{status}"
    );
    assert_eq!(runner.take_status_change(), None, "reported only once");
}

#[test]
fn auto_measures_both_backends_then_settles_without_changing_history() {
    let mut runner = BackendRunner::new(BackendPreference::Auto, open_serial).expect("auto opens");
    let mut world = v2_world();
    let _ = runner.step(&mut world).expect("auto never fails");
    assert_eq!(
        runner.take_status_change(),
        Some(&BackendStatus::AutoMeasuring {
            device: "serial test device".to_owned()
        })
    );
    assert_eq!(runner.status().backend(), None);

    assert_matches_builtin(&mut runner);
    let status = runner.status();
    assert!(
        matches!(status, BackendStatus::AutoMeasured { device, .. } if device == "serial test device"),
        "{status}"
    );
    assert!(status.backend().is_some());
}

#[test]
fn auto_redoes_a_failed_gpu_tick_on_the_cpu_and_stays_there() {
    let mut runner = BackendRunner::new(BackendPreference::Auto, open_lost).expect("auto opens");
    assert_matches_builtin(&mut runner);
    assert_eq!(
        runner.status(),
        &BackendStatus::AutoGpuFailed {
            device: "external brain backend".to_owned(),
            error: "brain backend failed: device lost".to_owned(),
        }
    );
    assert_eq!(runner.status().backend(), Some(Backend::Cpu));
}
