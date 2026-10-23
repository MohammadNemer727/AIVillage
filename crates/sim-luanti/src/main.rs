//! Command-line host for the loopback Luanti bridge.
//!
//! `sim-luanti` serves the bridge; `sim-luanti replay <trace.ndjson>` replays a recorded trace
//! through the founder runtime selected by the same `SIM_LUANTI_SEED` and `SIM_LUANTI_AGENT_ID`.

use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::{Duration, Instant},
};

use sim_core::runtime::{AffordanceConfig, EpisodicConfig, ExperimentMode, SpatialConfig};
use sim_format::{PopulationBlockReason, PopulationLifecycleEvent};
use sim_luanti::{
    Bridge, LifecycleConfig, PopulationCoordinator, TraceRecord, ablation, benchmark,
    bind_loopback, contingency, episodic_eval, foraging, lineage, load_state, prediction_eval,
    reaching, replay_trace, save_state, serve_once_concurrent,
};

const USAGE: &str = "usage: sim-luanti [replay <trace.ndjson>|prediction-eval|episodic-eval|prediction-ablation <trace.ndjson> [report.json]|online-prediction off|policy|prediction|both|graph <trace.ndjson> [report.json]|founders|lineage <genome-rotation> <population-report.json>|benchmark|contingency|foraging|reaching [report.json] [report.md]]";

/// Decisions between saves of the founder's brain (about ten seconds of watched play).
const SAVE_EVERY_DECISIONS: u64 = 40;

/// Writes an experiment report to `[report.json] [report.md]`, defaulting to
/// `target/<name>.json` and `docs/<name>.md`.
fn write_report(
    mut args: impl Iterator<Item = String>,
    name: &str,
    report: &impl serde::Serialize,
    markdown: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let json = args.next().map_or_else(
        || PathBuf::from(format!("target/{name}.json")),
        PathBuf::from,
    );
    let markdown_path = args
        .next()
        .map_or_else(|| PathBuf::from(format!("docs/{name}.md")), PathBuf::from);
    if args.next().is_some() {
        return Err(
            "usage: sim-luanti benchmark|contingency|foraging|reaching [report.json] [report.md]"
                .into(),
        );
    }
    std::fs::write(&json, serde_json::to_vec_pretty(report)?)?;
    std::fs::write(&markdown_path, markdown)?;
    println!("wrote {} and {}", json.display(), markdown_path.display());
    Ok(())
}

/// Replays a recorded trace exactly through the runtime.
fn replay(
    seed: u64,
    agent_id: u64,
    path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let records = read_trace(path)?;
    replay_trace(seed, agent_id, &records)?;
    println!("replayed {} recorded transitions exactly", records.len());
    Ok(())
}

/// Loads the contiguous records of one agent from a bridge trace.
fn agent_records(
    path: &str,
    agent_id: u64,
) -> Result<Vec<TraceRecord>, Box<dyn std::error::Error + Send + Sync>> {
    Ok(read_trace(path)?
        .into_iter()
        .filter(|record| record.request.agent_id == agent_id)
        .collect())
}

/// Reads every record of a bridge NDJSON trace.
fn read_trace(path: &str) -> Result<Vec<TraceRecord>, Box<dyn std::error::Error + Send + Sync>> {
    Ok(std::fs::read_to_string(path)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<Vec<TraceRecord>, _>>()?)
}

/// Writes a JSON report to `output`, or prints it when no path is given.
fn emit(
    report: &impl serde::Serialize,
    output: Option<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let json = serde_json::to_vec_pretty(report)?;
    if let Some(output) = output {
        std::fs::write(output, json)?;
    } else {
        println!("{}", String::from_utf8(json)?);
    }
    Ok(())
}

/// The expressed developmental parameters of each founder ordinal in a seeded cohort.
fn founder_genotypes(seed: u64, first_id: u64, count: u32) -> Vec<serde_json::Value> {
    (0..u64::from(count))
        .map(|ordinal| {
            let genome = sim_format::FounderSpec::new(seed, first_id + ordinal).genome;
            let config = genome.default_brain_config();
            serde_json::json!({
                "ordinal": ordinal,
                "genome": genome,
                "connection_stride": config.connection_stride,
                "plasticity_scale": config.plasticity_scale,
            })
        })
        .collect()
}

