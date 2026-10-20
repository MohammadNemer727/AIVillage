//! Headless command boundary for reproducible AI Village experiments.

#![forbid(unsafe_code)]

use std::{env, fs, path::PathBuf, process, time::Instant};

use serde::Deserialize;
use sim_core::{
    CulturalTransmissionControls, STATE_HASH_VERSION, World, WorldSeed, WorldSettings,
    run_cultural_evolution_experiments, run_physical_social_learning_experiments,
    run_unknown_hazard_experiments,
};
use sim_format::{
    CONFIG_SCHEMA_VERSION, CheckpointFile, SNAPSHOT_SCHEMA_VERSION, V1_FREEZE_LABEL, WorldConfig,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let arguments: Vec<_> = env::args().skip(1).collect();
    let command = arguments.first().map_or("all", String::as_str);
    match command {
        "all" => {
            verify_determinism(1_000)?;
            benchmark_populations(2_000)?;
            soak(20_000)?;
        }
        "determinism" => verify_determinism(1_000)?,
        "population" => benchmark_populations(2_000)?,
        "soak" => soak(20_000)?,
        "golden" => verify_golden_fixtures(false)?,
        "golden-print" => verify_golden_fixtures(true)?,
        "development" => development_controls()?,
        "unknown-hazards" => unknown_hazard_controls()?,
        "social-learning" => social_learning_controls()?,
        "cultural-evolution" => cultural_evolution_controls()?,
        "experiment-report" => replicated_experiment_report(&arguments)?,
        "save" => save_checkpoint(&arguments)?,
        "resume" => resume_checkpoint(&arguments)?,
        "--help" | "-h" | "help" => print_usage(),
        _ => return Err(format!("unknown verification command: {command}")),
    }
    Ok(())
}

fn print_usage() {
    println!(
        "usage: sim-cli [all|determinism|population|soak|golden|golden-print|development|unknown-hazards|social-learning|cultural-evolution|experiment-report|save|resume]\n\
         \n\
         V1 freeze ({V1_FREEZE_LABEL}): config schema {CONFIG_SCHEMA_VERSION}, \
         snapshot schema {SNAPSHOT_SCHEMA_VERSION}, state-hash version {STATE_HASH_VERSION}\n\
         golden        verify tests/fixtures/v1_golden_replay.toml\n\
         golden-print  print computed hashes for regenerating fixtures\n\
         development   run seeded V2 mutation and mutation-disabled replicate controls\n\
         unknown-hazards  run delayed physical-hazard causal-learning gates\n\
         social-learning  run physical demonstrator, cross-fostering, and channel controls\n\
         cultural-evolution  run replacement-cohort tradition and cumulative-practice controls\n\
         experiment-report [path]  write deterministic replicated V2 metrics as TOML (stdout if omitted)\n\
         save <path> [ticks]    write a versioned deterministic checkpoint\n\
         resume <path> [ticks]  load a checkpoint and advance it"
    );
}

