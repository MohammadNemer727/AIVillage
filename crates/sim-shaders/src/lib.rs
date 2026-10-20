//! GPU compute entry points. Each one is a thin binding around a `sim-kernels` function, so the
//! simulation logic itself is written exactly once.

#![no_std]

use sim_kernels::gpu::{GpuBrain, GpuBrainConfig, GpuBrainStep, GpuFrame, brain_step_packed};
use spirv_std::{glam::UVec3, spirv};

/// Advances one V2 predictive brain per invocation. The workgroup width must equal
/// `sim_kernels::gpu::WORKGROUP_SIZE`.
#[spirv(compute(threads(64)))]
pub fn brain_step(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] brains: &mut [GpuBrain],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] frames: &[GpuFrame],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] configs: &[GpuBrainConfig],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] steps: &mut [GpuBrainStep],
) {
    let index = id.x as usize;
    if index < brains.len() {
        steps[index] = brain_step_packed(&mut brains[index], &frames[index], &configs[index]);
    }
}
