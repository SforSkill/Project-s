//! In-game presentation: input sampling, camera, remote players, grenades,
//! smokes, fires, bomb, tracers, viewmodel and event effects.

use crate::audio::{Sfx, Sounds};
use crate::character::anim::Hold;
use crate::character::{CharEvent, CharState, CharacterFeed, FirstPerson};
use crate::net::{Clock, Net};
use crate::settings::{FOV, Settings, vertical_fov};
use crate::world::{MapEntity, SCALE, spawn_map};
use bevy::camera::visibility::RenderLayers;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, WindowMode};
use ps_sim::events::GameEvent;
use ps_sim::smoke::SmokeVolume;
use ps_sim_core::cmd::Buttons;
use ps_sim_core::{PlayerId, Team, ViewAngles};
use ps_sim_net::proto::{SelfState, eflags};
use ps_weapons::{GrenadeKind, HitGroup, Slot, WeaponId, WeaponKind};
use std::collections::HashMap;

pub const VIEWMODEL_LAYER: usize = 1;

#[derive(Component)]
pub struct MainCam;
#[derive(Component)]
pub struct ViewCam;
#[derive(Component)]
pub struct Viewmodel;

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
    mut images: ResMut<Assets<Image>>,
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
            fov: vertical_fov(FOV),
            near: 0.02,
            far: 600.0,
            ..default()
        }),
        Transform::from_xyz(0.0, 2.0, 0.0),
        crate::sky::skybox(&mut images),
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
    rebind: Res<crate::bindings::Rebind>,
) {
    if autotest.is_some() {
        return;
    }
    let pressed = |name: &str| crate::bindings::pressed(name, &keys, &mouse);
    let just = |name: &str| crate::bindings::just_pressed(name, &keys, &mouse);
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
    // Toggles available even with the cursor free. During a key capture in the
    // settings, Escape belongs to the capture (it cancels it).
    if keys.just_pressed(KeyCode::Escape) && rebind.0.is_none() {
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
    let k = &settings.keys;
    hud.scoreboard = pressed(&k.scoreboard) && !hud.chat_open;
    if !capture {
        s.timeline.set_buttons(now, Buttons(0));
        return;
    }
    if just(&k.buy) {
        hud.buy_menu = true;
    }
    if just(&k.inspector) {
        hud.inspector = !hud.inspector;
    }
    let chat_team = just(&k.chat_team);
    if chat_team || just(&k.chat) {
        hud.chat_open = true;
        hud.chat_team = chat_team;
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
    let mut b = 0u32;
    let map = [
        (&k.forward, Buttons::FORWARD),
        (&k.back, Buttons::BACK),
        (&k.left, Buttons::LEFT),
        (&k.right, Buttons::RIGHT),
        (&k.jump, Buttons::JUMP),
        (&k.duck, Buttons::DUCK),
        (&k.walk, Buttons::WALK),
        (&k.reload, Buttons::RELOAD),
        (&k.use_key, Buttons::USE),
        (&k.drop, Buttons::DROP),
        (&k.inspect, Buttons::INSPECT),
        (&k.attack, Buttons::ATTACK),
        (&k.attack2, Buttons::ATTACK2),
    ];
    for (name, bit) in map {
        if pressed(name) {
            b |= bit;
        }
    }
    s.timeline.set_buttons(now, Buttons(b));
    for (name, slot) in k.slots() {
        if just(name) {
            s.timeline.select(slot);
        }
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
    textures: Res<crate::textures::SurfaceTextures>,
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
                &textures,
            );
            scene.map = Some(name);
        }
    }
}

