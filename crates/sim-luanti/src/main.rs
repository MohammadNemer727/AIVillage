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

use sim_core::runtime::{AffordanceConfig, EpisodicConfig, ExperimentMode};
use sim_format::{PopulationBlockReason, PopulationLifecycleEvent};
use sim_luanti::{
    Bridge, LifecycleConfig, PopulationCoordinator, TraceRecord, benchmark, bind_loopback,
    contingency, episodic_eval, foraging, load_state, prediction_eval, reaching, replay_trace,
    save_state, serve_once_concurrent,
};

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
    let records = std::fs::read_to_string(path)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<Vec<TraceRecord>, _>>()?;
    replay_trace(seed, agent_id, &records)?;
    println!("replayed {} recorded transitions exactly", records.len());
    Ok(())
}

fn prediction_evaluation(
    seed: u64,
    agent_id: u64,
    path: &str,
    output: Option<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let records: Vec<TraceRecord> = std::fs::read_to_string(path)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<Vec<TraceRecord>, _>>()?;
    let records: Vec<_> = records
        .into_iter()
        .filter(|record| record.request.agent_id == agent_id)
        .collect();
    let report = prediction_eval::evaluate(seed, &records)?;
    let json = serde_json::to_vec_pretty(&report)?;
    if let Some(output) = output {
        std::fs::write(output, json)?;
    } else {
        println!("{}", String::from_utf8(json)?);
    }
    Ok(())
}

fn episodic_evaluation(
    seed: u64,
    agent_id: u64,
    path: &str,
    output: Option<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let records: Vec<TraceRecord> = std::fs::read_to_string(path)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<Vec<TraceRecord>, _>>()?;
    let records: Vec<_> = records
        .into_iter()
        .filter(|record| record.request.agent_id == agent_id)
        .collect();
    let report = episodic_eval::evaluate(seed, &records)?;
    let json = serde_json::to_vec_pretty(&report)?;
    if let Some(output) = output {
        std::fs::write(output, json)?;
    } else {
        println!("{}", String::from_utf8(json)?);
    }
    Ok(())
}

