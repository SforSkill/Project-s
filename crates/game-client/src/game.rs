//! In-game presentation: input sampling, camera, remote players, grenades,
//! smokes, fires, bomb, tracers, viewmodel and event effects.

use crate::audio::{Sfx, Sounds};
use crate::net::{Clock, Net};
use crate::settings::{Settings, vertical_fov};
use crate::world::{MapEntity, SCALE, spawn_map};
use bevy::camera::visibility::RenderLayers;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, WindowMode};
use engine_core::cmd::Buttons;
use engine_core::{PlayerId, Team, ViewAngles};
use engine_weapons::{GrenadeKind, HitGroup, Slot, WeaponId, WeaponKind};
use game_sim::events::GameEvent;
use game_sim::proto::{SelfState, eflags};
use game_sim::smoke::SmokeVolume;
use std::collections::HashMap;

pub const VIEWMODEL_LAYER: usize = 1;

#[derive(Component)]
pub struct MainCam;
#[derive(Component)]
pub struct ViewCam;
#[derive(Component)]
pub struct Viewmodel;
#[derive(Component)]
pub struct ViewmodelPart;

/// What the screen shows.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Menu,
    Game,
}

#[derive(Debug, Clone)]
pub struct Kill {
    pub killer: String,
    pub killer_team: Team,
    pub victim: String,
    pub victim_team: Team,
    pub weapon: String,
    pub headshot: bool,
    pub penetrated: bool,
    pub me_involved: bool,
    pub at: f64,
}

/// HUD state fed by events (read by the egui layer).
#[derive(Resource, Default)]
pub struct Hud {
    pub escape_menu: bool,
    pub buy_menu: bool,
    pub scoreboard: bool,
    pub inspector: bool,
    pub chat_open: bool,
    pub chat_team: bool,
    pub chat_text: String,
    pub chat_log: Vec<(String, f64)>,
    pub kills: Vec<Kill>,
    pub notices: Vec<(String, f64)>,
    pub hit_marker: f64,
    pub hit_head: bool,
    pub damage_from: Vec<(Vec3, f64)>,
    pub flash_until: f64,
    pub flash_full_until: f64,
    pub flash_alpha: f32,
    pub round_banner: Option<(String, f64)>,
    pub settings_open: bool,
    pub host_cmd: String,
    pub fps: f32,
    pub frame_ms: Vec<f32>,
    /// World-space eye and view used this frame (for labels and the inspector).
    pub eye: Vec3,
    pub view: ViewAngles,
    pub last_shot_kick: f64,
}

impl Hud {
    pub fn ui_captures_input(&self) -> bool {
        self.escape_menu || self.buy_menu || self.chat_open || self.settings_open
    }

    pub fn notice(&mut self, text: impl Into<String>, now: f64) {
        self.notices.push((text.into(), now));
        if self.notices.len() > 6 {
            self.notices.remove(0);
        }
    }
}

#[derive(Default)]
struct SmokeView {
    vol: Option<SmokeVolume>,
    voxels: Vec<(usize, Entity)>,
}

#[derive(Resource, Default)]
pub struct Scene {
    pub map: Option<String>,
    players: HashMap<u8, Entity>,
    grenades: HashMap<u16, Entity>,
    drops: HashMap<u16, Entity>,
    smokes: HashMap<u16, SmokeView>,
    fires: HashMap<u16, Vec<Entity>>,
    bomb: Option<Entity>,
    temps: Vec<(Entity, f64)>,
    assets: Option<SceneAssets>,
}

#[derive(Clone)]
struct SceneAssets {
    cube: Handle<Mesh>,
    sphere: Handle<Mesh>,
    attack: Handle<StandardMaterial>,
    defense: Handle<StandardMaterial>,
    neutral: Handle<StandardMaterial>,
    head: Handle<StandardMaterial>,
    smoke: Handle<StandardMaterial>,
    fire: Handle<StandardMaterial>,
    tracer: Handle<StandardMaterial>,
    impact: Handle<StandardMaterial>,
    grenade: Handle<StandardMaterial>,
    weapon: Handle<StandardMaterial>,
    bomb: Handle<StandardMaterial>,
}

pub fn setup_cameras(
    mut commands: Commands,
    mut egui_settings: ResMut<bevy_egui::EguiGlobalSettings>,
) {
    egui_settings.auto_create_primary_context = false;
    commands.insert_resource(ClearColor(Color::srgb(0.55, 0.68, 0.85)));
    commands.insert_resource(bevy::light::GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 350.0,
        ..default()
    });
    commands.spawn((
        MainCam,
        Camera3d::default(),
        Projection::from(PerspectiveProjection {
            fov: vertical_fov(90.0),
            near: 0.02,
            far: 600.0,
            ..default()
        }),
        Transform::from_xyz(0.0, 2.0, 0.0),
        bevy::audio::SpatialListener::new(0.25),
        children![(
            ViewCam,
            Camera3d::default(),
            Camera {
                order: 1,
                ..default()
            },
            Projection::from(PerspectiveProjection {
                fov: vertical_fov(68.0),
                near: 0.01,
                far: 10.0,
                ..default()
            }),
            RenderLayers::layer(VIEWMODEL_LAYER),
        )],
    ));
    // Interface camera (draws nothing but the egui layer, on top of both).
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 2,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        RenderLayers::layer(30),
        bevy_egui::PrimaryEguiContext,
    ));
    // Light for the viewmodel layer.
    commands.spawn((
        DirectionalLight {
            illuminance: 6000.0,
            ..default()
        },
        Transform::from_xyz(1.0, 2.0, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
        RenderLayers::layer(VIEWMODEL_LAYER),
    ));
}

