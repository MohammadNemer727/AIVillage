//! Fixed-topology quantized recurrent controller used by V1 agents and the V2 projection.

use crate::{
    math::{amplitude, clamp_i16},
    senses::RawSensoryFrame,
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
    for (node, value) in hidden.iter_mut().enumerate() {
        let mut sum = i32::from(biases[node]) + i32::from(plastic_biases[node]);
        for (input, input_value) in inputs.iter().enumerate() {
            sum += i32::from(*input_value) * i32::from(weights[node * INPUTS + input]) / 128;
        }
        let recurrent_offset = RECURRENT_OFFSET + node * HIDDEN;
        for (source, state) in previous.iter().enumerate() {
            sum += i32::from(*state) * i32::from(weights[recurrent_offset + source]) / 128;
        }
        *value = clamp_i16(sum, -512, 512);
    }
    *recurrent = hidden;
    let mut outputs = [0_i16; OUTPUTS];
    for (node, value) in outputs.iter_mut().enumerate() {
        let mut sum = i32::from(biases[HIDDEN + node]) + i32::from(plastic_biases[HIDDEN + node]);
        for (source, state) in hidden.iter().enumerate() {
            sum += i32::from(*state) * i32::from(weights[OUTPUT_OFFSET + node * HIDDEN + source])
                / 128;
        }
        *value = clamp_i16(sum, -512, 512);
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
    for (node, adjustment) in plastic_biases.iter_mut().enumerate() {
        let delta = (outputs[node % OUTPUTS] / 128).clamp(-1, 1);
        *adjustment = adjustment.saturating_add(delta).clamp(-32, 32);
    }
}

/// Projects all receptor amplitudes into the controller's fixed input width.
///
/// This is deliberately lossy but contains no semantic field or world coordinate.
#[must_use]
pub fn raw_controller_projection(frame: &RawSensoryFrame) -> [i16; INPUTS] {
    let mut inputs = [0_i16; INPUTS];
    let mut channel = 0_usize;
    let mut accumulate = |value: i16| {
        inputs[channel % INPUTS] = inputs[channel % INPUTS].saturating_add(value);
        channel += 1;
    };
    for ray in frame.retinal {
        for value in [ray.red, ray.green, ray.blue, ray.brightness, ray.occlusion] {
            accumulate(amplitude(value) / 4);
        }
        accumulate(ray.motion / 4);
    }
    for value in frame.chemicals {
        accumulate(amplitude(value));
    }
    for value in [
        frame.vibration,
        frame.touch,
        frame.joint_position,
        frame.muscle_tension,
        frame.orientation,
        frame.energy,
        frame.gut_pressure,
        frame.toxin_stress,
        frame.hydration,
        frame.fatigue,
        frame.damage,
    ] {
        accumulate(amplitude(value));
    }
    accumulate(frame.acceleration);
    inputs
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
