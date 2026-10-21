//! GPU execution of `sim-kernels` through wgpu.
//!
//! The compute shader in `shaders/sim_shaders.spv` is compiled by rust-gpu from the very same
//! `sim-kernels` source the CPU path runs (see `crates/sim-shaders`). Integer-only kernels make
//! the GPU results bit-identical to the CPU results, which the tests in this crate verify on
//! real hardware.

use std::{error::Error, fmt};

use sim_core::{
    BoxedBrainBackend, BrainBackendError, BrainConfig, BrainStep, PredictiveBrain, RawSensoryFrame,
};
use sim_kernels::gpu::{GpuBrain, GpuBrainConfig, GpuBrainStep, GpuFrame, WORKGROUP_SIZE};
use wgpu::util::DeviceExt;

/// rust-gpu output for the `sim-shaders` crate; regenerate with `build-shaders.sh`.
const SHADER_SPIRV: &[u8] = include_bytes!("../shaders/sim_shaders.spv");
const BRAIN_STEP_ENTRY_POINT: &str = "brain_step";

/// Device features the shaders need: SPIR-V passthrough plus 16-bit integers. wgpu enables
/// 8-bit integers automatically wherever the Vulkan device supports them.
const REQUIRED_FEATURES: wgpu::Features =
    wgpu::Features::PASSTHROUGH_SHADERS.union(wgpu::Features::SHADER_I16);

/// Why the GPU backend is unavailable or a dispatch failed.
#[derive(Debug)]
pub enum GpuError {
    /// No hardware Vulkan adapter supports the shaders; each entry says why one was rejected.
    NoSuitableAdapter(Vec<String>),
    /// The adapter refused to create a device.
    RequestDevice(wgpu::RequestDeviceError),
    /// The device reported a validation, out-of-memory, or internal error.
    Device(wgpu::Error),
    /// The batch exceeds this device's buffer or dispatch limits.
    BatchTooLarge(usize),
    /// Input slices had different lengths.
    LengthMismatch,
    /// Reading results back from the GPU failed.
    Readback(String),
}

impl fmt::Display for GpuError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuitableAdapter(rejected) if rejected.is_empty() => {
                formatter.write_str("no Vulkan GPU adapter found")
            }
            Self::NoSuitableAdapter(rejected) => {
                write!(
                    formatter,
                    "no suitable GPU adapter: {}",
                    rejected.join("; ")
                )
            }
            Self::RequestDevice(error) => write!(formatter, "GPU device request failed: {error}"),
            Self::Device(error) => write!(formatter, "GPU device error: {error}"),
            Self::BatchTooLarge(agents) => {
                write!(
                    formatter,
                    "{agents} brains exceed the GPU's buffer or dispatch limits"
                )
            }
            Self::LengthMismatch => formatter.write_str("brain, frame, and config counts differ"),
            Self::Readback(error) => write!(formatter, "GPU readback failed: {error}"),
        }
    }
}

impl Error for GpuError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::RequestDevice(error) => Some(error),
            Self::Device(error) => Some(error),
            Self::NoSuitableAdapter(_)
            | Self::BatchTooLarge(_)
            | Self::LengthMismatch
            | Self::Readback(_) => None,
        }
    }
}

/// Runs the V2 predictive-brain kernel for many agents in one GPU dispatch.
#[derive(Debug)]
pub struct GpuBrainStepper {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    adapter_info: wgpu::AdapterInfo,
}

