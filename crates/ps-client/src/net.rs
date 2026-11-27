//! Connection to a server (remote, or the embedded listen server).

use crate::settings::Settings;
use bevy::prelude::*;
use ps_netclient::input::InputTimeline;
use ps_netclient::{ClientConfig, NetClient, Variant};
use ps_server::run::ServerHandle;
use ps_server::{ServerConfig, spawn_local};
use ps_sim::GameData;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::time::Instant;

#[derive(Resource)]
pub struct GameDataRes(pub Arc<GameData>);

/// Monotonic clock shared by networking and input.
#[derive(Resource)]
pub struct Clock(pub Instant);

impl Clock {
    pub fn now(&self) -> f64 {
        self.0.elapsed().as_secs_f64()
    }
}

pub struct Session {
    pub client: NetClient,
    pub socket: UdpSocket,
    pub server: SocketAddr,
    pub local: Option<ServerHandle>,
    pub timeline: InputTimeline,
    pub last_cmd_end: f64,
    pub started: f64,
}

#[derive(Resource, Default)]
pub struct Net {
    pub session: Option<Session>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LocalGame {
    pub map: String,
    pub profile: String,
    pub bots: u32,
    pub fill_bots: bool,
    pub difficulty: f32,
}

fn client_config(s: &Settings) -> ClientConfig {
    ClientConfig {
        name: s.name.clone(),
        variant: Variant::parse(&s.net_variant).unwrap_or_default(),
        extrapolation_horizon: s.extrapolation_ms.clamp(0.0, 60.0) / 1000.0,
        ..ClientConfig::default()
    }
}

impl Net {
    pub fn connect(&mut self, addr: &str, settings: &Settings, data: Arc<GameData>, now: f64) {
        self.disconnect(now);
        let server = match addr.to_socket_addrs().ok().and_then(|mut a| {
            a.find(|x| x.is_ipv4())
                .or_else(|| addr.to_socket_addrs().ok()?.next())
        }) {
            Some(a) => a,
            None => {
                self.error = Some(format!("adresse invalide : {addr}"));
                return;
            }
        };
        let bind = if server.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let socket = match UdpSocket::bind(bind) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(format!("socket UDP : {e}"));
                return;
            }
        };
        let _ = socket.set_nonblocking(true);
        self.error = None;
        self.session = Some(Session {
            client: NetClient::new(client_config(settings), data, now),
            socket,
            server,
            local: None,
            timeline: InputTimeline::default(),
            last_cmd_end: now,
            started: now,
        });
    }

    pub fn host(&mut self, g: &LocalGame, settings: &Settings, data: Arc<GameData>, now: f64) {
        self.disconnect(now);
        let cfg = ServerConfig {
            map: g.map.clone(),
            profile: g.profile.clone(),
            bots: g.bots,
            fill_with_bots: g.fill_bots,
            bot_difficulty: g.difficulty,
            replay_dir: Some(crate::settings::config_dir().join("replays")),
            ..ServerConfig::default()
        };
        match spawn_local(cfg, data.clone()) {
            Ok(h) => {
                let addr = h.addr.to_string();
                self.connect(&addr, settings, data, now);
                if let Some(s) = &mut self.session {
                    s.local = Some(h);
                }
            }
            Err(e) => self.error = Some(format!("serveur local : {e}")),
        }
    }

    pub fn disconnect(&mut self, now: f64) {
        if let Some(mut s) = self.session.take() {
            for p in s.client.leave(now) {
                let _ = s.socket.send_to(&p, s.server);
            }
            if let Some(h) = s.local.take() {
                h.shutdown();
            }
        }
    }
}

/// Receives datagrams (runs first every frame).
pub fn receive(mut net: ResMut<Net>, clock: Res<Clock>) {
    let now = clock.now();
    let Some(s) = &mut net.session else { return };
    let mut buf = [0u8; 2048];
    loop {
        match s.socket.recv_from(&mut buf) {
            Ok((n, from)) => {
                if from == s.server {
                    s.client.receive(&buf[..n], now);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            // ICMP port unreachable on Windows: the server is not there (yet).
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => continue,
            Err(_) => break,
        }
    }
    let failed = match s.client.state() {
        ps_transport::ClientState::Rejected(r) => Some(format!("connexion refusée : {r}")),
        ps_transport::ClientState::TimedOut => {
            Some("délai de connexion dépassé (serveur injoignable ?)".to_string())
        }
        _ => None,
    };
    let kicked = s.client.kicked.clone();
    if let Some(e) = failed.or(kicked) {
        net.error = Some(e);
        net.disconnect(now);
    }
}

/// Produces the commands due this frame, then flushes (runs after input).
pub fn send(mut net: ResMut<Net>, clock: Res<Clock>) {
    let now = clock.now();
    let Some(s) = &mut net.session else { return };
    let n = s.client.commands_due(now);
    if n > 0 {
        // The n commands share the time elapsed since the last one: every input
        // transition up to `now` lands in a command, at its position in time.
        let start0 = s.last_cmd_end.max(now - 0.25);
        let span = (now - start0) / n as f64;
        for k in 0..n {
            let start = start0 + span * k as f64;
            let end = if k + 1 == n { now } else { start + span };
            let cmd = s.timeline.build(start, end);
            s.client.push_cmd(cmd, now);
        }
        s.last_cmd_end = now;
    }
    for p in s.client.poll(now) {
        let _ = s.socket.send_to(&p, s.server);
    }
}
