//! `--autotest`: plays a local match with scripted input, takes screenshots and
//! writes a JSON report, then quits. Used to check a build end to end on a real
//! GPU and window (`docs/client.md`, « Autotest »).

use crate::game::{Hud, Screen};
use crate::net::{Clock, GameDataRes, LocalGame, Net};
use crate::settings::Settings;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use ps_sim_core::cmd::Buttons;
use std::path::PathBuf;

#[derive(Resource)]
pub struct AutoTest {
    pub game: LocalGame,
    /// `--connect ADDR`: join that server instead of hosting a local game.
    pub connect: Option<String>,
    pub seconds: f64,
    pub out: PathBuf,
    started: Option<f64>,
    shots_taken: usize,
    frames: u64,
    frame_ms: Vec<f32>,
    max_remote: usize,
    events: u64,
    done: bool,
}

const SHOTS: [(f64, &str); 3] = [
    (5.0, "autotest-1-jeu.png"),
    (10.0, "autotest-2-graphe-reseau.png"),
    (14.0, "autotest-3-scores.png"),
];

impl AutoTest {
    pub fn from_args(args: &[String]) -> Option<AutoTest> {
        let i = args.iter().position(|a| a == "--autotest")?;
        let get = |k: &str| {
            args.iter()
                .position(|a| a == k)
                .and_then(|j| args.get(j + 1))
                .cloned()
        };
        let _ = i;
        Some(AutoTest {
            game: LocalGame {
                map: get("--map").unwrap_or_else(|| "ps_citadel".into()),
                profile: get("--mode").unwrap_or_else(|| "deathmatch".into()),
                bots: get("--bots").and_then(|b| b.parse().ok()).unwrap_or(5),
                fill_bots: false,
                difficulty: 0.5,
            },
            connect: get("--connect"),
            seconds: get("--seconds")
                .and_then(|s| s.parse().ok())
                .unwrap_or(20.0),
            out: get("--out")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("autotest")),
            started: None,
            shots_taken: 0,
            frames: 0,
            frame_ms: Vec::new(),
            max_remote: 0,
            events: 0,
            done: false,
        })
    }
}

pub fn run(
    mut commands: Commands,
    at: Option<ResMut<AutoTest>>,
    mut net: ResMut<Net>,
    clock: Res<Clock>,
    data: Res<GameDataRes>,
    mut settings: ResMut<Settings>,
    mut screen: ResMut<Screen>,
    mut hud: ResMut<Hud>,
    time: Res<Time>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(mut at) = at else { return };
    if at.done {
        return;
    }
    let now = clock.now();
    let Some(start) = at.started else {
        let _ = std::fs::create_dir_all(&at.out);
        match at.connect.clone() {
            Some(addr) => net.connect(&addr, &settings, data.0.clone(), now),
            None => {
                let g = at.game.clone();
                net.host(&g, &settings, data.0.clone(), now);
            }
        }
        *screen = Screen::Game;
        at.started = Some(now);
        return;
    };
    let t = now - start;
    at.frames += 1;
    at.frame_ms.push(time.delta_secs() * 1000.0);
    let Some(s) = &mut net.session else {
        at.done = true;
        write_report(&at, None, "session perdue");
        exit.write(AppExit::error());
        return;
    };
    if s.client.in_game() {
        at.max_remote = at.max_remote.max(s.client.remote_players(now).len());
        // Scripted input: strafe left/right with counter-strafe stops, turn, shoot in bursts.
        let phase = (t * 1.25) as u64 % 4;
        let mut b = match phase {
            0 => Buttons::FORWARD | Buttons::LEFT,
            1 => 0,
            2 => Buttons::FORWARD | Buttons::RIGHT,
            _ => 0,
        };
        if phase % 2 == 1 && ((t * 8.0) as u64).is_multiple_of(3) {
            b |= Buttons::ATTACK;
        }
        s.timeline.set_buttons(now, Buttons(b));
        let mut v = s.timeline.view();
        v.yaw += (time.delta_secs() * 25.0)
            * if ((t / 6.0) as u64).is_multiple_of(2) {
                1.0
            } else {
                -1.0
            };
        v.pitch = -3.0;
        s.timeline.set_view(now, v);
    }
    at.events = s.client.stats.urgent_events;
    settings.net_graph = t > 8.0;
    hud.scoreboard = t > 13.0;
    hud.inspector = t > 13.0;
    if at.shots_taken < SHOTS.len() && t >= SHOTS[at.shots_taken].0 {
        let path = at.out.join(SHOTS[at.shots_taken].1);
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
        at.shots_taken += 1;
    }
    if t >= at.seconds {
        at.done = true;
        let ok = s.client.in_game();
        write_report(&at, Some(&s.client), if ok { "ok" } else { "pas en jeu" });
        net.disconnect(now);
        exit.write(if ok {
            AppExit::Success
        } else {
            AppExit::error()
        });
    }
}

fn write_report(at: &AutoTest, c: Option<&ps_netclient::NetClient>, status: &str) {
    let mut ms = at.frame_ms.clone();
    ms.sort_by(|a, b| a.total_cmp(b));
    let pct = |p: f32| {
        if ms.is_empty() {
            0.0
        } else {
            ms[((ms.len() - 1) as f32 * p) as usize]
        }
    };
    let avg = if ms.is_empty() {
        0.0
    } else {
        ms.iter().sum::<f32>() / ms.len() as f32
    };
    let st = c.map(|c| c.stats.clone()).unwrap_or_default();
    let report = serde_json::json!({
        "status": status,
        "server": at.connect.clone().unwrap_or_else(|| "local (embarqué)".into()),
        // With --connect the server chooses map and mode: --map/--mode are not used.
        "map": if at.connect.is_some() { serde_json::Value::Null } else { at.game.map.clone().into() },
        "mode": if at.connect.is_some() { serde_json::Value::Null } else { at.game.profile.clone().into() },
        "bots": at.game.bots,
        "seconds": at.seconds,
        "frames": at.frames,
        "frame_ms": { "avg": avg, "p50": pct(0.5), "p95": pct(0.95), "p99": pct(0.99), "max": pct(1.0) },
        "fps_avg": if avg > 0.0 { 1000.0 / avg } else { 0.0 },
        "max_remote_players": at.max_remote,
        "net": {
            "tick_rate": c.map(|c| c.rate()).unwrap_or(0.0),
            "snapshots": st.snapshots,
            "snapshots_lost": st.snapshots_lost,
            "snapshot_bytes_avg": if st.snapshots > 0 { st.snapshot_bytes as f64 / st.snapshots as f64 } else { 0.0 },
            "corrections": st.corrections,
            "correction_p95": st.correction_p95,
            "rtt_ms": st.rtt_ms,
            "jitter_ms": st.jitter_ms,
            "interp_ticks": st.interp_ticks,
            "cmd_buffer": st.cmd_buffer,
            "urgent_events": st.urgent_events,
            "decode_errors": st.decode_errors,
            "shots_inspected": c.map(|c| c.shots.len()).unwrap_or(0),
        },
        "screenshots": SHOTS.iter().take(at.shots_taken).map(|s| s.1).collect::<Vec<_>>(),
    });
    let path = at.out.join("autotest-report.json");
    match std::fs::write(
        &path,
        serde_json::to_string_pretty(&report).unwrap_or_default(),
    ) {
        Ok(()) => println!("autotest : {status}, rapport {}", path.display()),
        Err(e) => eprintln!("autotest : rapport impossible ({e})"),
    }
}
