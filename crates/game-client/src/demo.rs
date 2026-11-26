//! `--character-demo`: test scene for the liquid characters.
//!
//! 360° orbit, every animation and weapon, palettes, quality levels, shader
//! parameters, hitbox overlay, distance lineup (LOD), dark and bright zones,
//! first-person preview and a stress crowd. With `--shots DIR` it plays a
//! fixed script, saves screenshots and a JSON report (frame times per
//! quality level, triangle counts, pools), then quits (`docs/characters.md`).

use crate::character::anim::Hold;
use crate::character::fx::LiquidFx;
use crate::character::liquid::{CharacterStyle, LiquidMaterials};
use crate::character::runtime::{CharacterAssets, Characters};
use crate::character::style::{LiquidStyle, PaletteId, Quality};
use crate::character::{self, CharEvent, CharState, CharacterFeed, FirstPerson};
use crate::game::{VIEWMODEL_LAYER, Viewmodel};
use crate::settings::{Settings, vertical_fov};
use crate::world::SCALE;
use bevy::camera::Viewport;
use bevy::camera::visibility::RenderLayers;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::PresentMode;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use engine_core::Team;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Motion {
    Idle,
    Walk,
    Run,
    Strafe,
    Backward,
    Crouch,
    CrouchWalk,
    Jump,
}

impl Motion {
    const ALL: [Motion; 8] = [
        Motion::Idle,
        Motion::Walk,
        Motion::Run,
        Motion::Strafe,
        Motion::Backward,
        Motion::Crouch,
        Motion::CrouchWalk,
        Motion::Jump,
    ];

    fn label(self) -> &'static str {
        match self {
            Motion::Idle => "Repos",
            Motion::Walk => "Marche",
            Motion::Run => "Course",
            Motion::Strafe => "Pas latéral",
            Motion::Backward => "Recul",
            Motion::Crouch => "Accroupi",
            Motion::CrouchWalk => "Marche accroupie",
            Motion::Jump => "Saut / chute",
        }
    }

    /// Model-space velocity (u/s) and crouch amount.
    fn velocity(self) -> (Vec3, f32) {
        match self {
            Motion::Idle | Motion::Jump => (Vec3::ZERO, 0.0),
            Motion::Walk => (Vec3::new(0.0, 0.0, -130.0), 0.0),
            Motion::Run => (Vec3::new(0.0, 0.0, -250.0), 0.0),
            Motion::Strafe => (Vec3::new(240.0, 0.0, 0.0), 0.0),
            Motion::Backward => (Vec3::new(0.0, 0.0, 230.0), 0.0),
            Motion::Crouch => (Vec3::ZERO, 1.0),
            Motion::CrouchWalk => (Vec3::new(0.0, 0.0, -85.0), 1.0),
        }
    }
}

type Apply = Box<dyn Fn(&mut Demo, &mut Settings, &mut LiquidStyle) + Send + Sync>;

struct Step {
    name: &'static str,
    wait: f32,
    apply: Apply,
    /// Frame times are recorded for this step (performance measurement).
    measure: bool,
}

#[derive(Default)]
struct Script {
    out: PathBuf,
    steps: Vec<Step>,
    index: usize,
    t: f32,
    applied: bool,
    results: Vec<serde_json::Value>,
    frame_ms: Vec<f32>,
}

#[derive(Resource)]
struct Demo {
    motion: Motion,
    hold: Hold,
    pitch: f32,
    team: Team,
    palette: Option<PaletteId>,
    alive: bool,
    working: bool,
    auto_fire: bool,
    auto_orbit: bool,
    yaw: f32,
    cam_pitch: f32,
    dist: f32,
    focus: Vec3,
    hitboxes: bool,
    subject: bool,
    lineup: bool,
    palettes: bool,
    zones: bool,
    first_person: bool,
    crowd: usize,
    reload: f32,
    t: f32,
    jump_t: f32,
    fire_acc: f32,
    events: Vec<(u8, CharEvent)>,
    script: Option<Script>,
}

