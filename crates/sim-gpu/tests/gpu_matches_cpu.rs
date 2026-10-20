//! The GPU brain kernel must be bit-identical to the CPU kernel it is compiled from.
//!
//! Skips when no capable GPU is present; set `AI_VILLAGE_REQUIRE_GPU=1` to fail instead.

use sim_core::{World, WorldSeed, WorldSettings};
use sim_gpu::{GpuBrainStepper, GpuError};
use sim_kernels::{
    brain::{BrainConfig, PredictiveBrain},
    derive,
    gpu::{GpuBrain, GpuBrainConfig, GpuFrame},
    senses::{RawSensoryFrame, RetinalSample},
};

const RANDOM_AGENTS: u64 = 1_024;
const RANDOM_TICKS: u64 = 64;

fn stepper() -> Option<GpuBrainStepper> {
    match GpuBrainStepper::new() {
        Ok(stepper) => {
            eprintln!("running on GPU: {}", stepper.adapter_name());
            Some(stepper)
        }
        Err(error) if std::env::var_os("AI_VILLAGE_REQUIRE_GPU").is_none() => {
            eprintln!("skipping GPU test: {error}");
            None
        }
        Err(error) => panic!("GPU required but unavailable: {error}"),
    }
}

fn bounded(seed: u64, agent: u64, tag: u64, range: u64) -> u16 {
    u16::try_from(derive(seed, agent, tag, 0x4750_5554) % range).expect("range fits u16")
}

fn signed(seed: u64, agent: u64, tag: u64, magnitude: u16) -> i16 {
    i16::try_from(bounded(seed, agent, tag, u64::from(magnitude) * 2 + 1)).expect("fits i16")
        - i16::try_from(magnitude).expect("fits i16")
}

/// A receptor frame with every amplitude drawn from its full sensor range.
fn random_frame(tick: u64, agent: u64) -> RawSensoryFrame {
    let value = |tag| bounded(tick, agent, tag, 256);
    let mut frame = RawSensoryFrame::EMPTY;
    for (ray, sample) in (0_u64..).zip(frame.retinal.iter_mut()) {
        *sample = RetinalSample {
            red: value(ray * 8),
            green: value(ray * 8 + 1),
            blue: value(ray * 8 + 2),
            brightness: value(ray * 8 + 3),
            motion: signed(tick, agent, ray * 8 + 4, 255),
            occlusion: value(ray * 8 + 5) / 255 * 255,
        };
    }
    for (receptor, chemical) in (200_u64..).zip(frame.chemicals.iter_mut()) {
        *chemical = value(receptor);
    }
    frame.vibration = value(300);
    frame.touch = value(301) / 128 * 255;
    frame.joint_position = value(302);
    frame.muscle_tension = value(303);
    frame.acceleration = i16::try_from(value(304)).expect("fits i16");
    frame.orientation = value(305);
    frame.energy = value(306);
    frame.gut_pressure = value(307);
    frame.toxin_stress = value(308);
    frame.hydration = value(309);
    frame.fatigue = value(310);
    frame.damage = value(311);
    frame
}

fn random_config(agent: u64) -> BrainConfig {
    let flags = bounded(0, agent, 400, 16);
    BrainConfig {
        connection_stride: 1 + usize::from(bounded(0, agent, 401, 15)),
        plasticity_scale: 1 + i32::from(bounded(0, agent, 402, 16)),
        plasticity_enabled: flags & 1 != 0,
        episodic_memory_enabled: flags & 2 != 0,
        action_contingency_enabled: flags & 4 != 0,
        prediction_enabled: flags & 8 != 0,
    }
}

/// Steps `brains` on both backends for every tick's frames and asserts identical results.
fn assert_backends_agree(
    stepper: &GpuBrainStepper,
    configs: &[BrainConfig],
    frames_per_tick: &[Vec<RawSensoryFrame>],
) {
    let mut cpu = vec![PredictiveBrain::default(); configs.len()];
    let mut gpu: Vec<GpuBrain> = cpu.iter().map(GpuBrain::pack).collect();
    let packed_configs: Vec<_> = configs.iter().map(GpuBrainConfig::pack).collect();
    for (tick, frames) in frames_per_tick.iter().enumerate() {
        let packed_frames: Vec<_> = frames.iter().map(GpuFrame::pack).collect();
        let gpu_steps = stepper
            .step(&mut gpu, &packed_frames, &packed_configs)
            .expect("GPU dispatch succeeds");
        for (agent, (brain, frame)) in cpu.iter_mut().zip(frames).enumerate() {
            let expected = brain.step(&configs[agent], frame);
            assert_eq!(
                gpu_steps[agent].unpack(),
                expected,
                "step, tick {tick} agent {agent}"
            );
            assert_eq!(
                gpu[agent].unpack(),
                *brain,
                "brain state, tick {tick} agent {agent}"
            );
        }
    }
}

#[test]
fn gpu_brain_matches_cpu_on_random_receptor_frames() {
    let Some(stepper) = stepper() else { return };
    let configs: Vec<_> = (0..RANDOM_AGENTS).map(random_config).collect();
    let frames: Vec<Vec<_>> = (0..RANDOM_TICKS)
        .map(|tick| {
            (0..RANDOM_AGENTS)
                .map(|agent| random_frame(tick, agent))
                .collect()
        })
        .collect();
    assert_backends_agree(&stepper, &configs, &frames);
}

#[test]
fn gpu_brain_matches_cpu_on_simulated_world_frames() {
    let Some(stepper) = stepper() else { return };
    let settings = WorldSettings::new(32, 32, 96).expect("valid settings");
    let mut world = World::with_settings_v2(WorldSeed::new(77), settings).expect("valid V2 world");
    let mut frames_per_tick = Vec::new();
    for _ in 0..80 {
        let snapshot = world.step();
        // A fixed-width population keeps agent indices aligned across ticks.
        let frames: Vec<_> = snapshot
            .agents()
            .iter()
            .filter_map(|agent| world.raw_senses_of(agent.id))
            .chain(std::iter::repeat(RawSensoryFrame::EMPTY))
            .take(96)
            .collect();
        frames_per_tick.push(frames);
    }
    let configs: Vec<_> = (0..96).map(random_config).collect();
    assert_backends_agree(&stepper, &configs, &frames_per_tick);
}

#[test]
fn mismatched_inputs_are_rejected() {
    let Some(stepper) = stepper() else { return };
    let mut brains = vec![GpuBrain::pack(&PredictiveBrain::default()); 2];
    let frames = vec![GpuFrame::pack(&RawSensoryFrame::EMPTY); 1];
    let configs = vec![GpuBrainConfig::pack(&random_config(0)); 2];
    assert!(matches!(
        stepper.step(&mut brains, &frames, &configs),
        Err(GpuError::LengthMismatch)
    ));
}

#[test]
fn world_history_with_gpu_brains_matches_cpu() {
    let Some(mut stepper) = stepper() else { return };
    let settings = WorldSettings::new(48, 48, 300).expect("valid settings");
    let mut cpu = World::with_settings_v2(WorldSeed::new(2_026), settings).expect("valid V2 world");
    let mut gpu = World::with_settings_v2(WorldSeed::new(2_026), settings).expect("valid V2 world");
    for tick in 0..300 {
        let expected = cpu.step().state_hash();
        let actual = gpu
            .step_with_brains(&mut stepper)
            .expect("GPU tick succeeds")
            .state_hash();
        assert_eq!(actual, expected, "state hash diverged at tick {tick}");
    }
    assert!(
        gpu.agent_count() > 0,
        "population survived, so brains were exercised"
    );
}
