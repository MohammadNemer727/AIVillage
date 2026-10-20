//! `World::step_with_brains` contract: identical histories for conforming backends, and no
//! state change when a backend fails.

use sim_core::{
    BrainBackend, BrainBackendError, BrainConfig, BrainStep, PredictiveBrain, RawSensoryFrame,
    World, WorldSeed, WorldSettings,
};

/// Reference backend that runs the CPU kernel directly, one agent at a time.
struct SerialCpuBrains;

impl BrainBackend for SerialCpuBrains {
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
}

struct FailingBrains;

impl BrainBackend for FailingBrains {
    fn step_brains(
        &mut self,
        _: &mut [PredictiveBrain],
        _: &[RawSensoryFrame],
        _: &[BrainConfig],
    ) -> Result<Vec<BrainStep>, BrainBackendError> {
        Err(BrainBackendError::new("device lost"))
    }
}

struct TruncatingBrains;

impl BrainBackend for TruncatingBrains {
    fn step_brains(
        &mut self,
        _: &mut [PredictiveBrain],
        _: &[RawSensoryFrame],
        _: &[BrainConfig],
    ) -> Result<Vec<BrainStep>, BrainBackendError> {
        Ok(Vec::new())
    }
}

fn v2_world() -> World {
    let settings = WorldSettings::new(24, 24, 64).expect("valid settings");
    World::with_settings_v2(WorldSeed::new(404), settings).expect("valid V2 world")
}

#[test]
fn conforming_backend_reproduces_builtin_history() {
    let mut builtin = v2_world();
    let mut backed = v2_world();
    for tick in 0..200 {
        let expected = builtin.step().state_hash();
        let actual = backed
            .step_with_brains(&mut SerialCpuBrains)
            .expect("CPU backend cannot fail")
            .state_hash();
        assert_eq!(actual, expected, "diverged at tick {tick}");
    }
}

#[test]
fn failing_backend_leaves_world_unchanged() {
    let mut world = v2_world();
    for _ in 0..10 {
        let _ = world.step();
    }
    let before = world.snapshot();
    let error = world
        .step_with_brains(&mut FailingBrains)
        .expect_err("backend failure is reported");
    assert!(error.to_string().contains("device lost"));
    assert_eq!(world.snapshot(), before);
    // The world continues exactly as if the failed tick never happened.
    let mut reference = v2_world();
    for _ in 0..11 {
        let _ = reference.step();
    }
    assert_eq!(world.step().state_hash(), reference.snapshot().state_hash());
}

#[test]
fn backend_returning_wrong_step_count_is_rejected() {
    let mut world = v2_world();
    let before = world.snapshot();
    let error = world
        .step_with_brains(&mut TruncatingBrains)
        .expect_err("short result is rejected");
    assert!(error.to_string().contains("steps for"));
    assert_eq!(world.snapshot(), before);
}