/// The founder the bridge serves: the one saved at `state_path` (`SIM_LUANTI_STATE`, where its brain
/// is kept between sessions) if there is one, continuing with everything it learned in the mode
/// it was saved in; otherwise a newborn in the mode `SIM_LUANTI_MODE` selects.
fn founder_bridge(
    seed: u64,
    agent_id: u64,
    founder_count: u32,
    population_capacity: u32,
    state_path: Option<&std::path::Path>,
    population_path: Option<&std::path::Path>,
    lifecycle_config: LifecycleConfig,
) -> Result<Bridge, Box<dyn std::error::Error + Send + Sync>> {
    if let Some(path) = population_path
        && path.join("population.json").exists()
    {
        let mut population = PopulationCoordinator::load(path)?;
        if let Some(config) = episodic_config()? {
            population.set_episodic_config(config);
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
        Ok("baseline") | Err(_) => ExperimentMode::PredictiveBaseline,
        Ok(value) => {
            return Err(format!(
                "SIM_LUANTI_MODE must be baseline, affordance or homeostatic, got {value}"
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
                    settings,
                )?
            } else {
                Bridge::new_with_affordance(seed, agent_id, settings)?
            }
        }
        Ok(_) => return Err("SIM_LUANTI_AFFORDANCE requires SIM_LUANTI_MODE=affordance".into()),
        Err(_) if population_mode => Bridge::new_population_with_mode(
            seed,
            agent_id,
            founder_count,
            population_capacity,
            mode,
        )?,
        Err(_) => Bridge::new_with_mode(seed, agent_id, mode),
    };
    if let Some(path) = state_path
        && let Some(mut runtime) = load_state(path, seed, agent_id)?
    {
        if let Some(config) = episodic_config()? {
            runtime.set_episodic_config(config);
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
    if let Some(path) = population_path {
        bridge.set_lifecycle_config(lifecycle_config)?;
        bridge.bind_storage(path)?;
    }
    Ok(bridge)
}

fn episodic_config() -> Result<Option<EpisodicConfig>, Box<dyn std::error::Error + Send + Sync>> {
    let config = match std::env::var("SIM_LUANTI_EPISODIC").as_deref() {
        Err(_) => return Ok(None),
        Ok("off") => EpisodicConfig::default(),
        Ok("prediction") => EpisodicConfig {
            prediction_enabled: true,
            policy_enabled: false,
        },
        Ok("policy") => EpisodicConfig {
            prediction_enabled: false,
            policy_enabled: true,
        },
        Ok("both") => EpisodicConfig {
            prediction_enabled: true,
            policy_enabled: true,
        },
        Ok(value) => {
            return Err(format!(
                "SIM_LUANTI_EPISODIC must be off, prediction, policy, or both; got {value}"
            )
            .into());
        }
    };
    Ok(Some(config))
}

struct ServerConfig {
    port: u16,
    seed: u64,
    agent_id: u64,
    founder_count: u32,
    population_capacity: u32,
    lifecycle_config: LifecycleConfig,
}

fn server_config() -> Result<ServerConfig, Box<dyn std::error::Error + Send + Sync>> {
    let founder_count = std::env::var("SIM_LUANTI_FOUNDER_COUNT")
        .map_or(Ok(1), |value| value.parse::<u32>())
        .map_err(|_| "SIM_LUANTI_FOUNDER_COUNT must be a positive integer")?;
    if founder_count == 0 {
        return Err("SIM_LUANTI_FOUNDER_COUNT must be a positive integer".into());
    }
    let population_capacity = std::env::var("SIM_LUANTI_POPULATION_CAPACITY")
        .map_or(Ok(founder_count), |value| value.parse::<u32>())
        .map_err(|_| "SIM_LUANTI_POPULATION_CAPACITY must be a positive integer")?;
    let number =
        |name: &str, default: u64| -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
            Ok(std::env::var(name).map_or(Ok(default), |value| value.parse::<u64>())?)
        };
    let mutation_enabled = match std::env::var("SIM_LUANTI_MUTATION").as_deref() {
        Ok("enabled") | Err(_) => true,
        Ok("disabled") => false,
        Ok(_) => return Err("SIM_LUANTI_MUTATION must be enabled or disabled".into()),
    };
    Ok(ServerConfig {
        port: std::env::var("SIM_LUANTI_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(8_481),
        seed: std::env::var("SIM_LUANTI_SEED")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1),
        agent_id: std::env::var("SIM_LUANTI_AGENT_ID")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1),
        founder_count,
        population_capacity,
        lifecycle_config: LifecycleConfig {
            maturity_decisions: number("SIM_LUANTI_REPRODUCTIVE_MATURITY", 100)?,
            minimum_reproductive_reserve: u16::try_from(number(
                "SIM_LUANTI_MINIMUM_REPRODUCTIVE_RESERVE",
                160,
            )?)?,
            reproductive_cost: u16::try_from(number("SIM_LUANTI_REPRODUCTIVE_COST", 80)?)?,
            mutation_enabled,
        },
    })
}

fn stall_config() -> Option<(u64, Duration)> {
    std::env::var("SIM_LUANTI_STALL_SEQUENCE")
        .ok()?
        .parse()
        .ok()
        .map(|sequence| {
            let milliseconds = std::env::var("SIM_LUANTI_STALL_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(750);
            (sequence, Duration::from_millis(milliseconds))
        })
}

#[allow(clippy::too_many_lines)]
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ServerConfig {
        port,
        seed,
        agent_id,
        founder_count,
        population_capacity,
        lifecycle_config,
    } = server_config()?;
    let mut args = std::env::args().skip(1);
    if let Some(command) = args.next() {
        if command == "prediction-eval" {
            let path = args
                .next()
                .ok_or("usage: sim-luanti prediction-eval <trace.ndjson> [report.json]")?;
            let output = args.next();
            if args.next().is_some() {
                return Err(
                    "usage: sim-luanti prediction-eval <trace.ndjson> [report.json]".into(),
                );
            }
            return prediction_evaluation(seed, agent_id, &path, output);
        }
        if command == "episodic-eval" {
            let path = args
                .next()
                .ok_or("usage: sim-luanti episodic-eval <trace.ndjson> [report.json]")?;
            let output = args.next();
            if args.next().is_some() {
                return Err("usage: sim-luanti episodic-eval <trace.ndjson> [report.json]".into());
            }
            return episodic_evaluation(seed, agent_id, &path, output);
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
        let (Some(path), None, "replay") = (args.next(), args.next(), command.as_str()) else {
            return Err(
                "usage: sim-luanti [replay <trace.ndjson>|prediction-eval|episodic-eval <trace.ndjson> [report.json]|benchmark|contingency|foraging|reaching [report.json] [report.md]]"
                    .into(),
            );
        };
        return replay(seed, agent_id, &path);
    }
    let server = bind_loopback(port)?;
    eprintln!("sim-luanti listening on 127.0.0.1:{port}");
    let state_path = std::env::var_os("SIM_LUANTI_STATE").map(PathBuf::from);
    let population_path = std::env::var_os("SIM_LUANTI_POPULATION").map(PathBuf::from);
    let bridge = founder_bridge(
        seed,
        agent_id,
        founder_count,
        population_capacity,
        state_path.as_deref(),
        population_path.as_deref(),
        lifecycle_config,
    )?;
    if population_path.is_some() {
        eprintln!(
            "sim-luanti population: {} live individuals, capacity {}",
            bridge.manifest().individuals.len(),
            bridge.manifest().capacity
        );
    }
    let mut saved_at = bridge.total_decisions();
    let trace_path = std::env::var_os("SIM_LUANTI_TRACE").map(PathBuf::from);
    let stall_sequence = stall_config();
    let bridge = Arc::new(Mutex::new(bridge));
    let stalled = Arc::new(AtomicBool::new(false));
    let experiment_report = std::env::var_os("SIM_LUANTI_EXPERIMENT_001_REPORT").map(PathBuf::from);
    let run_provenance = std::env::var_os("SIM_LUANTI_RUN_PROVENANCE").map(PathBuf::from);
    let experiment_target = std::env::var("SIM_LUANTI_EXPERIMENT_001_BIRTHS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100_u64);
    let experiment_deadline = Duration::from_secs(
        std::env::var("SIM_LUANTI_EXPERIMENT_001_DEADLINE_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(86_400),
    );
    let experiment_started = Instant::now();
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
        if let Some(path) = &population_path {
            let decisions = bridge.total_decisions();
            if decisions >= saved_at + SAVE_EVERY_DECISIONS {
                bridge.save(path)?;
                saved_at = decisions;
            }
        } else if let Some(path) = &state_path {
            let decisions = bridge.total_decisions();
            if decisions >= saved_at + SAVE_EVERY_DECISIONS {
                save_state(path, bridge.runtime())?;
                saved_at = decisions;
            }
        }
        if let Some(path) = &trace_path {
            let mut file = OpenOptions::new().create(true).append(true).open(path)?;
            for record in bridge.drain_trace() {
                let mut line = serde_json::to_vec(&record)?;
                line.push(b'\n');
                file.write_all(&line)?;
            }
        } else {
            let _ = bridge.drain_trace();
        }
        if let Some(path) = &experiment_report {
            let births = bridge
                .lifecycle_events()
                .iter()
                .filter(|event| matches!(event, PopulationLifecycleEvent::Birth { .. }))
                .count() as u64;
            let live = bridge.manifest().individuals.len();
            let terminal = if births >= experiment_target {
                Some("birth_target")
            } else if live == 0 {
                Some("natural_extinction")
            } else if experiment_started.elapsed() >= experiment_deadline {
                Some("deadline")
            } else {
                None
            };
            if let Some(stop_reason) = terminal {
                let attempts = bridge
                    .lifecycle_events()
                    .iter()
                    .filter(|event| {
                        matches!(event, PopulationLifecycleEvent::ReproductionAttempt { .. })
                    })
                    .count() as u64;
                let blocked = |reason| {
                    bridge
                        .lifecycle_events()
                        .iter()
                        .filter(|event| {
                            matches!(
                                event,
                                PopulationLifecycleEvent::ReproductionBlocked {
                                    reason: found,
                                    ..
                                } if *found == reason
                            )
                        })
                        .count()
                };
                bridge.save(
                    population_path
                        .as_deref()
                        .ok_or("Experiment 001 requires SIM_LUANTI_POPULATION")?,
                )?;
                let provenance = run_provenance
                    .as_ref()
                    .map(std::fs::read)
                    .transpose()?
                    .map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes))
                    .transpose()?;
                let report = serde_json::json!({
                    "experiment": "001-autonomous-generational-turnover",
                    "stop_reason": stop_reason,
                    "birth_target": experiment_target,
                    "births": births,
                    "controller_reproduction_attempts": attempts,
                    "live_individuals": live,
                    "elapsed_seconds": experiment_started.elapsed().as_secs(),
                    "mutation": if bridge.manifest().mutation_enabled { "enabled" } else { "disabled" },
                    "blocked": {
                        "no_free_slot": blocked(PopulationBlockReason::NoFreeSlot),
                        "biological_eligibility": blocked(PopulationBlockReason::BiologicalEligibility),
                        "infrastructure": blocked(PopulationBlockReason::Infrastructure),
                    },
                    "canonical_recovery": {"enabled": false, "excluded": true},
                    "run_provenance": provenance,
                    "manifest": bridge.manifest(),
                });
                std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
                println!(
                    "Experiment 001 stopped at {stop_reason}; wrote {}",
                    path.display()
                );
                return Ok(());
            }
        }
    }
}
