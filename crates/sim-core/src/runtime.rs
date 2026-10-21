//! Standalone deterministic one-agent runtime.

use core::fmt;
use std::error::Error;

use serde::{Deserialize, Serialize};
use sim_kernels::{brain::EPISODE_CAPACITY, derive, gpu::GpuBrain, heading};

use crate::{
    BrainConfig, CHEMICAL_RECEPTOR_COUNT, PredictiveBrain, RETINAL_RAY_COUNT, RawSensoryFrame,
    adapt_chemical_receptors, compose_raw_sensory_frame, decode_motor_outputs,
    environment::{
        DeadChannels, EnvironmentAdapter, EnvironmentCapabilities, EnvironmentObservation,
        EnvironmentTransition, PrimitiveAction, TransportStatus,
    },
    scale_v2_orientation_turn,
};

/// Frozen internal stores used by the external one-agent runtime.
///
/// The defaults sit far enough above the founder reflex thresholds that receptor noise cannot
/// trigger the low-energy, swallowing, or toxin reflexes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FrozenPhysiology {
    /// Energy store.
    pub energy: u16,
    /// Gut store.
    pub gut: u16,
    /// Hydration store.
    pub hydration: u16,
    /// Fatigue store.
    pub fatigue: u16,
    /// Toxin store.
    pub toxin: u16,
    /// Damage store.
    pub damage: u16,
}

impl Default for FrozenPhysiology {
    fn default() -> Self {
        Self {
            energy: 120,
            gut: 120,
            hydration: 120,
            fatigue: 0,
            toxin: 0,
            damage: 0,
        }
    }
}

impl FrozenPhysiology {
    /// Returns the stores in [`compose_raw_sensory_frame`] order.
    #[must_use]
    pub const fn stores(self) -> [u16; 6] {
        [
            self.energy,
            self.gut,
            self.toxin,
            self.hydration,
            self.fatigue,
            self.damage,
        ]
    }
}

/// Exact mutable runtime state. It serializes through a validated representation, so a decoded
/// state can never index outside the brain's fixed buffers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "StateRepr", into = "StateRepr")]
pub struct AgentRuntimeState {
    /// Mutable predictive brain.
    pub brain: PredictiveBrain,
    /// Derived developmental brain configuration.
    pub developmental_config: BrainConfig,
    /// Leaky chemical receptor adaptation.
    pub chemical_adaptation: [u16; CHEMICAL_RECEPTOR_COUNT],
    /// Frozen stores.
    pub physiology: FrozenPhysiology,
    /// Decisions taken so far; keys receptor noise.
    pub decision_tick: u64,
    /// Sequence number the next observation must carry.
    pub next_sequence: u64,
}

/// Rejected observation; the runtime state is unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeError {
    /// The observation is a duplicate, stale, or from the future.
    UnexpectedSequence {
        /// Sequence the runtime was waiting for.
        expected: u64,
        /// Sequence the observation carried.
        received: u64,
    },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedSequence { expected, received } => write!(
                formatter,
                "expected observation sequence {expected}, received {received}"
            ),
        }
    }
}

impl Error for RuntimeError {}

/// Single-agent execution state, intentionally independent of [`crate::World`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentRuntime {
    seed: u64,
    agent_id: u64,
    state: AgentRuntimeState,
}

impl AgentRuntime {
    /// Constructs a fresh runtime with a blank lifetime brain.
    #[must_use]
    pub fn new(seed: u64, agent_id: u64, developmental_config: BrainConfig) -> Self {
        Self::from_state(
            seed,
            agent_id,
            AgentRuntimeState {
                brain: PredictiveBrain::default(),
                developmental_config,
                chemical_adaptation: [0; CHEMICAL_RECEPTOR_COUNT],
                physiology: FrozenPhysiology::default(),
                decision_tick: 0,
                next_sequence: 0,
            },
        )
    }

    /// Restores an exact runtime state.
    #[must_use]
    pub const fn from_state(seed: u64, agent_id: u64, state: AgentRuntimeState) -> Self {
        Self {
            seed,
            agent_id,
            state,
        }
    }

    /// Returns the run seed that keys receptor noise.
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// Returns the stable agent ID that keys receptor noise.
    #[must_use]
    pub const fn agent_id(&self) -> u64 {
        self.agent_id
    }

    /// Returns the exact mutable state for checkpointing.
    #[must_use]
    pub const fn state(&self) -> &AgentRuntimeState {
        &self.state
    }