impl Demo {
    fn new(shots: Option<PathBuf>) -> Demo {
        Demo {
            motion: Motion::Idle,
            hold: Hold::Rifle,
            pitch: 0.0,
            team: Team::Defense,
            palette: None,
            alive: true,
            working: false,
            auto_fire: false,
            auto_orbit: shots.is_none(),
            yaw: 20.0,
            cam_pitch: -8.0,
            dist: 3.2,
            focus: Vec3::new(0.0, 1.0, 0.0),
            hitboxes: false,
            subject: true,
            lineup: false,
            palettes: false,
            zones: true,
            first_person: true,
            crowd: 0,
            reload: 0.0,
            t: 0.0,
            jump_t: 0.0,
            fire_acc: 0.0,
            events: Vec::new(),
            script: shots.map(|out| Script {
                out,
                steps: script_steps(),
                ..Default::default()
            }),
        }
    }

    fn fire(&mut self) {
        self.events.push((SUBJECT, CharEvent::Fire));
        self.events.push((ME, CharEvent::Fire));
    }

    fn hit(&mut self, head: bool, from_m: Vec3) {
        self.events.push((
            SUBJECT,
            CharEvent::Hit {
                from: from_m / SCALE,
                head,
            },
        ));
    }

    /// Resets the scene to the default framing (script steps start here).
    fn reset(&mut self) {
        let script = self.script.take();
        *self = Demo::new(None);
        self.script = script;
        self.auto_orbit = false;
    }
}

const SUBJECT: u8 = 1;
const ME: u8 = 200;

#[derive(Component)]
struct DemoCam;

pub fn run(args: &[String]) -> AppExit {
    let shots = args
        .iter()
        .position(|a| a == "--shots")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    let mut settings = Settings::load();
    if shots.is_some() {
        settings.characters = Default::default();
    }
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Projet S — personnages liquides".into(),
                resolution: (1600, 900).into(),
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_plugins(character::plugin)
        // Edited live, never saved: the demo does not touch settings.toml.
        .insert_resource(settings)
        .insert_resource(Demo::new(shots))
        .insert_resource(ClearColor(Color::srgb(0.53, 0.6, 0.68)))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                script,
                orbit,
                feed,
                character::runtime::drive,
                character::fx::update_fx,
                gizmos,
            )
                .chain(),
        )
        .add_systems(EguiPrimaryContextPass, panel)
        .run()
}

fn setup(
    mut commands: Commands,
    mut egui_settings: ResMut<bevy_egui::EguiGlobalSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let mat = |mats: &mut Assets<StandardMaterial>, c: Color| {
        mats.add(StandardMaterial {
            base_color: c,
            perceptual_roughness: 0.9,
            ..default()
        })
    };
    // egui draws on the main camera, not on the first-person inset.
    egui_settings.auto_create_primary_context = false;
    let ground = mat(&mut mats, Color::srgb(0.36, 0.37, 0.38));
    let white = mat(&mut mats, Color::srgb(0.92, 0.92, 0.9));
    let dark = mat(&mut mats, Color::srgb(0.07, 0.07, 0.08));
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let mut block = |pos: Vec3, size: Vec3, m: &Handle<StandardMaterial>| {
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(m.clone()),
            Transform::from_translation(pos).with_scale(size),
        ));
    };
    block(
        Vec3::new(0.0, -0.05, -20.0),
        Vec3::new(80.0, 0.1, 140.0),
        &ground,
    );
    // Bright zone (right): white floor and wall.
    block(Vec3::new(5.5, 0.01, 0.0), Vec3::new(3.0, 0.02, 3.0), &white);
    block(Vec3::new(5.5, 1.5, -1.6), Vec3::new(3.0, 3.0, 0.2), &white);
    // Dark zone (left): black box with a roof, in full shadow.
    block(Vec3::new(-5.5, 0.01, 0.0), Vec3::new(3.0, 0.02, 3.0), &dark);
    block(Vec3::new(-5.5, 2.6, -0.2), Vec3::new(3.4, 0.2, 3.4), &dark);
    block(Vec3::new(-5.5, 1.3, -1.8), Vec3::new(3.4, 2.6, 0.2), &dark);
    block(Vec3::new(-7.1, 1.3, -0.2), Vec3::new(0.2, 2.6, 3.4), &dark);
    // Same lighting as the game maps (`world.rs`).
    commands.insert_resource(bevy::light::GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 350.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 9000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(20.0, 60.0, 30.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 1800.0,
            shadow_maps_enabled: false,
            color: Color::srgb(0.75, 0.82, 1.0),
            ..default()
        },
        Transform::from_xyz(-30.0, 40.0, -20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DemoCam,
        bevy_egui::PrimaryEguiContext,
        Camera3d::default(),
        Projection::from(PerspectiveProjection {
            fov: vertical_fov(70.0),
            near: 0.02,
            far: 600.0,
            ..default()
        }),
        Transform::from_xyz(0.0, 1.4, 3.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    ));
    // First-person preview (bottom right): same arms as in game.
    let layer = RenderLayers::layer(VIEWMODEL_LAYER);
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 1,
            viewport: Some(Viewport {
                physical_position: UVec2::new(1100, 590),
                physical_size: UVec2::new(480, 290),
                ..default()
            }),
            clear_color: ClearColorConfig::Custom(Color::srgb(0.2, 0.22, 0.25)),
            ..default()
        },
        Projection::from(PerspectiveProjection {
            fov: vertical_fov(68.0),
            near: 0.01,
            far: 10.0,
            ..default()
        }),
        layer.clone(),
        children![(
            Viewmodel,
            Transform::from_xyz(0.06, -0.055, -0.16),
            Visibility::Inherited,
            layer.clone(),
        )],
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 6000.0,
            ..default()
        },
        Transform::from_xyz(1.0, 2.0, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
        layer,
    ));
}