fn clear_dynamic(commands: &mut Commands, scene: &mut Scene) {
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

/// Remote players, grenades, drops, smokes, fires, bomb; expires temporary effects.
pub fn update_scene(
    mut commands: Commands,
    mut scene: ResMut<Scene>,
    mut net: ResMut<Net>,
    clock: Res<Clock>,
    mut feed: ResMut<CharacterFeed>,
    mut transforms: Query<(&mut Transform, &mut Visibility), Without<MainCam>>,
) {
    let now = clock.now();
    feed.players.clear();
    let blasts: Vec<(Vec3, f64)> = net
        .session
        .as_mut()
        .map(|s| std::mem::take(&mut s.client.smoke_blasts))
        .unwrap_or_default();
    let Some(s) = &net.session else {
        feed.me = None;
        if scene.map.is_none() {
            clear_dynamic(&mut commands, &mut scene);
        }
        return;
    };
    let Some(a) = scene.assets.clone() else {
        return;
    };
    let c = &s.client;
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
    let catalog = &c.data.catalog;
    let hold_of = |w: Option<WeaponId>, flags: u16| {
        let def = w.and_then(|w| catalog.try_get(w));
        let hold = if flags & eflags::NO_WEAPON != 0 {
            Hold::Empty
        } else if flags & eflags::PLANTING != 0 {
            Hold::Bomb
        } else {
            crate::character::hold_for(def.map(|d| d.kind))
        };
        (
            hold,
            def.map_or(2.5, |d| d.reload_time),
            def.map_or(1.0, |d| d.deploy_time),
        )
    };
    for r in &remotes {
        let (hold, reload_time, deploy_time) = hold_of(r.weapon, r.flags);
        feed.players.push(CharState {
            id: r.id,
            origin: r.origin,
            velocity: r.velocity,
            yaw: r.yaw,
            pitch: r.pitch,
            duck: r.duck,
            on_ground: r.flags & eflags::ON_GROUND != 0,
            alive: r.alive(),
            visible: r.flags & eflags::STALE == 0,
            working: r.flags & (eflags::PLANTING | eflags::DEFUSING) != 0,
            reloading: r.flags & eflags::RELOADING != 0,
            team: r.team,
            hold,
            weapon_key: r.weapon.map_or(u16::MAX - 1, |w| w.0 as u16),
            reload_time,
            deploy_time,
            palette: None,
        });
    }
    let me_state = c.latest().and_then(|sn| sn.me.as_ref());
    feed.viewer = me_state.map_or(Team::Spectator, |m| m.team);
    feed.free_for_all = c
        .welcome
        .as_ref()
        .is_some_and(|w| w.rules.mode == ps_rules::Mode::Deathmatch);
    feed.me = match (me_state, c.me()) {
        (Some(m), Some(id)) if m.team != Team::Spectator => {
            let w = active_weapon(m);
            let (hold, _, _) = hold_of(w, 0);
            let yaw = s.timeline.view().yaw;
            Some(FirstPerson {
                id,
                alive: m.alive,
                team: m.team,
                hold,
                weapon_key: w.map_or(u16::MAX - 1, |w| w.0 as u16),
                reload_left: m
                    .hand
                    .reload_until
                    .map(|t| (t - server_now).max(0.0) as f32),
                deploy_left: (m.hand.deploy_until - server_now).max(0.0) as f32,
                velocity: Quat::from_rotation_y(-yaw.to_radians()) * c.mv.velocity,
                on_ground: c.mv.on_ground,
            })
        }
        _ => None,
    };
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
    let mut fov = FOV;
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

/// Viewmodel anchor on its own render layer: `camera` places it, the
/// liquid arms and their weapon hang from it (`character::runtime`).
pub fn setup_viewmodel(mut commands: Commands, cam: Query<Entity, With<ViewCam>>) {
    let Ok(cam) = cam.single() else { return };
    let vm = commands
        .spawn((
            Viewmodel,
            Transform::from_xyz(0.12, -0.12, -0.3),
            Visibility::Hidden,
            RenderLayers::layer(VIEWMODEL_LAYER),
        ))
        .id();
    commands.entity(cam).add_child(vm);
}

fn player_name(players: &[ps_sim_net::proto::PlayerInfo], id: PlayerId) -> (String, Team) {
    players
        .iter()
        .find(|p| p.id == id.0)
        .map(|p| (p.name.clone(), p.team))
        .unwrap_or_else(|| (format!("#{}", id.0), Team::Spectator))
}

fn weapon_name(data: &ps_sim::GameData, w: Option<WeaponId>) -> String {
    w.and_then(|w| data.catalog.try_get(w))
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "monde".into())
}

/// Cosmetic triggers for the liquid characters (animation and droplets).
fn forward_character_events(evs: &[GameEvent], feed: &mut CharacterFeed) {
    for ev in evs {
        let e = match ev {
            GameEvent::Shot { shooter, .. } => (shooter.0, CharEvent::Fire),
            GameEvent::Damage {
                victim,
                group,
                from,
                ..
            } => (
                victim.0,
                CharEvent::Hit {
                    from: *from,
                    head: *group == Some(HitGroup::Head),
                },
            ),
            GameEvent::Footstep { player, .. } => (player.0, CharEvent::Footstep),
            GameEvent::Land { player, .. } => (player.0, CharEvent::Land),
            _ => continue,
        };
        feed.events.push(e);
    }
}

/// Game events to sounds, HUD feedback and short-lived visual effects.
pub fn events(
    mut commands: Commands,
    mut net: ResMut<Net>,
    clock: Res<Clock>,
    mut hud: ResMut<Hud>,
    mut scene: ResMut<Scene>,
    mut feed: ResMut<CharacterFeed>,
    sounds: Option<Res<Sounds>>,
) {
    let now = clock.now();
    feed.eye = hud.eye * SCALE;
    let Some(s) = &mut net.session else { return };
    let evs = s.client.take_events();
    forward_character_events(&evs, &mut feed);
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
                    ps_rules::RoundEndReason::AttackersEliminated => "attaquants éliminés",
                    ps_rules::RoundEndReason::DefendersEliminated => "défenseurs éliminés",
                    ps_rules::RoundEndReason::BombExploded => "la bombe a explosé",
                    ps_rules::RoundEndReason::BombDefused => "bombe désamorcée",
                    ps_rules::RoundEndReason::TimeExpired => "temps écoulé",
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

/// Keeps the mixer in step with the settings.
pub fn sync_volume(settings: Res<Settings>, sounds: Option<ResMut<Sounds>>) {
    if let Some(mut s) = sounds {
        let mix = settings.mix();
        if s.mix != mix {
            s.mix = mix;
        }
    }
}

/// Saves the settings shortly after the audio sliders stop moving (one write
/// per drag, not one per frame).
pub fn autosave_audio(
    settings: Res<Settings>,
    time: Res<Time>,
    mut last: Local<Option<crate::audio::Mix>>,
    mut dirty_since: Local<Option<f32>>,
) {
    let mix = settings.mix();
    let now = time.elapsed_secs();
    match *last {
        None => *last = Some(mix),
        Some(prev) if prev != mix => {
            *last = Some(mix);
            *dirty_since = Some(now);
        }
        _ => {}
    }
    if let Some(t) = *dirty_since
        && now - t > 0.6
    {
        settings.save();
        *dirty_since = None;
    }
}
