//! Interactive, renderer-only view of the deterministic AI Village simulation.
//!
//! The [`sim_core::World`] resource owns all simulation truth. Bevy only retains adjacent
//! snapshots, interpolates their positions, and presents controls and diagnostics.

#![forbid(unsafe_code)]
#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems must receive ECS parameters by value."
)]

use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    input::mouse::MouseWheel,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    winit::{UpdateMode, WinitSettings},
};
use sim_core::{
    AgentId, AgentSnapshot, CulturalTransmissionControls, FIXED_SCALE, World, WorldSeed,
    WorldSnapshot,
};

const TICKS_PER_SECOND: f64 = 20.0;
const CELL_SIZE: f32 = 6.0;
const AGENT_SIZE: f32 = 4.25;
const CAMERA_SPEED: f32 = 520.0;
const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 4.0;
const HISTORY_SAMPLES: usize = 180;
/// Redraw ceiling while paused and focused; keeps held-key panning usable without spinning at vsync.
const PAUSED_FOCUSED_WAIT: Duration = Duration::from_millis(33);
/// Redraw ceiling while paused and unfocused.
const PAUSED_UNFOCUSED_WAIT: Duration = Duration::from_secs(5);

/// Application lifecycle states retained from the Bevy 2D template pattern.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, States)]
enum AppState {
    /// Interactive simulation display.
    #[default]
    Running,
    /// Interactive display with authoritative ticks paused.
    Paused,
    /// Playback of persisted experiment data.
    #[allow(
        dead_code,
        reason = "Replay mode is reserved for sim-format-backed playback, which is intentionally out of scope."
    )]
    Replay,
}

/// Bevy-side bridge around the renderer-independent simulation authority.
#[derive(Resource)]
struct SimulationBridge {
    world: World,
    previous: WorldSnapshot,
    current: WorldSnapshot,
    paused: bool,
    speed: u8,
}

impl SimulationBridge {
    fn new() -> Self {
        let world = World::new_v2(WorldSeed::new(0xA171_1A6E));
        let snapshot = world.snapshot();
        Self {
            world,
            previous: snapshot.clone(),
            current: snapshot,
            paused: false,
            speed: 1,
        }
    }
}

#[derive(Component)]
struct VillageAgent {
    id: AgentId,
}

#[derive(Resource, Default)]
struct AgentEntities(HashMap<AgentId, [Entity; 4]>);

#[derive(Clone, Copy, Component)]
enum AnimalPart {
    Torso,
    Head,
    LeftLeg,
    RightLeg,
}

#[derive(Resource, Default)]
struct Inspector {
    selected: Option<AgentId>,
}

#[derive(Resource)]
struct History {
    last_tick: u64,
    population: VecDeque<u32>,
    mean_energy: VecDeque<u16>,
    lineage_depth: VecDeque<u32>,
}

impl Default for History {
    fn default() -> Self {
        Self {
            last_tick: 0,
            population: VecDeque::with_capacity(HISTORY_SAMPLES),
            mean_energy: VecDeque::with_capacity(HISTORY_SAMPLES),
            lineage_depth: VecDeque::with_capacity(HISTORY_SAMPLES),
        }
    }
}

#[derive(Resource, Default)]
struct OverlaySettings {
    sensors: bool,
    trajectories: bool,
}

#[derive(Resource, Default)]
struct BlockEntities(HashMap<usize, Entity>);

#[derive(Component)]
struct DiagnosticsText;

#[derive(Component)]
struct ControlHintText;

#[derive(Component)]
struct InspectorText;

#[derive(Component)]
struct PlotText;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "AI Village — Deterministic Live View".to_owned(),
                resolution: (1280, 800).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .insert_resource(WinitSettings::game())
        .insert_resource(ClearColor(Color::srgb(0.02, 0.04, 0.07)))
        .init_state::<AppState>()
        .insert_resource(Time::<Fixed>::from_hz(TICKS_PER_SECOND))
        .insert_resource(SimulationBridge::new())
        .init_resource::<AgentEntities>()
        .init_resource::<BlockEntities>()
        .init_resource::<Inspector>()
        .init_resource::<History>()
        .init_resource::<OverlaySettings>()
        .add_systems(Startup, setup_scene)
        .add_systems(OnEnter(AppState::Paused), enter_low_power_redraw)
        .add_systems(OnExit(AppState::Paused), exit_low_power_redraw)
        .add_systems(
            FixedUpdate,
            advance_authoritative_world.run_if(in_state(AppState::Running)),
        )
        .add_systems(
            Update,
            (
                simulation_controls,
                camera_controls,
                sync_agent_entities,
                sync_block_entities,
                interpolate_agents,
                select_agent,
                update_history,
                draw_overlays,
                update_diagnostics,
                update_control_hint,
                update_inspector,
                update_plots,
                maybe_capture_verify_screenshot,
            )
                .run_if(state_is_interactive),
        )
        .run();
}

