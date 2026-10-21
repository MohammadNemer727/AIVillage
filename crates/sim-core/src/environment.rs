//! Transport-neutral one-agent environment contracts.
//!
//! These contracts deliberately expose bounded receptor amplitudes and primitive motors only.
//! They never carry semantic map facts, object identities, coordinates, recipes, labels, or reward.

use serde::{Deserialize, Serialize};

use crate::{CHEMICAL_RECEPTOR_COUNT, RawSensoryFrame, RetinalSample};

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
    /// Version of the bounded body contract implemented by this endpoint.
    #[serde(default, skip_serializing_if = "is_zero_u16")]
    pub body_contract_version: u16,
    /// Whether the endpoint can preserve a resumable sequence.
    pub checkpoint_resume: bool,
    /// Receptors the endpoint leaves unimplemented.
    pub dead: DeadChannels,
}

/// Nonsemantic body feedback added by body-contract version 3.
///
/// These amplitudes deliberately describe only the body outcome, never an object identity,
/// location, node, reward, or task label.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BodyProprioception {
    /// Vertical eye angle, centered at 128.
    pub eye_pitch: u16,
    /// Whether the body was grounded during this observation interval.
    pub grounded: u16,
    /// Signed vertical velocity.
    pub vertical_motion: i16,
    /// Whether the most recent generic hand pulse contacted a physical test object.
    pub hand_contact: u16,
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

/// Columns of the forward retina, 7.5 degrees apart, spanning about 180 degrees.
pub const RETINA_COLUMNS: usize = 24;
/// Rows of the forward retina, 20 degrees apart, spanning about 80 degrees.
pub const RETINA_ROWS: usize = 5;
/// Points of the forward retina.
pub const RETINA_POINTS: usize = RETINA_COLUMNS * RETINA_ROWS;

/// A forward-facing grid of eye rays (body contract version 4), turned and tilted with the
/// gaze like an eye's field of view. Point `row * RETINA_COLUMNS + column` looks
/// `(column - 11.5) * 7.5` degrees to the left of the gaze and `(row - 2) * 20` degrees above it,
/// so column 0 is the far right and row 0 the lowest. Each point carries the same bounded
/// receptor amplitudes as a legacy eye ray, and no identity or position.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "Vec<RetinaPoint>", into = "Vec<RetinaPoint>")]
pub struct Retina {
    /// Receptor amplitudes of every point.
    pub points: [RetinalSample; RETINA_POINTS],
}

/// Wire form of one retina point.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct RetinaPoint {
    red: u16,
    green: u16,
    blue: u16,
    brightness: u16,
    motion: i16,
    occlusion: u16,
}

impl TryFrom<Vec<RetinaPoint>> for Retina {
    type Error = String;

    fn try_from(points: Vec<RetinaPoint>) -> Result<Self, Self::Error> {
        if points.len() != RETINA_POINTS {
            return Err(format!(
                "retina has {} points, not {RETINA_POINTS}",
                points.len()
            ));
        }
        let mut retina = Self {
            points: [RetinalSample::default(); RETINA_POINTS],
        };
        for (sample, point) in retina.points.iter_mut().zip(points) {
            let channels = [
                point.red,
                point.green,
                point.blue,
                point.brightness,
                point.occlusion,
            ];
            if channels.iter().any(|value| *value > 255) || point.motion.unsigned_abs() > 255 {
                return Err("retina amplitude is out of the receptor range".to_owned());
            }
            *sample = RetinalSample {
                red: point.red,
                green: point.green,
                blue: point.blue,
                brightness: point.brightness,
                motion: point.motion,
                occlusion: point.occlusion,
            };
        }
        Ok(retina)
    }
}

impl From<Retina> for Vec<RetinaPoint> {
    fn from(retina: Retina) -> Self {
        retina
            .points
            .iter()
            .map(|sample| RetinaPoint {
                red: sample.red,
                green: sample.green,
                blue: sample.blue,
                brightness: sample.brightness,
                motion: sample.motion,
                occlusion: sample.occlusion,
            })
            .collect()
    }
}

/// A bounded sensory observation at one sequence boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnvironmentObservation {
    /// Monotonic environment sequence number.
    pub sequence: u64,
    /// External receptor amplitudes. Interoceptive fields are ignored: the runtime owns them.
    #[serde(with = "wire_frame")]
    pub senses: RawSensoryFrame,
    /// Versioned, bounded body feedback not represented in the legacy V2 sensory frame.
    #[serde(default, skip_serializing_if = "is_default_body")]
    pub body: BodyProprioception,
    /// Endpoint capabilities applicable to this observation.
    pub capabilities: EnvironmentCapabilities,
    /// Transport state for this boundary.
    pub transport: TransportStatus,
    /// Forward retina, from endpoints implementing body contract version 4. The legacy eye
    /// rays in `senses` remain for the founder's brain and older learners.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retina: Option<Retina>,
}

/// Primitive motor amplitudes emitted by the runtime.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrimitiveAction {
    /// Forward/backward motor command.
    pub thrust: i16,
    /// Angular motor command.
    pub turn: i16,
    /// Lateral movement command.
    #[serde(default, skip_serializing_if = "is_zero_i16")]
    pub strafe: i16,
    /// Vertical eye rotation command.
    #[serde(default, skip_serializing_if = "is_zero_i16")]
    pub look_pitch: i16,
    /// Bounded jump pulse.
    #[serde(default, skip_serializing_if = "is_zero_u16")]
    pub jump: u16,
    /// Jaw aperture command.
    pub jaw: u16,
    /// Generic hand pulse, limited by endpoints to a nearby physical test object.
    #[serde(default, skip_serializing_if = "is_zero_u16")]
    pub hand_pulse: u16,
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
    /// The unchanged predictive founder output before an experiment-mode override.
    #[serde(default, skip_serializing_if = "is_default_action")]
    pub baseline_action: PrimitiveAction,
    /// Intrinsic information score of the selected action, in experiment mode.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub intrinsic_score: u32,
    /// Fingerprint of the checkpointed affordance learner, or zero for the baseline.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub affordance_fingerprint: u64,
    /// Fingerprint of the brain state after this decision.
    pub brain_fingerprint: u64,
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip predicate signature"
)]
fn is_default_body(value: &BodyProprioception) -> bool {
    *value == BodyProprioception::default()
}
fn is_default_action(value: &PrimitiveAction) -> bool {
    *value == PrimitiveAction::default()
}
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip predicate signature"
)]
const fn is_zero_i16(value: &i16) -> bool {
    *value == 0
}
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip predicate signature"
)]
const fn is_zero_u16(value: &u16) -> bool {
    *value == 0
}
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip predicate signature"
)]
const fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip predicate signature"
)]
const fn is_zero_u64(value: &u64) -> bool {
    *value == 0
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