/// The founder the bridge serves: the one saved at `state_path` (`SIM_LUANTI_STATE`, where its brain
/// is kept between sessions) if there is one, continuing with everything it learned in the mode
/// it was saved in; otherwise a newborn in the mode `SIM_LUANTI_MODE` selects.
fn founder_bridge(
    config: &ServerConfig,
    state_path: Option<&std::path::Path>,
    population_path: Option<&std::path::Path>,
) -> Result<Bridge, Box<dyn std::error::Error + Send + Sync>> {
    let &ServerConfig {
        seed,
        agent_id,
        founder_count,
        population_capacity,
        genome_rotation,
        lifecycle_config,
        ..
    } = config;
    if let Some(path) = population_path
        && path.join("population.json").exists()
    {
        let mut population = PopulationCoordinator::load(path)?;
        if let Some(config) = episodic_config()? {
            population.set_episodic_config(config);
        }
        if let Some(config) = spatial_config()? {
            population.set_spatial_config(config);
        }
        eprintln!(
            "sim-luanti: continuing {} independently saved individuals from {}",
            population.manifest().individuals.len(),
            path.display()
        );
        return Ok(population);
    }
    let mode = match std::env::var("SIM_LUANTI_MODE").as_deref() {
        Ok("affordance") => ExperimentMode::AffordanceDiscovery,
        Ok("homeostatic") => ExperimentMode::Homeostatic,
        Ok("wander") => ExperimentMode::Wander,
        Ok("baseline") | Err(_) => ExperimentMode::PredictiveBaseline,
        Ok(value) => {
            return Err(format!(
                "SIM_LUANTI_MODE must be baseline, affordance, homeostatic or wander, got {value}"
            )
            .into());
        }
    };
    eprintln!("sim-luanti mode: {mode:?}");
    // Optional affordance learner settings as JSON; omitted fields keep their defaults, e.g.
    // SIM_LUANTI_AFFORDANCE='{"rule":"LearningProgress","action_horizon":16}'.
    let population_mode = population_path.is_some() || founder_count > 1;
    let mut bridge = match std::env::var("SIM_LUANTI_AFFORDANCE") {
        Ok(json) if mode == ExperimentMode::AffordanceDiscovery => {
            let settings: AffordanceConfig = serde_json::from_str(&json)
                .map_err(|error| format!("SIM_LUANTI_AFFORDANCE is not valid JSON: {error}"))?;
            eprintln!("sim-luanti affordance settings: {settings:?}");
            if population_mode {
                Bridge::new_population_with_affordance(
                    seed,
                    agent_id,
                    founder_count,
                    population_capacity,
                    genome_rotation,
                    settings,
                )?
            } else {
                Bridge::new_with_affordance(seed, agent_id, settings)?
            }
        }
        Ok(_) => return Err("SIM_LUANTI_AFFORDANCE requires SIM_LUANTI_MODE=affordance".into()),
        Err(_) if population_mode => Bridge::new_rotated_population_with_mode(
            seed,
            agent_id,
            founder_count,
            population_capacity,
            mode,
            genome_rotation,
        )?,
        Err(_) => Bridge::new_with_mode(seed, agent_id, mode),
    };
    if let Some(path) = state_path
        && let Some(mut runtime) = load_state(path, seed, agent_id)?
    {
        if let Some(config) = episodic_config()? {
            runtime.set_episodic_config(config);
        }
        if let Some(config) = spatial_config()? {
            runtime.set_spatial_config(config);
        }
        eprintln!(
            "sim-luanti: continuing the saved founder ({:?}, {} decisions) from {}",
            runtime.state().experiment_mode,
            runtime.state().decision_tick,
            path.display()
        );
        return Ok(Bridge::from_runtime(runtime));
    }
    if let Some(config) = episodic_config()? {
        bridge.set_episodic_config(config);
    }
    if let Some(config) = spatial_config()? {
        bridge.set_spatial_config(config);
    }
    if let Some(path) = population_path {
        bridge.set_lifecycle_config(lifecycle_config)?;
        bridge.bind_storage(path)?;
    }
    Ok(bridge)
}

