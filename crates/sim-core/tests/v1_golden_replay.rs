//! V1 freeze golden replay: known seed → tick → `state_hash` checkpoints.
//!
//! Fixture source of truth: `tests/fixtures/v1_golden_replay.toml`.
//! Reproduce via `cargo run -p sim-cli -- golden`.

#![allow(missing_docs, reason = "Integration test crate.")]

use std::{fs, path::PathBuf};

use serde::Deserialize;
use sim_core::{STATE_HASH_VERSION, World, WorldSeed, WorldSettings};

#[derive(Debug, Deserialize)]
struct GoldenFixtureFile {
    schema_version: u32,
    state_hash_version: u32,
    cases: Vec<GoldenCase>,
}

#[derive(Debug, Deserialize)]
struct GoldenCase {
    name: String,
    seed: u64,
    width: u16,
    height: u16,
    initial_agents: u32,
    hashes: Vec<GoldenHash>,
}

#[derive(Debug, Deserialize)]
struct GoldenHash {
    tick: u64,
    state_hash: String,
}

#[test]
fn v1_golden_replay_fixtures_match() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/v1_golden_replay.toml");
    let raw = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("read {}: {error}", path.display());
    });
    let fixtures: GoldenFixtureFile =
        toml::from_str(&raw).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));

    assert_eq!(fixtures.schema_version, 1);
    assert_eq!(fixtures.state_hash_version, STATE_HASH_VERSION);

    for case in fixtures.cases {
        let settings = WorldSettings::new(case.width, case.height, case.initial_agents)
            .unwrap_or_else(|error| panic!("case {}: {error}", case.name));
        let mut world = World::with_settings(WorldSeed::new(case.seed), settings)
            .unwrap_or_else(|error| panic!("case {}: {error}", case.name));
        let mut next_index = 0;
        let max_tick = case
            .hashes
            .iter()
            .map(|entry| entry.tick)
            .max()
            .unwrap_or_else(|| panic!("case {}: empty hashes", case.name));

        while next_index < case.hashes.len() {
            let current_tick = world.tick().get();
            let snapshot = world.snapshot();
            while next_index < case.hashes.len() && case.hashes[next_index].tick == current_tick {
                let expected = &case.hashes[next_index];
                let actual = format!("{:016x}", snapshot.state_hash());
                assert_eq!(
                    expected.state_hash, actual,
                    "case {} tick {}",
                    case.name, expected.tick
                );
                next_index += 1;
            }
            if current_tick >= max_tick {
                break;
            }
            let _ = world.step();
        }

        assert_eq!(
            next_index,
            case.hashes.len(),
            "case {}: unfinished checkpoints",
            case.name
        );
    }
}
