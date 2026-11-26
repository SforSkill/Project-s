//! Client networking, sans-IO: feed datagrams and a clock, get datagrams and a
//! presentable world state.
//!
//! * **Clock sync**: commands are produced for `pred_tick`, ahead of the server
//!   so that they arrive a couple of ticks before being simulated. The server
//!   reports how far ahead they arrive (`cmd_buffer`); the client speeds up or
//!   slows down its tick clock by at most a few percent to hold the target.
//! * **Prediction / reconciliation**: the local player's movement is predicted
//!   with the server's own movement code; when a snapshot disagrees with the
//!   prediction for the same command, the client rewinds to the server state and
//!   replays the newer commands. The visual error is smoothed out.
//! * **Remote players**: interpolated between snapshots `interp` ticks in the
//!   past (adaptive to jitter). Variants B and C may extrapolate a bounded
//!   horizon with collision and gravity, with a confidence that shrinks with
//!   jitter, packet age and acceleration; never through walls.
//!
//! Network-lab variants (`docs/network-lab.md`): A = interpolation + local
//! prediction; B = A + adaptive physical extrapolation of opponents; C = B +
//! smoothed (adaptive) prediction correction; D = A at 256 Hz. The urgent
//! stream (shots, hits, deaths sent at once, outside snapshots) is a server
//! feature and is presented on arrival by every variant.

pub mod input;

use engine_core::cmd::UserCmd;
use engine_core::{Aabb, SimTime, Team};
use engine_movement::MoveState;
use engine_network::{Channel, ClientEndpoint, ClientState};
use engine_physics::world::TraceMask;
use game_sim::proto::{
    self, ClientMsg, EntityQ, PlayerInfo, ServerMsg, ShotNet, SmokeNet, Snapshot, Welcome, eflags,
};
use game_sim::{GameData, GameEvent, Level};
use glam::Vec3;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Variant {
    A,
    B,
    #[default]
    C,
    D,
}

impl Variant {
    pub fn parse(s: &str) -> Option<Variant> {
        match s.to_ascii_uppercase().as_str() {
            "A" => Some(Variant::A),
            "B" => Some(Variant::B),
            "C" => Some(Variant::C),
            "D" => Some(Variant::D),
            _ => None,
        }
    }

    pub fn extrapolates(self) -> bool {
        matches!(self, Variant::B | Variant::C)
    }

    /// C smooths small prediction corrections over ~100 ms; A, B and D snap.
    pub fn smooths_corrections(self) -> bool {
        matches!(self, Variant::C)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClientConfig {
    pub name: String,
    pub variant: Variant,
    /// Maximum extrapolation horizon (seconds); 0, 0.015 or 0.030 in the lab.
    pub extrapolation_horizon: f32,
    /// Minimum interpolation delay in ticks (adaptive above it).
    pub min_interp_ticks: f32,
    /// Command buffer the client aims for on the server, ticks.
    pub target_cmd_buffer: f32,
    pub reconnect_token: Option<u64>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            name: "Joueur".into(),
            variant: Variant::C,
            extrapolation_horizon: 0.015,
            min_interp_ticks: 2.0,
            target_cmd_buffer: 2.0,
            reconnect_token: None,
        }
    }
}

/// A remote player ready to draw (and to collide with for prediction).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RemotePlayer {
    pub id: u8,
    pub origin: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub duck: f32,
    pub flags: u16,
    pub team: Team,
    pub weapon: Option<engine_weapons::WeaponId>,
    pub health: u8,
    pub armor: u8,
    /// Extrapolated beyond the last snapshot by this many seconds.
    pub extrapolated: f32,
}

impl RemotePlayer {
    pub fn alive(&self) -> bool {
        self.flags & eflags::ALIVE != 0
    }
}

#[derive(Debug, Clone, Default)]
pub struct NetStats {
    pub snapshots: u64,
    pub snapshot_bytes: u64,
    pub snapshots_lost: u64,
    pub corrections: u64,
    pub last_correction: f32,
    pub correction_p95: f32,
    pub interp_ticks: f32,
    pub cmd_buffer: f32,
    /// Buffer the clock aims for (base target + uplink jitter margin).
    pub cmd_buffer_target: f32,
    /// Snapshots that arrived after a newer one (not counted as lost).
    pub snapshots_reordered: u64,
    pub time_scale: f32,
    pub jitter_ms: f32,
    pub rtt_ms: f32,
    pub loss: f32,
    pub extrapolated_ms: f32,
    pub resyncs: u64,
    pub decode_errors: u64,
    pub last_decode_error: String,
    pub missing_baselines: u64,
    pub urgent_events: u64,
    /// Seconds between an urgent event's tick on the server and its reception.
    pub urgent_delay_ms: f32,
    corrections_window: Vec<f32>,
}