    /// Consumes the next observation and emits primitive motor commands.
    ///
    /// An observation reporting a transport fault is consumed but not sensed: the brain and
    /// decision tick stay unchanged, and the emitted action is zero so the body holds still.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::UnexpectedSequence`] for a duplicate, stale, or future
    /// observation, leaving the runtime unchanged.
    pub fn step(
        &mut self,
        observation: EnvironmentObservation,
    ) -> Result<EnvironmentTransition, RuntimeError> {
        if observation.sequence != self.state.next_sequence {
            return Err(RuntimeError::UnexpectedSequence {
                expected: self.state.next_sequence,
                received: observation.sequence,
            });
        }
        self.state.next_sequence = observation.sequence + 1;
        if observation.transport != TransportStatus::Healthy {
            return Ok(EnvironmentTransition {
                decision_tick: self.state.decision_tick,
                observation,
                action: PrimitiveAction::default(),
                brain_fingerprint: brain_fingerprint(&self.state.brain),
            });
        }
        let mut external = observation.senses;
        observation.capabilities.dead.silence(&mut external);
        let (chemicals, adaptation) =
            adapt_chemical_receptors(external.chemicals, self.state.chemical_adaptation);
        external.chemicals = chemicals;
        let frame = compose_raw_sensory_frame(
            external,
            self.state.physiology.stores(),
            self.seed,
            self.state.decision_tick,
            self.agent_id,
        );
        let step = self
            .state
            .brain
            .step(&self.state.developmental_config, &frame);
        let decision_tick = self.state.decision_tick;
        self.state.chemical_adaptation = adaptation;
        self.state.decision_tick += 1;
        Ok(EnvironmentTransition {
            decision_tick,
            observation,
            action: decode_motor_outputs(&step.outputs),
            brain_fingerprint: brain_fingerprint(&self.state.brain),
        })
    }
}

/// Returns a stable fingerprint of every field of a brain.
#[must_use]
pub fn brain_fingerprint(brain: &PredictiveBrain) -> u64 {
    brain_words(brain).iter().fold(0, |signature, word| {
        derive(signature, i64::from(*word).cast_unsigned(), 0, 0x4252_4149)
    })
}

fn brain_words(brain: &PredictiveBrain) -> Vec<i32> {
    bytemuck::cast_slice(&[GpuBrain::pack(brain)]).to_vec()
}

/// Serialized form of [`AgentRuntimeState`]. The brain travels as its lossless GPU packing.
#[derive(Deserialize, Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Mirrors the independent ablation switches of BrainConfig."
)]
struct StateRepr {
    brain: Vec<i32>,
    connection_stride: usize,
    plasticity_scale: i32,
    plasticity_enabled: bool,
    episodic_memory_enabled: bool,
    action_contingency_enabled: bool,
    prediction_enabled: bool,
    chemical_adaptation: [u16; CHEMICAL_RECEPTOR_COUNT],
    physiology: FrozenPhysiology,
    decision_tick: u64,
    next_sequence: u64,
}

impl From<AgentRuntimeState> for StateRepr {
    fn from(state: AgentRuntimeState) -> Self {
        let config = state.developmental_config;
        Self {
            brain: brain_words(&state.brain),
            connection_stride: config.connection_stride,
            plasticity_scale: config.plasticity_scale,
            plasticity_enabled: config.plasticity_enabled,
            episodic_memory_enabled: config.episodic_memory_enabled,
            action_contingency_enabled: config.action_contingency_enabled,
            prediction_enabled: config.prediction_enabled,
            chemical_adaptation: state.chemical_adaptation,
            physiology: state.physiology,
            decision_tick: state.decision_tick,
            next_sequence: state.next_sequence,
        }
    }
}

impl TryFrom<StateRepr> for AgentRuntimeState {
    type Error = String;

    fn try_from(repr: StateRepr) -> Result<Self, Self::Error> {
        let packed: GpuBrain = bytemuck::try_pod_read_unaligned(bytemuck::cast_slice(&repr.brain))
            .map_err(|_| format!("brain has {} words", repr.brain.len()))?;
        let brain = packed.unpack();
        if GpuBrain::pack(&brain) != packed || brain.next_episode >= EPISODE_CAPACITY {
            return Err("brain state is out of range".to_owned());
        }
        if repr.connection_stride == 0 || repr.plasticity_scale < 1 {
            return Err("developmental configuration is out of range".to_owned());
        }
        Ok(Self {
            brain,
            developmental_config: BrainConfig {
                connection_stride: repr.connection_stride,
                plasticity_scale: repr.plasticity_scale,
                plasticity_enabled: repr.plasticity_enabled,
                episodic_memory_enabled: repr.episodic_memory_enabled,
                action_contingency_enabled: repr.action_contingency_enabled,
                prediction_enabled: repr.prediction_enabled,
            },
            chemical_adaptation: repr.chemical_adaptation,
            physiology: repr.physiology,
            decision_tick: repr.decision_tick,
            next_sequence: repr.next_sequence,
        })
    }
}