impl GpuBrainStepper {
    /// Opens the best hardware Vulkan adapter and builds the brain-step pipeline.
    ///
    /// Software renderers such as llvmpipe and lavapipe are never chosen: they emulate the GPU
    /// on the CPU, far more slowly than the native CPU path. Among adapters that support the
    /// shaders, discrete GPUs are preferred over integrated ones.
    ///
    /// # Errors
    ///
    /// Returns [`GpuError`] when no hardware adapter supports SPIR-V passthrough and 16-bit
    /// integers, or the device cannot be created.
    pub fn new() -> Result<Self, GpuError> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self, GpuError> {
        // SPIR-V passthrough is only meaningful on Vulkan.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle_from_env()
        });
        let adapter = select_adapter(instance.enumerate_adapters(wgpu::Backends::VULKAN).await)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("sim-gpu"),
                required_features: REQUIRED_FEATURES,
                required_limits: wgpu::Limits::default(),
                ..wgpu::DeviceDescriptor::default()
            })
            .await
            .map_err(GpuError::RequestDevice)?;

        let (bind_group_layout, pipeline) =
            capture_errors(&device, || Self::build_pipeline(&device))?;
        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
            adapter_info: adapter.get_info(),
        })
    }

    /// Creates the bind group layout and pipeline for `sim_shaders::brain_step`.
    fn build_pipeline(device: &wgpu::Device) -> (wgpu::BindGroupLayout, wgpu::ComputePipeline) {
        let module = load_shader_module(device);
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        // Matches the `#[spirv(storage_buffer, ...)]` bindings of `sim_shaders::brain_step`.
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("brain_step"),
            entries: &[
                storage(0, false),
                storage(1, true),
                storage(2, true),
                storage(3, false),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("brain_step"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("brain_step"),
            layout: Some(&layout),
            module: &module,
            entry_point: Some(BRAIN_STEP_ENTRY_POINT),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        (bind_group_layout, pipeline)
    }

    /// Describes the adapter in use, for logs.
    pub fn adapter_name(&self) -> &str {
        &self.adapter_info.name
    }

    /// Advances every brain one tick in a single dispatch, updating `brains` in place.
    ///
    /// # Errors
    ///
    /// Returns [`GpuError::LengthMismatch`] for inconsistent inputs,
    /// [`GpuError::BatchTooLarge`] beyond the device's limits, [`GpuError::Device`] if the device
    /// rejects the work, or [`GpuError::Readback`] if results cannot be read back.
    pub fn step(
        &self,
        brains: &mut [GpuBrain],
        frames: &[GpuFrame],
        configs: &[GpuBrainConfig],
    ) -> Result<Vec<GpuBrainStep>, GpuError> {
        if frames.len() != brains.len() || configs.len() != brains.len() {
            return Err(GpuError::LengthMismatch);
        }
        if brains.is_empty() {
            return Ok(Vec::new());
        }
        self.check_limits(brains.len())?;
        let (readback, brain_size) =
            capture_errors(&self.device, || self.submit(brains, frames, configs))?;
        self.read_back(&readback, brain_size, brains)
    }

    /// Rejects batches the device cannot hold, which wgpu would otherwise report only as a
    /// validation error during the dispatch.
    fn check_limits(&self, agents: usize) -> Result<(), GpuError> {
        let limits = self.device.limits();
        let brain_size = byte_size::<GpuBrain>(agents);
        let step_size = byte_size::<GpuBrainStep>(agents);
        let largest_binding = brain_size
            .max(step_size)
            .max(byte_size::<GpuFrame>(agents))
            .max(byte_size::<GpuBrainConfig>(agents));
        if largest_binding > limits.max_storage_buffer_binding_size
            || brain_size + step_size > limits.max_buffer_size
            || workgroups(agents) > limits.max_compute_workgroups_per_dimension
        {
            return Err(GpuError::BatchTooLarge(agents));
        }
        Ok(())
    }

    /// Uploads one batch and submits its dispatch, returning the readback buffer and the byte
    /// offset at which its step results begin.
    fn submit(
        &self,
        brains: &[GpuBrain],
        frames: &[GpuFrame],
        configs: &[GpuBrainConfig],
    ) -> (wgpu::Buffer, u64) {
        let storage = |label, contents: &[u8], usage| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage: wgpu::BufferUsages::STORAGE | usage,
                })
        };
        let brain_buffer = storage(
            "brains",
            bytemuck::cast_slice(brains),
            wgpu::BufferUsages::COPY_SRC,
        );
        let frame_buffer = storage(
            "frames",
            bytemuck::cast_slice(frames),
            wgpu::BufferUsages::empty(),
        );
        let config_buffer = storage(
            "configs",
            bytemuck::cast_slice(configs),
            wgpu::BufferUsages::empty(),
        );
        let step_size = byte_size::<GpuBrainStep>(brains.len());
        let step_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("steps"),
            size: step_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let brain_size = byte_size::<GpuBrain>(brains.len());
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: brain_size + step_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("brain_step"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: brain_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: frame_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: config_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: step_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("brain_step"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("brain_step"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(workgroups(brains.len()), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&brain_buffer, 0, &readback, 0, brain_size);
        encoder.copy_buffer_to_buffer(&step_buffer, 0, &readback, brain_size, step_size);
        self.queue.submit([encoder.finish()]);
        (readback, brain_size)
    }

    /// Waits for the submitted dispatch, then copies brains and step results out of `readback`.
    fn read_back(
        &self,
        readback: &wgpu::Buffer,
        brain_size: u64,
        brains: &mut [GpuBrain],
    ) -> Result<Vec<GpuBrainStep>, GpuError> {
        let (sender, receiver) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                // The receiver outlives this callback; a send error cannot occur.
                let _ = sender.send(result);
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| GpuError::Readback(error.to_string()))?;
        receiver
            .recv()
            .map_err(|error| GpuError::Readback(error.to_string()))?
            .map_err(|error| GpuError::Readback(error.to_string()))?;

        let mapped = readback
            .slice(..)
            .get_mapped_range()
            .map_err(|error| GpuError::Readback(error.to_string()))?;
        let split =
            usize::try_from(brain_size).map_err(|error| GpuError::Readback(error.to_string()))?;
        brains.copy_from_slice(bytemuck::cast_slice(&mapped[..split]));
        let steps = bytemuck::cast_slice(&mapped[split..]).to_vec();
        drop(mapped);
        readback.unmap();
        Ok(steps)
    }
}