struct Predicted {
    cmd: UserCmd,
    after: MoveState,
}

pub struct NetClient {
    pub cfg: ClientConfig,
    pub data: Arc<GameData>,
    endpoint: ClientEndpoint,
    join_sent: bool,
    pub welcome: Option<Welcome>,
    pub level: Option<Arc<Level>>,
    rate: f64,
    /// Received snapshots, oldest first (for interpolation).
    pub snapshots: VecDeque<Snapshot>,
    baselines: VecDeque<(u32, HashMap<u8, EntityQ>)>,
    newest_tick: u32,
    /// Estimated `server_tick - now * rate` from snapshot arrivals.
    offset: Option<f64>,
    jitter: f64,
    interp: f64,
    // Prediction.
    pub pred_tick: u32,
    accumulator: f64,
    time_scale: f64,
    buffer_filtered: f64,
    buffer_dev: f64,
    buffer_target: f64,
    /// Ignore buffer feedback from snapshots older than this command tick.
    feedback_from: u32,
    recent_ticks: VecDeque<u32>,
    last_frame: Option<f64>,
    history: VecDeque<Predicted>,
    pub mv: MoveState,
    prev_mv: MoveState,
    visual_error: Vec3,
    predicting: bool,
    // World state.
    pub events: Vec<GameEvent>,
    pub smokes: HashMap<u16, SmokeNet>,
    pub smoke_blasts: Vec<(Vec3, f64)>,
    pub fires: HashMap<u16, (Vec<Vec3>, f64, f64)>,
    pub players: Vec<PlayerInfo>,
    pub notices: Vec<String>,
    pub shots: VecDeque<ShotNet>,
    pub kicked: Option<String>,
    pub stats: NetStats,
    /// Set when a new Welcome arrives (map change): the presentation must reload.
    pub world_changed: bool,
}

impl NetClient {
    pub fn new(cfg: ClientConfig, data: Arc<GameData>, now: f64) -> NetClient {
        let target = cfg.target_cmd_buffer as f64;
        NetClient {
            cfg,
            data,
            endpoint: ClientEndpoint::new(now),
            join_sent: false,
            welcome: None,
            level: None,
            rate: 128.0,
            snapshots: VecDeque::new(),
            baselines: VecDeque::new(),
            newest_tick: 0,
            offset: None,
            jitter: 0.0,
            interp: 2.0,
            pred_tick: 0,
            accumulator: 0.0,
            time_scale: 1.0,
            buffer_filtered: 0.0,
            buffer_dev: 0.0,
            buffer_target: target,
            feedback_from: 0,
            recent_ticks: VecDeque::new(),
            last_frame: None,
            history: VecDeque::new(),
            mv: MoveState::new(Vec3::ZERO),
            prev_mv: MoveState::new(Vec3::ZERO),
            visual_error: Vec3::ZERO,
            predicting: false,
            events: Vec::new(),
            smokes: HashMap::new(),
            smoke_blasts: Vec::new(),
            fires: HashMap::new(),
            players: Vec::new(),
            notices: Vec::new(),
            shots: VecDeque::new(),
            kicked: None,
            stats: NetStats::default(),
            world_changed: false,
        }
    }

    pub fn connected(&self) -> bool {
        self.endpoint.is_connected()
    }

    pub fn state(&self) -> &ClientState {
        &self.endpoint.state
    }

    /// Joined and in a world.
    pub fn in_game(&self) -> bool {
        self.welcome.is_some() && self.level.is_some() && self.kicked.is_none()
    }

    pub fn me(&self) -> Option<u8> {
        self.welcome.as_ref().map(|w| w.you)
    }

    pub fn rate(&self) -> f64 {
        self.rate
    }

    pub fn dt(&self) -> f64 {
        1.0 / self.rate
    }