fn episodic_config() -> Result<Option<EpisodicConfig>, Box<dyn std::error::Error + Send + Sync>> {
    match std::env::var("SIM_LUANTI_EPISODIC") {
        Err(_) => Ok(None),
        Ok(value) => Ok(Some(episodic_arm(&value).map_err(|value| {
            format!(
                "SIM_LUANTI_EPISODIC must be off, prediction, policy, both, or graph; got {value}"
            )
        })?)),
    }
}

/// Spatial memory from `SIM_LUANTI_SPATIAL`: `off`, `on`, or the sham `scrambled`.
fn spatial_config() -> Result<Option<SpatialConfig>, Box<dyn std::error::Error + Send + Sync>> {
    Ok(match std::env::var("SIM_LUANTI_SPATIAL").as_deref() {
        Err(_) => None,
        Ok("off") => Some(SpatialConfig::default()),
        Ok("on") => Some(SpatialConfig {
            enabled: true,
            scrambled: false,
        }),
        Ok("scrambled") => Some(SpatialConfig {
            enabled: true,
            scrambled: true,
        }),
        Ok(value) => {
            return Err(
                format!("SIM_LUANTI_SPATIAL must be off, on, or scrambled; got {value}").into(),
            );
        }
    })
}

/// Parses an episodic switch setting by its arm name.
fn episodic_arm(value: &str) -> Result<EpisodicConfig, &str> {
    // (prediction retrieval, policy retrieval, semantic graph feeding the policy)
    let (prediction_enabled, policy_enabled, graph_enabled) = match value {
        "off" => (false, false, false),
        "prediction" => (true, false, false),
        "policy" => (false, true, false),
        "both" => (true, true, false),
        "graph" => (false, true, true),
        _ => return Err(value),
    };
    Ok(EpisodicConfig {
        prediction_enabled,
        policy_enabled,
        graph_enabled,
    })
}

struct ServerConfig {
    port: u16,
    seed: u64,
    agent_id: u64,
    founder_count: u32,
    population_capacity: u32,
    genome_rotation: u32,
    lifecycle_config: LifecycleConfig,
    stop: ExperimentStop,
}

/// Unattended population-run stop rule and report (named for Experiment 001, which added it).
struct ExperimentStop {
    report: Option<PathBuf>,
    provenance: Option<PathBuf>,
    name: String,
    birth_target: u64,
    deadline: Duration,
}

/// Reads a numeric setting, rejecting a value that is present but malformed.
fn number<T: core::str::FromStr>(
    name: &str,
    default: T,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
    std::env::var(name).map_or(Ok(default), |value| {
        value
            .parse()
            .map_err(|_| format!("{name} must be a non-negative integer, got {value}").into())
    })
}

fn server_config() -> Result<ServerConfig, Box<dyn std::error::Error + Send + Sync>> {
    let founder_count = number("SIM_LUANTI_FOUNDER_COUNT", 1_u32)?;
    if founder_count == 0 {
        return Err("SIM_LUANTI_FOUNDER_COUNT must be a positive integer".into());
    }
    let mutation_enabled = match std::env::var("SIM_LUANTI_MUTATION").as_deref() {
        Ok("enabled") | Err(_) => true,
        Ok("disabled") => false,
        Ok(_) => return Err("SIM_LUANTI_MUTATION must be enabled or disabled".into()),
    };
    Ok(ServerConfig {
        port: number("SIM_LUANTI_PORT", 8_481)?,
        seed: number("SIM_LUANTI_SEED", 1)?,
        agent_id: number("SIM_LUANTI_AGENT_ID", 1)?,
        founder_count,
        population_capacity: number("SIM_LUANTI_POPULATION_CAPACITY", founder_count)?,
        genome_rotation: number("SIM_LUANTI_FOUNDER_GENOME_ROTATION", 0)?,
        lifecycle_config: LifecycleConfig {
            maturity_decisions: number("SIM_LUANTI_REPRODUCTIVE_MATURITY", 100)?,
            minimum_reproductive_reserve: number("SIM_LUANTI_MINIMUM_REPRODUCTIVE_RESERVE", 160)?,
            reproductive_cost: number("SIM_LUANTI_REPRODUCTIVE_COST", 80)?,
            mutation_enabled,
        },
        stop: ExperimentStop {
            report: std::env::var_os("SIM_LUANTI_EXPERIMENT_001_REPORT").map(PathBuf::from),
            provenance: std::env::var_os("SIM_LUANTI_RUN_PROVENANCE").map(PathBuf::from),
            name: std::env::var("SIM_LUANTI_EXPERIMENT_NAME")
                .unwrap_or_else(|_| "001-autonomous-generational-turnover".to_owned()),
            birth_target: number("SIM_LUANTI_EXPERIMENT_001_BIRTHS", 100)?,
            deadline: Duration::from_secs(number(
                "SIM_LUANTI_EXPERIMENT_001_DEADLINE_SECONDS",
                86_400,
            )?),
        },
    })
}