pub fn key_from_name(name: &str) -> Option<KeyCode> {
    use KeyCode::*;
    let k = match name {
        "Space" => Space,
        "ControlLeft" | "Ctrl" => ControlLeft,
        "ControlRight" => ControlRight,
        "ShiftLeft" | "Shift" => ShiftLeft,
        "ShiftRight" => ShiftRight,
        "AltLeft" | "Alt" => AltLeft,
        "Tab" => Tab,
        "CapsLock" => CapsLock,
        "Enter" => Enter,
        "Backquote" => Backquote,
        "F1" => F1,
        "F2" => F2,
        "F3" => F3,
        "F4" => F4,
        "F5" => F5,
        "F6" => F6,
        "F7" => F7,
        "F8" => F8,
        "F9" => F9,
        "F10" => F10,
        s if s.len() == 1 => {
            let c = s.chars().next()?.to_ascii_uppercase();
            const LETTERS: [KeyCode; 26] = [
                KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN,
                KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
            ];
            const DIGITS: [KeyCode; 10] = [
                Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
            ];
            if c.is_ascii_uppercase() {
                LETTERS[(c as u8 - b'A') as usize]
            } else if c.is_ascii_digit() {
                DIGITS[(c as u8 - b'0') as usize]
            } else {
                return None;
            }
        }
        _ => return None,
    };
    Some(k)
}

fn pressed(keys: &ButtonInput<KeyCode>, name: &str) -> bool {
    key_from_name(name).is_some_and(|k| keys.pressed(k))
}

fn just(keys: &ButtonInput<KeyCode>, name: &str) -> bool {
    key_from_name(name).is_some_and(|k| keys.just_pressed(k))
}

pub fn active_weapon(me: &SelfState) -> Option<WeaponId> {
    match me.active {
        Slot::Primary => me.primary.map(|p| p.0),
        Slot::Secondary => me.secondary.map(|p| p.0),
        Slot::Melee => me.knife,
        Slot::Grenade => me.grenades.get(me.grenade_index as usize).copied(),
        Slot::Bomb => None,
    }
}

/// Window state seen at the last cursor update.
#[derive(Default)]
pub struct GrabState(Option<(bool, WindowMode, UVec2)>);

