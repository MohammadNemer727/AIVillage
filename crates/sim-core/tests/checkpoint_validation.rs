//! Checkpoints must refuse states that could not have arisen from a real history.

use sim_core::{CheckpointError, ModelVersion, World, WorldCheckpoint, WorldSeed, WorldSettings};

/// Returns a V1 checkpoint, as a TOML table, whose last slot is dead and on the free list.
fn checkpoint_with_free_slot() -> toml::Table {
    let world =
        World::with_settings(WorldSeed::new(9), WorldSettings::default()).expect("valid V1");
    let checkpoint = world.checkpoint().expect("V1 worlds checkpoint");
    let mut table = toml::Table::try_from(&checkpoint).expect("checkpoint encodes");
    let slots = table["slots"].as_array_mut().expect("slot table");
    let last = slots.len() - 1;
    slots[last]["alive"] = toml::Value::Boolean(false);
    table["free_slots"] = toml::Value::Array(vec![toml::Value::Integer(
        i64::try_from(last).expect("slot index fits"),
    )]);
    table
}

fn restore(table: toml::Table) -> Result<World, CheckpointError> {
    let checkpoint: WorldCheckpoint = table.try_into().expect("checkpoint decodes");
    World::from_checkpoint(checkpoint)
}

#[test]
fn valid_free_list_restores() {
    assert!(restore(checkpoint_with_free_slot()).is_ok());
}

#[test]
fn repeated_free_slot_is_rejected() {
    let mut table = checkpoint_with_free_slot();
    let free_slots = table["free_slots"].as_array_mut().expect("free list");
    let first = free_slots[0].clone();
    free_slots.push(first);
    assert_eq!(restore(table).err(), Some(CheckpointError::InvalidFreeSlot));
}

#[test]
fn v2_world_refuses_a_v1_checkpoint() {
    let world = World::new_v2(WorldSeed::new(9));
    let error = world.checkpoint().expect_err("V2 state would be lost");
    assert_eq!(
        error,
        CheckpointError::UnsupportedModel(ModelVersion::V2Physics)
    );
    assert_eq!(
        error.to_string(),
        "checkpoints cannot represent V2Physics worlds yet"
    );
}