/// Deterministic in-memory endpoint for fixtures. Its world is a fixed ring of 16 coloured
/// sectors around the body; turning moves the ring across the rays and thrust changes the
/// apparent motion, so actions have consequences the brain can learn from. Chemistry and
/// vibration are dead, as in the Luanti milestone.
#[derive(Clone, Debug)]
pub struct FakeEnvironment {
    seed: u64,
    sequence: u64,
    heading: u8,
    speed: i16,
    previous_speed: i16,
    fault: TransportStatus,
}

impl FakeEnvironment {
    /// Creates a deterministic fake endpoint.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            seed,
            sequence: 0,
            heading: 0,
            speed: 0,
            previous_speed: 0,
            fault: TransportStatus::Healthy,
        }
    }

    /// Makes the next observation report `status` instead of a healthy transport.
    pub const fn fail_next(&mut self, status: TransportStatus) {
        self.fault = status;
    }

    fn sector(&self, angle: u8, tag: u64) -> u16 {
        u16::try_from(derive(self.seed, 0, u64::from(angle >> 4), tag) % 256).unwrap_or_default()
    }
}

impl EnvironmentAdapter for FakeEnvironment {
    fn observe(&mut self) -> EnvironmentObservation {
        let mut senses = RawSensoryFrame::EMPTY;
        for (ray, sample) in senses.retinal.iter_mut().enumerate() {
            let offset = u8::try_from(ray * 256 / RETINAL_RAY_COUNT).unwrap_or_default();
            let angle = self.heading.wrapping_add(offset);
            sample.red = self.sector(angle, 0x5245_4400);
            sample.green = self.sector(angle, 0x4752_4E00);
            sample.blue = self.sector(angle, 0x424C_5500);
            sample.brightness = self.sector(angle, 0x4252_4900);
            sample.occlusion = self.sector(angle, 0x4F43_4300);
            sample.motion =
                i16::try_from(i32::from(self.speed) * heading(offset).0 / 256).unwrap_or_default();
        }
        senses.touch = if self.speed > 0 && senses.retinal[0].occlusion > 200 {
            255
        } else {
            0
        };
        senses.joint_position = u16::from(self.heading);
        senses.orientation = u16::from(self.heading);
        senses.muscle_tension = self.speed.unsigned_abs();
        senses.acceleration = (self.speed - self.previous_speed).clamp(-255, 255);
        let observation = EnvironmentObservation {
            sequence: self.sequence,
            senses,
            capabilities: EnvironmentCapabilities {
                checkpoint_resume: false,
                dead: DeadChannels {
                    chemicals: [true; CHEMICAL_RECEPTOR_COUNT],
                    vibration: true,
                    ..DeadChannels::default()
                },
            },
            transport: self.fault,
        };
        self.fault = TransportStatus::Healthy;
        self.sequence += 1;
        observation
    }

    fn act(&mut self, action: &PrimitiveAction) {
        self.heading = self
            .heading
            .wrapping_add_signed(scale_v2_orientation_turn(action.turn));
        self.previous_speed = self.speed;
        self.speed = (self.speed * 7 / 8 + action.thrust / 8).clamp(-255, 255);
    }
}

#[cfg(test)]
mod tests {
    use sim_kernels::gpu::GpuFrame;

    use super::{AgentRuntime, FakeEnvironment, FrozenPhysiology, RuntimeError};
    use crate::{
        BrainConfig, RawSensoryFrame, compose_raw_sensory_frame,
        environment::{
            EnvironmentAdapter, EnvironmentObservation, PrimitiveAction, TransportStatus,
        },
    };

    // Thresholds of the founder reflexes in `sim_kernels::brain::apply_reflexes`.
    const LOW_ENERGY_REFLEX_BELOW: u16 = 96;
    const SWALLOW_REFLEX_BELOW: u16 = 96;
    const TOXIN_TURN_REFLEX_FROM: u16 = 5;

    fn config() -> BrainConfig {
        BrainConfig {
            connection_stride: 1,
            plasticity_scale: 1,
            plasticity_enabled: true,
            episodic_memory_enabled: true,
            action_contingency_enabled: true,
            prediction_enabled: true,
        }
    }

    #[test]
    fn frozen_physiology_never_reaches_founder_reflexes() {
        let stores = FrozenPhysiology::default().stores();
        for seed in 0..32 {
            for agent_id in 0..8 {
                for tick in 0..256 {
                    let frame = compose_raw_sensory_frame(
                        RawSensoryFrame::EMPTY,
                        stores,
                        seed,
                        tick,
                        agent_id,
                    );
                    assert!(frame.energy >= LOW_ENERGY_REFLEX_BELOW);
                    assert!(frame.gut_pressure >= SWALLOW_REFLEX_BELOW);
                    assert!(frame.toxin_stress < TOXIN_TURN_REFLEX_FROM);
                }
            }
        }
    }

