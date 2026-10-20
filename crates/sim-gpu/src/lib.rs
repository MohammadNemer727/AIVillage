//! GPU execution of `sim-kernels` through wgpu.
//!
//! The compute shader in `shaders/sim_shaders.spv` is compiled by rust-gpu from the very same
//! `sim-kernels` source the CPU path runs (see `crates/sim-shaders`). Integer-only kernels make
//! the GPU results bit-identical to the CPU results, which the tests in this crate verify on
//! real hardware.

use std::{error::Error, fmt};

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
    /// No suitable adapter was found.
    NoAdapter(wgpu::RequestAdapterError),
    /// The adapter lacks features the rust-gpu shaders need.
    MissingFeatures(wgpu::Features),
    /// The adapter refused to create a device.
    RequestDevice(wgpu::RequestDeviceError),
    /// Input slices had different lengths.
    LengthMismatch,
    /// Reading results back from the GPU failed.
    Readback(String),
}

impl fmt::Display for GpuError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter(error) => write!(formatter, "no GPU adapter: {error}"),
            Self::MissingFeatures(missing) => {
                write!(
                    formatter,
                    "GPU adapter lacks required features: {missing:?}"
                )
            }
            Self::RequestDevice(error) => write!(formatter, "GPU device request failed: {error}"),
            Self::LengthMismatch => formatter.write_str("brain, frame, and config counts differ"),
            Self::Readback(error) => write!(formatter, "GPU readback failed: {error}"),
        }
    }
}

impl Error for GpuError {}

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
    /// Opens the best available Vulkan adapter and builds the brain-step pipeline.
    ///
    /// # Errors
    ///
    /// Returns [`GpuError`] when no adapter supports SPIR-V passthrough and 16-bit integers.
    pub fn new() -> Result<Self, GpuError> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self, GpuError> {
        // SPIR-V passthrough is only meaningful on Vulkan.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle_from_env()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..wgpu::RequestAdapterOptions::default()
            })
            .await
            .map_err(GpuError::NoAdapter)?;
        let missing = REQUIRED_FEATURES - adapter.features();
        if !missing.is_empty() {
            return Err(GpuError::MissingFeatures(missing));
        }
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("sim-gpu"),
                required_features: REQUIRED_FEATURES,
                required_limits: wgpu::Limits::default(),
                ..wgpu::DeviceDescriptor::default()
            })
            .await
            .map_err(GpuError::RequestDevice)?;

        let module = load_shader_module(&device);
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
        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
            adapter_info: adapter.get_info(),
        })
    }

    /// Describes the adapter in use, for logs.
    pub fn adapter_name(&self) -> &str {
        &self.adapter_info.name
    }

    /// Advances every brain one tick in a single dispatch, updating `brains` in place.
    ///
    /// # Errors
    ///
    /// Returns [`GpuError::LengthMismatch`] for inconsistent inputs, or
    /// [`GpuError::Readback`] if results cannot be read back.
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
        self.read_back(&readback, brain_size, brains)
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

fn byte_size<T>(count: usize) -> u64 {
    (core::mem::size_of::<T>() * count) as u64
}

fn workgroups(invocations: usize) -> u32 {
    u32::try_from(invocations.div_ceil(WORKGROUP_SIZE as usize)).unwrap_or(u32::MAX)
}