/// Keeps the cursor captured (hidden, held inside the window) while playing.
///
/// Windows drops the cursor clip on Alt+Tab and keeps the old rectangle when
/// the window changes size or mode, while Bevy only forwards a grab mode that
/// differs from the last one. After a focus, mode or size change the grab is
/// therefore released for one frame, then requested again. `Locked` falls back
/// to `Confined` on Windows, where locking is not supported.
fn update_cursor_grab(st: &mut GrabState, cursor: &mut CursorOptions, capture: bool, win: &Window) {
    let now = (win.focused, win.mode, win.physical_size());
    let changed = st.0 != Some(now);
    st.0 = Some(now);
    let grab = if capture && !(changed && cursor.grab_mode != CursorGrabMode::None) {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    if cursor.grab_mode != grab {
        cursor.grab_mode = grab;
    }
    if cursor.visible == capture {
        cursor.visible = !capture;
    }
}

/// Mouse and keyboard to the input timeline (sub-tick timestamps), cursor capture.
pub fn input(
    screen: Res<Screen>,
    mut hud: ResMut<Hud>,
    settings: Res<Settings>,
    clock: Res<Clock>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut net: ResMut<Net>,
    mut window: Single<(&Window, &mut CursorOptions)>,
    mut grab_state: Local<GrabState>,
    autotest: Option<Res<crate::autotest::AutoTest>>,
) {
    if autotest.is_some() {
        return;
    }
    let now = clock.now();
    let in_game =
        *screen == Screen::Game && net.session.as_ref().is_some_and(|s| s.client.in_game());
    let (win, cursor) = &mut *window;
    let capture = in_game && !hud.ui_captures_input() && win.focused;
    update_cursor_grab(&mut grab_state, cursor, capture, win);
    let Some(s) = &mut net.session else { return };
    if !in_game {
        return;
    }
    // Toggles available even with the cursor free.
    if keys.just_pressed(KeyCode::Escape) {
        if hud.chat_open {
            hud.chat_open = false;
        } else if hud.buy_menu {
            hud.buy_menu = false;
        } else if hud.settings_open {
            hud.settings_open = false;
        } else {
            hud.escape_menu = !hud.escape_menu;
        }
    }
    hud.scoreboard = pressed(&keys, &settings.keys.scoreboard) && !hud.chat_open;
    if !capture {
        s.timeline.set_buttons(now, Buttons(0));
        return;
    }
    if just(&keys, &settings.keys.buy) {
        hud.buy_menu = true;
    }
    if just(&keys, &settings.keys.inspector) {
        hud.inspector = !hud.inspector;
    }
    if keys.just_pressed(KeyCode::KeyY) || keys.just_pressed(KeyCode::KeyU) {
        hud.chat_open = true;
        hud.chat_team = keys.just_pressed(KeyCode::KeyU);
        hud.chat_text.clear();
        s.timeline.set_buttons(now, Buttons(0));
        return;
    }
    // View: raw mouse counts (Source formula: degrees = counts × sensitivity × m_yaw).
    let zoomed = s
        .client
        .latest()
        .and_then(|sn| sn.me.as_ref())
        .is_some_and(|m| m.hand.zoom > 0);
    let d = motion.delta;
    if d != Vec2::ZERO {
        let mut v = s.timeline.view();
        v.yaw -= d.x * settings.yaw_per_count(zoomed);
        v.pitch -= d.y * settings.pitch_per_count(zoomed);
        s.timeline.set_view(now, v);
    }
    let k = &settings.keys;
    let mut b = 0u32;
    let map = [
        (pressed(&keys, &k.forward), Buttons::FORWARD),
        (pressed(&keys, &k.back), Buttons::BACK),
        (pressed(&keys, &k.left), Buttons::LEFT),
        (pressed(&keys, &k.right), Buttons::RIGHT),
        (pressed(&keys, &k.jump), Buttons::JUMP),
        (pressed(&keys, &k.duck), Buttons::DUCK),
        (pressed(&keys, &k.walk), Buttons::WALK),
        (pressed(&keys, &k.reload), Buttons::RELOAD),
        (pressed(&keys, &k.use_key), Buttons::USE),
        (pressed(&keys, &k.drop), Buttons::DROP),
        (pressed(&keys, "F"), Buttons::INSPECT),
        (mouse.pressed(MouseButton::Left), Buttons::ATTACK),
        (mouse.pressed(MouseButton::Right), Buttons::ATTACK2),
    ];
    for (on, bit) in map {
        if on {
            b |= bit;
        }
    }
    s.timeline.set_buttons(now, Buttons(b));
    for (key, slot) in [
        (KeyCode::Digit1, 1),
        (KeyCode::Digit2, 2),
        (KeyCode::Digit3, 3),
        (KeyCode::Digit4, 4),
        (KeyCode::Digit5, 5),
        (KeyCode::KeyQ, 6),
    ] {
        if keys.just_pressed(key) {
            s.timeline.select(slot);
        }
    }
}

fn team_mat(a: &SceneAssets, t: Team) -> Handle<StandardMaterial> {
    match t {
        Team::Attack => a.attack.clone(),
        Team::Defense => a.defense.clone(),
        Team::Spectator => a.neutral.clone(),
    }
}

fn make_assets(meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> SceneAssets {
    let m = |mats: &mut Assets<StandardMaterial>, c: Color| {
        mats.add(StandardMaterial {
            base_color: c,
            perceptual_roughness: 0.8,
            ..default()
        })
    };
    SceneAssets {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        sphere: meshes.add(Sphere::new(0.5)),
        attack: m(mats, Color::srgb(0.85, 0.55, 0.15)),
        defense: m(mats, Color::srgb(0.2, 0.45, 0.9)),
        neutral: m(mats, Color::srgb(0.8, 0.8, 0.8)),
        head: m(mats, Color::srgb(0.9, 0.75, 0.6)),
        smoke: mats.add(StandardMaterial {
            base_color: Color::srgb(0.78, 0.79, 0.8),
            perceptual_roughness: 1.0,
            ..default()
        }),
        fire: mats.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.45, 0.05, 0.7),
            emissive: LinearRgba::rgb(6.0, 1.6, 0.1),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        tracer: mats.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.9, 0.5, 0.8),
            emissive: LinearRgba::rgb(4.0, 3.0, 1.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        impact: mats.add(StandardMaterial {
            base_color: Color::srgb(0.1, 0.1, 0.1),
            unlit: true,
            ..default()
        }),
        grenade: m(mats, Color::srgb(0.25, 0.35, 0.2)),
        weapon: m(mats, Color::srgb(0.12, 0.12, 0.13)),
        bomb: mats.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.1, 0.1),
            emissive: LinearRgba::rgb(2.0, 0.0, 0.0),
            ..default()
        }),
    }
}

/// Loads the map when the server announces one, clears it when leaving.
pub fn sync_map(
    mut commands: Commands,
    mut scene: ResMut<Scene>,
    mut net: ResMut<Net>,
    map_entities: Query<Entity, With<MapEntity>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    data: Res<crate::net::GameDataRes>,
) {
    if scene.assets.is_none() {
        scene.assets = Some(make_assets(&mut meshes, &mut materials));
    }
    let wanted = net.session.as_mut().and_then(|s| {
        let changed = std::mem::take(&mut s.client.world_changed);
        s.client.welcome.as_ref().map(|w| (w.map.clone(), changed))
    });
    let reload = match (&wanted, &scene.map) {
        (Some((m, changed)), Some(cur)) => m != cur || *changed,
        (Some(_), None) => true,
        (None, Some(_)) => true,
        (None, None) => false,
    };
    if !reload {
        return;
    }
    for e in &map_entities {
        commands.entity(e).despawn();
    }
    clear_dynamic(&mut commands, &mut scene);
    scene.map = None;
    if let Some((name, _)) = wanted {
        let level = net.session.as_ref().and_then(|s| s.client.level.clone());
        if let Some(level) = level {
            spawn_map(
                &mut commands,
                &mut meshes,
                &mut materials,
                &level.def,
                &data.0.materials,
            );
            scene.map = Some(name);
        }
    }
}

