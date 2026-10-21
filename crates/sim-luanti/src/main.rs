//! Command-line host for the loopback Luanti bridge.
//!
//! `sim-luanti` serves the bridge; `sim-luanti replay <trace.ndjson>` replays a recorded trace
//! through the founder runtime selected by the same `SIM_LUANTI_SEED` and `SIM_LUANTI_AGENT_ID`.

use std::{fs::OpenOptions, io::Write, path::PathBuf, time::Duration};

use sim_core::runtime::ExperimentMode;
use sim_luanti::{
    Bridge, TraceRecord, benchmark, bind_loopback, replay_trace, serve_once_with_stall,
};

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
            let json = args.next().map_or_else(
                || PathBuf::from("target/affordance-benchmark.json"),
                PathBuf::from,
            );
            let markdown = args.next().map_or_else(
                || PathBuf::from("docs/affordance-benchmark.md"),
                PathBuf::from,
            );
            if args.next().is_some() {
                return Err("usage: sim-luanti benchmark [report.json] [report.md]".into());
            }
            let report = benchmark::run();
            std::fs::write(&json, serde_json::to_vec_pretty(&report)?)?;
            std::fs::write(&markdown, benchmark::markdown(&report))?;
            println!("wrote {} and {}", json.display(), markdown.display());
            return Ok(());
        }
        let (Some(path), None, "replay") = (args.next(), args.next(), command.as_str()) else {
            return Err(
                "usage: sim-luanti [replay <trace.ndjson>|benchmark [report.json] [report.md]]"
                    .into(),
            );
        };
        let records = std::fs::read_to_string(&path)?
            .lines()
            .map(serde_json::from_str)
            .collect::<Result<Vec<TraceRecord>, _>>()?;
        replay_trace(seed, agent_id, &records)?;
        println!("replayed {} recorded transitions exactly", records.len());
        return Ok(());
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
    let mut bridge = Bridge::new_with_mode(seed, agent_id, mode);
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
