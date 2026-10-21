//! Command-line host for the loopback Luanti bridge.
//!
//! `sim-luanti` serves the bridge; `sim-luanti replay <trace.ndjson>` replays a recorded trace
//! through the founder runtime selected by the same `SIM_LUANTI_SEED` and `SIM_LUANTI_AGENT_ID`.

use std::{fs::OpenOptions, io::Write, path::PathBuf, time::Duration};

use sim_luanti::{Bridge, TraceRecord, bind_loopback, replay_trace, serve_once_with_stall};

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
        let (Some(path), None, "replay") = (args.next(), args.next(), command.as_str()) else {
            return Err("usage: sim-luanti [replay <trace.ndjson>]".into());
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
    let mut bridge = Bridge::new(seed, agent_id);
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
