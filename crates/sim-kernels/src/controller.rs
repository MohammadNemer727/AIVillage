//! Fixed-topology quantized recurrent controller used by V1 agents and the V2 projection.

use crate::{
    math::{amplitude, clamp_i16, saturating_add_i16},
    senses::{CHEMICAL_RECEPTOR_COUNT, RETINAL_RAY_COUNT, RawSensoryFrame},
};

/// Width of the blank, ungrounded signal alphabet carried on controller inputs and outputs.
pub const SIGNAL_CHANNELS: usize = 7;
/// Controller input width: eight local sensing channels plus one per signal symbol.
pub const INPUTS: usize = 8 + SIGNAL_CHANNELS;
/// Recurrent hidden-layer width.
pub const HIDDEN: usize = 8;
/// Controller output width: eight motor/action channels plus one per signal symbol.
pub const OUTPUTS: usize = 8 + SIGNAL_CHANNELS;
/// Inherited weight count: input→hidden, hidden→hidden, then hidden→output, row-major per node.
pub const GENOME_WEIGHTS: usize = INPUTS * HIDDEN + HIDDEN * HIDDEN + HIDDEN * OUTPUTS;

const RECURRENT_OFFSET: usize = INPUTS * HIDDEN;
const OUTPUT_OFFSET: usize = RECURRENT_OFFSET + HIDDEN * HIDDEN;

/// Evaluates one controller step and replaces `recurrent` with the new hidden state.
///
/// `biases` and `plastic_biases` hold hidden-node entries first, then output-node entries.
#[must_use]
pub fn evaluate_controller(
    weights: &[i16; GENOME_WEIGHTS],
    biases: &[i16; HIDDEN + OUTPUTS],
    plastic_biases: &[i16; HIDDEN + OUTPUTS],
    recurrent: &mut [i16; HIDDEN],
    inputs: &[i16; INPUTS],
) -> [i16; OUTPUTS] {
    let previous = *recurrent;
    let mut hidden = [0_i16; HIDDEN];
    for node in 0..HIDDEN {
        let mut sum = i32::from(biases[node]) + i32::from(plastic_biases[node]);
        for input in 0..INPUTS {
            sum += i32::from(inputs[input]) * i32::from(weights[node * INPUTS + input]) / 128;
        }
        let recurrent_offset = RECURRENT_OFFSET + node * HIDDEN;
        for source in 0..HIDDEN {
            sum +=
                i32::from(previous[source]) * i32::from(weights[recurrent_offset + source]) / 128;
        }
        hidden[node] = clamp_i16(sum, -512, 512);
    }
    *recurrent = hidden;
    let mut outputs = [0_i16; OUTPUTS];
    for node in 0..OUTPUTS {
        let mut sum = i32::from(biases[HIDDEN + node]) + i32::from(plastic_biases[HIDDEN + node]);
        for source in 0..HIDDEN {
            sum += i32::from(hidden[source])
                * i32::from(weights[OUTPUT_OFFSET + node * HIDDEN + source])
                / 128;
        }
        outputs[node] = clamp_i16(sum, -512, 512);
    }
    outputs
}

/// Nudges each lifetime bias one step toward the sign of its strongly active output.
///
/// Plastic biases are bounded to `±32` and are never inherited.
pub fn apply_lifetime_plasticity(
    plastic_biases: &mut [i16; HIDDEN + OUTPUTS],
    outputs: &[i16; OUTPUTS],
) {
    for node in 0..HIDDEN + OUTPUTS {
        let delta = (outputs[node % OUTPUTS] / 128).clamp(-1, 1);
        plastic_biases[node] = saturating_add_i16(plastic_biases[node], delta).clamp(-32, 32);
    }
}

/// Saturating fan-in of receptor amplitudes onto controller inputs, cycling through channels.
struct Projection {
    inputs: [i16; INPUTS],
    channel: usize,
}

impl Projection {
    fn add(&mut self, value: i16) {
        let channel = self.channel % INPUTS;
        self.inputs[channel] = saturating_add_i16(self.inputs[channel], value);
        self.channel += 1;
    }
}

/// Projects all receptor amplitudes into the controller's fixed input width.
///
/// This is deliberately lossy but contains no semantic field or world coordinate.
#[must_use]
pub fn raw_controller_projection(frame: &RawSensoryFrame) -> [i16; INPUTS] {
    let mut projection = Projection {
        inputs: [0; INPUTS],
        channel: 0,
    };
    for ray in 0..RETINAL_RAY_COUNT {
        let sample = &frame.retinal[ray];
        projection.add(amplitude(sample.red) / 4);
        projection.add(amplitude(sample.green) / 4);
        projection.add(amplitude(sample.blue) / 4);
        projection.add(amplitude(sample.brightness) / 4);
        projection.add(amplitude(sample.occlusion) / 4);
        projection.add(sample.motion / 4);
    }
    for receptor in 0..CHEMICAL_RECEPTOR_COUNT {
        projection.add(amplitude(frame.chemicals[receptor]));
    }
    projection.add(amplitude(frame.vibration));
    projection.add(amplitude(frame.touch));
    projection.add(amplitude(frame.joint_position));
    projection.add(amplitude(frame.muscle_tension));
    projection.add(amplitude(frame.orientation));
    projection.add(amplitude(frame.energy));
    projection.add(amplitude(frame.gut_pressure));
    projection.add(amplitude(frame.toxin_stress));
    projection.add(amplitude(frame.hydration));
    projection.add(amplitude(frame.fatigue));
    projection.add(amplitude(frame.damage));
    projection.add(frame.acceleration);
    projection.inputs
}

#[cfg(test)]
mod tests {
    use super::{
        GENOME_WEIGHTS, HIDDEN, INPUTS, OUTPUTS, apply_lifetime_plasticity, evaluate_controller,
        raw_controller_projection,
    };
    use crate::senses::RawSensoryFrame;

    #[test]
    fn zero_weights_pass_biases_through_and_clamp() {
        let weights = [0; GENOME_WEIGHTS];
        let mut biases = [0; HIDDEN + OUTPUTS];
        biases[HIDDEN] = 900;
        biases[HIDDEN + 1] = -7;
        let mut recurrent = [5; HIDDEN];
        let outputs = evaluate_controller(
            &weights,
            &biases,
            &[0; HIDDEN + OUTPUTS],
            &mut recurrent,
            &[100; INPUTS],
        );
        assert_eq!(outputs[0], 512);
        assert_eq!(outputs[1], -7);
        assert_eq!(recurrent, [0; HIDDEN]);
    }

    #[test]
    fn plasticity_is_bounded() {
        let mut plastic = [0; HIDDEN + OUTPUTS];
        let mut outputs = [0; OUTPUTS];
        outputs[0] = 512;
        for _ in 0..100 {
            apply_lifetime_plasticity(&mut plastic, &outputs);
        }
        assert_eq!(plastic[0], 32);
        assert_eq!(plastic[1], 0);
    }

    #[test]
    fn empty_frame_projects_to_silence() {
        assert_eq!(
            raw_controller_projection(&RawSensoryFrame::EMPTY),
            [0; INPUTS]
        );
    }
}
