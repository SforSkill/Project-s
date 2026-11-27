//! End-to-end: real server code and real client code over simulated links.

use ps_netclient::{ClientConfig, NetClient};
use ps_server::{Server, ServerConfig};
use ps_sim::GameData;
use ps_sim_core::ViewAngles;
use ps_sim_core::cmd::{Buttons, UserCmd};
use ps_transport::linksim::{Link, LinkProfile};
use std::net::SocketAddr;
use std::sync::Arc;

/// Project-s-content: `PS_CONTENT_DIR` (CI) or a sibling checkout.
fn data() -> Arc<GameData> {
    let dir = ps_server::find_data_dir(None).unwrap();
    Arc::new(GameData::load(&dir).unwrap())
}

struct Peer {
    client: NetClient,
    addr: SocketAddr,
    up: Link,
    down: Link,
}

struct Result {
    server: Server,
    peers: Vec<Peer>,
}

/// Runs `seconds` of simulated time; client `i` holds `buttons(i, t)`.
fn run(
    profile: &str,
    map: &str,
    pim: bool,
    link: LinkProfile,
    clients: usize,
    seconds: f64,
    buttons: impl Fn(usize, f64) -> u32,
) -> Result {
    let d = data();
    let mut cfg = ServerConfig {
        map: map.into(),
        profile: profile.into(),
        admin_bind: None,
        seed: Some(7),
        ..ServerConfig::default()
    };
    cfg.visibility.enabled = pim;
    let mut server = Server::new(cfg, d.clone()).unwrap();
    let mut peers: Vec<Peer> = (0..clients)
        .map(|i| Peer {
            client: NetClient::new(
                ClientConfig {
                    name: format!("P{i}"),
                    ..ClientConfig::default()
                },
                d.clone(),
                0.0,
            ),
            addr: SocketAddr::from(([10, 0, 0, i as u8 + 1], 40000)),
            up: Link::new(link, 11 + i as u64),
            down: Link::new(link, 101 + i as u64),
        })
        .collect();
    let dt = server.sim.dt();
    let frame = 1.0 / 240.0;
    let mut t = 0.0;
    let mut next_tick = 0.0;
    let mut out = Vec::new();
    while t < seconds {
        for (i, p) in peers.iter_mut().enumerate() {
            for pkt in p.down.deliver(t) {
                p.client.receive(&pkt, t);
            }
            let n = p.client.commands_due(t);
            for _ in 0..n {
                let b = Buttons(buttons(i, t));
                let v = ViewAngles {
                    pitch: 0.0,
                    yaw: 90.0,
                };
                p.client.push_cmd(
                    UserCmd {
                        buttons_start: b,
                        view_start: v,
                        view_end: v,
                        ..Default::default()
                    },
                    t,
                );
            }
            for pkt in p.client.poll(t) {
                p.up.send(t, pkt);
            }
        }
        for p in peers.iter_mut() {
            for pkt in p.up.deliver(t) {
                server.receive(p.addr, &pkt, t, &mut out);
            }
        }
        while next_tick <= t {
            server.tick(t, &mut out);
            next_tick += dt;
        }
        for (to, pkt) in out.drain(..) {
            if let Some(p) = peers.iter_mut().find(|p| p.addr == to) {
                p.down.send(t, pkt);
            }
        }
        t += frame;
    }
    Result { server, peers }
}

#[test]
fn clients_join_predict_and_see_each_other() {
    // Client 0 strafes left/right, client 1 stands still.
    let r = run(
        "deathmatch",
        "ps_duel_lab",
        false,
        LinkProfile::AVERAGE,
        2,
        12.0,
        |i, t| {
            if i == 0 && t > 4.0 {
                if ((t * 2.0) as u32).is_multiple_of(2) {
                    Buttons::LEFT
                } else {
                    Buttons::RIGHT
                }
            } else {
                0
            }
        },
    );
    assert_eq!(r.server.clients(), 2);
    let st = &r.server.stats;
    assert!(st.snapshots > 2000, "snapshots {}", st.snapshots);
    // Nearly all commands arrive in time despite 1 % loss (redundancy).
    let simulated = 2 * 128 * 10;
    assert!(
        (st.missing_cmds as f64) < simulated as f64 * 0.05,
        "missing commands {}",
        st.missing_cmds
    );
    for p in &r.peers {
        let c = &p.client;
        assert!(c.in_game(), "{:?}", c.kicked);
        eprintln!(
            "{}: snapshots {} corrections {} p95 {:.3} interp {:.1} buffer {:.2} scale {:.3} jitter {:.1} ms",
            c.cfg.name,
            c.stats.snapshots,
            c.stats.corrections,
            c.stats.correction_p95,
            c.stats.interp_ticks,
            c.stats.cmd_buffer,
            c.stats.time_scale,
            c.stats.jitter_ms
        );
        assert!(
            c.stats.cmd_buffer > 0.5 && c.stats.cmd_buffer < 6.0,
            "cmd buffer {}",
            c.stats.cmd_buffer
        );
    }
    // The strafing player's prediction stays close to the server.
    let me = r.peers[0].client.me().unwrap();
    let server_pos = r
        .server
        .sim
        .player(ps_sim_core::PlayerId(me))
        .unwrap()
        .mv
        .origin;
    let predicted = r.peers[0].client.mv.origin;
    // Prediction runs ahead of the server by RTT + buffer: compare horizontal speed bound.
    assert!(
        server_pos.distance(predicted) < 250.0 * 0.2,
        "server {server_pos} predicted {predicted}"
    );
    assert!(
        r.peers[0].client.stats.corrections < 200,
        "too many corrections: {}",
        r.peers[0].client.stats.corrections
    );
    // Client 1 sees client 0 near its true (delayed) position.
    let now = 12.0;
    let remote = r.peers[1].client.remote_players(now);
    let other = remote
        .iter()
        .find(|p| p.id == me)
        .expect("remote player replicated");
    assert!(
        other.origin.distance(server_pos) < 60.0,
        "remote {} vs server {}",
        other.origin,
        server_pos
    );
}