/// Opens the best hardware GPU as a brain backend; matches [`sim_core::OpenGpu`], so it can be
/// passed straight to [`sim_core::BackendRunner::new`].
///
/// # Errors
///
/// Returns the [`GpuError`] from [`GpuBrainStepper::new`].
pub fn open_brain_backend() -> Result<BoxedBrainBackend, BrainBackendError> {
    let stepper = GpuBrainStepper::new().map_err(BrainBackendError::new)?;
    Ok(Box::new(stepper))
}

/// Lets [`sim_core::World::step_with_brains`] run V2 brains on the GPU.
impl sim_core::BrainBackend for GpuBrainStepper {
    fn device_name(&self) -> &str {
        self.adapter_name()
    }

    fn step_brains(
        &mut self,
        brains: &mut [PredictiveBrain],
        frames: &[RawSensoryFrame],
        configs: &[BrainConfig],
    ) -> Result<Vec<BrainStep>, BrainBackendError> {
        let mut packed: Vec<GpuBrain> = brains.iter().map(GpuBrain::pack).collect();
        let frames: Vec<GpuFrame> = frames.iter().map(GpuFrame::pack).collect();
        let configs: Vec<GpuBrainConfig> = configs.iter().map(GpuBrainConfig::pack).collect();
        let steps = self
            .step(&mut packed, &frames, &configs)
            .map_err(BrainBackendError::new)?;
        for (brain, packed) in brains.iter_mut().zip(&packed) {
            *brain = packed.unpack();
        }
        Ok(steps.iter().map(GpuBrainStep::unpack).collect())
    }
}

