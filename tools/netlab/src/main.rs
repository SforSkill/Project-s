//! Network lab: runs the real server and real clients in one process over
//! simulated links (latency, jitter, loss, duplication) and measures what each
//! netcode variant shows the players. Every number written comes from a run;
//! nothing is estimated. See `docs/network-lab.md`.
//!
//! `cargo run --release -p netlab -- [--seconds S] [--seeds N] [--out DIR] [--map M]`
//! writes `netlab.csv` (one row per run) and `netlab.json` (runs + summary per
//! variant and profile).
//!
//! Scenario: player 0 strafes left/right with random switch intervals
//! (0.15–0.8 s, like peeking), player 1 stands and watches. Measured after a
//! 4 s warm-up, every client frame (240 Hz):
//! * `view_error`: what player 1 draws for player 0 vs. where the server has
//!   player 0 at that instant (the cost of latency + interpolation delay);
//! * `rewind_error`: what player 1 draws vs. the server position at the view
//!   time the client claims in its commands, i.e. where lag compensation will
//!   test a shot. This is the "my crosshair was on him" error;
//! * prediction corrections of player 0, visual snaps of its own view,
//!   bandwidth per client, server tick cost.

use engine_core::ViewAngles;
use engine_core::cmd::{Buttons, UserCmd};
use engine_network::linksim::{Link, LinkProfile};
use game_netclient::{ClientConfig, NetClient, Variant};
use game_server::{Server, ServerConfig};
use game_sim::GameData;
use glam::Vec3;
use serde::Serialize;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

const FRAME: f64 = 1.0 / 240.0;
const WARMUP: f64 = 4.0;

#[derive(Debug, Clone, Serialize)]
struct Run {
    variant: String,
    profile: String,
    seed: u64,
    tick_rate: u32,
    latency_ms: f64,
    jitter_ms: f64,
    loss_pct: f64,
    samples: usize,
    view_error_p50: f32,
    view_error_p95: f32,
    view_error_max: f32,
    rewind_error_p50: f32,
    rewind_error_p95: f32,
    rewind_error_max: f32,
    extrapolated_pct: f32,
    corrections: u64,
    correction_p95: f32,
    own_snap_max: f32,
    snapshots_lost: u64,
    missing_cmds: u64,
    rtt_ms: f32,
    interp_ticks: f32,
    cmd_buffer: f32,
    down_kbps: f64,
    up_kbps: f64,
    server_tick_us_avg: f64,
    server_tick_us_p99: f64,
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn pct(v: &mut [f32], p: f64) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() - 1) as f64 * p).round() as usize]
}

/// Server position of `id` at fractional tick `t`, from the recorded truth.
fn truth_at(hist: &VecDeque<(u32, Vec3)>, t: f64) -> Option<Vec3> {
    let i = hist.iter().position(|(k, _)| *k as f64 >= t)?;
    if i == 0 {
        return Some(hist[0].1);
    }
    let (k0, p0) = hist[i - 1];
    let (k1, p1) = hist[i];
    let a = ((t - k0 as f64) / (k1 - k0).max(1) as f64) as f32;
    Some(p0.lerp(p1, a))
}