#[test]
fn bots_match_over_bad_network() {
    let r = run(
        "competitive",
        "ps_citadel",
        true,
        LinkProfile::BAD,
        1,
        8.0,
        |_, _| 0,
    );
    let c = &r.peers[0].client;
    assert!(c.in_game());
    assert!(c.stats.snapshots > 500);
    assert!(!c.players.is_empty());
}

#[test]
fn hidden_enemies_are_not_sent() {
    // Same scenario with predictive visibility on: whenever the server says the
    // two players cannot see each other, the client must not have received them.
    let r = run(
        "deathmatch",
        "ps_duel_lab",
        true,
        LinkProfile::GOOD,
        2,
        8.0,
        |_, _| 0,
    );
    let a = ps_sim_core::PlayerId(r.peers[0].client.me().unwrap());
    let b = ps_sim_core::PlayerId(r.peers[1].client.me().unwrap());
    let sim = &r.server.sim;
    let mv = &sim.data.movement;
    let (pa, pb) = (sim.player(a).unwrap(), sim.player(b).unwrap());
    let visible = sim.visible(pa.mv.eye(mv), pb.mv.eye(mv))
        || sim.visible(pa.mv.eye(mv), pb.mv.origin + glam::Vec3::Y * 40.0);
    let replicated = r.peers[0]
        .client
        .latest()
        .unwrap()
        .entities
        .iter()
        .any(|e| e.id == b.0);
    eprintln!(
        "visible {visible} replicated {replicated} culled {}",
        r.server.stats.culled_entities
    );
    if !visible {
        assert!(!replicated, "enemy behind walls was replicated");
        assert!(r.server.stats.culled_entities > 0);
    } else {
        assert!(replicated, "visible enemy must be replicated");
    }
}

#[test]
fn real_udp_listen_server() {
    // The embedded server as used by the client for local play, on real sockets.
    let d = data();
    let cfg = ServerConfig {
        map: "ps_training_lab".into(),
        profile: "training".into(),
        ..ServerConfig::default()
    };
    let handle = ps_server::spawn_local(cfg, d.clone()).unwrap();
    let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    sock.set_nonblocking(true).unwrap();
    let start = std::time::Instant::now();
    let mut client = NetClient::new(
        ClientConfig {
            name: "Local".into(),
            ..ClientConfig::default()
        },
        d,
        0.0,
    );
    let mut buf = [0u8; 2048];
    while start.elapsed().as_secs_f64() < 3.0 {
        let now = start.elapsed().as_secs_f64();
        while let Ok((n, _)) = sock.recv_from(&mut buf) {
            client.receive(&buf[..n], now);
        }
        for _ in 0..client.commands_due(now) {
            client.push_cmd(UserCmd::default(), now);
        }
        for p in client.poll(now) {
            sock.send_to(&p, handle.addr).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(client.in_game(), "kicked: {:?}", client.kicked);
    assert!(
        client.welcome.as_ref().unwrap().host,
        "loopback client is the host"
    );
    assert!(
        client.stats.snapshots > 200,
        "snapshots {}",
        client.stats.snapshots
    );
    let status = handle.command("status").unwrap();
    assert!(status.contains("ps_training_lab"), "{status}");
    eprintln!(
        "udp: {} snapshots, rtt {:.2} ms, {status}",
        client.stats.snapshots,
        client.rtt() * 1000.0
    );
    handle.shutdown();
}
