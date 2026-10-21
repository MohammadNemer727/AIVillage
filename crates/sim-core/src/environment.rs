//! Transport-neutral one-agent environment contracts.
//!
//! These contracts deliberately expose bounded receptor amplitudes and primitive motors only.
//! They never carry semantic map facts, object identities, coordinates, recipes, labels, or reward.

use serde::{Deserialize, Serialize};

use crate::{CHEMICAL_RECEPTOR_COUNT, RawSensoryFrame};

/// Number of blank signal outputs recorded alongside the primitive motors.
pub use sim_kernels::controller::SIGNAL_CHANNELS;

/// External receptors an endpoint does not implement. Dead receptors are zeroed by the runtime
/// before sensing, so a stub can never leak signal. Retinal rays and the Rust-owned
/// interoceptive channels are never dead.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Each flag marks one independent receptor channel."
)]
pub struct DeadChannels {
    /// Chemical receptor lobes.
    pub chemicals: [bool; CHEMICAL_RECEPTOR_COUNT],
    /// Acoustic vibration.
    pub vibration: bool,
    /// Contact.
    pub touch: bool,
    /// Joint-angle proprioception.
    pub joint_position: bool,
    /// Muscle-tension proprioception.
    pub muscle_tension: bool,
    /// Body acceleration.
    pub acceleration: bool,
    /// Body orientation.
    pub orientation: bool,
}

impl DeadChannels {
    /// Zeroes every dead receptor in `frame`.
    pub fn silence(self, frame: &mut RawSensoryFrame) {
        for (receptor, dead) in self.chemicals.into_iter().enumerate() {
            if dead {
                frame.chemicals[receptor] = 0;
            }
        }
        if self.vibration {
            frame.vibration = 0;
        }
        if self.touch {
            frame.touch = 0;
        }
        if self.joint_position {
            frame.joint_position = 0;
        }
        if self.muscle_tension {
            frame.muscle_tension = 0;
        }
        if self.acceleration {
            frame.acceleration = 0;
        }
        if self.orientation {
            frame.orientation = 0;
        }
    }
}

/// Capabilities negotiated with an environment endpoint.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnvironmentCapabilities {
    /// Whether the endpoint can preserve a resumable sequence.
    pub checkpoint_resume: bool,
    /// Receptors the endpoint leaves unimplemented.
    pub dead: DeadChannels,
}

/// Current transport health, separate from sensory data.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum TransportStatus {
    /// No transport fault was observed.
    #[default]
    Healthy,
    /// The most recent request timed out.
    Timeout,
    /// The endpoint rejected the request.
    Rejected,
    /// The endpoint disconnected.
    Disconnected,
}

/// A bounded sensory observation at one sequence boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnvironmentObservation {
    /// Monotonic environment sequence number.
    pub sequence: u64,
    /// External receptor amplitudes. Interoceptive fields are ignored: the runtime owns them.
    #[serde(with = "wire_frame")]
    pub senses: RawSensoryFrame,
    /// Endpoint capabilities applicable to this observation.
    pub capabilities: EnvironmentCapabilities,
    /// Transport state for this boundary.
    pub transport: TransportStatus,
}

/// Primitive motor amplitudes emitted by the runtime.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrimitiveAction {
    /// Forward/backward motor command.
    pub thrust: i16,
    /// Angular motor command.
    pub turn: i16,
    /// Jaw aperture command.
    pub jaw: u16,
    /// Acoustic motor amplitude.
    pub acoustic: u16,
    /// Chemical gland amplitude.
    pub chemical: u16,
    /// Visual display amplitude.
    pub visual: u16,
    /// Blank signal outputs, recorded only; nothing receives them in the one-agent milestone.
    pub signals: [i16; SIGNAL_CHANNELS],
}

/// One recorded observation-to-action decision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnvironmentTransition {
    /// Runtime decision counter; it advances once per accepted healthy observation.
    /// Accepted non-healthy transport observations retain the current counter.
    pub decision_tick: u64,
    /// Observation consumed by the runtime, exactly as received.
    pub observation: EnvironmentObservation,
    /// Action selected from that observation.
    pub action: PrimitiveAction,
    /// Fingerprint of the brain state after this decision.
    pub brain_fingerprint: u64,
}