fn setup_scene(mut commands: Commands, bridge: Res<SimulationBridge>) {
    let settings = bridge.world.settings();
    let width = f32::from(settings.width()) * CELL_SIZE;
    let height = f32::from(settings.height()) * CELL_SIZE;

    commands.spawn((
        Camera2d,
        Transform::from_xyz(width * 0.5, height * 0.5, 10.0),
    ));
    commands.spawn((
        Sprite::from_color(Color::srgb(0.025, 0.045, 0.075), Vec2::new(width, height)),
        Transform::from_xyz(width * 0.5, height * 0.5, -1.0),
    ));
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(17.0),
            ..default()
        },
        TextColor(Color::srgb(0.78, 0.9, 1.0)),
        Node {
            position_type: PositionType::Absolute,
            top: px(18),
            left: px(20),
            ..default()
        },
        DiagnosticsText,
    ));
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(15.0),
            ..default()
        },
        TextColor(Color::srgb(0.62, 0.72, 0.84)),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(18),
            left: px(20),
            ..default()
        },
        ControlHintText,
    ));
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.84, 0.58)),
        Node {
            position_type: PositionType::Absolute,
            top: px(18),
            right: px(20),
            width: px(320),
            ..default()
        },
        InspectorText,
    ));
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(13.0),
            ..default()
        },
        TextColor(Color::srgb(0.55, 0.86, 0.72)),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(18),
            right: px(20),
            width: px(320),
            ..default()
        },
        PlotText,
    ));
}

fn state_is_interactive(state: Res<State<AppState>>) -> bool {
    matches!(*state.get(), AppState::Running | AppState::Paused)
}

/// While paused nothing changes between frames, so only redraw on input or a slow timeout.
fn enter_low_power_redraw(mut winit: ResMut<WinitSettings>) {
    *winit = WinitSettings {
        focused_mode: UpdateMode::reactive(PAUSED_FOCUSED_WAIT),
        unfocused_mode: UpdateMode::reactive_low_power(PAUSED_UNFOCUSED_WAIT),
    };
}

fn exit_low_power_redraw(mut winit: ResMut<WinitSettings>) {
    *winit = WinitSettings::game();
}