fn stall_config() -> Result<Option<(u64, Duration)>, Box<dyn std::error::Error + Send + Sync>> {
    if std::env::var_os("SIM_LUANTI_STALL_SEQUENCE").is_none() {
        return Ok(None);
    }
    Ok(Some((
        number("SIM_LUANTI_STALL_SEQUENCE", 0)?,
        Duration::from_millis(number("SIM_LUANTI_STALL_MS", 750)?),
    )))
}

/// Runs an offline command: evaluation, replay, lineage, or a fixed simulated test.
fn run_subcommand(
    command: &str,
    mut args: impl Iterator<Item = String>,
    config: &ServerConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let &ServerConfig {
        seed,
        agent_id,
        founder_count,
        ..
    } = config;
    if matches!(
        command,
        "prediction-eval" | "episodic-eval" | "prediction-ablation" | "online-prediction"
    ) {
        let arm = if command == "online-prediction" {
            Some(episodic_arm(&args.next().ok_or(USAGE)?).map_err(|_| USAGE)?)
        } else {
            None
        };
        let path = args.next().ok_or(USAGE)?;
        let output = args.next();
        if args.next().is_some() {
            return Err(USAGE.into());
        }
        let records = agent_records(&path, agent_id)?;
        return match (command, arm) {
            ("prediction-eval", _) => emit(&prediction_eval::evaluate(seed, &records)?, output),
            ("episodic-eval", _) => emit(&episodic_eval::evaluate(seed, &records)?, output),
            (_, Some(arm)) => emit(&ablation::online_prediction(seed, &records, arm)?, output),
            _ => emit(&ablation::frozen_replay(seed, &records)?, output),
        };
    }
    if command == "lineage" {
        let (Some(rotation), Some(path), None) = (args.next(), args.next(), args.next()) else {
            return Err(USAGE.into());
        };
        let report: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
        let manifest = serde_json::from_value(report["manifest"].clone())?;
        return emit(
            &lineage::reconstruct(&manifest, agent_id, founder_count, rotation.parse()?)?,
            None,
        );
    }
    if command == "founders" {
        if args.next().is_some() {
            return Err(USAGE.into());
        }
        return emit(&founder_genotypes(seed, agent_id, founder_count), None);
    }
    if command == "benchmark" {
        let report = benchmark::run();
        let markdown = benchmark::markdown(&report);
        return write_report(args, "affordance-benchmark", &report, &markdown);
    }
    if command == "reaching" {
        let report = reaching::run();
        let markdown = reaching::markdown(&report);
        return write_report(args, "reaching-test", &report, &markdown);
    }
    if command == "foraging" {
        let report = foraging::run();
        let markdown = foraging::markdown(&report);
        return write_report(args, "foraging-test", &report, &markdown);
    }
    if command == "contingency" {
        let report = contingency::run();
        let markdown = contingency::markdown(&report);
        return write_report(args, "contingency-test", &report, &markdown);
    }
    let (Some(path), None, "replay") = (args.next(), args.next(), command) else {
        return Err(USAGE.into());
    };
    replay(seed, agent_id, &path)
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = server_config()?;
    let mut args = std::env::args().skip(1);
    if let Some(command) = args.next() {
        return run_subcommand(&command, args, &config);
    }
    let server = bind_loopback(config.port)?;
    eprintln!("sim-luanti listening on 127.0.0.1:{}", config.port);
    let state_path = std::env::var_os("SIM_LUANTI_STATE").map(PathBuf::from);
    let population_path = std::env::var_os("SIM_LUANTI_POPULATION").map(PathBuf::from);
    let bridge = founder_bridge(&config, state_path.as_deref(), population_path.as_deref())?;
    if population_path.is_some() {
        eprintln!(
            "sim-luanti population: {} live individuals, capacity {}",
            bridge.manifest().individuals.len(),
            bridge.manifest().capacity
        );
    }
    let mut saved_at = bridge.total_decisions();
    let mut trace = std::env::var_os("SIM_LUANTI_TRACE")
        .map(|path| OpenOptions::new().create(true).append(true).open(path))
        .transpose()?;
    let stall_sequence = stall_config()?;
    let bridge = Arc::new(Mutex::new(bridge));
    let stalled = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    loop {
        serve_once_concurrent(
            &server,
            &bridge,
            Duration::from_millis(100),
            stall_sequence,
            &stalled,
        )?;
        let mut bridge = bridge
            .lock()
            .map_err(|_| "population coordinator lock was poisoned")?;
        let decisions = bridge.total_decisions();
        if decisions >= saved_at + SAVE_EVERY_DECISIONS {
            if let Some(path) = &population_path {
                bridge.save(path)?;
            } else if let Some(path) = &state_path {
                save_state(path, bridge.runtime())?;
            }
            saved_at = decisions;
        }
        let records = bridge.drain_trace();
        if let Some(file) = &mut trace {
            for record in records {
                let mut line = serde_json::to_vec(&record)?;
                line.push(b'\n');
                file.write_all(&line)?;
            }
        }
        if let Some(path) = &config.stop.report
            && let Some(stop_reason) = stop_reason(&bridge, &config.stop, started)
        {
            bridge.save(
                population_path
                    .as_deref()
                    .ok_or("an experiment report requires SIM_LUANTI_POPULATION")?,
            )?;
            let report = experiment_report(&bridge, &config, stop_reason, started)?;
            std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
            println!(
                "{} stopped at {stop_reason}; wrote {}",
                config.stop.name,
                path.display()
            );
            return Ok(());
        }
    }
}