    pub fn rtt(&self) -> f64 {
        self.endpoint.conn.as_ref().map(|c| c.rtt()).unwrap_or(0.0)
    }

    pub fn conn_stats(&self) -> Option<engine_network::ConnStats> {
        self.endpoint.conn.as_ref().map(|c| c.stats.clone())
    }

    fn send(&mut self, ch: Channel, m: &ClientMsg) {
        if let Some(c) = &mut self.endpoint.conn {
            let _ = c.send(ch, proto::encode(m));
        }
    }

    pub fn send_reliable(&mut self, m: ClientMsg) {
        self.send(Channel::Reliable, &m);
    }

    pub fn leave(&mut self, now: f64) -> Vec<Vec<u8>> {
        self.send(Channel::Reliable, &ClientMsg::Leave);
        let mut out = self.poll(now);
        if let Some(c) = &mut self.endpoint.conn {
            c.disconnect("leave");
            out.extend(c.flush(now, true));
        }
        out
    }

    /// Outgoing datagrams (handshake, join, commands, acks).
    pub fn poll(&mut self, now: f64) -> Vec<Vec<u8>> {
        if self.endpoint.is_connected() && !self.join_sent {
            self.join_sent = true;
            let m = ClientMsg::Join {
                name: self.cfg.name.clone(),
                protocol: proto::GAME_PROTOCOL,
                data_fingerprint: self.data.fingerprint(),
                version: env!("CARGO_PKG_VERSION").into(),
                reconnect: self.cfg.reconnect_token,
            };
            self.send(Channel::Reliable, &m);
        }
        self.endpoint.poll(now)
    }

    pub fn receive(&mut self, pkt: &[u8], now: f64) {
        let Ok(msgs) = self.endpoint.receive(pkt, now) else {
            return;
        };
        for (ch, data) in msgs {
            match ch {
                Channel::Snapshot => self.on_snapshot(&data, now),
                Channel::Urgent | Channel::Reliable => {
                    if let Ok(m) = proto::decode::<ServerMsg>(&data) {
                        if ch == Channel::Urgent {
                            self.stats.urgent_events += 1;
                        }
                        self.on_msg(m, now);
                    }
                }
                Channel::Input => {}
            }
        }
    }

    fn on_msg(&mut self, m: ServerMsg, now: f64) {
        match m {
            ServerMsg::Welcome(w) => {
                self.rate = w.tick_rate as f64;
                let level = self.data.maps.get(&w.map).cloned().and_then(|def| {
                    Level::build(def, &self.data.materials, &self.data.movement).ok()
                });
                if level.is_none() {
                    self.kicked = Some(format!("carte {} absente des données du client", w.map));
                }
                self.level = level.map(Arc::new);
                self.cfg.reconnect_token = Some(w.reconnect_token);
                let rtt_ticks = (self.rtt() * self.rate).ceil() as u32;
                self.pred_tick = w.tick + rtt_ticks + self.cfg.target_cmd_buffer as u32 + 1;
                self.feedback_from = self.pred_tick;
                self.recent_ticks.clear();
                self.history.clear();
                self.snapshots.clear();
                self.baselines.clear();
                self.newest_tick = 0;
                self.offset = None;
                self.smokes.clear();
                self.fires.clear();
                self.predicting = false;
                self.welcome = Some(*w);
                self.world_changed = true;
                let _ = now;
            }
            ServerMsg::Events(evs) => {
                for e in &evs {
                    if let GameEvent::SmokeBlast { pos } = e {
                        self.smoke_blasts.push((*pos, self.server_time(now)));
                    }
                }
                self.events.extend(evs);
            }
            ServerMsg::Smoke(s) => {
                self.smokes.insert(s.id, s);
            }
            ServerMsg::SmokeBlast { pos, time } => self.smoke_blasts.push((pos, time)),
            ServerMsg::Fire {
                id,
                cells,
                start,
                end,
            } => {
                self.fires.insert(id, (cells, start, end));
            }
            ServerMsg::Players(p) => self.players = p,
            ServerMsg::Shot(s) => {
                self.shots.push_back(*s);
                while self.shots.len() > 64 {
                    self.shots.pop_front();
                }
            }
            ServerMsg::Notice(n) => self.notices.push(n),
            ServerMsg::Kicked(r) => self.kicked = Some(r),
        }
    }