/// Runs `work` with every wgpu error captured, so it is returned rather than reaching wgpu's
/// default handler, which panics and would bypass the caller's fallback.
fn capture_errors<T>(device: &wgpu::Device, work: impl FnOnce() -> T) -> Result<T, GpuError> {
    let scopes = [
        wgpu::ErrorFilter::Internal,
        wgpu::ErrorFilter::OutOfMemory,
        wgpu::ErrorFilter::Validation,
    ]
    .map(|filter| device.push_error_scope(filter));
    let value = work();
    // Scopes must be popped innermost first; report the innermost error.
    let mut first_error = None;
    for scope in scopes.into_iter().rev() {
        let error = pollster::block_on(scope.pop());
        first_error = first_error.or(error);
    }
    first_error.map_or(Ok(value), |error| Err(GpuError::Device(error)))
}

/// Picks the most capable hardware adapter that supports [`REQUIRED_FEATURES`].
fn select_adapter(adapters: Vec<wgpu::Adapter>) -> Result<wgpu::Adapter, GpuError> {
    let mut rejected = Vec::new();
    let mut best: Option<(u8, wgpu::Adapter)> = None;
    for adapter in adapters {
        let info = adapter.get_info();
        let missing = REQUIRED_FEATURES - adapter.features();
        let rank = match info.device_type {
            wgpu::DeviceType::DiscreteGpu => 0,
            wgpu::DeviceType::IntegratedGpu => 1,
            wgpu::DeviceType::Other => 2,
            wgpu::DeviceType::VirtualGpu => 3,
            wgpu::DeviceType::Cpu => {
                rejected.push(format!("{} is a software renderer", info.name));
                continue;
            }
        };
        if !missing.is_empty() {
            rejected.push(format!("{} lacks {missing:?}", info.name));
        } else if best.as_ref().is_none_or(|(best_rank, _)| rank < *best_rank) {
            best = Some((rank, adapter));
        }
    }
    best.map(|(_, adapter)| adapter)
        .ok_or(GpuError::NoSuitableAdapter(rejected))
}

#[allow(
    unsafe_code,
    reason = "wgpu cannot validate rust-gpu SPIR-V; passthrough is the only way to load it."
)]
fn load_shader_module(device: &wgpu::Device) -> wgpu::ShaderModule {
    let entry_points = [wgpu::PassthroughShaderEntryPoint {
        name: BRAIN_STEP_ENTRY_POINT.into(),
        workgroup_size: (WORKGROUP_SIZE, 1, 1),
    }];
    // SAFETY: the module is produced by rust-gpu, which runs spirv-val on its output, and it is
    // embedded at compile time, so it cannot be swapped at runtime. Its bindings and workgroup
    // size are mirrored by the layout above and `WORKGROUP_SIZE`, which the round-trip tests
    // exercise on hardware.
    unsafe {
        device.create_shader_module_passthrough(wgpu::ShaderModuleDescriptorPassthrough {
            label: Some("sim-shaders"),
            entry_points: entry_points.as_slice().into(),
            spirv: Some(wgpu::util::make_spirv_raw(SHADER_SPIRV)),
            ..wgpu::ShaderModuleDescriptorPassthrough::default()
        })
    }
}

/// Bytes needed for `count` values of `T`, saturating so oversized batches fail the limit check
/// instead of overflowing.
fn byte_size<T>(count: usize) -> u64 {
    let count = u64::try_from(count).unwrap_or(u64::MAX);
    u64::try_from(core::mem::size_of::<T>()).map_or(u64::MAX, |size| size.saturating_mul(count))
}

fn workgroups(invocations: usize) -> u32 {
    u32::try_from(invocations.div_ceil(WORKGROUP_SIZE as usize)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_batches_are_rejected_before_any_gpu_work() {
        let Ok(stepper) = GpuBrainStepper::new() else {
            return;
        };
        assert!(stepper.check_limits(1_024).is_ok());
        assert!(matches!(
            stepper.check_limits(usize::MAX),
            Err(GpuError::BatchTooLarge(usize::MAX))
        ));
    }
}