/// The unattended run's terminal condition, if one has been reached.
fn stop_reason(bridge: &Bridge, stop: &ExperimentStop, started: Instant) -> Option<&'static str> {
    let births = bridge
        .lifecycle_events()
        .iter()
        .filter(|event| matches!(event, PopulationLifecycleEvent::Birth { .. }))
        .count() as u64;
    if births >= stop.birth_target {
        Some("birth_target")
    } else if bridge.manifest().individuals.is_empty() {
        Some("natural_extinction")
    } else if started.elapsed() >= stop.deadline {
        Some("deadline")
    } else {
        None
    }
}

fn experiment_report(
    bridge: &Bridge,
    config: &ServerConfig,
    stop_reason: &str,
    started: Instant,
) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    let events = bridge.lifecycle_events();
    let count = |predicate: &dyn Fn(&PopulationLifecycleEvent) -> bool| {
        events.iter().filter(|event| predicate(event)).count()
    };
    let blocked = |reason| {
        count(&|event| {
            matches!(
                event,
                PopulationLifecycleEvent::ReproductionBlocked { reason: found, .. } if *found == reason
            )
        })
    };
    let provenance = config
        .stop
        .provenance
        .as_ref()
        .map(std::fs::read)
        .transpose()?
        .map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes))
        .transpose()?;
    let manifest = bridge.manifest();
    Ok(serde_json::json!({
        "experiment": config.stop.name,
        "founder_genome_rotation": config.genome_rotation,
        "stop_reason": stop_reason,
        "birth_target": config.stop.birth_target,
        "births": count(&|event| matches!(event, PopulationLifecycleEvent::Birth { .. })),
        "controller_reproduction_attempts":
            count(&|event| matches!(event, PopulationLifecycleEvent::ReproductionAttempt { .. })),
        "live_individuals": manifest.individuals.len(),
        "elapsed_seconds": started.elapsed().as_secs(),
        "mutation": if manifest.mutation_enabled { "enabled" } else { "disabled" },
        "blocked": {
            "no_free_slot": blocked(PopulationBlockReason::NoFreeSlot),
            "biological_eligibility": blocked(PopulationBlockReason::BiologicalEligibility),
            "infrastructure": blocked(PopulationBlockReason::Infrastructure),
        },
        "canonical_recovery": {"enabled": false, "excluded": true},
        "run_provenance": provenance,
        "manifest": manifest,
    }))
}