    fn on_snapshot(&mut self, data: &[u8], now: f64) {
        let Ok((tick, baseline)) = proto::snapshot_ticks(data) else {
            return;
        };
        if tick <= self.newest_tick && self.newest_tick != 0 {
            // Old or duplicate. A late (reordered) one was counted lost: undo that.
            if !self.recent_ticks.contains(&tick) && tick + 64 > self.newest_tick {
                self.recent_ticks.push_back(tick);
                self.stats.snapshots_lost = self.stats.snapshots_lost.saturating_sub(1);
                self.stats.snapshots_reordered += 1;
            }
            return;
        }
        let base = if baseline != 0 {
            self.baselines
                .iter()
                .find(|b| b.0 == baseline)
                .map(|b| &b.1)
        } else {
            None
        };
        if baseline != 0 && base.is_none() {
            // Baseline unknown: wait for one we can decode (the ack stays older).
            self.stats.missing_baselines += 1;
            return;
        }
        let snap = match proto::decode_snapshot(data, base) {
            Ok(s) => s,
            Err(e) => {
                self.stats.decode_errors += 1;
                self.stats.last_decode_error = e.to_string();
                tracing::debug!("snapshot {tick} illisible : {e}");
                return;
            }
        };
        if self.newest_tick != 0 && tick > self.newest_tick + 1 {
            self.stats.snapshots_lost += (tick - self.newest_tick - 1) as u64;
        }
        self.newest_tick = tick;
        self.recent_ticks.push_back(tick);
        while self.recent_ticks.len() > 64 {
            self.recent_ticks.pop_front();
        }
        self.stats.snapshots += 1;
        self.stats.snapshot_bytes += data.len() as u64;
        self.baselines
            .push_back((tick, proto::entity_map(&snap.entities)));
        while self.baselines.len() > 64 {
            self.baselines.pop_front();
        }
        // Clock: arrival timeline.
        let sample = tick as f64 - now * self.rate;
        match self.offset {
            None => self.offset = Some(sample),
            Some(o) => {
                let d = sample - o;
                self.jitter += (d.abs() - self.jitter) * 0.05;
                // Late packets pull slowly, early packets quickly (track the fastest path).
                let k = if d > 0.0 { 0.2 } else { 0.02 };
                self.offset = Some(o + d * k);
            }
        }
        let divider = 1.0;
        self.interp = (self.cfg.min_interp_ticks as f64)
            .max(divider + 1.0 + self.jitter * 2.5)
            .min(32.0);
        self.stats.interp_ticks = self.interp as f32;
        self.stats.jitter_ms = (self.jitter / self.rate * 1000.0) as f32;
        // Command clock feedback. `i16::MIN`: the server has no command from us
        // yet; snapshots that predate the last resync describe the old clock.
        if snap.cmd_buffer != i16::MIN && snap.last_cmd >= self.feedback_from {
            let b = snap.cmd_buffer as f64;
            self.buffer_filtered += (b - self.buffer_filtered) * 0.1;
            // Uplink jitter makes the buffer vary: keep enough margin that a
            // late command packet still arrives before its tick.
            self.buffer_dev += ((b - self.buffer_filtered).abs() - self.buffer_dev) * 0.02;
            self.buffer_target =
                (self.cfg.target_cmd_buffer as f64 + 2.0 * self.buffer_dev).min(12.0);
            let err = self.buffer_target - self.buffer_filtered;
            if err.abs() > 24.0 {
                // Way off (pause, clock jump, respawn): resynchronise at once.
                self.pred_tick = (self.pred_tick as i64 + err.round() as i64).max(0) as u32;
                self.feedback_from = self.pred_tick;
                self.buffer_filtered = self.buffer_target;
                self.history.clear();
                self.stats.resyncs += 1;
            }
            self.time_scale = 1.0 + (err * 0.01).clamp(-0.05, 0.05);
        }
        self.stats.cmd_buffer_target = self.buffer_target as f32;
        self.stats.cmd_buffer = self.buffer_filtered as f32;
        self.stats.time_scale = self.time_scale as f32;
        self.stats.rtt_ms = (self.rtt() * 1000.0) as f32;
        self.stats.loss = self
            .conn_stats()
            .map(|s| s.loss_ratio() as f32)
            .unwrap_or(0.0);
        self.reconcile(&snap);
        self.snapshots.push_back(snap);
        while self.snapshots.len() > 2 && self.snapshots.len() > (self.rate as usize) {
            self.snapshots.pop_front();
        }
    }