fn run(
    data: &Arc<GameData>,
    map: &str,
    variant: Variant,
    name: &str,
    link: LinkProfile,
    seed: u64,
    seconds: f64,
) -> Run {
    let tick_rate = if variant == Variant::D { 256 } else { 128 };
    let mut cfg = ServerConfig {
        map: map.into(),
        profile: "deathmatch".into(),
        admin_bind: None,
        seed: Some(seed),
        tick_rate,
        bots: 0,
        ..ServerConfig::default()
    };
    // Both players must stay replicated to measure what one sees of the other.
    cfg.visibility.enabled = false;
    let mut server = Server::new(cfg, data.clone()).expect("server");
    let addrs = [
        SocketAddr::from(([10, 0, 0, 1], 40000)),
        SocketAddr::from(([10, 0, 0, 2], 40000)),
    ];
    let mut clients: Vec<NetClient> = (0..2)
        .map(|i| {
            NetClient::new(
                ClientConfig {
                    name: format!("P{i}"),
                    variant,
                    ..ClientConfig::default()
                },
                data.clone(),
                0.0,
            )
        })
        .collect();
    let mut up: Vec<Link> = (0..2)
        .map(|i| Link::new(link, seed * 1000 + 11 + i))
        .collect();
    let mut down: Vec<Link> = (0..2)
        .map(|i| Link::new(link, seed * 1000 + 101 + i))
        .collect();
    let mut rng = Rng(seed);
    let dt = server.sim.dt();
    let (mut t, mut next_tick) = (0.0, 0.0);
    let mut out = Vec::new();
    let (mut strafe_left, mut next_switch) = (true, 0.0);
    let mut truth: VecDeque<(u32, Vec3)> = VecDeque::new();
    let (mut view_err, mut rewind_err) = (Vec::new(), Vec::new());
    let (mut extrapolated, mut samples) = (0usize, 0usize);
    let mut own_prev: Option<Vec3> = None;
    let mut own_snap_max = 0.0f32;
    let (mut bytes_down, mut bytes_up) = ([0u64; 2], [0u64; 2]);
    let mut tick_us = Vec::new();
    let mut corrections_at_warmup = 0;
    let mut missing_at_warmup = 0;
    while t < seconds {
        if t >= next_switch {
            strafe_left = !strafe_left;
            next_switch = t + 0.15 + rng.next() * 0.65;
        }
        for i in 0..2 {
            for pkt in down[i].deliver(t) {
                clients[i].receive(&pkt, t);
            }
            let n = clients[i].commands_due(t);
            for _ in 0..n {
                let b = if i == 0 && t > 2.0 {
                    if strafe_left {
                        Buttons::LEFT
                    } else {
                        Buttons::RIGHT
                    }
                } else {
                    0
                };
                let v = ViewAngles {
                    pitch: 0.0,
                    yaw: 90.0,
                };
                clients[i].push_cmd(
                    UserCmd {
                        buttons_start: Buttons(b),
                        view_start: v,
                        view_end: v,
                        ..Default::default()
                    },
                    t,
                );
            }
            for pkt in clients[i].poll(t) {
                if t >= WARMUP {
                    bytes_up[i] += pkt.len() as u64 + 28;
                }
                up[i].send(t, pkt);
            }
        }
        for i in 0..2 {
            for pkt in up[i].deliver(t) {
                server.receive(addrs[i], &pkt, t, &mut out);
            }
        }
        while next_tick <= t {
            let start = Instant::now();
            server.tick(t, &mut out);
            if t >= WARMUP {
                tick_us.push(start.elapsed().as_secs_f32() * 1e6);
            }
            next_tick += dt;
            if let Some(id) = clients[0].me()
                && let Some(p) = server.sim.player(engine_core::PlayerId(id))
            {
                truth.push_back((server.sim.tick, p.mv.origin));
                while truth.len() > 2 * tick_rate as usize {
                    truth.pop_front();
                }
            }
        }
        for (to, pkt) in out.drain(..) {
            if let Some(i) = addrs.iter().position(|a| *a == to) {
                if t >= WARMUP {
                    bytes_down[i] += pkt.len() as u64 + 28;
                }
                down[i].send(t, pkt);
            }
        }
        if t >= WARMUP {
            if corrections_at_warmup == 0 {
                corrections_at_warmup = clients[0].stats.corrections + 1;
                missing_at_warmup = server.stats.missing_cmds;
            }
            if let Some(id) = clients[0].me()
                && let Some(p) = server.sim.player(engine_core::PlayerId(id))
                && let Some(seen) = clients[1].remote_players(t).iter().find(|r| r.id == id)
            {
                samples += 1;
                view_err.push(seen.origin.distance(p.mv.origin));
                if seen.extrapolated > 0.0 {
                    extrapolated += 1;
                }
                if let Some(at) = truth_at(&truth, clients[1].render_tick(t)) {
                    rewind_err.push(seen.origin.distance(at));
                }
            }
            // Own view: any frame-to-frame move beyond max ground speed is a visible snap.
            let own = clients[0].local_origin();
            if let Some(prev) = own_prev {
                own_snap_max =
                    own_snap_max.max((own.distance(prev) - 320.0 * FRAME as f32).max(0.0));
            }
            own_prev = Some(own);
        }
        if std::env::var_os("NETLAB_TRACE").is_some() && (t * 4.0).fract() < FRAME * 4.0 {
            let c = &clients[0];
            eprintln!(
                "t {t:.2} srv {} pred? buf {:.2} tgt {:.2} scale {:.3} resync {} rtt {:.1} interp {:.2} lost {} missing {}",
                server.sim.tick,
                c.stats.cmd_buffer,
                c.stats.cmd_buffer_target,
                c.stats.time_scale,
                c.stats.resyncs,
                c.stats.rtt_ms,
                c.stats.interp_ticks,
                c.stats.snapshots_lost,
                server.stats.missing_cmds
            );
        }
        t += FRAME;
    }
    if std::env::var_os("NETLAB_DEBUG").is_some() {
        for (i, c) in clients.iter().enumerate() {
            eprintln!(
                "  client {i}: in_game {} kicked {:?} resyncs {} me {:?} pred_cmd_buffer {:.2} scale {:.3}",
                c.in_game(),
                c.kicked,
                c.stats.resyncs,
                c.me(),
                c.stats.cmd_buffer,
                c.stats.time_scale
            );
        }
        eprintln!(
            "  server clients {} missing {} late {:?}",
            server.clients(),
            server.stats.missing_cmds,
            server.stats
        );
    }
    let measured = seconds - WARMUP;
    let c0 = &clients[0].stats;
    let c1 = &clients[1].stats;
    let avg_us = if tick_us.is_empty() {
        0.0
    } else {
        tick_us.iter().map(|v| *v as f64).sum::<f64>() / tick_us.len() as f64
    };
    Run {
        variant: format!("{variant:?}"),
        profile: name.into(),
        seed,
        tick_rate,
        latency_ms: link.latency * 1000.0,
        jitter_ms: link.jitter * 1000.0,
        loss_pct: link.loss * 100.0,
        samples,
        view_error_p50: pct(&mut view_err, 0.5),
        view_error_p95: pct(&mut view_err, 0.95),
        view_error_max: pct(&mut view_err, 1.0),
        rewind_error_p50: pct(&mut rewind_err, 0.5),
        rewind_error_p95: pct(&mut rewind_err, 0.95),
        rewind_error_max: pct(&mut rewind_err, 1.0),
        extrapolated_pct: if samples > 0 {
            extrapolated as f32 * 100.0 / samples as f32
        } else {
            0.0
        },
        corrections: (c0.corrections + 1).saturating_sub(corrections_at_warmup),
        correction_p95: c0.correction_p95,
        own_snap_max,
        snapshots_lost: c1.snapshots_lost,
        missing_cmds: server.stats.missing_cmds - missing_at_warmup,
        rtt_ms: c1.rtt_ms,
        interp_ticks: c1.interp_ticks,
        cmd_buffer: c0.cmd_buffer,
        down_kbps: (bytes_down[0] + bytes_down[1]) as f64 * 8.0 / 1000.0 / measured / 2.0,
        up_kbps: (bytes_up[0] + bytes_up[1]) as f64 * 8.0 / 1000.0 / measured / 2.0,
        server_tick_us_avg: avg_us,
        server_tick_us_p99: pct(&mut tick_us, 0.99) as f64,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let seconds: f64 = get("--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20.0);
    let seeds: u64 = get("--seeds").and_then(|s| s.parse().ok()).unwrap_or(3);
    let out = PathBuf::from(get("--out").unwrap_or_else(|| "netlab-results".into()));
    let map = get("--map").unwrap_or_else(|| "ps_duel_lab".into());
    let dir = game_server::find_data_dir(get("--data").map(PathBuf::from).as_deref())
        .expect("data directory");
    let data = Arc::new(GameData::load(&dir).expect("game data"));
    // "harsh" is beyond the shipped profiles: it makes extrapolation and
    // corrections actually happen so the variants can differ.
    let harsh = LinkProfile {
        latency: 0.08,
        jitter: 0.04,
        loss: 0.10,
        duplicate: 0.02,
    };
    let profiles = [
        ("lan", LinkProfile::LAN),
        ("good", LinkProfile::GOOD),
        ("average", LinkProfile::AVERAGE),
        ("bad", LinkProfile::BAD),
        ("harsh", harsh),
    ];
    let variants = [Variant::A, Variant::B, Variant::C, Variant::D];
    let mut runs = Vec::new();
    for (name, link) in profiles {
        for v in variants {
            for seed in 1..=seeds {
                if let Ok(only) = std::env::var("NETLAB_ONLY")
                    && only != format!("{name}:{v:?}")
                {
                    continue;
                }
                let r = run(&data, &map, v, name, link, seed, seconds);
                eprintln!(
                    "{name:8} {v:?} seed {seed}: view p95 {:.1} u, rewind p95 {:.2} u, extrap {:.1} %, corrections {}, down {:.0} kbit/s",
                    r.view_error_p95,
                    r.rewind_error_p95,
                    r.extrapolated_pct,
                    r.corrections,
                    r.down_kbps
                );
                runs.push(r);
            }
        }
    }
    std::fs::create_dir_all(&out).expect("output directory");
    // CSV
    let header = "variant,profile,seed,tick_rate,latency_ms,jitter_ms,loss_pct,samples,view_error_p50,view_error_p95,view_error_max,rewind_error_p50,rewind_error_p95,rewind_error_max,extrapolated_pct,corrections,correction_p95,own_snap_max,snapshots_lost,missing_cmds,rtt_ms,interp_ticks,cmd_buffer,down_kbps,up_kbps,server_tick_us_avg,server_tick_us_p99";
    let mut csv = String::from(header);
    csv.push('\n');
    for r in &runs {
        csv.push_str(&format!(
            "{},{},{},{},{:.1},{:.1},{:.1},{},{:.2},{:.2},{:.2},{:.3},{:.3},{:.3},{:.2},{},{:.3},{:.3},{},{},{:.2},{:.2},{:.2},{:.1},{:.1},{:.1},{:.1}\n",
            r.variant, r.profile, r.seed, r.tick_rate, r.latency_ms, r.jitter_ms, r.loss_pct, r.samples, r.view_error_p50, r.view_error_p95, r.view_error_max, r.rewind_error_p50, r.rewind_error_p95, r.rewind_error_max, r.extrapolated_pct, r.corrections, r.correction_p95, r.own_snap_max, r.snapshots_lost, r.missing_cmds, r.rtt_ms, r.interp_ticks, r.cmd_buffer, r.down_kbps, r.up_kbps, r.server_tick_us_avg, r.server_tick_us_p99
        ));
    }
    std::fs::write(out.join("netlab.csv"), csv).expect("write csv");
    // Summary: mean over seeds per (profile, variant).
    let mut summary = Vec::new();
    for (name, _) in profiles {
        for v in variants {
            let rs: Vec<&Run> = runs
                .iter()
                .filter(|r| r.profile == name && r.variant == format!("{v:?}"))
                .collect();
            let mean = |f: &dyn Fn(&Run) -> f64| {
                rs.iter().map(|r| f(r)).sum::<f64>() / rs.len().max(1) as f64
            };
            summary.push(serde_json::json!({
                "profile": name, "variant": format!("{v:?}"), "runs": rs.len(),
                "view_error_p95": mean(&|r| r.view_error_p95 as f64),
                "rewind_error_p95": mean(&|r| r.rewind_error_p95 as f64),
                "rewind_error_max": mean(&|r| r.rewind_error_max as f64),
                "extrapolated_pct": mean(&|r| r.extrapolated_pct as f64),
                "corrections": mean(&|r| r.corrections as f64),
                "own_snap_max": mean(&|r| r.own_snap_max as f64),
                "down_kbps": mean(&|r| r.down_kbps),
                "server_tick_us_avg": mean(&|r| r.server_tick_us_avg),
            }));
        }
    }
    let json = serde_json::json!({
        "tool": "netlab", "version": env!("CARGO_PKG_VERSION"), "map": map, "seconds": seconds, "warmup": WARMUP, "seeds": seeds,
        "scenario": "player 0 strafes with random 0.15-0.8 s switches, player 1 observes; 2 clients, no bots",
        "units": "distances in game units (1 u = 1 inch), bandwidth per client in kbit/s incl. 28 B IP/UDP headers",
        "summary": summary, "runs": runs,
    });
    std::fs::write(
        out.join("netlab.json"),
        serde_json::to_string_pretty(&json).expect("json"),
    )
    .expect("write json");
    println!(
        "netlab : {} runs, résultats dans {}",
        runs.len(),
        out.display()
    );
}