fn orbit(
    mut demo: ResMut<Demo>,
    time: Res<Time>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut contexts: EguiContexts,
    mut cam: Query<&mut Transform, With<DemoCam>>,
) {
    let over_ui = contexts
        .ctx_mut()
        .is_ok_and(|c| c.egui_wants_pointer_input() || c.is_pointer_over_egui());
    if demo.auto_orbit {
        demo.yaw += time.delta_secs() * 24.0;
    }
    if !over_ui {
        if buttons.pressed(MouseButton::Left) {
            demo.yaw -= motion.delta.x * 0.3;
            demo.cam_pitch = (demo.cam_pitch - motion.delta.y * 0.3).clamp(-80.0, 80.0);
        }
        demo.dist = (demo.dist * (1.0 - scroll.delta.y * 0.1)).clamp(0.8, 80.0);
    }
    let Ok(mut t) = cam.single_mut() else { return };
    // Orbit 0° = facing the subject (models look down -Z).
    let rot = Quat::from_euler(
        EulerRot::YXZ,
        (demo.yaw + 180.0).to_radians(),
        demo.cam_pitch.to_radians(),
        0.0,
    );
    t.translation = demo.focus + rot * Vec3::new(0.0, 0.0, demo.dist);
    t.look_at(demo.focus, Vec3::Y);
}

fn idle(id: u8, pos_m: Vec3, yaw: f32, team: Team) -> CharState {
    CharState {
        id,
        origin: pos_m / SCALE,
        velocity: Vec3::ZERO,
        yaw,
        pitch: 0.0,
        duck: 0.0,
        on_ground: true,
        alive: true,
        visible: true,
        working: false,
        reloading: false,
        team,
        hold: Hold::Rifle,
        weapon_key: 0,
        reload_time: 2.5,
        deploy_time: 1.0,
        palette: None,
    }
}

