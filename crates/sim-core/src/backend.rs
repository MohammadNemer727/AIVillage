//! Runtime choice between the built-in CPU brain path and an external [`BrainBackend`].
//!
//! A conforming backend is bit-identical to the CPU kernels, so where brains run is purely a
//! performance decision: switching at any tick boundary cannot change a history. That is what
//! lets [`BackendRunner`] measure both paths on the live world and keep whichever is faster,
//! instead of assuming that a GPU, once present, must win.

use core::{fmt, mem, str::FromStr, time::Duration};
use std::{env, error::Error, time::Instant};

use crate::{BrainBackend, BrainBackendError, ModelVersion, World, WorldSnapshot};

/// Environment variable read by [`BackendPreference::from_env`].
pub const BACKEND_ENV: &str = "AI_VILLAGE_BACKEND";

/// Ticks per backend discarded at the start of a measurement, covering driver warm-up.
const WARMUP_TICKS: u32 = 2;
/// Timed ticks per backend before a measurement is decided.
const SAMPLE_TICKS: usize = 15;
/// The GPU must take at most this many tenths of the CPU's median tick time to be chosen, so
/// near-ties stay on the CPU, which has no device to lose.
const GPU_ADVANTAGE_TENTHS: u128 = 9;

/// A backend that can be stored in a long-lived, thread-shared owner such as a Bevy resource.
pub type BoxedBrainBackend = Box<dyn BrainBackend + Send + Sync>;

/// Opens the GPU brain backend; provided by the binary so `sim-core` stays free of GPU code.
pub type OpenGpu = fn() -> Result<BoxedBrainBackend, BrainBackendError>;

/// The [`OpenGpu`] for builds without GPU support: always fails with a rebuild hint, so `auto`
/// settles on the CPU and an explicit `gpu` request explains what is missing.
///
/// # Errors
///
/// Always.
pub fn gpu_not_built() -> Result<BoxedBrainBackend, BrainBackendError> {
    Err(BrainBackendError::new(
        "this build has no GPU support; rebuild with `--features gpu`",
    ))
}

/// Where the caller asked V2 brains to run.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum BackendPreference {
    /// Use the GPU only when one opens and measures faster than the CPU; otherwise the CPU.
    #[default]
    Auto,
    /// Always use the CPU and never touch a GPU driver.
    Cpu,
    /// Always use the GPU; failing to open or run it is an error, never a silent fallback.
    Gpu,
}

impl BackendPreference {
    /// Reads [`BACKEND_ENV`], treating an unset or empty variable as no preference.
    ///
    /// # Errors
    ///
    /// Returns [`ParseBackendPreferenceError`] for any other value than `auto`, `cpu`, or `gpu`.
    pub fn from_env() -> Result<Option<Self>, ParseBackendPreferenceError> {
        match env::var_os(BACKEND_ENV) {
            None => Ok(None),
            Some(value) if value.is_empty() => Ok(None),
            Some(value) => value.to_string_lossy().parse().map(Some),
        }
    }
}

impl FromStr for BackendPreference {
    type Err = ParseBackendPreferenceError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "cpu" => Ok(Self::Cpu),
            "gpu" => Ok(Self::Gpu),
            _ => Err(ParseBackendPreferenceError(input.to_owned())),
        }
    }
}

impl fmt::Display for BackendPreference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Auto => "auto",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        })
    }
}

/// A backend name other than `auto`, `cpu`, or `gpu`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseBackendPreferenceError(String);

impl fmt::Display for ParseBackendPreferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown backend `{}`, expected auto, cpu, or gpu",
            self.0
        )
    }
}

impl Error for ParseBackendPreferenceError {}

/// Hardware that ran a brain step.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Backend {
    /// The built-in, rayon-parallel CPU kernels.
    Cpu,
    /// The external backend supplied through [`OpenGpu`].
    Gpu,
}

impl fmt::Display for Backend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        })
    }
}

/// What a [`BackendRunner`] is currently doing and why, for logs and diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendStatus {
    /// The CPU was requested explicitly.
    CpuRequested,
    /// The GPU was requested explicitly and opened.
    GpuRequested {
        /// The GPU backend's device name.
        device: String,
    },
    /// `auto` has not stepped a V2 world yet, so it has not opened a GPU.
    AutoPending,
    /// `auto` could not open a GPU and runs on the CPU for the rest of the run.
    AutoGpuUnavailable {
        /// Why opening failed.
        error: String,
    },
    /// `auto` alternates ticks between CPU and GPU to time both.
    AutoMeasuring {
        /// The GPU backend's device name.
        device: String,
    },
    /// `auto` timed both backends and kept the faster one.
    AutoMeasured {
        /// The GPU backend's device name.
        device: String,
        /// The backend now in use.
        chosen: Backend,
        /// Median whole-tick time on the CPU.
        cpu: Duration,
        /// Median whole-tick time with GPU brains.
        gpu: Duration,
    },
    /// The GPU failed mid-run; `auto` redid that tick on the CPU and stays there.
    AutoGpuFailed {
        /// The GPU backend's device name.
        device: String,
        /// The backend's error.
        error: String,
    },
}