    fn others_boxes(&self) -> Vec<Aabb> {
        let me = self.me();
        let Some(s) = self.snapshots.back() else {
            return Vec::new();
        };
        let list: Vec<(Vec3, bool)> = s
            .entities
            .iter()
            .filter(|e| Some(e.id) != me && e.alive())
            .map(|e| (e.origin(), e.flags & eflags::DUCKED != 0))
            .collect();
        game_sim::predict::boxes(&self.data, &list)
    }

    fn frozen(&self, snap: &Snapshot) -> bool {
        let me = snap.me.as_ref();
        matches!(
            snap.phase,
            game_rules::Phase::Freeze | game_rules::Phase::HalfTime | game_rules::Phase::GameOver
        ) && self
            .welcome
            .as_ref()
            .is_some_and(|w| w.rules.mode.has_rounds())
            || me.is_some_and(|m| m.planting_since.is_some() || m.defusing_since.is_some())
    }

    fn reconcile(&mut self, snap: &Snapshot) {
        let Some(me) = &snap.me else { return };
        let alive = me.alive && me.team.is_playing() && me.spectating.is_none();
        if !alive {
            self.predicting = false;
            self.mv = me.mv;
            self.prev_mv = me.mv;
            self.history.clear();
            return;
        }
        if !self.predicting {
            self.predicting = true;
            self.mv = me.mv;
            self.prev_mv = me.mv;
            self.history.clear();
            return;
        }
        // Drop commands the server has already simulated, keeping the one to compare.
        while self
            .history
            .front()
            .is_some_and(|h| h.cmd.tick < snap.last_cmd)
        {
            self.history.pop_front();
        }
        let Some(front) = self.history.front() else {
            // Nothing predicted for that tick (start, resync): adopt the server state.
            self.mv = me.mv;
            return;
        };
        if front.cmd.tick != snap.last_cmd {
            return;
        }
        let err = front.after.origin.distance(me.mv.origin);
        let verr = front.after.velocity.distance(me.mv.velocity);
        let mismatch = err > 0.03
            || verr > 0.5
            || front.after.ducked != me.mv.ducked
            || front.after.on_ground != me.mv.on_ground;
        self.history.pop_front();
        if !mismatch {
            return;
        }
        // Rewind and replay.
        let Some(level) = self.level.clone() else {
            return;
        };
        let before = self.mv.origin;
        let mut mv = me.mv;
        let speed = game_sim::predict::speed_cap(&self.data, me);
        let frozen = self.frozen(snap);
        let boxes = self.others_boxes();
        let dt = self.dt();
        for h in self.history.iter_mut() {
            game_sim::predict::predict_cmd(
                &self.data, &level, &mut mv, &h.cmd, speed, frozen, &boxes, dt,
            );
            h.after = mv;
        }
        let delta = before - mv.origin;
        self.mv = mv;
        // Small errors are smoothed visually (variant C); teleports never are.
        self.visual_error = if self.cfg.variant.smooths_corrections() && delta.length() < 64.0 {
            self.visual_error + delta
        } else {
            Vec3::ZERO
        };
        self.stats.corrections += 1;
        self.stats.last_correction = err;
        let w = &mut self.stats.corrections_window;
        w.push(err);
        if w.len() >= 64 {
            let mut v = std::mem::take(w);
            v.sort_by(f32::total_cmp);
            self.stats.correction_p95 = v[(v.len() * 95 / 100).min(v.len() - 1)];
        }
    }

    /// Advances the command clock; returns how many commands are due now.
    pub fn commands_due(&mut self, now: f64) -> u32 {
        let elapsed = match self.last_frame {
            Some(t) => (now - t).clamp(0.0, 0.25),
            None => 0.0,
        };
        self.last_frame = Some(now);
        if !self.in_game() {
            return 0;
        }
        self.accumulator += elapsed * self.rate * self.time_scale;
        // At most 8 commands per frame; after a hitch the rest is caught up over
        // the next frames instead of being dropped (the server buffer absorbs it).
        let n = self.accumulator.floor().min(8.0);
        self.accumulator -= n;
        if self.accumulator > 32.0 {
            // Long stall (window drag, loading): give up the backlog, the clock
            // feedback resynchronises.
            self.accumulator = self.accumulator.fract();
        }
        n as u32
    }