fn feed(
    mut demo: ResMut<Demo>,
    time: Res<Time>,
    mut f: ResMut<CharacterFeed>,
    cam: Query<&Transform, With<DemoCam>>,
) {
    let dt = time.delta_secs();
    let d = &mut *demo;
    d.t += dt;
    d.reload = (d.reload - dt).max(0.0);
    if d.auto_fire && d.alive {
        d.fire_acc += dt;
        if d.fire_acc > 0.12 {
            d.fire_acc = 0.0;
            d.fire();
        }
    }
    let eye = cam.single().map(|t| t.translation).unwrap_or(Vec3::ZERO);
    let (mut vel, duck) = d.motion.velocity();
    let mut origin = Vec3::ZERO;
    let mut on_ground = true;
    if d.motion == Motion::Jump {
        d.jump_t += dt;
        let t = d.jump_t % 1.5;
        if (0.3..1.05).contains(&t) {
            let u = (t - 0.3) / 0.75;
            origin.y = 4.0 * u * (1.0 - u) * 50.0;
            vel.y = (1.0 - 2.0 * u) * 260.0;
            on_ground = false;
        }
        if t >= 1.05 && t - dt < 1.05 {
            d.events.push((SUBJECT, CharEvent::Land));
        }
    }
    f.players.clear();
    if d.subject {
        f.players.push(CharState {
            origin,
            velocity: vel,
            pitch: d.pitch,
            duck,
            on_ground,
            alive: d.alive,
            working: d.working,
            reloading: d.reload > 0.0,
            hold: d.hold,
            weapon_key: d.hold as u16,
            palette: d.palette,
            ..idle(SUBJECT, Vec3::ZERO, 0.0, d.team)
        });
    }
    if d.zones {
        f.players
            .push(idle(2, Vec3::new(-5.5, 0.0, 0.0), 0.0, Team::Attack));
        f.players
            .push(idle(3, Vec3::new(5.5, 0.0, 0.0), 0.0, Team::Defense));
    }
    if d.lineup {
        for (i, dist) in [5.0, 12.0, 20.0, 30.0, 45.0, 60.0].into_iter().enumerate() {
            let team = if i % 2 == 0 {
                Team::Attack
            } else {
                Team::Defense
            };
            f.players.push(idle(
                10 + i as u8,
                Vec3::new(-1.0 + i as f32 * 0.4, 0.0, -dist),
                180.0,
                team,
            ));
        }
    }
    if d.palettes {
        for (i, p) in PaletteId::ALL.into_iter().enumerate() {
            let mut c = idle(
                30 + i as u8,
                Vec3::new(-4.0 + i as f32, 0.0, -2.5),
                0.0,
                Team::Defense,
            );
            c.palette = Some(p);
            f.players.push(c);
        }
    }
    for i in 0..d.crowd {
        let a = d.t * 0.5 + i as f32 / d.crowd as f32 * std::f32::consts::TAU;
        let r = 6.0 + (i % 3) as f32;
        let pos = Vec3::new(a.cos() * r, 0.0, a.sin() * r - 8.0);
        // Running along the circle (tangent).
        let yaw = (-a).to_degrees();
        let mut c = idle(
            60 + i as u8,
            pos,
            yaw,
            if i % 2 == 0 {
                Team::Attack
            } else {
                Team::Defense
            },
        );
        c.velocity = Quat::from_rotation_y(yaw.to_radians()) * Vec3::new(0.0, 0.0, -250.0);
        f.players.push(c);
        if i % 4 == 0 && (d.t * 3.0 + i as f32).fract() < dt * 3.0 {
            d.events.push((60 + i as u8, CharEvent::Fire));
        }
    }
    // The model-space velocity of the subject is turned into world space
    // (yaw 0 → identity), the feed expects world velocities.
    f.viewer = Team::Defense;
    f.free_for_all = false;
    f.eye = eye;
    f.me = d.first_person.then_some(FirstPerson {
        id: ME,
        alive: d.alive,
        team: d.team,
        hold: d.hold,
        weapon_key: d.hold as u16,
        reload_left: (d.reload > 0.0).then_some(d.reload),
        deploy_left: 0.6,
        velocity: vel,
        on_ground,
    });
    f.events.append(&mut d.events);
}