/// An endpoint that senses for, and moves, one agent body.
pub trait EnvironmentAdapter {
    /// Returns the next observation.
    fn observe(&mut self) -> EnvironmentObservation;

    /// Applies the action chosen for the most recent observation.
    fn act(&mut self, action: &PrimitiveAction);
}

/// Serde representation of [`RawSensoryFrame`] that rejects amplitudes outside the receptor
/// range: `0..=255`, or `-255..=255` for the signed motion and acceleration channels.
mod wire_frame {
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

    use crate::{CHEMICAL_RECEPTOR_COUNT, RETINAL_RAY_COUNT, RawSensoryFrame, RetinalSample};

    const MAX: u16 = 255;

    #[derive(Deserialize, Serialize)]
    struct Ray {
        red: u16,
        green: u16,
        blue: u16,
        brightness: u16,
        motion: i16,
        occlusion: u16,
    }

    #[derive(Deserialize, Serialize)]
    struct Frame {
        retinal: [Ray; RETINAL_RAY_COUNT],
        chemicals: [u16; CHEMICAL_RECEPTOR_COUNT],
        vibration: u16,
        touch: u16,
        joint_position: u16,
        muscle_tension: u16,
        acceleration: i16,
        orientation: u16,
        energy: u16,
        gut_pressure: u16,
        toxin_stress: u16,
        hydration: u16,
        fatigue: u16,
        damage: u16,
    }

    pub fn serialize<S: Serializer>(
        frame: &RawSensoryFrame,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Frame {
            retinal: frame.retinal.map(|ray| Ray {
                red: ray.red,
                green: ray.green,
                blue: ray.blue,
                brightness: ray.brightness,
                motion: ray.motion,
                occlusion: ray.occlusion,
            }),
            chemicals: frame.chemicals,
            vibration: frame.vibration,
            touch: frame.touch,
            joint_position: frame.joint_position,
            muscle_tension: frame.muscle_tension,
            acceleration: frame.acceleration,
            orientation: frame.orientation,
            energy: frame.energy,
            gut_pressure: frame.gut_pressure,
            toxin_stress: frame.toxin_stress,
            hydration: frame.hydration,
            fatigue: frame.fatigue,
            damage: frame.damage,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<RawSensoryFrame, D::Error> {
        let wire = Frame::deserialize(deserializer)?;
        let mut amplitudes = wire
            .retinal
            .iter()
            .flat_map(|ray| [ray.red, ray.green, ray.blue, ray.brightness, ray.occlusion])
            .chain(wire.chemicals)
            .chain([
                wire.vibration,
                wire.touch,
                wire.joint_position,
                wire.muscle_tension,
                wire.orientation,
                wire.energy,
                wire.gut_pressure,
                wire.toxin_stress,
                wire.hydration,
                wire.fatigue,
                wire.damage,
            ]);
        let mut signed = wire
            .retinal
            .iter()
            .map(|ray| ray.motion)
            .chain([wire.acceleration]);
        if amplitudes.any(|value| value > MAX) || signed.any(|value| value.unsigned_abs() > MAX) {
            return Err(D::Error::custom("receptor amplitude out of range"));
        }
        Ok(RawSensoryFrame {
            retinal: wire.retinal.map(|ray| RetinalSample {
                red: ray.red,
                green: ray.green,
                blue: ray.blue,
                brightness: ray.brightness,
                motion: ray.motion,
                occlusion: ray.occlusion,
            }),
            chemicals: wire.chemicals,
            vibration: wire.vibration,
            touch: wire.touch,
            joint_position: wire.joint_position,
            muscle_tension: wire.muscle_tension,
            acceleration: wire.acceleration,
            orientation: wire.orientation,
            energy: wire.energy,
            gut_pressure: wire.gut_pressure,
            toxin_stress: wire.toxin_stress,
            hydration: wire.hydration,
            fatigue: wire.fatigue,
            damage: wire.damage,
        })
    }
}