    /// Fraction of the next command already elapsed (render interpolation).
    pub fn alpha(&self) -> f32 {
        self.accumulator.clamp(0.0, 1.0) as f32
    }

    /// Tick at which remote entities are shown.
    pub fn render_tick(&self, now: f64) -> f64 {
        match self.offset {
            Some(o) => now * self.rate + o - self.interp,
            None => 0.0,
        }
    }

    /// Estimated current server time (seconds).
    pub fn server_time(&self, now: f64) -> f64 {
        match self.offset {
            Some(o) => (now * self.rate + o) / self.rate,
            None => 0.0,
        }
    }

    /// Submits the command for `pred_tick` (tick, view_time are filled here),
    /// predicts it and sends it with redundancy.
    pub fn push_cmd(&mut self, mut cmd: UserCmd, now: f64) {
        cmd.tick = self.pred_tick;
        cmd.view_time = SimTime::from_ticks_f64(self.render_tick(now).max(0.0));
        self.pred_tick += 1;
        self.prev_mv = self.mv;
        if self.predicting
            && let (Some(level), Some(snap)) = (self.level.clone(), self.snapshots.back())
            && let Some(me) = &snap.me
        {
            let speed = game_sim::predict::speed_cap(&self.data, me);
            let frozen = self.frozen(snap);
            let boxes = self.others_boxes();
            let mut mv = self.mv;
            game_sim::predict::predict_cmd(
                &self.data,
                &level,
                &mut mv,
                &cmd,
                speed,
                frozen,
                &boxes,
                self.dt(),
            );
            self.mv = mv;
        }
        self.history.push_back(Predicted {
            cmd: cmd.clone(),
            after: self.mv,
        });
        while self.history.len() > 256 {
            self.history.pop_front();
        }
        let n = self.history.len();
        let cmds: Vec<UserCmd> = self
            .history
            .iter()
            .skip(n.saturating_sub(proto::CMD_REDUNDANCY))
            .map(|h| h.cmd.clone())
            .collect();
        let m = ClientMsg::Cmds {
            cmds,
            snapshot_ack: self.newest_tick,
            interp_ticks: self.interp as f32,
        };
        self.send(Channel::Input, &m);
        // Decay of the smoothed correction (about 100 ms).
        self.visual_error *= (1.0 - 10.0 * self.dt() as f32).max(0.0);
        if self.visual_error.length() < 0.01 {
            self.visual_error = Vec3::ZERO;
        }
    }

    /// Local player feet position to draw (interpolated between predicted ticks).
    pub fn local_origin(&self) -> Vec3 {
        let a = self.alpha();
        self.prev_mv.origin.lerp(self.mv.origin, a) + self.visual_error
    }

    pub fn local_duck(&self) -> f32 {
        let a = self.alpha();
        self.prev_mv.duck_amount + (self.mv.duck_amount - self.prev_mv.duck_amount) * a
    }

    pub fn latest(&self) -> Option<&Snapshot> {
        self.snapshots.back()
    }

