//! Simulated network link (latency, jitter, loss, duplication, reordering) for
//! tests and the network lab. Deterministic for a given seed.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkProfile {
    /// One-way base latency, seconds.
    pub latency: f64,
    /// Uniform extra delay in [0, jitter].
    pub jitter: f64,
    pub loss: f64,
    pub duplicate: f64,
}

impl LinkProfile {
    pub const LAN: LinkProfile = LinkProfile {
        latency: 0.001,
        jitter: 0.0005,
        loss: 0.0,
        duplicate: 0.0,
    };
    pub const GOOD: LinkProfile = LinkProfile {
        latency: 0.015,
        jitter: 0.002,
        loss: 0.0,
        duplicate: 0.0,
    };
    pub const AVERAGE: LinkProfile = LinkProfile {
        latency: 0.035,
        jitter: 0.008,
        loss: 0.01,
        duplicate: 0.0,
    };
    pub const BAD: LinkProfile = LinkProfile {
        latency: 0.06,
        jitter: 0.025,
        loss: 0.05,
        duplicate: 0.01,
    };
}

pub struct Link {
    pub profile: LinkProfile,
    state: u64,
    queue: Vec<(f64, u64, Vec<u8>)>,
    counter: u64,
    pub dropped: u64,
}

impl Link {
    pub fn new(profile: LinkProfile, seed: u64) -> Link {
        Link {
            profile,
            state: seed | 1,
            queue: Vec::new(),
            counter: 0,
            dropped: 0,
        }
    }

    fn rand(&mut self) -> f64 {
        // xorshift64*
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        (self.state.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn send(&mut self, now: f64, pkt: Vec<u8>) {
        if self.rand() < self.profile.loss {
            self.dropped += 1;
            return;
        }
        let copies = if self.rand() < self.profile.duplicate {
            2
        } else {
            1
        };
        for _ in 0..copies {
            let at = now + self.profile.latency + self.rand() * self.profile.jitter;
            self.counter += 1;
            self.queue.push((at, self.counter, pkt.clone()));
        }
    }

    /// Packets delivered at or before `now`, in arrival order.
    pub fn deliver(&mut self, now: f64) -> Vec<Vec<u8>> {
        self.queue
            .sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let n = self.queue.iter().take_while(|p| p.0 <= now).count();
        self.queue.drain(..n).map(|p| p.2).collect()
    }
}
