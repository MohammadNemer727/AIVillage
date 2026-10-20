//! Sensing and thinking run in parallel; histories must not depend on the thread count.

use sim_core::{SignalIntervention, World, WorldSeed, WorldSettings};

fn history(threads: usize, build: fn() -> World) -> Vec<u64> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("test thread pool builds");
    pool.install(|| {
        let mut world = build();
        (0..300).map(|_| world.step().state_hash()).collect()
    })
}

fn settings() -> WorldSettings {
    WorldSettings::new(24, 24, 96).expect("valid test settings")
}

#[test]
fn v1_history_is_independent_of_thread_count() {
    let build = || {
        let mut world = World::with_settings(WorldSeed::new(31), settings()).expect("valid V1");
        world.set_signal_intervention(SignalIntervention::Shuffle);
        world
    };
    assert_eq!(history(1, build), history(8, build));
}

#[test]
fn v2_history_is_independent_of_thread_count() {
    let build = || World::with_settings_v2(WorldSeed::new(32), settings()).expect("valid V2");
    assert_eq!(history(1, build), history(8, build));
}
