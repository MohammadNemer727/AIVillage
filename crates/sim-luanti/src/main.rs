//! Command-line host for the loopback Luanti bridge.
//!
//! `sim-luanti` serves the bridge; `sim-luanti replay <trace.ndjson>` replays a recorded trace
//! through the founder runtime selected by the same `SIM_LUANTI_SEED` and `SIM_LUANTI_AGENT_ID`.

use std::{fs::OpenOptions, io::Write, path::PathBuf, time::Duration};

use sim_core::runtime::{AffordanceConfig, ExperimentMode};
use sim_luanti::{
    Bridge, TraceRecord, benchmark, bind_loopback, contingency, reaching, replay_trace,
    serve_once_with_stall,
};

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
            "usage: sim-luanti benchmark|contingency|reaching [report.json] [report.md]".into(),
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

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port = std::env::var("SIM_LUANTI_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8_481);
    let seed = std::env::var("SIM_LUANTI_SEED")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let agent_id = std::env::var("SIM_LUANTI_AGENT_ID")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let mut args = std::env::args().skip(1);
    if let Some(command) = args.next() {
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
        if command == "contingency" {
            let report = contingency::run();
            let markdown = contingency::markdown(&report);
            return write_report(args, "contingency-test", &report, &markdown);
        }
        let (Some(path), None, "replay") = (args.next(), args.next(), command.as_str()) else {
            return Err(
                "usage: sim-luanti [replay <trace.ndjson>|benchmark|contingency|reaching [report.json] [report.md]]"
                    .into(),
            );
        };
        return replay(seed, agent_id, &path);
    }
    let server = bind_loopback(port)?;
    eprintln!("sim-luanti listening on 127.0.0.1:{port}");
    let mode = match std::env::var("SIM_LUANTI_MODE").as_deref() {
        Ok("affordance") => ExperimentMode::AffordanceDiscovery,
        Ok("baseline") | Err(_) => ExperimentMode::PredictiveBaseline,
        Ok(value) => {
            return Err(
                format!("SIM_LUANTI_MODE must be baseline or affordance, got {value}").into(),
            );
        }
    };
    eprintln!("sim-luanti mode: {mode:?}");
    // Optional affordance learner settings as JSON; omitted fields keep their defaults, e.g.
    // SIM_LUANTI_AFFORDANCE='{"rule":"LearningProgress","action_horizon":16}'.
    let mut bridge = match std::env::var("SIM_LUANTI_AFFORDANCE") {
        Ok(json) if mode == ExperimentMode::AffordanceDiscovery => {
            let settings: AffordanceConfig = serde_json::from_str(&json)
                .map_err(|error| format!("SIM_LUANTI_AFFORDANCE is not valid JSON: {error}"))?;
            eprintln!("sim-luanti affordance settings: {settings:?}");
            Bridge::new_with_affordance(seed, agent_id, settings)?
        }
        Ok(_) => return Err("SIM_LUANTI_AFFORDANCE requires SIM_LUANTI_MODE=affordance".into()),
        Err(_) => Bridge::new_with_mode(seed, agent_id, mode),
    };
    let trace_path = std::env::var_os("SIM_LUANTI_TRACE").map(PathBuf::from);
    let stall_sequence = std::env::var("SIM_LUANTI_STALL_SEQUENCE")
        .ok()
        .and_then(|value| value.parse().ok())
        .map(|sequence| {
            let milliseconds = std::env::var("SIM_LUANTI_STALL_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(750);
            (sequence, Duration::from_millis(milliseconds))
        });
    let mut stalled = false;
    loop {
        serve_once_with_stall(
            &server,
            &mut bridge,
            Duration::from_millis(100),
            stall_sequence,
            &mut stalled,
        )?;
        if let Some(path) = &trace_path {
            let mut file = OpenOptions::new().create(true).append(true).open(path)?;
            for record in bridge.drain_trace() {
                serde_json::to_writer(&mut file, &record)?;
                file.write_all(b"\n")?;
            }
        } else {
            let _ = bridge.drain_trace();
        }
    }
}