fn gizmos(demo: Res<Demo>, f: Res<CharacterFeed>, mut g: Gizmos) {
    if !demo.hitboxes {
        return;
    }
    for p in &f.players {
        if !p.alive {
            continue;
        }
        let hb = engine_hitreg::hitboxes(&engine_hitreg::Pose {
            origin: p.origin,
            yaw: p.yaw,
            duck: p.duck,
        });
        for c in &hb.capsules {
            let (a, b, r) = (c.a * SCALE, c.b * SCALE, c.radius * SCALE);
            let col = Color::srgb(1.0, 0.9, 0.1);
            g.line(a, b, col);
            g.sphere(Isometry3d::from_translation(a), r, col);
            g.sphere(Isometry3d::from_translation(b), r, col);
        }
    }
}

fn script(
    mut commands: Commands,
    mut demo: ResMut<Demo>,
    mut settings: ResMut<Settings>,
    mut style: ResMut<CharacterStyle>,
    time: Res<Time>,
    chars: Res<Characters>,
    fx: Res<LiquidFx>,
    mats: Res<LiquidMaterials>,
    assets: Option<Res<CharacterAssets>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(mut s) = demo.script.take() else {
        return;
    };
    // Let shaders compile and the pools settle before the first step.
    if time.elapsed_secs() < 3.0 {
        let _ = std::fs::create_dir_all(&s.out);
        demo.script = Some(s);
        return;
    }
    if s.index >= s.steps.len() {
        // Give the last screenshot time to be written.
        s.t += time.delta_secs();
        if s.t < 1.5 {
            demo.script = Some(s);
            return;
        }
        let report = serde_json::json!({
            "steps": s.results,
            "triangles_per_lod": assets.map(|a| a.triangles.to_vec()),
            "adapter": "voir le journal wgpu",
        });
        let _ = std::fs::create_dir_all(&s.out);
        let _ = std::fs::write(
            s.out.join("report.json"),
            serde_json::to_string_pretty(&report).unwrap_or_default(),
        );
        exit.write(AppExit::Success);
        return;
    }
    if !s.applied {
        demo.reset();
        let mut st = LiquidStyle::default();
        settings.characters = Default::default();
        (s.steps[s.index].apply)(&mut demo, &mut settings, &mut st);
        if style.0 != st {
            style.0 = st;
        }
        s.applied = true;
        s.t = 0.0;
        s.frame_ms.clear();
    }
    s.t += time.delta_secs();
    let step = &s.steps[s.index];
    if step.measure && s.t > 1.0 {
        s.frame_ms.push(time.delta_secs() * 1000.0);
    }
    if s.t >= step.wait {
        let path = s.out.join(format!("{:02}-{}.png", s.index + 1, step.name));
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
        let mut ms = s.frame_ms.clone();
        ms.sort_by(|a, b| a.total_cmp(b));
        let pct = |p: f32| {
            ms.get(((ms.len().max(1) - 1) as f32 * p) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        let avg = if ms.is_empty() {
            0.0
        } else {
            ms.iter().sum::<f32>() / ms.len() as f32
        };
        s.results.push(serde_json::json!({
            "step": step.name,
            "quality": settings.characters.quality.label(),
            "characters": chars.count(),
            "drops_active": fx.active_drops(),
            "drops_pool": fx.pool_size(),
            "drops_refused": fx.dropped,
            "puddles": fx.active_puddles(),
            "materials_shared": mats.shared_count(),
            "materials_lent": mats.lent_count(),
            "frame_ms": if step.measure { serde_json::json!({"avg": avg, "p50": pct(0.5), "p95": pct(0.95), "max": pct(1.0), "frames": ms.len()}) } else { serde_json::Value::Null },
        }));
        s.index += 1;
        s.applied = false;
        s.t = 0.0;
    }
    demo.script = Some(s);
}

fn view(yaw: f32, pitch: f32, dist: f32) -> impl Fn(&mut Demo) {
    move |d: &mut Demo| {
        d.yaw = yaw;
        d.cam_pitch = pitch;
        d.dist = dist;
    }
}

fn script_steps() -> Vec<Step> {
    fn step(
        name: &'static str,
        wait: f32,
        f: impl Fn(&mut Demo, &mut Settings, &mut LiquidStyle) + Send + Sync + 'static,
    ) -> Step {
        Step {
            name,
            wait,
            apply: Box::new(f),
            measure: false,
        }
    }
    fn perf(name: &'static str, q: Quality) -> Step {
        Step {
            name,
            wait: 5.0,
            apply: Box::new(move |d, s, _| {
                s.characters.quality = q;
                d.crowd = 24;
                d.lineup = true;
                d.yaw = 30.0;
                d.cam_pitch = -18.0;
                d.dist = 14.0;
                d.focus = Vec3::new(0.0, 0.5, -6.0);
            }),
            measure: true,
        }
    }
    vec![
        step("face", 1.2, |d, _, _| view(0.0, -6.0, 2.8)(d)),
        step("trois-quarts", 1.2, |d, _, _| view(35.0, -6.0, 2.8)(d)),
        step("profil", 1.2, |d, _, _| view(90.0, -4.0, 2.8)(d)),
        step("dos", 1.2, |d, _, _| view(180.0, -6.0, 2.8)(d)),
        step("hitboxes", 1.2, |d, _, _| {
            view(35.0, -6.0, 2.8)(d);
            d.hitboxes = true;
        }),
        step("course", 1.37, |d, _, _| {
            view(90.0, -4.0, 3.0)(d);
            d.motion = Motion::Run;
        }),
        step("marche", 1.5, |d, _, _| {
            view(90.0, -4.0, 3.0)(d);
            d.motion = Motion::Walk;
        }),
        step("pas-lateral", 1.3, |d, _, _| {
            view(20.0, -4.0, 3.0)(d);
            d.motion = Motion::Strafe;
        }),
        step("accroupi", 1.2, |d, _, _| {
            view(35.0, -8.0, 2.6)(d);
            d.motion = Motion::Crouch;
        }),
        step("accroupi-hitboxes", 1.2, |d, _, _| {
            view(90.0, -4.0, 2.6)(d);
            d.motion = Motion::CrouchWalk;
            d.hitboxes = true;
        }),
        step("saut", 0.75, |d, _, _| {
            view(60.0, -4.0, 3.2)(d);
            d.motion = Motion::Jump;
        }),
        step("pistolet-visee-haute", 1.2, |d, _, _| {
            view(60.0, -4.0, 2.6)(d);
            d.hold = Hold::Pistol;
            d.pitch = 35.0;
        }),
        step("sniper-visee-basse", 1.2, |d, _, _| {
            view(60.0, -4.0, 2.6)(d);
            d.hold = Hold::Sniper;
            d.pitch = -30.0;
        }),
        step("couteau", 1.2, |d, _, _| {
            view(30.0, -4.0, 2.6)(d);
            d.hold = Hold::Knife;
        }),
        step("grenade", 1.2, |d, _, _| {
            view(30.0, -4.0, 2.6)(d);
            d.hold = Hold::Grenade;
        }),
        step("rechargement", 1.0, |d, _, _| {
            view(40.0, -4.0, 2.4)(d);
            d.reload = 2.5;
        }),
        step("pose-bombe", 1.5, |d, _, _| {
            view(40.0, -8.0, 2.6)(d);
            d.hold = Hold::Bomb;
            d.working = true;
        }),
        step("impact", 0.12, |d, _, _| {
            view(25.0, -4.0, 2.6)(d);
            d.hit(false, Vec3::new(0.0, 1.4, 5.0));
            d.hit(true, Vec3::new(0.0, 1.4, 5.0));
        }),
        step("mort-0.4s", 0.4, |d, _, _| {
            view(30.0, -12.0, 3.0)(d);
            d.alive = false;
        }),
        step("mort-1.1s", 1.1, |d, _, _| {
            view(30.0, -12.0, 3.0)(d);
            d.alive = false;
        }),
        step("flaque-2.5s", 2.5, |d, _, _| {
            view(30.0, -20.0, 3.0)(d);
            d.alive = false;
        }),
        step("palettes", 1.2, |d, _, _| {
            view(0.0, -6.0, 6.5)(d);
            d.palettes = true;
            d.focus = Vec3::new(0.0, 0.9, -2.0);
        }),
        step("zones-sombre-claire", 1.2, |d, _, _| {
            view(0.0, -6.0, 10.0)(d);
            d.focus = Vec3::new(0.0, 1.0, 0.0);
        }),
        step("zone-sombre", 1.2, |d, _, _| {
            view(0.0, -4.0, 3.0)(d);
            d.focus = Vec3::new(-5.5, 1.0, 0.0);
        }),
        step("zone-claire", 1.2, |d, _, _| {
            view(0.0, -4.0, 3.0)(d);
            d.focus = Vec3::new(5.5, 1.0, 0.0);
        }),
        step("distances-lod", 1.2, |d, _, _| {
            d.lineup = true;
            d.subject = false;
            d.zones = false;
            view(180.0, -2.0, 0.1)(d);
            d.focus = Vec3::new(0.0, 1.6, 2.0);
        }),
        step("qualite-faible", 1.2, |d, s, _| {
            view(35.0, -6.0, 2.8)(d);
            s.characters.quality = Quality::Low;
        }),
        step("qualite-moyenne", 1.2, |d, s, _| {
            view(35.0, -6.0, 2.8)(d);
            s.characters.quality = Quality::Medium;
        }),
        step("qualite-ultra", 1.2, |d, s, _| {
            view(35.0, -6.0, 2.8)(d);
            s.characters.quality = Quality::Ultra;
        }),
        step("faible-loin", 1.2, |d, s, _| {
            s.characters.quality = Quality::Low;
            d.lineup = true;
            d.subject = false;
            d.zones = false;
            view(180.0, -2.0, 0.1)(d);
            d.focus = Vec3::new(0.0, 1.6, 2.0);
        }),
        step("ultra-loin", 1.2, |d, s, _| {
            s.characters.quality = Quality::Ultra;
            d.lineup = true;
            d.subject = false;
            d.zones = false;
            view(180.0, -2.0, 0.1)(d);
            d.focus = Vec3::new(0.0, 1.6, 2.0);
        }),
        perf("perf-faible", Quality::Low),
        perf("perf-moyenne", Quality::Medium),
        perf("perf-elevee", Quality::High),
        perf("perf-ultra", Quality::Ultra),
    ]
}

fn panel(
    mut contexts: EguiContexts,
    mut demo: ResMut<Demo>,
    mut settings: ResMut<Settings>,
    mut style: ResMut<CharacterStyle>,
    time: Res<Time>,
    chars: Res<Characters>,
    fx: Res<LiquidFx>,
    mats: Res<LiquidMaterials>,
    assets: Option<Res<CharacterAssets>>,
    cam: Query<&Transform, With<DemoCam>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let d = &mut *demo;
    egui::Window::new("Personnages liquides")
        .anchor(egui::Align2::LEFT_TOP, [8.0, 8.0])
        .default_width(320.0)
        .default_height(860.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Personnages liquides");
                ui.label(format!(
                    "{:.0} i/s · {:.2} ms",
                    1.0 / time.delta_secs().max(1e-4),
                    time.delta_secs() * 1000.0
                ));
                if d.script.is_some() {
                    ui.colored_label(egui::Color32::YELLOW, "Script de captures en cours…");
                }
                ui.separator();
                ui.strong("Vue");
                ui.checkbox(&mut d.auto_orbit, "Rotation 360° automatique");
                ui.add(egui::Slider::new(&mut d.yaw, -180.0..=540.0).text("Orbite"));
                ui.add(
                    egui::Slider::new(&mut d.dist, 0.8..=80.0)
                        .logarithmic(true)
                        .text("Distance (m)"),
                );
                ui.checkbox(&mut d.hitboxes, "Afficher les hitboxes");
                ui.checkbox(&mut d.subject, "Personnage principal");
                ui.checkbox(&mut d.zones, "Zones sombre / claire");
                ui.checkbox(&mut d.lineup, "Alignement en distance (LOD)");
                ui.checkbox(&mut d.palettes, "Toutes les palettes");
                ui.checkbox(&mut d.first_person, "Aperçu première personne");
                ui.add(egui::Slider::new(&mut d.crowd, 0..=60).text("Foule (test de charge)"));
                ui.separator();
                ui.strong("Couleurs et qualité");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut d.team, Team::Defense, "Défense");
                    ui.selectable_value(&mut d.team, Team::Attack, "Attaque");
                });
                egui::ComboBox::from_label("Palette forcée")
                    .selected_text(d.palette.map_or("aucune", |p| p.label()))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut d.palette, None, "aucune");
                        for p in PaletteId::ALL {
                            ui.selectable_value(&mut d.palette, Some(p), p.label());
                        }
                    });
                egui::Grid::new("demo_settings")
                    .num_columns(2)
                    .show(ui, |ui| {
                        crate::ui::character_settings_ui(ui, &mut settings.characters);
                    });
                ui.separator();
                ui.strong("Animation");
                ui.horizontal_wrapped(|ui| {
                    for m in Motion::ALL {
                        ui.selectable_value(&mut d.motion, m, m.label());
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for h in Hold::ALL {
                        ui.selectable_value(&mut d.hold, h, h.label());
                    }
                });
                ui.add(egui::Slider::new(&mut d.pitch, -89.0..=89.0).text("Visée (°)"));
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Tirer / frapper / lancer").clicked() {
                        d.fire();
                    }
                    ui.checkbox(&mut d.auto_fire, "Rafale");
                    if ui.button("Recharger").clicked() {
                        d.reload = 2.5;
                    }
                    ui.checkbox(&mut d.working, "Poser / désamorcer");
                    let from = cam.single().map(|t| t.translation).unwrap_or(Vec3::Z * 5.0);
                    if ui.button("Impact torse").clicked() {
                        d.hit(false, from);
                    }
                    if ui.button("Impact tête").clicked() {
                        d.hit(true, from);
                    }
                    if ui
                        .button(if d.alive { "Mort" } else { "Réapparition" })
                        .clicked()
                    {
                        d.alive = !d.alive;
                    }
                });
                ui.separator();
                ui.strong("Shader");
                let s = &mut style.bypass_change_detection().0;
                let before = *s;
                ui.add(egui::Slider::new(&mut s.contour_intensity, 0.0..=1.0).text("Contour"));
                ui.add(egui::Slider::new(&mut s.highlight, 0.0..=1.5).text("Reflets"));
                ui.add(egui::Slider::new(&mut s.roughness, 0.05..=1.0).text("Rugosité"));
                ui.add(
                    egui::Slider::new(&mut s.ripple_speed, 0.0..=2.0)
                        .text("Vitesse des ondulations"),
                );
                ui.add(egui::Slider::new(&mut s.deformation, 0.0..=1.0).text("Déformation"));
                ui.add(egui::Slider::new(&mut s.drip_rate, 0.0..=3.0).text("Gouttes / s"));
                ui.add(egui::Slider::new(&mut s.splash, 0.0..=30.0).text("Éclaboussures"));
                ui.add(
                    egui::Slider::new(&mut s.bands, 1.0..=6.0)
                        .step_by(1.0)
                        .text("Bandes de lumière"),
                );
                ui.add(egui::Slider::new(&mut s.opacity, 0.5..=1.0).text("Opacité (flaques)"));
                if ui.button("Valeurs par défaut").clicked() {
                    *s = LiquidStyle::default();
                }
                let changed = *s != before;
                if changed {
                    style.set_changed();
                }
                ui.separator();
                ui.strong("Statistiques");
                if let Some(a) = &assets {
                    ui.label(format!(
                        "Triangles LOD : {} / {} / {}",
                        a.triangles[0], a.triangles[1], a.triangles[2]
                    ));
                }
                ui.label(format!("Personnages : {}", chars.count()));
                ui.label(format!(
                    "Gouttes : {} actives / {} en réserve ({} refusées)",
                    fx.active_drops(),
                    fx.pool_size(),
                    fx.dropped
                ));
                ui.label(format!("Flaques : {}", fx.active_puddles()));
                ui.label(format!(
                    "Matériaux : {} partagés, {} prêtés",
                    mats.shared_count(),
                    mats.lent_count()
                ));
            });
        });
    Ok(())
}