fn clear_dynamic(commands: &mut Commands, scene: &mut Scene) {
    for (_, e) in scene.players.drain() {
        commands.entity(e).despawn();
    }
    for (_, e) in scene.grenades.drain().chain(scene.drops.drain()) {
        commands.entity(e).despawn();
    }
    for (_, sv) in scene.smokes.drain() {
        for (_, e) in sv.voxels {
            commands.entity(e).despawn();
        }
    }
    for (_, v) in scene.fires.drain() {
        for e in v {
            commands.entity(e).despawn();
        }
    }
    if let Some(e) = scene.bomb.take() {
        commands.entity(e).despawn();
    }
    for (e, _) in scene.temps.drain(..) {
        commands.entity(e).despawn();
    }
}

fn spawn_player_model(commands: &mut Commands, a: &SceneAssets, team: Team) -> Entity {
    commands
        .spawn((Transform::default(), Visibility::default()))
        .with_children(|p| {
            // Sizes in metres for a standing player (hull 32 × 72 u = 0.81 × 1.83 m);
            // the parent is squashed vertically when crouching.
            p.spawn((
                Mesh3d(a.cube.clone()),
                MeshMaterial3d(team_mat(a, team)),
                Transform::from_xyz(0.0, 1.12, 0.0).with_scale(Vec3::new(0.52, 0.62, 0.3)),
            ));
            p.spawn((
                Mesh3d(a.cube.clone()),
                MeshMaterial3d(team_mat(a, team)),
                Transform::from_xyz(-0.13, 0.41, 0.0).with_scale(Vec3::new(0.2, 0.82, 0.24)),
            ));
            p.spawn((
                Mesh3d(a.cube.clone()),
                MeshMaterial3d(team_mat(a, team)),
                Transform::from_xyz(0.13, 0.41, 0.0).with_scale(Vec3::new(0.2, 0.82, 0.24)),
            ));
            p.spawn((
                Mesh3d(a.sphere.clone()),
                MeshMaterial3d(a.head.clone()),
                Transform::from_xyz(0.0, 1.62, 0.0).with_scale(Vec3::splat(0.27)),
            ));
            p.spawn((
                Mesh3d(a.cube.clone()),
                MeshMaterial3d(a.weapon.clone()),
                Transform::from_xyz(0.16, 1.3, -0.3).with_scale(Vec3::new(0.05, 0.08, 0.5)),
            ));
        })
        .id()
}