    /// Remote players at the render time (interpolation, bounded extrapolation).
    pub fn remote_players(&self, now: f64) -> Vec<RemotePlayer> {
        let me = self.me();
        let t = self.render_tick(now);
        let n = self.snapshots.len();
        if n == 0 {
            return Vec::new();
        }
        // Snapshots bracketing t.
        let mut i1 = n - 1;
        while i1 > 0 && self.snapshots[i1 - 1].tick as f64 >= t {
            i1 -= 1;
        }
        let s1 = &self.snapshots[i1];
        let newest = self.snapshots.back().expect("snapshot");
        let mut out = Vec::new();
        if (s1.tick as f64) < t || i1 == 0 && (s1.tick as f64) <= t {
            // Past the newest snapshot: hold or extrapolate.
            let ahead = ((t - newest.tick as f64) / self.rate) as f32;
            for e in &newest.entities {
                if Some(e.id) == me {
                    continue;
                }
                let mut r = remote(e);
                if self.cfg.variant.extrapolates() && r.alive() && ahead > 0.0 {
                    let h = self.horizon(e.id, ahead);
                    r.origin =
                        self.extrapolate(r.origin, r.velocity, h, r.flags & eflags::ON_GROUND != 0);
                    r.extrapolated = h;
                }
                out.push(r);
            }
            return out;
        }
        let s0 = if i1 > 0 { &self.snapshots[i1 - 1] } else { s1 };
        let span = (s1.tick as f64 - s0.tick as f64).max(1.0);
        let a = ((t - s0.tick as f64) / span).clamp(0.0, 1.0) as f32;
        let prev: HashMap<u8, &EntityQ> = s0.entities.iter().map(|e| (e.id, e)).collect();
        for e in &s1.entities {
            if Some(e.id) == me {
                continue;
            }
            let mut r = remote(e);
            if let Some(p) = prev.get(&e.id)
                && p.alive() == e.alive()
            {
                let o0 = p.origin();
                // No interpolation across teleports (respawn).
                if o0.distance(r.origin) < 200.0 {
                    r.origin = o0.lerp(r.origin, a);
                    r.yaw = lerp_angle(proto::dq_yaw(p.yaw), r.yaw, a);
                    r.pitch = p.pitch as f32 / 100.0 + (r.pitch - p.pitch as f32 / 100.0) * a;
                    r.duck = p.duck as f32 / 255.0 + (r.duck - p.duck as f32 / 255.0) * a;
                }
            }
            out.push(r);
        }
        out
    }

    /// Extrapolation horizon given the confidence in an entity's motion.
    fn horizon(&self, id: u8, ahead: f32) -> f32 {
        let max = self.cfg.extrapolation_horizon;
        if max <= 0.0 {
            return 0.0;
        }
        // Confidence: jitter, acceleration over the last snapshots, packet age.
        let n = self.snapshots.len();
        let mut accel = 0.0f32;
        if n >= 3 {
            let v = |s: &Snapshot| {
                s.entities
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| proto::dq_vel(e.vel))
            };
            if let (Some(a), Some(b)) = (v(&self.snapshots[n - 3]), v(&self.snapshots[n - 1])) {
                let dt = (self.snapshots[n - 1].tick - self.snapshots[n - 3].tick).max(1) as f32
                    / self.rate as f32;
                accel = (b - a).length() / dt;
            }
        }
        let jitter = self.stats.jitter_ms / 1000.0;
        let conf = (1.0 - jitter / 0.03).clamp(0.0, 1.0)
            * (1.0 - accel / 3000.0).clamp(0.0, 1.0)
            * (1.0 - ahead / 0.25).clamp(0.0, 1.0);
        ahead.min(max * conf)
    }

    /// Moves `origin` by `vel * h` with gravity, stopping at walls (never through).
    fn extrapolate(&self, origin: Vec3, vel: Vec3, h: f32, on_ground: bool) -> Vec3 {
        let Some(level) = &self.level else {
            return origin;
        };
        if h <= 0.0 {
            return origin;
        }
        let g = if on_ground {
            0.0
        } else {
            self.data.movement.gravity
        };
        let end = origin + vel * h - Vec3::Y * (0.5 * g * h * h);
        let (mins, maxs) = engine_movement::controller::hull_for(&self.data.movement, false);
        let tr = level.world.trace_box(
            origin + Vec3::Y * 0.5,
            end + Vec3::Y * 0.5,
            mins,
            maxs,
            TraceMask::Players,
        );
        if tr.start_solid {
            origin
        } else {
            tr.end - Vec3::Y * 0.5
        }
    }

    /// Events received since the last call.
    pub fn take_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.events)
    }
}

fn remote(e: &EntityQ) -> RemotePlayer {
    RemotePlayer {
        id: e.id,
        origin: e.origin(),
        velocity: proto::dq_vel(e.vel),
        yaw: proto::dq_yaw(e.yaw),
        pitch: e.pitch as f32 / 100.0,
        duck: e.duck as f32 / 255.0,
        flags: e.flags,
        team: Team::from_u8(e.team).unwrap_or_default(),
        weapon: (e.flags & eflags::NO_WEAPON == 0).then_some(engine_weapons::WeaponId(e.weapon)),
        health: e.health,
        armor: e.armor,
        extrapolated: 0.0,
    }
}

fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let mut d = (b - a) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    a + d * t
}