impl BackendStatus {
    /// Returns the backend running brains, or `None` while `auto` alternates between both.
    #[must_use]
    pub const fn backend(&self) -> Option<Backend> {
        match self {
            Self::GpuRequested { .. } => Some(Backend::Gpu),
            Self::AutoMeasured { chosen, .. } => Some(*chosen),
            Self::AutoMeasuring { .. } => None,
            Self::CpuRequested
            | Self::AutoPending
            | Self::AutoGpuUnavailable { .. }
            | Self::AutoGpuFailed { .. } => Some(Backend::Cpu),
        }
    }
}

impl fmt::Display for BackendStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CpuRequested => formatter.write_str("cpu (requested)"),
            Self::GpuRequested { device } => write!(formatter, "gpu on {device} (requested)"),
            Self::AutoPending => formatter.write_str("cpu (auto, no V2 world stepped yet)"),
            Self::AutoGpuUnavailable { error } => {
                write!(formatter, "cpu (auto, gpu unavailable: {error})")
            }
            Self::AutoMeasuring { device } => {
                write!(formatter, "measuring cpu against gpu on {device} (auto)")
            }
            Self::AutoMeasured {
                device,
                chosen: Backend::Gpu,
                cpu,
                gpu,
            } => write!(
                formatter,
                "gpu on {device} (auto, {gpu:.2?} per tick against {cpu:.2?} on cpu)"
            ),
            Self::AutoMeasured {
                device,
                chosen: Backend::Cpu,
                cpu,
                gpu,
            } => write!(
                formatter,
                "cpu (auto, {cpu:.2?} per tick against {gpu:.2?} on gpu {device})"
            ),
            Self::AutoGpuFailed { device, error } => {
                write!(formatter, "cpu (auto, gpu {device} failed: {error})")
            }
        }
    }
}

/// Steps worlds with V2 brains on the CPU or the GPU according to a [`BackendPreference`].
///
/// Every backend produces bit-identical histories, so the preference only affects speed. V1
/// worlds have no predictive brains and always step on the CPU without opening a GPU.
///
/// With [`BackendPreference::Auto`] the GPU is opened on the first V2 tick. If it opens, the
/// runner alternates ticks between both backends, discards warm-up ticks, and keeps the GPU
/// only if its median tick is clearly faster. It measures again whenever the living
/// population has halved or doubled since the last decision. A GPU failure redoes that tick
/// on the CPU, which is exact because a failed tick leaves the world unchanged.
pub struct BackendRunner {
    mode: Mode,
    status: BackendStatus,
    status_changed: bool,
}

enum Mode {
    Cpu,
    Gpu(BoxedBrainBackend),
    Auto(Auto),
}

enum Auto {
    Unopened(OpenGpu),
    CpuOnly,
    Measuring {
        gpu: BoxedBrainBackend,
        measurement: Measurement,
    },
    Settled {
        gpu: BoxedBrainBackend,
        chosen: Backend,
        population: usize,
    },
}

impl BackendRunner {
    /// Creates a runner, opening the GPU immediately when it is requested explicitly.
    ///
    /// `open_gpu` is called at most once: here for [`BackendPreference::Gpu`], or on the first
    /// V2 tick for [`BackendPreference::Auto`]. It is never called for
    /// [`BackendPreference::Cpu`].
    ///
    /// # Errors
    ///
    /// Returns `open_gpu`'s error when the GPU was requested explicitly and cannot be opened.
    pub fn new(
        preference: BackendPreference,
        open_gpu: OpenGpu,
    ) -> Result<Self, BrainBackendError> {
        let (mode, status) = match preference {
            BackendPreference::Cpu => (Mode::Cpu, BackendStatus::CpuRequested),
            BackendPreference::Gpu => {
                let gpu = open_gpu()?;
                let device = gpu.device_name().to_owned();
                (Mode::Gpu(gpu), BackendStatus::GpuRequested { device })
            }
            BackendPreference::Auto => (
                Mode::Auto(Auto::Unopened(open_gpu)),
                BackendStatus::AutoPending,
            ),
        };
        Ok(Self {
            mode,
            status_changed: preference != BackendPreference::Auto,
            status,
        })
    }

    /// Returns what the runner is currently doing and why.
    #[must_use]
    pub const fn status(&self) -> &BackendStatus {
        &self.status
    }