/// Remote players, grenades, drops, smokes, fires, bomb; expires temporary effects.
pub fn update_scene(
    mut commands: Commands,
    mut scene: ResMut<Scene>,
    mut net: ResMut<Net>,
    clock: Res<Clock>,
    mut transforms: Query<(&mut Transform, &mut Visibility), Without<MainCam>>,
) {
    let now = clock.now();
    let blasts: Vec<(Vec3, f64)> = net
        .session
        .as_mut()
        .map(|s| std::mem::take(&mut s.client.smoke_blasts))
        .unwrap_or_default();
    let Some(s) = &net.session else {
        if scene.map.is_none() && !scene.players.is_empty() {
            clear_dynamic(&mut commands, &mut scene);
        }
        return;
    };
    let Some(a) = scene.assets.clone() else {
        return;
    };
    let c = &s.client;
    let mv = &c.data.movement;
    let server_now = c.server_time(now);
    // Players.
    // Never draw the local player, nor the one whose eyes the camera borrows.
    let spectated = c
        .latest()
        .and_then(|sn| sn.me.as_ref())
        .and_then(|m| m.spectating);
    let remotes: Vec<_> = c
        .remote_players(now)
        .into_iter()
        .filter(|r| Some(r.id) != c.me() && Some(r.id) != spectated)
        .collect();
    let mut seen = Vec::with_capacity(remotes.len());
    for r in &remotes {
        seen.push(r.id);
        let e = match scene.players.get(&r.id) {
            Some(e) => *e,
            None => {
                let e = spawn_player_model(&mut commands, &a, r.team);
                scene.players.insert(r.id, e);
                continue;
            }
        };
        if let Ok((mut t, mut v)) = transforms.get_mut(e) {
            let height = mv.hull_height + (mv.duck_hull_height - mv.hull_height) * r.duck;
            t.translation = r.origin * SCALE;
            t.rotation = Quat::from_rotation_y(r.yaw.to_radians());
            t.scale = Vec3::new(1.0, height / mv.hull_height, 1.0);
            *v = if r.alive() && r.flags & eflags::STALE == 0 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
    let gone: Vec<u8> = scene
        .players
        .keys()
        .copied()
        .filter(|id| !seen.contains(id))
        .collect();
    for id in gone {
        if let Some(e) = scene.players.remove(&id) {
            commands.entity(e).despawn();
        }
    }
    let Some(snap) = c.latest() else { return };
    // Grenades in flight.
    let mut live = Vec::new();
    for g in &snap.grenades {
        live.push(g.id);
        let pos = g.pos * SCALE;
        match scene.grenades.get(&g.id) {
            Some(e) => {
                if let Ok((mut t, _)) = transforms.get_mut(*e) {
                    t.translation = t.translation.lerp(pos, 0.6);
                }
            }
            None => {
                let e = commands
                    .spawn((
                        Mesh3d(a.sphere.clone()),
                        MeshMaterial3d(a.grenade.clone()),
                        Transform::from_translation(pos).with_scale(Vec3::splat(0.12)),
                    ))
                    .id();
                scene.grenades.insert(g.id, e);
            }
        }
    }
    scene.grenades.retain(|id, e| {
        let keep = live.contains(id);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    // Weapons on the ground.
    let mut live = Vec::new();
    for d in &snap.drops {
        live.push(d.id);
        let t = Transform::from_translation(d.pos * SCALE + Vec3::Y * 0.05)
            .with_scale(Vec3::new(0.12, 0.08, 0.7))
            .with_rotation(Quat::from_rotation_y(d.id as f32));
        match scene.drops.get(&d.id) {
            Some(e) => {
                if let Ok((mut tr, _)) = transforms.get_mut(*e) {
                    *tr = t;
                }
            }
            None => {
                let e = commands
                    .spawn((Mesh3d(a.cube.clone()), MeshMaterial3d(a.weapon.clone()), t))
                    .id();
                scene.drops.insert(d.id, e);
            }
        }
    }
    scene.drops.retain(|id, e| {
        let keep = live.contains(id);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    // Bomb (dropped or planted).
    let bomb_pos = match snap.bomb.kind {
        2 | 3 => Some(snap.bomb.pos),
        _ => None,
    };
    match (bomb_pos, scene.bomb) {
        (Some(p), Some(e)) => {
            if let Ok((mut t, _)) = transforms.get_mut(e) {
                t.translation = p * SCALE + Vec3::Y * 0.08;
            }
        }
        (Some(p), None) => {
            let e = commands
                .spawn((
                    Mesh3d(a.cube.clone()),
                    MeshMaterial3d(a.bomb.clone()),
                    Transform::from_translation(p * SCALE + Vec3::Y * 0.08)
                        .with_scale(Vec3::new(0.3, 0.15, 0.22)),
                ))
                .id();
            scene.bomb = Some(e);
        }
        (None, Some(e)) => {
            commands.entity(e).despawn();
            scene.bomb = None;
        }
        (None, None) => {}
    }
    // Smokes: server voxel grids, opaque, with HE holes applied locally.
    let cfg = &s
        .client
        .welcome
        .as_ref()
        .map(|w| w.sim.smoke.clone())
        .unwrap_or_default();
    for (id, net_smoke) in &c.smokes {
        if scene.smokes.contains_key(id) || server_now >= net_smoke.end {
            continue;
        }
        let filled: Vec<bool> = (0..(net_smoke.dims.x * net_smoke.dims.y * net_smoke.dims.z)
            as usize)
            .map(|i| {
                net_smoke
                    .bits
                    .get(i / 8)
                    .is_some_and(|b| b & (1 << (i % 8)) != 0)
            })
            .collect();
        let vol = SmokeVolume::from_parts(
            *id,
            net_smoke.center,
            net_smoke.origin,
            net_smoke.dims,
            net_smoke.voxel,
            filled,
            net_smoke.start,
            net_smoke.end,
            cfg,
        );
        let mut voxels = Vec::new();
        for i in 0..vol.filled.len() {
            if vol.filled[i] {
                // Slightly oversized, jittered cubes so the cloud reads as a volume.
                let j = ((i as u32).wrapping_mul(2654435761) >> 24) as f32 / 255.0;
                let size = vol.voxel * SCALE * (1.25 + 0.3 * j);
                let e = commands
                    .spawn((
                        Mesh3d(a.cube.clone()),
                        MeshMaterial3d(a.smoke.clone()),
                        Transform::from_translation(vol.voxel_center(i) * SCALE)
                            .with_scale(Vec3::splat(size)),
                        Visibility::Hidden,
                        NotShadowCaster,
                    ))
                    .id();
                voxels.push((i, e));
            }
        }
        scene.smokes.insert(
            *id,
            SmokeView {
                vol: Some(vol),
                voxels,
            },
        );
    }
    for (pos, time) in &blasts {
        for sv in scene.smokes.values_mut() {
            if let Some(v) = &mut sv.vol {
                v.blast(*pos, *time, cfg);
            }
        }
    }
    let mut expired = Vec::new();
    for (id, sv) in scene.smokes.iter() {
        let Some(vol) = &sv.vol else { continue };
        if server_now >= vol.end || !c.smokes.contains_key(id) {
            expired.push(*id);
            continue;
        }
        let scale = vol.scale(server_now, cfg);
        for (i, e) in &sv.voxels {
            if let Ok((_, mut v)) = transforms.get_mut(*e) {
                *v = if scale > 0.0 && vol.voxel_on(*i, server_now, scale) {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
    for id in expired {
        if let Some(sv) = scene.smokes.remove(&id) {
            for (_, e) in sv.voxels {
                commands.entity(e).despawn();
            }
        }
    }
    // Fires (molotov cells).
    for (id, (cells, _start, end)) in &c.fires {
        if server_now >= *end {
            continue;
        }
        if !scene.fires.contains_key(id) {
            let v = cells
                .iter()
                .map(|p| {
                    commands
                        .spawn((
                            Mesh3d(a.cube.clone()),
                            MeshMaterial3d(a.fire.clone()),
                            Transform::from_translation(*p * SCALE + Vec3::Y * 0.15)
                                .with_scale(Vec3::new(0.55, 0.3, 0.55)),
                            NotShadowCaster,
                        ))
                        .id()
                })
                .collect();
            scene.fires.insert(*id, v);
        }
    }
    let ended: Vec<u16> = scene
        .fires
        .keys()
        .copied()
        .filter(|id| c.fires.get(id).is_none_or(|f| server_now >= f.2))
        .collect();
    for id in ended {
        if let Some(v) = scene.fires.remove(&id) {
            for e in v {
                commands.entity(e).despawn();
            }
        }
    }
    // Temporary effects.
    scene.temps.retain(|(e, until)| {
        let keep = now < *until;
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
}

/// First-person camera at the predicted eye, or following the spectated player.
pub fn camera(
    net: Res<Net>,
    clock: Res<Clock>,
    settings: Res<Settings>,
    mut hud: ResMut<Hud>,
    mut cam: Query<(&mut Transform, &mut Projection), (With<MainCam>, Without<ViewCam>)>,
    mut vcam: Query<&mut Projection, (With<ViewCam>, Without<MainCam>)>,
    mut vm: Query<(&mut Transform, &mut Visibility), (With<Viewmodel>, Without<MainCam>)>,
) {
    let now = clock.now();
    let Ok((mut t, mut proj)) = cam.single_mut() else {
        return;
    };
    let Some(s) = &net.session else { return };
    let c = &s.client;
    let mv_cfg = &c.data.movement;
    let me = c.latest().and_then(|sn| sn.me.as_ref());
    let mut view = s.timeline.view();
    let mut fov = settings.fov;
    let mut eye = c.local_origin()
        + Vec3::Y
            * (mv_cfg.eye_height + (mv_cfg.duck_eye_height - mv_cfg.eye_height) * c.local_duck());
    let mut show_vm = settings.show_viewmodel;
    if let Some(me) = me {
        if !me.alive {
            show_vm = false;
            if let Some(target) = me.spectating
                && let Some(r) = c.remote_players(now).into_iter().find(|r| r.id == target)
            {
                eye = r.origin
                    + Vec3::Y
                        * (mv_cfg.eye_height
                            + (mv_cfg.duck_eye_height - mv_cfg.eye_height) * r.duck);
                view = ViewAngles::new(r.pitch, r.yaw);
            } else {
                // Death camera: above the body, looking down at it.
                eye = me.mv.origin + Vec3::new(0.0, 110.0, 0.0) - view.forward_flat() * 60.0;
                view.pitch = -50.0;
            }
        } else if let Some(w) = active_weapon(me).and_then(|w| c.data.catalog.try_get(w)) {
            if let Some(z) = w.zoom_fov(me.hand.zoom) {
                fov = z;
                show_vm = false;
            }
            // Cosmetic camera kick: half the deterministic recoil (the shot goes
            // to the full offset, shown by the recoil marker in the HUD).
            let (rp, ry) = me.hand.recoil_now(w, c.server_time(now));
            view.pitch += rp * 0.5;
            view.yaw += ry * 0.5;
        }
    }
    hud.eye = eye;
    hud.view = s.timeline.view();
    t.translation = eye * SCALE;
    t.rotation = Quat::from_euler(
        EulerRot::YXZ,
        view.yaw.to_radians(),
        view.pitch.to_radians(),
        0.0,
    );
    if let Projection::Perspective(p) = proj.as_mut() {
        let f = vertical_fov(fov);
        if (p.fov - f).abs() > 1e-4 {
            p.fov = f;
        }
    }
    if let Ok(mut vp) = vcam.single_mut()
        && let Projection::Perspective(p) = vp.as_mut()
    {
        p.fov = vertical_fov(settings.viewmodel_fov);
    }
    if let Ok((mut vt, mut vv)) = vm.single_mut() {
        *vv = if show_vm && me.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let o = settings.viewmodel_offset;
        let speed = c.mv.velocity.length().min(260.0) / 260.0;
        let bob = (now * 9.0).sin() as f32 * 0.006 * speed;
        let kick = ((hud.last_shot_kick - now).max(-0.08) + 0.08).max(0.0) as f32 * 0.4;
        vt.translation = Vec3::new(
            0.06 + o[0] * 0.006,
            -0.055 + o[1] * 0.006 + bob,
            -0.16 + o[2] * 0.006 + kick * 0.6,
        );
    }
}

/// Viewmodel: a simple procedural gun shown on its own render layer.
pub fn setup_viewmodel(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    cam: Query<Entity, With<ViewCam>>,
) {
    let Ok(cam) = cam.single() else { return };
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let dark = mats.add(StandardMaterial {
        base_color: Color::srgb(0.1, 0.1, 0.11),
        perceptual_roughness: 0.5,
        metallic: 0.6,
        ..default()
    });
    let grip = mats.add(StandardMaterial {
        base_color: Color::srgb(0.25, 0.2, 0.15),
        perceptual_roughness: 0.9,
        ..default()
    });
    let hand = mats.add(StandardMaterial {
        base_color: Color::srgb(0.35, 0.33, 0.3),
        ..default()
    });
    let layer = RenderLayers::layer(VIEWMODEL_LAYER);
    let vm = commands
        .spawn((
            Viewmodel,
            Transform::from_xyz(0.12, -0.12, -0.3),
            Visibility::Hidden,
            layer.clone(),
        ))
        .with_children(|p| {
            p.spawn((
                ViewmodelPart,
                Mesh3d(cube.clone()),
                MeshMaterial3d(dark.clone()),
                Transform::from_xyz(0.0, 0.0, -0.05).with_scale(Vec3::new(0.022, 0.032, 0.2)),
                layer.clone(),
                NotShadowCaster,
            ));
            p.spawn((
                ViewmodelPart,
                Mesh3d(cube.clone()),
                MeshMaterial3d(dark.clone()),
                Transform::from_xyz(0.0, 0.008, -0.2).with_scale(Vec3::new(0.011, 0.011, 0.13)),
                layer.clone(),
                NotShadowCaster,
            ));
            p.spawn((
                ViewmodelPart,
                Mesh3d(cube.clone()),
                MeshMaterial3d(grip.clone()),
                Transform::from_xyz(0.0, -0.032, 0.012)
                    .with_scale(Vec3::new(0.02, 0.05, 0.026))
                    .with_rotation(Quat::from_rotation_x(-0.3)),
                layer.clone(),
                NotShadowCaster,
            ));
            p.spawn((
                ViewmodelPart,
                Mesh3d(cube.clone()),
                MeshMaterial3d(hand.clone()),
                Transform::from_xyz(0.006, -0.045, 0.035).with_scale(Vec3::new(0.04, 0.04, 0.09)),
                layer.clone(),
                NotShadowCaster,
            ));
        })
        .id();
    commands.entity(cam).add_child(vm);
}

fn player_name(players: &[game_sim::proto::PlayerInfo], id: PlayerId) -> (String, Team) {
    players
        .iter()
        .find(|p| p.id == id.0)
        .map(|p| (p.name.clone(), p.team))
        .unwrap_or_else(|| (format!("#{}", id.0), Team::Spectator))
}

fn weapon_name(data: &game_sim::GameData, w: Option<WeaponId>) -> String {
    w.and_then(|w| data.catalog.try_get(w))
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "monde".into())
}

/// Game events to sounds, HUD feedback and short-lived visual effects.
pub fn events(
    mut commands: Commands,
    mut net: ResMut<Net>,
    clock: Res<Clock>,
    mut hud: ResMut<Hud>,
    mut scene: ResMut<Scene>,
    sounds: Option<Res<Sounds>>,
) {
    let now = clock.now();
    let Some(s) = &mut net.session else { return };
    let evs = s.client.take_events();
    let notices: Vec<String> = std::mem::take(&mut s.client.notices);
    for n in notices {
        hud.notice(n, now);
    }
    let Some(sounds) = sounds else { return };
    let sounds = &*sounds;
    let c = &s.client;
    let me = c.me().map(PlayerId);
    let ear = hud.eye;
    let a = scene.assets.clone();
    for ev in evs {
        match &ev {
            GameEvent::Shot {
                shooter,
                weapon,
                origin,
                end,
                impacts,
            } => {
                let kind = c.data.catalog.try_get(*weapon).map(|w| w.kind);
                let sfx = match kind {
                    Some(WeaponKind::Knife) => Sfx::Knife,
                    Some(WeaponKind::Pistol) => Sfx::ShotPistol,
                    Some(WeaponKind::SniperLight | WeaponKind::SniperHeavy) => Sfx::ShotSniper,
                    _ => Sfx::ShotRifle,
                };
                if Some(*shooter) == me {
                    sounds.play(&mut commands, sfx, 0.7);
                    hud.last_shot_kick = now + 0.0;
                } else {
                    sounds.play_at(&mut commands, sfx, *origin, ear, 3500.0, 1.0);
                }
                if kind != Some(WeaponKind::Knife)
                    && let Some(a) = &a
                {
                    // Tracer from slightly below the eye (muzzle) to the end point.
                    let from = if Some(*shooter) == me {
                        *origin - Vec3::Y * 6.0
                    } else {
                        *origin
                    };
                    let d = *end - from;
                    let len = d.length();
                    if len > 1.0 {
                        let mid = (from + *end) * 0.5 * SCALE;
                        let rot = Quat::from_rotation_arc(Vec3::Z, d / len);
                        let e = commands
                            .spawn((
                                Mesh3d(a.cube.clone()),
                                MeshMaterial3d(a.tracer.clone()),
                                Transform::from_translation(mid)
                                    .with_rotation(rot)
                                    .with_scale(Vec3::new(0.012, 0.012, len * SCALE)),
                                NotShadowCaster,
                            ))
                            .id();
                        scene.temps.push((e, now + 0.06));
                    }
                    for p in impacts.iter().take(6) {
                        let e = commands
                            .spawn((
                                Mesh3d(a.cube.clone()),
                                MeshMaterial3d(a.impact.clone()),
                                Transform::from_translation(*p * SCALE)
                                    .with_scale(Vec3::splat(0.04)),
                                NotShadowCaster,
                            ))
                            .id();
                        scene.temps.push((e, now + 8.0));
                    }
                }
            }
            GameEvent::Damage {
                attacker,
                victim,
                group,
                from,
                ..
            } => {
                if *attacker == me && me.is_some() && Some(*victim) != me {
                    hud.hit_marker = now;
                    hud.hit_head = *group == Some(HitGroup::Head);
                    sounds.play(
                        &mut commands,
                        if hud.hit_head { Sfx::HitHead } else { Sfx::Hit },
                        0.8,
                    );
                }
                if Some(*victim) == me {
                    hud.damage_from.push((*from, now));
                }
            }
            GameEvent::Kill {
                killer,
                victim,
                weapon,
                headshot,
                penetrated,
                ..
            } => {
                let (vn, vt) = player_name(&c.players, *victim);
                let (kn, kt) = killer
                    .map(|k| player_name(&c.players, k))
                    .unwrap_or_else(|| ("".into(), Team::Spectator));
                let mine = *killer == me && me.is_some() || Some(*victim) == me;
                if *killer == me && me.is_some() && Some(*victim) != me {
                    sounds.play(&mut commands, Sfx::Kill, 0.8);
                }
                hud.kills.push(Kill {
                    killer: kn,
                    killer_team: kt,
                    victim: vn,
                    victim_team: vt,
                    weapon: weapon_name(&c.data, *weapon),
                    headshot: *headshot,
                    penetrated: *penetrated,
                    me_involved: mine,
                    at: now,
                });
                if hud.kills.len() > 8 {
                    hud.kills.remove(0);
                }
            }
            GameEvent::Footstep { player, pos, loud } => {
                if Some(*player) != me && *loud {
                    sounds.play_at(&mut commands, Sfx::Footstep, *pos, ear, 1100.0, 1.0);
                } else if Some(*player) == me {
                    sounds.play(&mut commands, Sfx::Footstep, 0.25);
                }
            }
            GameEvent::Land { player, pos } => {
                if Some(*player) == me {
                    sounds.play(&mut commands, Sfx::Land, 0.4);
                } else {
                    sounds.play_at(&mut commands, Sfx::Land, *pos, ear, 1100.0, 0.8);
                }
            }
            GameEvent::GrenadeDetonated { kind, pos, .. } => {
                let sfx = match kind {
                    GrenadeKind::He => Sfx::Explosion,
                    GrenadeKind::Flash => Sfx::Flash,
                    GrenadeKind::Smoke => Sfx::SmokePop,
                    GrenadeKind::Molotov => Sfx::Explosion,
                };
                sounds.play_at(
                    &mut commands,
                    sfx,
                    *pos,
                    ear,
                    3000.0,
                    if *kind == GrenadeKind::Molotov {
                        0.4
                    } else {
                        1.0
                    },
                );
                if *kind == GrenadeKind::He
                    && let Some(a) = &a
                {
                    let e = commands
                        .spawn((
                            Mesh3d(a.sphere.clone()),
                            MeshMaterial3d(a.fire.clone()),
                            Transform::from_translation(*pos * SCALE).with_scale(Vec3::splat(3.0)),
                            NotShadowCaster,
                        ))
                        .id();
                    scene.temps.push((e, now + 0.12));
                }
            }
            GameEvent::Flashed {
                player,
                duration,
                alpha,
            } => {
                if Some(*player) == me {
                    let full = (*duration * 0.35).min(1.5) as f64;
                    hud.flash_full_until = now + full;
                    hud.flash_until = now + *duration as f64;
                    hud.flash_alpha = *alpha;
                }
            }
            GameEvent::Reload { player } if Some(*player) == me => {
                sounds.play(&mut commands, Sfx::Reload, 0.5)
            }
            GameEvent::DryFire { player } if Some(*player) == me => {
                sounds.play(&mut commands, Sfx::DryFire, 0.6)
            }
            GameEvent::BombPlanted { .. } => {
                sounds.play(&mut commands, Sfx::Plant, 0.8);
                hud.round_banner = Some(("La bombe est posée".into(), now));
            }
            GameEvent::BombDefused { .. } => sounds.play(&mut commands, Sfx::Defuse, 0.8),
            GameEvent::RoundEnded { winner, reason, .. } => {
                let side = match winner {
                    Team::Attack => "Les attaquants",
                    Team::Defense => "Les défenseurs",
                    Team::Spectator => "Personne",
                };
                let why = match reason {
                    game_rules::RoundEndReason::AttackersEliminated => "attaquants éliminés",
                    game_rules::RoundEndReason::DefendersEliminated => "défenseurs éliminés",
                    game_rules::RoundEndReason::BombExploded => "la bombe a explosé",
                    game_rules::RoundEndReason::BombDefused => "bombe désamorcée",
                    game_rules::RoundEndReason::TimeExpired => "temps écoulé",
                };
                hud.round_banner = Some((format!("{side} remportent la manche ({why})"), now));
            }
            GameEvent::MatchEnded { winner_team_score } => {
                hud.round_banner = Some((
                    format!(
                        "Fin du match : {} – {}",
                        winner_team_score[0], winner_team_score[1]
                    ),
                    now,
                ));
            }
            GameEvent::Chat {
                from,
                team_only,
                text,
            } => {
                let (n, _) = from
                    .map(|f| player_name(&c.players, f))
                    .unwrap_or_else(|| ("serveur".into(), Team::Spectator));
                let tag = if *team_only { "(équipe) " } else { "" };
                hud.chat_log.push((format!("{tag}{n} : {text}"), now));
                if hud.chat_log.len() > 40 {
                    hud.chat_log.remove(0);
                }
            }
            GameEvent::Notice { text } => hud.notice(text.clone(), now),
            GameEvent::Bought { player, item } if Some(*player) == me => {
                sounds.play(&mut commands, Sfx::Click, 0.5);
                let name = c
                    .data
                    .catalog
                    .by_name(item)
                    .map(|w| c.data.catalog.get(w).name.clone())
                    .unwrap_or_else(|| item.clone());
                hud.notice(format!("Acheté : {name}"), now);
            }
            _ => {}
        }
    }
    hud.damage_from.retain(|(_, t)| now - *t < 1.5);
    hud.notices.retain(|(_, t)| now - *t < 6.0);
}

/// Keeps the sound volume in step with the settings.
pub fn sync_volume(settings: Res<Settings>, sounds: Option<ResMut<Sounds>>) {
    if let Some(mut s) = sounds
        && (s.volume - settings.volume).abs() > 1e-4
    {
        s.volume = settings.volume;
    }
}
