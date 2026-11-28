//! Projet S — game client (`Project-S.exe`).
//!
//! Bevy renders, egui draws the interface; all gameplay is in the shared
//! simulation crates. "Play" starts an embedded 128 Hz server on 127.0.0.1 and
//! connects to it through the same UDP protocol as a remote server.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod audio;
mod autotest;
mod bindings;
mod character;
mod demo;
mod game;
mod keyboard_layout;
mod net;
mod settings;
mod sky;
mod textures;
mod theme;
mod ui;
mod world;

use bevy::prelude::*;
use bevy::window::{MonitorSelection, PresentMode, WindowMode};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
enum Stage {
    Input,
    Send,
    Present,
}

#[derive(Resource)]
struct FrameLimiter {
    last: Instant,
}

fn main() -> AppExit {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("Projet S {}", env!("CARGO_PKG_VERSION"));
        return AppExit::Success;
    }
    if args.iter().any(|a| a == "--character-demo") {
        return demo::run(&args);
    }
    let explicit = args
        .iter()
        .position(|a| a == "--data")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from);
    let data = match ps_server::find_data_dir(explicit.as_deref())
        .and_then(|d| ps_sim::GameData::load(&d))
    {
        Ok(d) => Arc::new(d),
        Err(e) => {
            eprintln!("Projet S : données du jeu introuvables ou invalides : {e}");
            return AppExit::error();
        }
    };
    if args.iter().any(|a| a == "--check") {
        println!(
            "Projet S {} : données valides ({} cartes, empreinte {:016x})",
            env!("CARGO_PKG_VERSION"),
            data.maps.len(),
            data.fingerprint()
        );
        return AppExit::Success;
    }
    let settings = settings::Settings::load();
    let mut menu = ui::MenuState::default();
    if let Some(i) = args.iter().position(|a| a == "--connect")
        && let Some(addr) = args.get(i + 1)
    {
        menu.address = addr.clone();
        menu.tab = ui::MenuTab::Join;
    }
    let autotest = autotest::AutoTest::from_args(&args);
    let window = Window {
        title: format!("Projet S {}", env!("CARGO_PKG_VERSION")),
        present_mode: if settings.vsync {
            PresentMode::AutoVsync
        } else {
            PresentMode::AutoNoVsync
        },
        mode: if settings.fullscreen {
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        } else {
            WindowMode::Windowed
        },
        resolution: (1600, 900).into(),
        ..default()
    };
    let mut app = App::new();
    if let Some(at) = autotest {
        app.insert_resource(at);
    }
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(window),
        ..default()
    }))
    .add_plugins(EguiPlugin::default())
    .add_plugins(character::plugin)
    .insert_resource(net::GameDataRes(data))
    .insert_resource(net::Clock(Instant::now()))
    .insert_resource(settings)
    .insert_resource(menu)
    .insert_resource(FrameLimiter {
        last: Instant::now(),
    })
    .init_resource::<net::Net>()
    .init_resource::<bindings::Rebind>()
    .init_resource::<keyboard_layout::KeyLayout>()
    .init_resource::<game::Screen>()
    .init_resource::<game::Hud>()
    .init_resource::<game::Scene>()
    .configure_sets(Update, (Stage::Input, Stage::Send, Stage::Present).chain())
    .add_systems(
        Startup,
        (game::setup_cameras, audio::setup, textures::setup),
    )
    .add_systems(PostStartup, game::setup_viewmodel)
    .add_systems(PreUpdate, (net::receive, keyboard_layout::update))
    .add_systems(
        Update,
        (game::input, autotest::run).chain().in_set(Stage::Input),
    )
    .add_systems(Update, net::send.in_set(Stage::Send))
    .add_systems(
        Update,
        (
            game::sync_map,
            game::update_scene,
            game::events,
            game::camera,
            character::runtime::drive,
            character::fx::update_fx,
            game::sync_volume,
            game::autosave_audio,
            toggle_fullscreen,
            apply_window_settings,
        )
            .chain()
            .in_set(Stage::Present),
    )
    .add_systems(
        EguiPrimaryContextPass,
        (theme::apply, ui::hud, ui::menu).chain(),
    )
    .add_systems(Last, (limit_frame_rate, shutdown_on_exit))
    .run()
}

/// F11 or Alt+Enter switches between windowed and fullscreen, and saves it
/// (not while a key is being captured in the settings).
fn toggle_fullscreen(
    keys: Res<ButtonInput<KeyCode>>,
    rebind: Res<bindings::Rebind>,
    mut s: ResMut<settings::Settings>,
) {
    if rebind.0.is_some() {
        return;
    }
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let enter = keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]);
    if keys.just_pressed(KeyCode::F11) || (alt && enter) {
        s.fullscreen = !s.fullscreen;
        s.save();
    }
}

fn apply_window_settings(s: Res<settings::Settings>, mut window: Single<&mut Window>) {
    if !s.is_changed() {
        return;
    }
    let pm = if s.vsync {
        PresentMode::AutoVsync
    } else {
        PresentMode::AutoNoVsync
    };
    if window.present_mode != pm {
        window.present_mode = pm;
    }
    let mode = if s.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    if window.mode != mode {
        window.mode = mode;
    }
}

/// Optional frame cap (sleep, then a short spin for precision).
fn limit_frame_rate(s: Res<settings::Settings>, mut lim: ResMut<FrameLimiter>) {
    if s.fps_limit >= 30 {
        let target = Duration::from_secs_f64(1.0 / s.fps_limit as f64);
        let deadline = lim.last + target;
        let now = Instant::now();
        if deadline > now {
            let left = deadline - now;
            if left > Duration::from_millis(2) {
                std::thread::sleep(left - Duration::from_millis(1));
            }
            while Instant::now() < deadline {
                std::hint::spin_loop();
            }
        }
    }
    lim.last = Instant::now();
}

/// Leaves the server cleanly and stops the embedded one when the window closes.
fn shutdown_on_exit(
    mut exit: MessageReader<AppExit>,
    mut net: ResMut<net::Net>,
    clock: Res<net::Clock>,
    s: Res<settings::Settings>,
) {
    if exit.read().next().is_some() {
        net.disconnect(clock.now());
        s.save();
    }
}