/// Runs compact, independently seeded V2 worlds and emits a machine-readable archive.
///
/// This command intentionally consumes only the public headless core API.  The report is a
/// presentation artifact: generating it cannot alter a replicate's future state.
fn replicated_experiment_report(arguments: &[String]) -> Result<(), String> {
    const REPLICATES: u64 = 8;
    const TICKS: u32 = 1_200;
    let settings = WorldSettings::new(24, 24, 48)
        .map_err(|error| format!("invalid report settings: {error}"))?;
    let started = Instant::now();
    let mut rows = Vec::with_capacity(usize::try_from(REPLICATES).expect("replicate count fits"));
    for seed_offset in 0..REPLICATES {
        let seed = 90_000 + seed_offset;
        let mut world = World::with_settings_v2(WorldSeed::new(seed), settings)
            .map_err(|error| format!("create report replicate: {error}"))?;
        for _ in 0..TICKS {
            let _ = world.step();
        }
        let metrics = world.metrics();
        let snapshot = world.snapshot();
        let mean_energy = snapshot
            .agents()
            .iter()
            .map(|agent| u64::from(agent.energy))
            .sum::<u64>()
            / u64::try_from(snapshot.agents().len().max(1)).expect("population fits");
        rows.push((
            seed,
            snapshot.state_hash(),
            metrics.population,
            metrics.births,
            metrics.deaths,
            metrics.diversity_ppm,
            metrics.mean_generation_depth,
            mean_energy,
        ));
    }
    let report = format!(
        "schema_version = 1\nsuite = \"v2-observability-replicates\"\nreplicates = {REPLICATES}\nticks_per_replicate = {TICKS}\nelapsed_millis = {}\n\n{}",
        started.elapsed().as_millis(),
        rows.iter()
            .map(|(seed, hash, population, births, deaths, diversity, depth, energy)| format!(
                "[[replicate]]\nseed = {seed}\nfinal_hash = \"{hash:016x}\"\npopulation = {population}\nbirths = {births}\ndeaths = {deaths}\ndiversity_ppm = {diversity}\nmean_generation_depth = {depth}\nmean_energy = {energy}\n"
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if let Some(path) = arguments.get(1) {
        fs::write(path, &report).map_err(|error| format!("write {path}: {error}"))?;
        println!(
            "experiment-report: wrote {path} ({} replicates)",
            rows.len()
        );
    } else {
        print!("{report}");
    }
    Ok(())
}

fn social_learning_controls() -> Result<(), String> {
    for seed in 0..8 {
        let result = run_physical_social_learning_experiments(WorldSeed::new(seed));
        println!(
            "social-learning: seed={seed} demonstrator={} nonsocial={} cross_fostered={} isolated={} acoustic_ablated={} chemical_ablated={} visual_ablated={}",
            result.demonstrator_acquisition,
            result.nonsocial_acquisition,
            result.cross_fostered_acquisition,
            result.isolated_acquisition,
            result.acoustic_ablated,
            result.chemical_ablated,
            result.visual_ablated,
        );
        if result.demonstrator_acquisition <= result.nonsocial_acquisition
            || result.cross_fostered_acquisition <= result.isolated_acquisition
            || result.acoustic_ablated >= result.demonstrator_acquisition
            || result.chemical_ablated >= result.demonstrator_acquisition
            || result.visual_ablated >= result.demonstrator_acquisition
        {
            return Err(format!(
                "physical social-learning gate failed for seed {seed}"
            ));
        }
    }
    Ok(())
}

fn cultural_evolution_controls() -> Result<(), String> {
    for seed in 0..8 {
        let result = run_cultural_evolution_experiments(WorldSeed::new(seed));
        println!(
            "cultural-evolution: seed={seed} first={:?}/{:?} second={:?}/{:?} isolated={:?} genetic={:?} ecology_reset={:?} rediscovery={:?} cumulative={:?} bottleneck={}",
            result.first_tradition,
            result.first_profile,
            result.second_tradition,
            result.second_profile,
            result.social_isolation,
            result.genetic_inheritance_control,
            result.ecology_reset_control,
            result.individual_rediscovery_control,
            result.cumulative_practice,
            result.bottleneck_practice,
        );
        if result.first_tradition[3] <= result.social_isolation[3]
            || result.second_tradition[3] <= result.social_isolation[3]
            || result.first_profile == result.second_profile
            || result.first_tradition[3] <= result.genetic_inheritance_control[3]
            || result.ecology_reset_control[3] <= result.genetic_inheritance_control[3]
            || result.first_tradition[3] <= result.individual_rediscovery_control[3]
            || result.cumulative_practice[3] <= result.cumulative_practice[0]
            || result.bottleneck_practice <= result.cumulative_practice[0]
        {
            return Err(format!("cultural-evolution gate failed for seed {seed}"));
        }
    }
    Ok(())
}

fn unknown_hazard_controls() -> Result<(), String> {
    for seed in 0..8 {
        let result = run_unknown_hazard_experiments(WorldSeed::new(seed));
        println!(
            "unknown-hazards: seed={seed} mapping={:?} action={} observation={} devaluation={} degraded={} transfer={} shuffled_memory={} shuffled_action={}",
            result.cue_mapping,
            result.action_association,
            result.observation_association,
            result.devaluation_avoidance,
            result.degraded_association,
            result.remapping_transfer,
            result.timing_shuffled_memory_avoidance,
            result.shuffled_action_association,
        );
        if result.action_association <= result.observation_association
            || result.devaluation_avoidance <= 0
            || result.degraded_association >= result.action_association
            || result.remapping_transfer <= 0
            || result.timing_shuffled_memory_avoidance != 0
            || result.shuffled_action_association != 0
        {
            return Err(format!("unknown-hazard gate failed for seed {seed}"));
        }
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "The replicated experiment keeps its printed budget trace and gate together."
)]
fn development_controls() -> Result<(), String> {
    const REPLICATES: u64 = 8;
    const TICKS: u32 = 1_200;
    let settings = WorldSettings::new(20, 20, 24)
        .map_err(|error| format!("invalid development settings: {error}"))?;
    let mut mutated_depth = 0_u64;
    let mut control_depth = 0_u64;
    let mut mutated_diversity = 0_u64;
    let mut control_diversity = 0_u64;
    let mut mutated_births = 0_u64;
    let mut control_births = 0_u64;
    let mut mutated_surviving_descendants = 0_u64;
    let mut control_surviving_descendants = 0_u64;
    for seed in 0..REPLICATES {
        for mutation_enabled in [true, false] {
            let mut world = World::with_settings_v2(WorldSeed::new(50_000 + seed), settings)
                .map_err(|error| format!("create V2 replicate: {error}"))?;
            world.set_transmission_controls(CulturalTransmissionControls {
                genetic_mutation_enabled: mutation_enabled,
                ..CulturalTransmissionControls::default()
            });
            for tick in 1..=TICKS {
                let snapshot = world.step();
                if (tick == 1
                    || tick == 10
                    || tick == 40
                    || tick == 80
                    || tick == 120
                    || tick == 160
                    || tick == 200)
                    && seed == 0
                    && mutation_enabled
                {
                    let physiology: Vec<_> = snapshot
                        .agents()
                        .iter()
                        .filter_map(|agent| agent.physiology)
                        .collect();
                    let mean_energy = physiology
                        .iter()
                        .map(|state| u64::from(state.energy))
                        .sum::<u64>()
                        / u64::try_from(physiology.len().max(1)).expect("population fits");
                    let mean_gut = physiology
                        .iter()
                        .map(|state| u64::from(state.gut))
                        .sum::<u64>()
                        / u64::try_from(physiology.len().max(1)).expect("population fits");
                    let open_mouths = snapshot
                        .agents()
                        .iter()
                        .filter(|agent| agent.body.is_some_and(|body| body.mouth_open.raw() > 0))
                        .count();
                    let plant_total = snapshot.ecology().map_or(0, |ecology| {
                        ecology.plants.iter().map(|plant| u64::from(*plant)).sum()
                    });
                    let mean_toxin = physiology
                        .iter()
                        .map(|state| u64::from(state.toxin))
                        .sum::<u64>()
                        / u64::try_from(physiology.len().max(1)).expect("population fits");
                    let mean_torso = physiology
                        .iter()
                        .map(|state| u64::from(state.tissue[1]))
                        .sum::<u64>()
                        / u64::try_from(physiology.len().max(1)).expect("population fits");
                    let mean_hydration = physiology
                        .iter()
                        .map(|state| u64::from(state.hydration))
                        .sum::<u64>()
                        / u64::try_from(physiology.len().max(1)).expect("population fits");
                    let mean_fat = physiology
                        .iter()
                        .map(|state| u64::from(state.fat))
                        .sum::<u64>()
                        / u64::try_from(physiology.len().max(1)).expect("population fits");
                    let mean_reserve = physiology
                        .iter()
                        .map(|state| u64::from(state.reproductive_reserve))
                        .sum::<u64>()
                        / u64::try_from(physiology.len().max(1)).expect("population fits");
                    println!(
                        "  development diagnosis tick={tick} population={} mean_energy={mean_energy} mean_gut={mean_gut} mean_toxin={mean_toxin} mean_torso={mean_torso} mean_hydration={mean_hydration} mean_fat={mean_fat} mean_reserve={mean_reserve} open_mouths={open_mouths} plants={plant_total}",
                        physiology.len(),
                    );
                }
            }
            let metrics = world.metrics();
            let surviving_descendants = world
                .snapshot()
                .agents()
                .iter()
                .filter(|agent| agent.lineage_depth > 0)
                .count() as u64;
            println!(
                "  development replicate seed={seed} mutation={mutation_enabled} population={} births={} deaths={} depth={} diversity_ppm={}",
                metrics.population,
                metrics.births,
                metrics.deaths,
                metrics.mean_generation_depth,
                metrics.diversity_ppm,
            );
            if mutation_enabled {
                mutated_depth += u64::from(metrics.mean_generation_depth);
                mutated_diversity += u64::from(metrics.diversity_ppm);
                mutated_births += metrics.births;
                mutated_surviving_descendants += surviving_descendants;
            } else {
                control_depth += u64::from(metrics.mean_generation_depth);
                control_diversity += u64::from(metrics.diversity_ppm);
                control_births += metrics.births;
                control_surviving_descendants += surviving_descendants;
            }
        }
    }
    let mutation_depth_mean = mutated_depth / REPLICATES;
    let control_depth_mean = control_depth / REPLICATES;
    let mutation_diversity_mean = mutated_diversity / REPLICATES;
    let control_diversity_mean = control_diversity / REPLICATES;
    println!(
        "development: replicates={REPLICATES} ticks={TICKS} mutation_births={mutated_births} control_births={control_births} mutation_surviving_descendants={mutated_surviving_descendants} control_surviving_descendants={control_surviving_descendants} mutation_depth_mean={mutation_depth_mean} control_depth_mean={control_depth_mean} mutation_diversity_ppm_mean={mutation_diversity_mean} control_diversity_ppm_mean={control_diversity_mean}",
    );
    if mutated_births == 0
        || control_births == 0
        || mutated_surviving_descendants == 0
        || control_surviving_descendants == 0
        || mutation_diversity_mean <= control_diversity_mean
    {
        return Err(
            "development gate failed: require natural births, nonzero lineage depth, and mutation-driven diversity above mutation-disabled controls"
                .to_owned(),
        );
    }
    Ok(())
}

fn configured_world(seed: u64, settings: WorldSettings) -> World {
    let config = WorldConfig::default();
    let mut world = World::with_settings(WorldSeed::new(seed), settings)
        .expect("validated benchmark settings are valid");
    world.set_transmission_controls(CulturalTransmissionControls {
        genetic_mutation_enabled: config.transmission.genetic_mutation_enabled,
        plasticity_enabled: config.transmission.plasticity_enabled,
        signals_enabled: config.transmission.signals_enabled,
        artifacts_enabled: config.transmission.artifacts_enabled,
        physical_social_signals_enabled: config.transmission.physical_social_signals_enabled,
    });
    world
}

fn verify_determinism(ticks: u32) -> Result<(), String> {
    let settings =
        WorldSettings::new(32, 32, 128).expect("literal verification settings are valid");
    let mut first = configured_world(0, settings);
    let mut second = configured_world(0, settings);
    for tick in 1..=ticks {
        let first_snapshot = first.step();
        let second_snapshot = second.step();
        if first_snapshot != second_snapshot {
            return Err(format!("determinism check diverged at tick {tick}"));
        }
    }
    println!(
        "determinism: {ticks} matching ticks, hash={:016x}",
        first.snapshot().state_hash()
    );
    Ok(())
}

fn benchmark_populations(ticks: u32) -> Result<(), String> {
    for (population, side) in [(128, 32), (500, 48), (2_000, 64)] {
        let settings = WorldSettings::new(side, side, population)
            .map_err(|error| format!("invalid population benchmark settings: {error}"))?;
        let mut world = configured_world(u64::from(population), settings);
        let started = Instant::now();
        for _ in 0..ticks {
            let _ = world.step();
        }
        let elapsed = started.elapsed();
        let ticks_per_second = f64::from(ticks) / elapsed.as_secs_f64();
        println!(
            "population: requested={population}, living={}, ticks={ticks}, elapsed={elapsed:.3?}, ticks_per_second={ticks_per_second:.1}",
            world.agent_count()
        );
    }
    Ok(())
}

fn soak(ticks: u32) -> Result<(), String> {
    let settings = WorldSettings::new(64, 64, 2_000)
        .map_err(|error| format!("invalid soak settings: {error}"))?;
    let mut world = configured_world(0, settings);
    let initial_hash = world.snapshot().state_hash();
    let started = Instant::now();
    for _ in 0..ticks {
        let _ = world.step();
    }
    println!(
        "soak: ticks={ticks}, elapsed={:.3?}, living={}, initial_hash={initial_hash:016x}, final_hash={:016x}",
        started.elapsed(),
        world.agent_count(),
        world.snapshot().state_hash()
    );
    Ok(())
}

fn checkpoint_arguments(arguments: &[String]) -> Result<(&str, u32), String> {
    let path = arguments
        .get(1)
        .ok_or_else(|| "checkpoint path is required".to_owned())?;
    let ticks = arguments.get(2).map_or(Ok(0), |value| {
        value.parse::<u32>().map_err(|error| error.to_string())
    })?;
    Ok((path, ticks))
}

fn save_checkpoint(arguments: &[String]) -> Result<(), String> {
    let (path, ticks) = checkpoint_arguments(arguments)?;
    let settings = WorldSettings::default();
    let mut world = configured_world(0, settings);
    for _ in 0..ticks {
        let _ = world.step();
    }
    let checkpoint = CheckpointFile::v1(&world);
    fs::write(
        path,
        checkpoint.to_toml().map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("write {path}: {error}"))?;
    println!(
        "saved: path={path}, tick={}, hash={:016x}",
        world.tick(),
        world.snapshot().state_hash()
    );
    Ok(())
}

fn resume_checkpoint(arguments: &[String]) -> Result<(), String> {
    let (path, ticks) = checkpoint_arguments(arguments)?;
    let raw = fs::read_to_string(path).map_err(|error| format!("read {path}: {error}"))?;
    let mut world = CheckpointFile::from_toml(&raw)
        .and_then(CheckpointFile::restore)
        .map_err(|error| format!("load {path}: {error}"))?;
    for _ in 0..ticks {
        let _ = world.step();
    }
    println!(
        "resumed: path={path}, tick={}, hash={:016x}",
        world.tick(),
        world.snapshot().state_hash()
    );
    Ok(())
}

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
    #[serde(default)]
    state_hash: String,
}

fn fixture_path() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../tests/fixtures/v1_golden_replay.toml");
    path.canonicalize()
        .or_else(|_| {
            if path.exists() {
                Ok(path.clone())
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    path.display().to_string(),
                ))
            }
        })
        .map_err(|error| format!("golden fixture path: {error}"))
}