    #[test]
    fn receptor_noise_is_keyed_by_agent_and_decision_tick() {
        let stores = FrozenPhysiology::default().stores();
        let noise = |tick, agent_id| {
            let frame =
                compose_raw_sensory_frame(RawSensoryFrame::EMPTY, stores, 7, tick, agent_id);
            GpuFrame::pack(&frame).scalars
        };
        assert!((0..16).any(|agent| noise(3, 0) != noise(3, agent)));
        assert!((0..16).any(|tick| noise(0, 2) != noise(tick, 2)));
    }

    #[test]
    fn duplicate_stale_and_future_observations_are_rejected_without_change() {
        let mut fake = FakeEnvironment::new(9);
        let observations: Vec<EnvironmentObservation> = (0..3).map(|_| fake.observe()).collect();
        let mut runtime = AgentRuntime::new(71, 4, config());
        assert!(runtime.step(observations[0]).is_ok());
        assert!(runtime.step(observations[1]).is_ok());
        let before = runtime.clone();
        for (observation, received) in [(observations[1], 1), (observations[0], 0)] {
            assert_eq!(
                runtime.step(observation),
                Err(RuntimeError::UnexpectedSequence {
                    expected: 2,
                    received
                })
            );
        }
        let future = EnvironmentObservation {
            sequence: 5,
            ..observations[2]
        };
        assert!(runtime.step(future).is_err());
        assert_eq!(runtime, before);
        assert!(runtime.step(observations[2]).is_ok());
    }

    #[test]
    fn transport_faults_force_a_zero_action_without_sensing() {
        for fault in [
            TransportStatus::Timeout,
            TransportStatus::Rejected,
            TransportStatus::Disconnected,
        ] {
            let mut fake = FakeEnvironment::new(9);
            let mut runtime = AgentRuntime::new(71, 4, config());
            for _ in 0..6 {
                let transition = runtime.step(fake.observe()).expect("in sequence");
                fake.act(&transition.action);
            }
            let before = runtime.state().clone();
            fake.fail_next(fault);
            let faulted = runtime.step(fake.observe()).expect("in sequence");
            assert_eq!(faulted.observation.transport, fault);
            assert_eq!(faulted.action, PrimitiveAction::default());
            assert_eq!(faulted.decision_tick, before.decision_tick);
            assert_eq!(runtime.state().brain, before.brain);
            assert_eq!(runtime.state().decision_tick, before.decision_tick);
            assert_eq!(runtime.state().next_sequence, before.next_sequence + 1);
            assert!(runtime.step(faulted.observation).is_err(), "fault replayed");

            let recovered = runtime.step(fake.observe()).expect("in sequence");
            assert_eq!(recovered.observation.transport, TransportStatus::Healthy);
            assert_eq!(recovered.decision_tick, before.decision_tick);
            assert_ne!(runtime.state().brain, before.brain);
        }
    }

    #[test]
    fn dead_channels_are_silenced_before_sensing() {
        let mut fake = FakeEnvironment::new(9);
        let clean = fake.observe();
        let mut noisy = clean;
        noisy.senses.chemicals = [255; 4];
        noisy.senses.vibration = 255;
        let mut a = AgentRuntime::new(71, 4, config());
        let mut b = a.clone();
        let a = a.step(clean).map(|transition| transition.action);
        let b = b.step(noisy).map(|transition| transition.action);
        assert_eq!(a, b);
    }

    #[test]
    fn out_of_range_receptor_amplitudes_are_rejected_on_decode() {
        let observation = FakeEnvironment::new(9).observe();
        let mut json = serde_json::to_value(observation).expect("encode");
        assert_eq!(
            serde_json::from_value::<EnvironmentObservation>(json.clone()).expect("decode"),
            observation
        );
        json["senses"]["retinal"][3]["red"] = 256.into();
        assert!(serde_json::from_value::<EnvironmentObservation>(json.clone()).is_err());
        json["senses"]["retinal"][3]["red"] = 0.into();
        json["senses"]["acceleration"] = (-256).into();
        assert!(serde_json::from_value::<EnvironmentObservation>(json).is_err());
    }

    #[test]
    fn corrupt_runtime_state_is_rejected_on_decode() {
        let runtime = AgentRuntime::new(71, 4, config());
        let mut json = serde_json::to_value(runtime.state()).expect("encode");
        json["brain"].as_array_mut().expect("brain words").pop();
        assert!(serde_json::from_value::<super::AgentRuntimeState>(json).is_err());
    }
}