/// One-shot capture when `AI_VILLAGE_VERIFY_SCREENSHOT=/path.png` is set (debug/verify only).
fn maybe_capture_verify_screenshot(mut commands: Commands, mut frames: Local<u32>) {
    let Ok(path) = std::env::var("AI_VILLAGE_VERIFY_SCREENSHOT") else {
        return;
    };
    *frames += 1;
    if *frames != 45 {
        return;
    }
    info!("capturing verification screenshot to {path}");
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

/// Advances only the authoritative core and retains adjacent detached snapshots for rendering.
fn advance_authoritative_world(mut bridge: ResMut<SimulationBridge>) {
    if bridge.paused {
        return;
    }
    for _ in 0..bridge.speed {
        bridge.previous = bridge.current.clone();
        bridge.current = bridge.world.step();
    }
}

fn simulation_controls(
    keys: Res<ButtonInput<KeyCode>>,
    mut bridge: ResMut<SimulationBridge>,
    mut overlays: ResMut<OverlaySettings>,
    state: Res<State<AppState>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if keys.just_pressed(KeyCode::Space) {
        bridge.paused = !bridge.paused;
        next_state.set(if bridge.paused {
            AppState::Paused
        } else {
            AppState::Running
        });
    }
    if keys.just_pressed(KeyCode::Digit1) {
        bridge.speed = 1;
    } else if keys.just_pressed(KeyCode::Digit2) {
        bridge.speed = 2;
    } else if keys.just_pressed(KeyCode::Digit3) {
        bridge.speed = 4;
    } else if keys.just_pressed(KeyCode::Digit4) {
        bridge.speed = 8;
    }
    if keys.just_pressed(KeyCode::KeyR) && *state.get() != AppState::Replay {
        let was_paused = bridge.paused;
        *bridge = SimulationBridge::new();
        bridge.paused = was_paused;
    }
    if keys.just_pressed(KeyCode::KeyP) {
        let controls = bridge.world.transmission_controls();
        bridge
            .world
            .set_transmission_controls(CulturalTransmissionControls {
                plasticity_enabled: !controls.plasticity_enabled,
                ..controls
            });
    }
    if keys.just_pressed(KeyCode::KeyG) {
        overlays.sensors = !overlays.sensors;
    }
    if keys.just_pressed(KeyCode::KeyT) {
        overlays.trajectories = !overlays.trajectories;
    }
}

fn camera_controls(
    keys: Res<ButtonInput<KeyCode>>,
    mut scroll: MessageReader<MouseWheel>,
    time: Res<Time>,
    mut camera: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let Ok((mut transform, mut projection)) = camera.single_mut() else {
        return;
    };
    let mut direction = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    transform.translation += direction.normalize_or_zero() * CAMERA_SPEED * time.delta_secs();

    let wheel_delta: f32 = scroll.read().map(|event| event.y).sum();
    if let Projection::Orthographic(orthographic) = &mut *projection {
        orthographic.scale =
            (orthographic.scale * (1.0 - wheel_delta * 0.1)).clamp(MIN_ZOOM, MAX_ZOOM);
    }
}

fn sync_agent_entities(
    mut commands: Commands,
    bridge: Res<SimulationBridge>,
    mut entities: ResMut<AgentEntities>,
) {
    let snapshots = bridge.current.agents();
    for agent in snapshots {
        entities.0.entry(agent.id).or_insert_with(|| {
            let position = agent_to_world(agent);
            let torso = commands
                .spawn((
                    Sprite::from_color(agent_color(agent), Vec2::ONE),
                    Transform::from_xyz(position.x, position.y, 1.0),
                    VillageAgent { id: agent.id },
                    AnimalPart::Torso,
                ))
                .id();
            let head = commands
                .spawn((
                    Sprite::from_color(Color::srgb(0.96, 0.78, 0.52), Vec2::ONE),
                    Transform::from_xyz(position.x, position.y, 1.1),
                    VillageAgent { id: agent.id },
                    AnimalPart::Head,
                ))
                .id();
            let left_leg = commands
                .spawn((
                    Sprite::from_color(Color::srgb(0.32, 0.4, 0.52), Vec2::ONE),
                    Transform::from_xyz(position.x, position.y, 0.9),
                    VillageAgent { id: agent.id },
                    AnimalPart::LeftLeg,
                ))
                .id();
            let right_leg = commands
                .spawn((
                    Sprite::from_color(Color::srgb(0.32, 0.4, 0.52), Vec2::ONE),
                    Transform::from_xyz(position.x, position.y, 0.9),
                    VillageAgent { id: agent.id },
                    AnimalPart::RightLeg,
                ))
                .id();
            [torso, head, left_leg, right_leg]
        });
    }
    let live: std::collections::HashSet<_> = snapshots.iter().map(|agent| agent.id).collect();
    entities.0.retain(|id, entity| {
        if live.contains(id) {
            true
        } else {
            for part in entity {
                commands.entity(*part).despawn();
            }
            false
        }
    });
}

fn interpolate_agents(
    fixed_time: Res<Time<Fixed>>,
    bridge: Res<SimulationBridge>,
    mut agents: Query<(&VillageAgent, &AnimalPart, &mut Transform, &mut Sprite)>,
) {
    let previous: HashMap<_, _> = bridge
        .previous
        .agents()
        .iter()
        .map(|agent| (agent.id, agent.position))
        .collect();
    let alpha = fixed_time.overstep_fraction();
    for (agent, part, mut transform, mut sprite) in &mut agents {
        let Some(current) = bridge
            .current
            .agents()
            .iter()
            .find(|item| item.id == agent.id)
        else {
            continue;
        };
        let from = previous.get(&agent.id).copied().unwrap_or(current.position);
        let start = grid_to_world(from.x(), from.y());
        let end = agent_to_world(current);
        let centre = start.lerp(end, alpha);
        let (angle, torso_radius, head_radius, mouth_open) =
            current
                .body
                .map_or((0.0, AGENT_SIZE * 0.5, AGENT_SIZE * 0.26, 0.0), |body| {
                    (
                        f32::from(body.orientation.turns()) * core::f32::consts::TAU / 256.0,
                        fixed_to_render(body.torso_radius.raw()) * CELL_SIZE,
                        fixed_to_render(body.head_radius.raw()) * CELL_SIZE,
                        fixed_to_render(body.mouth_open.raw()),
                    )
                });
        let facing = Vec2::from_angle(angle);
        let side = Vec2::new(-facing.y, facing.x);
        let (offset, visual_size, color, z) = match part {
            AnimalPart::Torso => (
                Vec2::ZERO,
                Vec2::splat(torso_radius * 2.0),
                agent_color(current),
                1.0,
            ),
            AnimalPart::Head => (
                facing * (torso_radius + head_radius * 0.7),
                Vec2::splat(head_radius * 2.0 + mouth_open),
                Color::srgb(0.96, 0.78, 0.52),
                1.2,
            ),
            AnimalPart::LeftLeg => (
                -facing * torso_radius * 0.45 + side * torso_radius * 0.8,
                Vec2::new(torso_radius * 1.4, (torso_radius * 0.32).max(1.0)),
                Color::srgb(0.32, 0.4, 0.52),
                0.9,
            ),
            AnimalPart::RightLeg => (
                -facing * torso_radius * 0.45 - side * torso_radius * 0.8,
                Vec2::new(torso_radius * 1.4, (torso_radius * 0.32).max(1.0)),
                Color::srgb(0.32, 0.4, 0.52),
                0.9,
            ),
        };
        transform.translation = (centre + offset).extend(z);
        transform.rotation = Quat::from_rotation_z(angle);
        sprite.custom_size = Some(visual_size.max(Vec2::splat(1.0)));
        sprite.color = color;
    }
}

/// Selects only from the detached render snapshot; it cannot affect simulation authority.
fn select_agent(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    bridge: Res<SimulationBridge>,
    mut inspector: ResMut<Inspector>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let (Ok(window), Ok((camera, camera_transform))) = (windows.single(), camera.single()) else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(world_position) = camera.viewport_to_world_2d(camera_transform, cursor) else {
        return;
    };
    inspector.selected = bridge
        .current
        .agents()
        .iter()
        .filter_map(|agent| {
            let distance = agent_to_world(agent).distance(world_position);
            (distance <= AGENT_SIZE * 2.5).then_some((distance, agent.id))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, id)| id);
}

fn update_history(bridge: Res<SimulationBridge>, mut history: ResMut<History>) {
    let tick = bridge.current.tick().get();
    if tick == history.last_tick || !tick.is_multiple_of(4) {
        return;
    }
    history.last_tick = tick;
    let agents = bridge.current.agents();
    let count = agents.len().max(1);
    let energy = agents
        .iter()
        .map(|agent| u32::from(agent.energy))
        .sum::<u32>()
        / u32::try_from(count).expect("population fits u32");
    let lineage = agents.iter().map(|agent| agent.lineage_depth).sum::<u32>()
        / u32::try_from(count).expect("population fits u32");
    if history.population.len() == HISTORY_SAMPLES {
        let _ = history.population.pop_front();
        let _ = history.mean_energy.pop_front();
        let _ = history.lineage_depth.pop_front();
    }
    history
        .population
        .push_back(u32::try_from(agents.len()).expect("population fits u32"));
    history
        .mean_energy
        .push_back(u16::try_from(energy).expect("energy average is bounded"));
    history.lineage_depth.push_back(lineage);
}

fn draw_overlays(
    bridge: Res<SimulationBridge>,
    inspector: Res<Inspector>,
    overlays: Res<OverlaySettings>,
    mut gizmos: Gizmos,
) {
    let Some(id) = inspector.selected else {
        return;
    };
    let Some(agent) = bridge.current.agents().iter().find(|agent| agent.id == id) else {
        return;
    };
    let origin = agent_to_world(agent);
    if overlays.sensors {
        let angle = agent.body.map_or(0.0, |body| {
            f32::from(body.orientation.turns()) * core::f32::consts::TAU / 256.0
        });
        for ray in 0..16 {
            let offset = (f32::from(u16::try_from(ray).expect("ray count fits")) - 7.5) * 0.10;
            let endpoint = origin + Vec2::from_angle(angle + offset) * CELL_SIZE * 6.0;
            gizmos.line_2d(origin, endpoint, Color::srgba(0.4, 0.85, 1.0, 0.25));
        }
    }
    if overlays.trajectories {
        // A small presentation halo makes the current tracked individual legible at high zoom.
        gizmos.circle_2d(origin, AGENT_SIZE * 2.7, Color::srgb(1.0, 0.84, 0.25));
    }
}

fn update_inspector(
    bridge: Res<SimulationBridge>,
    inspector: Res<Inspector>,
    mut text: Query<&mut Text, With<InspectorText>>,
) {
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    let Some(id) = inspector.selected else {
        *text = Text::new(
            "INSPECTOR\nClick an animal body\n\nDetached snapshot diagnostics\nnever feed back into agents.",
        );
        return;
    };
    let Some(agent) = bridge.current.agents().iter().find(|agent| agent.id == id) else {
        *text = Text::new("INSPECTOR\nSelected animal no longer alive.");
        return;
    };
    let physiology = agent.physiology.map_or_else(
        || "V1 compatibility agent".to_owned(),
        |state| format!(
            "energy {:>4}  fat {:>4}  gut {:>4}\nhydration {:>4}  fatigue {:>4}\ntoxin {:>4}  infection {:>4}\ntissue H/T/L {:?}  pain {:>4}",
            state.energy, state.fat, state.gut, state.hydration, state.fatigue,
            state.toxin, state.infection, state.tissue, state.pain
        ),
    );
    let brain = bridge.world.brain_of(id).map_or_else(
        || "brain: unavailable".to_owned(),
        |brain| {
            format!(
                "brain: {} neurons  activity {:?}\nprediction {:?}  active episodes {}",
                brain.neurons,
                &brain.activity[..8],
                brain.prediction,
                brain
                    .episodes
                    .iter()
                    .filter(|episode| episode.retention > 0)
                    .count()
            )
        },
    );
    let development = bridge.world.development_of(id).map_or_else(
        || "genome: unavailable".to_owned(),
        |value| {
            format!(
                "development: stage {:?}  growth {}\nlineage cluster {:016x}",
                value.stage, value.growth, value.lineage_cluster
            )
        },
    );
    *text = Text::new(format!(
        "INSPECTOR  id {}\nage {} ticks  carried {}\n{}\n{}\n{}",
        id.slot(),
        agent.age,
        agent.materials,
        physiology,
        brain,
        development
    ));
}

fn sparkline(values: &VecDeque<u32>) -> String {
    const BARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let Some(maximum) = values.iter().copied().max() else {
        return "—".to_owned();
    };
    let scale = maximum.max(1);
    values
        .iter()
        .step_by((values.len() / 52).max(1))
        .map(|value| BARS[(usize::try_from(*value * 7 / scale).expect("bar bounded")).min(7)])
        .collect()
}

fn update_plots(history: Res<History>, mut text: Query<&mut Text, With<PlotText>>) {
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    let energy: VecDeque<u32> = history.mean_energy.iter().copied().map(u32::from).collect();
    *text = Text::new(format!(
        "SNAPSHOT PLOTS (last {} samples)\npopulation  {}\nmean energy {}\nlineage    {}\n\n[G] sensor rays  [T] tracked halo",
        history.population.len(),
        sparkline(&history.population),
        sparkline(&energy),
        sparkline(&history.lineage_depth),
    ));
}

fn sync_block_entities(
    mut commands: Commands,
    bridge: Res<SimulationBridge>,
    mut entities: ResMut<BlockEntities>,
) {
    let width = usize::from(bridge.world.settings().width());
    for (cell, durability) in bridge.current.blocks().iter().enumerate() {
        if *durability == 0 {
            continue;
        }
        entities.0.entry(cell).or_insert_with(|| {
            let x = u16::try_from(cell % width).expect("world width bounds x");
            let y = u16::try_from(cell / width).expect("world height bounds y");
            let position = grid_to_world(x, y);
            commands
                .spawn((
                    Sprite::from_color(Color::srgb(0.74, 0.48, 0.18), Vec2::splat(CELL_SIZE * 0.8)),
                    Transform::from_xyz(position.x, position.y, 0.5),
                ))
                .id()
        });
    }
    entities.0.retain(|cell, entity| {
        if bridge.current.blocks()[*cell] > 0 {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });
}

fn update_diagnostics(
    diagnostics: Res<DiagnosticsStore>,
    bridge: Res<SimulationBridge>,
    mut text: Query<&mut Text, With<DiagnosticsText>>,
) {
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(bevy::diagnostic::Diagnostic::smoothed)
        .unwrap_or(0.0);
    let loose_material: u64 = bridge
        .current
        .materials()
        .iter()
        .map(|material| u64::from(*material))
        .sum();
    let blocks = bridge
        .current
        .blocks()
        .iter()
        .filter(|durability| **durability > 0)
        .count();
    let signal_metrics = bridge.current.metrics();
    let transmission_metrics = signal_metrics.transmission;
    let receptor_debug = bridge
        .current
        .agents()
        .first()
        .and_then(|agent| {
            bridge.world.raw_senses_of(agent.id).map(|frame| {
                format!(
                    "receptors: ray0 rgb({:>3},{:>3},{:>3}) occ {:>3}  chem {:?}",
                    frame.retinal[0].red,
                    frame.retinal[0].green,
                    frame.retinal[0].blue,
                    frame.retinal[0].occlusion,
                    frame.chemicals
                )
            })
        })
        .unwrap_or_else(|| "receptors: V1 semantic compatibility path".to_owned());
    *text = Text::new(format!(
        "AI VILLAGE  •  LIVE\n\
         tick: {:>7}     population: {:>4}\n\
         sim: {}×{} @ {:.0} Hz     speed: {}×\n\
         material: {:>5}     blocks: {:>4}\n\
         signals: {:>5} sent / {:>5} heard\n\
         learning: {:>5} updates / {:>4} experienced\n\
         {receptor_debug}\n\
         render: {:>5.1} FPS     hash: {:016x}",
        bridge.current.tick().get(),
        bridge.current.agents().len(),
        bridge.world.settings().width(),
        bridge.world.settings().height(),
        TICKS_PER_SECOND,
        bridge.speed,
        loose_material,
        blocks,
        signal_metrics.signals_emitted,
        signal_metrics.signal_observations,
        transmission_metrics.learning_updates,
        transmission_metrics.experienced_agents,
        fps,
        bridge.current.state_hash(),
    ));
}

fn update_control_hint(
    bridge: Res<SimulationBridge>,
    mut text: Query<&mut Text, With<ControlHintText>>,
) {
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    let mode = if bridge.paused { "PAUSED" } else { "RUNNING" };
    *text = Text::new(format!(
        "[Space] {mode}   [1–4] speed (1× / 2× / 4× / 8×)   [P] plasticity: {}   [WASD / arrows] pan   [wheel] zoom   [R] reset seed",
        if bridge.world.transmission_controls().plasticity_enabled {
            "on"
        } else {
            "off"
        }
    ));
}

fn grid_to_world(x: u16, y: u16) -> Vec2 {
    Vec2::new(
        (f32::from(x) + 0.5) * CELL_SIZE,
        (f32::from(y) + 0.5) * CELL_SIZE,
    )
}

#[allow(
    clippy::cast_precision_loss,
    reason = "This renderer-only adapter intentionally converts bounded fixed-point snapshots to Bevy f32 coordinates."
)]
fn agent_to_world(agent: &AgentSnapshot) -> Vec2 {
    agent.body.map_or_else(
        || grid_to_world(agent.position.x(), agent.position.y()),
        |body| {
            Vec2::new(
                body.position.x.raw() as f32 / FIXED_SCALE as f32 * CELL_SIZE + CELL_SIZE * 0.5,
                body.position.y.raw() as f32 / FIXED_SCALE as f32 * CELL_SIZE + CELL_SIZE * 0.5,
            )
        },
    )
}

#[allow(
    clippy::cast_precision_loss,
    reason = "Renderer-only conversion of bounded fixed-point snapshot coordinates."
)]
fn fixed_to_render(value: i32) -> f32 {
    value as f32 / FIXED_SCALE as f32
}

fn agent_color(agent: &AgentSnapshot) -> Color {
    let hue = f32::from(u16::try_from(agent.id.slot() % 360).expect("hue is bounded"));
    Color::hsl(hue, 0.72, 0.62)
}