fn verify_golden_fixtures(print_only: bool) -> Result<(), String> {
    let path = fixture_path()?;
    let raw =
        fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let fixtures: GoldenFixtureFile =
        toml::from_str(&raw).map_err(|error| format!("parse {}: {error}", path.display()))?;

    if fixtures.schema_version != CONFIG_SCHEMA_VERSION {
        return Err(format!(
            "fixture schema_version {} != CONFIG_SCHEMA_VERSION {CONFIG_SCHEMA_VERSION}",
            fixtures.schema_version
        ));
    }
    if fixtures.state_hash_version != STATE_HASH_VERSION {
        return Err(format!(
            "fixture state_hash_version {} != STATE_HASH_VERSION {STATE_HASH_VERSION}",
            fixtures.state_hash_version
        ));
    }

    println!(
        "golden: {} ({V1_FREEZE_LABEL}) schema={CONFIG_SCHEMA_VERSION} state_hash_version={STATE_HASH_VERSION}",
        path.display()
    );

    for case in &fixtures.cases {
        let settings = WorldSettings::new(case.width, case.height, case.initial_agents)
            .map_err(|error| format!("case {}: invalid settings: {error}", case.name))?;
        let mut world = configured_world(case.seed, settings);
        let mut next_index = 0;
        let max_tick = case
            .hashes
            .iter()
            .map(|entry| entry.tick)
            .max()
            .ok_or_else(|| format!("case {}: no hash checkpoints", case.name))?;

        while next_index < case.hashes.len() {
            let current_tick = world.tick().get();
            let snapshot = world.snapshot();
            while next_index < case.hashes.len() && case.hashes[next_index].tick == current_tick {
                let expected = &case.hashes[next_index];
                let actual = format!("{:016x}", snapshot.state_hash());
                if print_only {
                    println!(
                        "  [{name}] tick={tick} state_hash={actual}",
                        name = case.name,
                        tick = expected.tick
                    );
                } else if expected.state_hash.is_empty() {
                    return Err(format!(
                        "case {}: tick {} missing expected state_hash (run golden-print)",
                        case.name, expected.tick
                    ));
                } else if expected.state_hash != actual {
                    return Err(format!(
                        "case {}: tick {} hash mismatch: expected {}, got {actual}",
                        case.name, expected.tick, expected.state_hash
                    ));
                } else {
                    println!("  ok {} tick={} hash={actual}", case.name, expected.tick);
                }
                next_index += 1;
            }
            if current_tick >= max_tick {
                break;
            }
            let _ = world.step();
        }

        if next_index != case.hashes.len() {
            return Err(format!(
                "case {}: only verified {next_index}/{} checkpoints",
                case.name,
                case.hashes.len()
            ));
        }
    }

    if !print_only {
        println!("golden: all cases matched");
    }
    Ok(())
}