    /// Returns the status if it changed since the last call, so callers can log each change
    /// once. An explicit preference reports its status on the first call.
    pub fn take_status_change(&mut self) -> Option<&BackendStatus> {
        mem::take(&mut self.status_changed).then_some(&self.status)
    }

    /// Advances `world` one tick on the selected backend.
    ///
    /// # Errors
    ///
    /// Fails only for [`BackendPreference::Gpu`], when the GPU cannot run the tick; the world
    /// is then unchanged. `auto` recovers from GPU failures on the CPU instead.
    pub fn step(&mut self, world: &mut World) -> Result<WorldSnapshot, BrainBackendError> {
        if world.model_version() != ModelVersion::V2Physics {
            return Ok(world.step());
        }
        match &mut self.mode {
            Mode::Cpu => Ok(world.step()),
            Mode::Gpu(gpu) => world.step_with_brains(gpu.as_mut()),
            Mode::Auto(auto) => {
                let (snapshot, status) = auto.step(world);
                if let Some(status) = status {
                    self.status = status;
                    self.status_changed = true;
                }
                Ok(snapshot)
            }
        }
    }
}

impl fmt::Debug for BackendRunner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BackendRunner")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl Auto {
    /// Advances one tick, returning the new status when the decision changed.
    fn step(&mut self, world: &mut World) -> (WorldSnapshot, Option<BackendStatus>) {
        match mem::replace(self, Self::CpuOnly) {
            Self::Unopened(open_gpu) => match open_gpu() {
                Ok(gpu) => self.measure(gpu, world),
                Err(error) => (
                    world.step(),
                    Some(BackendStatus::AutoGpuUnavailable {
                        error: error.to_string(),
                    }),
                ),
            },
            Self::CpuOnly => (world.step(), None),
            Self::Measuring {
                mut gpu,
                mut measurement,
            } => {
                let backend = measurement.next_backend();
                let started = Instant::now();
                let snapshot = match backend {
                    Backend::Cpu => world.step(),
                    Backend::Gpu => match world.step_with_brains(gpu.as_mut()) {
                        Ok(snapshot) => snapshot,
                        Err(error) => return (world.step(), Some(gpu_failed(&*gpu, &error))),
                    },
                };
                measurement.record(backend, started.elapsed());
                let Some(verdict) = measurement.verdict() else {
                    *self = Self::Measuring { gpu, measurement };
                    return (snapshot, None);
                };
                let status = BackendStatus::AutoMeasured {
                    device: gpu.device_name().to_owned(),
                    chosen: verdict.chosen,
                    cpu: verdict.cpu,
                    gpu: verdict.gpu,
                };
                *self = Self::Settled {
                    gpu,
                    chosen: verdict.chosen,
                    population: world.agent_count(),
                };
                (snapshot, Some(status))
            }
            Self::Settled {
                mut gpu,
                chosen,
                population,
            } => {
                if population_shifted(population, world.agent_count()) {
                    return self.measure(gpu, world);
                }
                let snapshot = match chosen {
                    Backend::Cpu => world.step(),
                    Backend::Gpu => match world.step_with_brains(gpu.as_mut()) {
                        Ok(snapshot) => snapshot,
                        Err(error) => return (world.step(), Some(gpu_failed(&*gpu, &error))),
                    },
                };
                *self = Self::Settled {
                    gpu,
                    chosen,
                    population,
                };
                (snapshot, None)
            }
        }
    }

    /// Starts a fresh measurement with `gpu` and runs its first tick.
    fn measure(
        &mut self,
        gpu: BoxedBrainBackend,
        world: &mut World,
    ) -> (WorldSnapshot, Option<BackendStatus>) {
        let measuring = BackendStatus::AutoMeasuring {
            device: gpu.device_name().to_owned(),
        };
        *self = Self::Measuring {
            gpu,
            measurement: Measurement::default(),
        };
        let (snapshot, status) = self.step(world);
        (snapshot, status.or(Some(measuring)))
    }
}

fn gpu_failed(gpu: &dyn BrainBackend, error: &BrainBackendError) -> BackendStatus {
    BackendStatus::AutoGpuFailed {
        device: gpu.device_name().to_owned(),
        error: error.to_string(),
    }
}

/// Whether the living population has halved or doubled since `decided`, which can move the
/// CPU/GPU crossover enough to be worth measuring again.
fn population_shifted(decided: usize, living: usize) -> bool {
    let decided = decided.max(1);
    living > decided.saturating_mul(2) || living.saturating_mul(2) < decided
}

/// Interleaved tick timings of both backends.
///
/// Alternating ticks exposes both backends to the same population and system load, so drift
/// over the measurement affects them equally.
#[derive(Debug, Default)]
struct Measurement {
    ticks: u32,
    cpu: Vec<Duration>,
    gpu: Vec<Duration>,
}

/// The outcome of a completed [`Measurement`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Verdict {
    chosen: Backend,
    cpu: Duration,
    gpu: Duration,
}

impl Measurement {
    /// Returns the backend for the next tick, starting with the GPU so it warms up first.
    const fn next_backend(&self) -> Backend {
        if self.ticks.is_multiple_of(2) {
            Backend::Gpu
        } else {
            Backend::Cpu
        }
    }

    /// Records the time of the tick run on `backend`, discarding warm-up ticks.
    fn record(&mut self, backend: Backend, elapsed: Duration) {
        let warming_up = self.ticks < 2 * WARMUP_TICKS;
        self.ticks = self.ticks.saturating_add(1);
        if warming_up {
            return;
        }
        match backend {
            Backend::Cpu => self.cpu.push(elapsed),
            Backend::Gpu => self.gpu.push(elapsed),
        }
    }

    /// Compares median tick times once both backends have enough samples.
    fn verdict(&mut self) -> Option<Verdict> {
        if self.cpu.len() < SAMPLE_TICKS || self.gpu.len() < SAMPLE_TICKS {
            return None;
        }
        let cpu = median(&mut self.cpu);
        let gpu = median(&mut self.gpu);
        let gpu_clearly_faster = gpu.as_nanos() * 10 <= cpu.as_nanos() * GPU_ADVANTAGE_TENTHS;
        Some(Verdict {
            chosen: if gpu_clearly_faster {
                Backend::Gpu
            } else {
                Backend::Cpu
            },
            cpu,
            gpu,
        })
    }
}

/// Returns the upper median, which ignores outliers such as a tick preempted by the OS.
fn median(samples: &mut [Duration]) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured(cpu_millis: u64, gpu_millis: u64) -> Verdict {
        let mut measurement = Measurement::default();
        loop {
            let backend = measurement.next_backend();
            let millis = match backend {
                Backend::Cpu => cpu_millis,
                Backend::Gpu => gpu_millis,
            };
            measurement.record(backend, Duration::from_millis(millis));
            if let Some(verdict) = measurement.verdict() {
                return verdict;
            }
        }
    }

    #[test]
    fn preference_parses_case_insensitively_and_round_trips() {
        for preference in [
            BackendPreference::Auto,
            BackendPreference::Cpu,
            BackendPreference::Gpu,
        ] {
            assert_eq!(preference.to_string().parse(), Ok(preference));
        }
        assert_eq!(" GPU ".parse(), Ok(BackendPreference::Gpu));
        let error = "cuda".parse::<BackendPreference>().unwrap_err();
        assert_eq!(
            error.to_string(),
            "unknown backend `cuda`, expected auto, cpu, or gpu"
        );
    }

    #[test]
    fn measurement_discards_warm_up_and_alternates_starting_with_gpu() {
        let mut measurement = Measurement::default();
        let mut order = Vec::new();
        for _ in 0..2 * WARMUP_TICKS + 2 {
            let backend = measurement.next_backend();
            order.push(backend);
            measurement.record(backend, Duration::from_millis(1));
        }
        assert_eq!(order[..2], [Backend::Gpu, Backend::Cpu]);
        assert_eq!((measurement.cpu.len(), measurement.gpu.len()), (1, 1));
    }

    #[test]
    fn gpu_is_chosen_only_when_clearly_faster() {
        assert_eq!(measured(20, 10).chosen, Backend::Gpu);
        assert_eq!(measured(20, 18).chosen, Backend::Gpu);
        assert_eq!(measured(20, 19).chosen, Backend::Cpu);
        assert_eq!(measured(20, 20).chosen, Backend::Cpu);
        assert_eq!(measured(20, 40).chosen, Backend::Cpu);
    }

    #[test]
    fn verdict_reports_medians_and_ignores_outliers() {
        let mut measurement = Measurement {
            ticks: 2 * WARMUP_TICKS,
            ..Measurement::default()
        };
        for tick in 0..2 * SAMPLE_TICKS {
            let backend = measurement.next_backend();
            let millis = if tick == 0 { 1_000 } else { 10 };
            measurement.record(backend, Duration::from_millis(millis));
        }
        let verdict = measurement.verdict().expect("enough samples");
        assert_eq!(verdict.gpu, Duration::from_millis(10));
        assert_eq!(verdict.cpu, Duration::from_millis(10));
    }

    #[test]
    fn population_must_halve_or_double_to_trigger_a_new_measurement() {
        assert!(!population_shifted(100, 100));
        assert!(!population_shifted(100, 200));
        assert!(!population_shifted(100, 50));
        assert!(population_shifted(100, 201));
        assert!(population_shifted(100, 49));
        assert!(population_shifted(0, 3));
        assert!(!population_shifted(0, 2));
    }
}
