//! Generates the original Projet S maps into `maps/*.ron`.
//!
//! `cargo run -p mapgen -- [output_dir]` (default `maps/`). The maps are authored in
//! code so they stay reviewable and reproducible: a grid of 128-unit cells is carved
//! out of solid rock, walls are merged into the fewest boxes, then props (crates,
//! platforms, thin wallbang walls, ramps) are added. All geometry is original.

use engine_core::Team;
use engine_physics::map::{
    BrushDef, LabelDef, MapDef, RampDir, SpawnDef, TargetDef, TargetKind, ZoneDef,
};
use glam::Vec3;
use std::path::PathBuf;

struct Builder {
    map: MapDef,
}

impl Builder {
    fn new(name: &str, display: &str, modes: &[&str], min: Vec3, max: Vec3) -> Self {
        Builder {
            map: MapDef {
                name: name.into(),
                display_name: display.into(),
                version: 1,
                modes: modes.iter().map(|s| s.to_string()).collect(),
                brushes: Vec::new(),
                spawns: Vec::new(),
                dm_spawns: Vec::new(),
                bomb_sites: Vec::new(),
                buy_zones: Vec::new(),
                targets: Vec::new(),
                labels: Vec::new(),
                bounds_min: min,
                bounds_max: max,
                sky_color: [0.55, 0.65, 0.78],
            },
        }
    }

    fn solid(&mut self, min: Vec3, max: Vec3, mat: &str) {
        self.map.brushes.push(BrushDef::Box {
            min,
            max,
            mat: mat.into(),
            players: true,
            bullets: true,
        });
    }

    fn clip(&mut self, min: Vec3, max: Vec3) {
        self.map.brushes.push(BrushDef::Clip { min, max });
    }

    fn ramp(&mut self, min: Vec3, max: Vec3, high: RampDir, mat: &str) {
        self.map.brushes.push(BrushDef::Ramp {
            min,
            max,
            high,
            mat: mat.into(),
        });
    }

    fn spawn(&mut self, team: Team, x: f32, y: f32, z: f32, yaw: f32) {
        self.map.spawns.push(SpawnDef {
            team,
            pos: Vec3::new(x, y + 1.0, z),
            yaw,
        });
    }

    fn dm(&mut self, x: f32, y: f32, z: f32, yaw: f32) {
        self.map.dm_spawns.push(SpawnDef {
            team: Team::Spectator,
            pos: Vec3::new(x, y + 1.0, z),
            yaw,
        });
    }

    fn label(&mut self, x: f32, y: f32, z: f32, text: &str) {
        self.map.labels.push(LabelDef {
            pos: Vec3::new(x, y, z),
            text: text.into(),
        });
    }

    fn target(&mut self, x: f32, z: f32, yaw: f32, kind: TargetKind, armor: u8, helmet: bool) {
        self.map.targets.push(TargetDef {
            pos: Vec3::new(x, 1.0, z),
            yaw,
            kind,
            armor,
            helmet,
        });
    }

    /// Floor slab under the whole map and an invisible clip shell above the walls.
    fn shell(&mut self, floor_mat: &str, wall_top: f32) {
        let (a, b) = (self.map.bounds_min, self.map.bounds_max);
        self.solid(
            Vec3::new(a.x, -32.0, a.z),
            Vec3::new(b.x, 0.0, b.z),
            floor_mat,
        );
        let top = 1536.0;
        let t = 64.0;
        self.clip(
            Vec3::new(a.x - t, wall_top, a.z - t),
            Vec3::new(b.x + t, top, a.z),
        );
        self.clip(
            Vec3::new(a.x - t, wall_top, b.z),
            Vec3::new(b.x + t, top, b.z + t),
        );
        self.clip(Vec3::new(a.x - t, wall_top, a.z), Vec3::new(a.x, top, b.z));
        self.clip(Vec3::new(b.x, wall_top, a.z), Vec3::new(b.x + t, top, b.z));
        self.clip(
            Vec3::new(a.x - t, top, a.z - t),
            Vec3::new(b.x + t, top + t, b.z + t),
        );
    }

    /// Four walls around the bounds.
    fn perimeter(&mut self, height: f32, mat: &str) {
        let (a, b) = (self.map.bounds_min, self.map.bounds_max);
        let t = 32.0;
        self.solid(
            Vec3::new(a.x, 0.0, a.z),
            Vec3::new(b.x, height, a.z + t),
            mat,
        );
        self.solid(
            Vec3::new(a.x, 0.0, b.z - t),
            Vec3::new(b.x, height, b.z),
            mat,
        );
        self.solid(
            Vec3::new(a.x, 0.0, a.z + t),
            Vec3::new(a.x + t, height, b.z - t),
            mat,
        );
        self.solid(
            Vec3::new(b.x - t, 0.0, a.z + t),
            Vec3::new(b.x, height, b.z - t),
            mat,
        );
    }
}

/// Grid carved out of solid; remaining cells become merged wall boxes.
struct Grid {
    cols: usize,
    rows: usize,
    cell: f32,
    origin: Vec3,
    solid: Vec<bool>,
}

impl Grid {
    fn new(cols: usize, rows: usize, cell: f32, origin: Vec3) -> Self {
        Grid {
            cols,
            rows,
            cell,
            origin,
            solid: vec![true; cols * rows],
        }
    }

    fn carve(&mut self, c0: usize, r0: usize, c1: usize, r1: usize) {
        for r in r0..=r1 {
            for c in c0..=c1 {
                self.solid[r * self.cols + c] = false;
            }
        }
    }

    fn x(&self, c: usize) -> f32 {
        self.origin.x + c as f32 * self.cell
    }

    fn z(&self, r: usize) -> f32 {
        self.origin.z + r as f32 * self.cell
    }

    fn center(&self, c: usize, r: usize) -> (f32, f32) {
        (self.x(c) + self.cell * 0.5, self.z(r) + self.cell * 0.5)
    }

    /// Cell rectangle to world box.
    fn rect(&self, c0: usize, r0: usize, c1: usize, r1: usize, y0: f32, y1: f32) -> (Vec3, Vec3) {
        (
            Vec3::new(self.x(c0), y0, self.z(r0)),
            Vec3::new(self.x(c1 + 1), y1, self.z(r1 + 1)),
        )
    }

    /// Greedy rectangle merge of the solid cells.
    fn walls(&self, b: &mut Builder, height: f32, mat: &str) {
        let mut used = vec![false; self.solid.len()];
        for r in 0..self.rows {
            for c in 0..self.cols {
                let i = r * self.cols + c;
                if !self.solid[i] || used[i] {
                    continue;
                }
                let mut c1 = c;
                while c1 + 1 < self.cols
                    && self.solid[r * self.cols + c1 + 1]
                    && !used[r * self.cols + c1 + 1]
                {
                    c1 += 1;
                }
                let mut r1 = r;
                'grow: while r1 + 1 < self.rows {
                    for cc in c..=c1 {
                        let j = (r1 + 1) * self.cols + cc;
                        if !self.solid[j] || used[j] {
                            break 'grow;
                        }
                    }
                    r1 += 1;
                }
                for rr in r..=r1 {
                    for cc in c..=c1 {
                        used[rr * self.cols + cc] = true;
                    }
                }
                let (min, max) = self.rect(c, r, c1, r1, 0.0, height);
                b.solid(min, max, mat);
            }
        }
    }
}

/// Competitive 5v5 map, two bomb sites, three lanes.
fn citadel() -> MapDef {
    let mut b = Builder::new(
        "ps_citadel",
        "Citadelle",
        &["competitive", "deathmatch", "training"],
        Vec3::new(-2048.0, -32.0, -2048.0),
        Vec3::new(2048.0, 1536.0, 2048.0),
    );
    b.map.sky_color = [0.62, 0.68, 0.78];
    let mut g = Grid::new(32, 32, 128.0, Vec3::new(-2048.0, 0.0, -2048.0));
    // Defender spawn (north) and the two sites.
    g.carve(12, 1, 19, 5);
    g.carve(1, 1, 8, 6); // site A
    g.carve(24, 1, 30, 6); // site B
    g.carve(9, 3, 11, 4); // spawn → A
    g.carve(20, 3, 23, 4); // spawn → B
    g.carve(14, 6, 17, 9); // spawn → mid
    g.carve(14, 10, 17, 21); // mid
    g.carve(6, 7, 7, 10); // A short (stairs side)
    g.carve(6, 11, 13, 12); // A short connector from mid
    g.carve(1, 7, 3, 20); // A long
    g.carve(1, 21, 8, 23); // A long entrance
    g.carve(8, 23, 10, 26); // T → A long
    g.carve(26, 7, 28, 18); // B tunnels
    g.carve(20, 19, 28, 21); // B tunnels entrance
    g.carve(24, 22, 25, 26); // T → B
    g.carve(21, 26, 25, 27);
    g.carve(18, 14, 25, 15); // mid → B window
    g.carve(14, 22, 17, 24); // T → mid
    g.carve(11, 25, 20, 30); // attacker spawn (south)
    g.walls(&mut b, 256.0, "concrete");
    b.shell("sand", 256.0);

    // Site A: raised platform with a 16-unit step all around its open sides.
    let (lo, hi) = g.rect(1, 1, 8, 5, 0.0, 32.0);
    b.solid(lo, hi, "tile");
    let (lo, hi) = g.rect(1, 6, 8, 6, 0.0, 16.0);
    b.solid(lo, hi, "tile");
    let (lo, hi) = g.rect(9, 3, 9, 4, 0.0, 16.0);
    b.solid(lo, hi, "tile");
    let (lo, hi) = g.rect(3, 2, 4, 3, 32.0, 96.0);
    b.solid(lo, hi, "crate");
    let (lo, hi) = g.rect(6, 4, 6, 4, 32.0, 80.0);
    b.solid(lo, hi, "crate");
    // Pillar on A.
    let (lo, hi) = g.rect(7, 1, 7, 1, 32.0, 256.0);
    b.solid(lo, hi, "brick");
    b.map
        .bomb_sites
        .push(zone(&g, "A", Team::Spectator, 1, 1, 8, 6));

    // Site B: crates, a stacked crate (boost) and a thin metal shed wall.
    let (lo, hi) = g.rect(27, 2, 28, 3, 0.0, 64.0);
    b.solid(lo, hi, "crate");
    let (lo, hi) = g.rect(27, 2, 27, 2, 64.0, 112.0);
    b.solid(lo, hi, "crate");
    let (lo, hi) = g.rect(25, 5, 25, 5, 0.0, 48.0);
    b.solid(lo, hi, "crate");
    let x = g.x(30);
    b.solid(
        Vec3::new(x - 8.0, 0.0, g.z(1)),
        Vec3::new(x, 128.0, g.z(3)),
        "metal_thin",
    );
    b.map
        .bomb_sites
        .push(zone(&g, "B", Team::Spectator, 24, 1, 30, 6));
    // Tunnel ceiling (low covered section).
    let (lo, hi) = g.rect(26, 9, 28, 16, 160.0, 192.0);
    b.solid(lo, hi, "concrete_dark");

    // Mid: central cover box, low wall near the defenders.
    let (lo, hi) = g.rect(15, 15, 16, 15, 0.0, 48.0);
    b.solid(lo, hi, "concrete");
    let (lo, hi) = g.rect(14, 9, 14, 9, 0.0, 64.0);
    b.solid(lo, hi, "crate");
    // Window: thin wood wall, half of the passage (wallbang spot).
    let x = g.x(21);
    b.solid(
        Vec3::new(x, 0.0, g.z(14)),
        Vec3::new(x + 8.0, 112.0, g.z(15)),
        "wood",
    );
    b.label(x + 4.0, 130.0, g.z(14) + 64.0, "Bois 8 u");
    // A short: metal door frame (thin plaster wall with a gap).
    let z = g.z(11);
    b.solid(
        Vec3::new(g.x(9), 0.0, z),
        Vec3::new(g.x(10), 256.0, z + 8.0),
        "plaster",
    );
    // A long: ramp up to a small ledge and back down.
    let (lo, hi) = g.rect(1, 14, 1, 15, 0.0, 48.0);
    b.solid(lo, hi, "brick");
    b.ramp(
        Vec3::new(g.x(1), 0.0, g.z(16)),
        Vec3::new(g.x(2), 48.0, g.z(16) + 96.0),
        RampDir::NegZ,
        "brick",
    );
    // T side crates.
    let (lo, hi) = g.rect(5, 22, 5, 22, 0.0, 64.0);
    b.solid(lo, hi, "crate");
    let (lo, hi) = g.rect(22, 20, 22, 20, 0.0, 64.0);
    b.solid(lo, hi, "crate");

    // Spawns.
    for (c, r) in [(13, 2), (15, 2), (17, 2), (14, 4), (16, 4)] {
        let (x, z) = g.center(c, r);
        b.spawn(Team::Defense, x, 0.0, z, 180.0);
    }
    for (c, r) in [(12, 27), (14, 27), (16, 27), (18, 27), (15, 29)] {
        let (x, z) = g.center(c, r);
        b.spawn(Team::Attack, x, 0.0, z, 0.0);
    }
    b.map
        .buy_zones
        .push(zone(&g, "buy_defense", Team::Defense, 12, 1, 19, 5));
    b.map
        .buy_zones
        .push(zone(&g, "buy_attack", Team::Attack, 11, 25, 20, 30));
    for (c, r, yaw) in [
        (2, 9, 180.0),
        (2, 19, 0.0),
        (7, 8, 0.0),
        (11, 12, 90.0),
        (15, 12, 0.0),
        (16, 19, 180.0),
        (27, 8, 180.0),
        (27, 17, 0.0),
        (23, 20, 270.0),
        (20, 14, 270.0),
        (4, 22, 270.0),
        (15, 23, 0.0),
        (13, 3, 180.0),
        (22, 26, 0.0),
        (25, 2, 180.0),
        (3, 5, 180.0),
    ] {
        let (x, z) = g.center(c, r);
        let y = if (1..=8).contains(&c) && (1..=5).contains(&r) {
            32.0
        } else {
            0.0
        };
        b.dm(x, y, z, yaw);
    }
    b.label(g.center(4, 3).0, 220.0, g.center(4, 3).1, "Site A");
    b.label(g.center(27, 4).0, 200.0, g.center(27, 4).1, "Site B");
    b.label(g.center(15, 12).0, 200.0, g.center(15, 12).1, "Mid");
    b.map
}

fn zone(g: &Grid, name: &str, team: Team, c0: usize, r0: usize, c1: usize, r1: usize) -> ZoneDef {
    let (min, max) = g.rect(c0, r0, c1, r1, 0.0, 200.0);
    ZoneDef {
        name: name.into(),
        team,
        min,
        max,
    }
}

/// Training room: shooting range, wallbang station, movement course, spray wall.
fn training_lab() -> MapDef {
    let mut b = Builder::new(
        "ps_training_lab",
        "Training Lab",
        &["training", "deathmatch"],
        Vec3::new(-1536.0, -32.0, -2048.0),
        Vec3::new(1536.0, 1536.0, 1024.0),
    );
    b.map.sky_color = [0.6, 0.66, 0.74];
    b.perimeter(384.0, "concrete_dark");
    b.shell("concrete", 384.0);
    b.spawn(Team::Attack, 0.0, 0.0, 800.0, 0.0);
    b.spawn(Team::Defense, 120.0, 0.0, 800.0, 0.0);
    b.spawn(Team::Attack, -120.0, 0.0, 800.0, 0.0);
    b.spawn(Team::Defense, 240.0, 0.0, 800.0, 0.0);
    for x in [-1200.0, -400.0, 400.0, 1200.0] {
        b.dm(x, 0.0, 700.0, 0.0);
        b.dm(x, 0.0, -1700.0, 180.0);
    }
    b.map.buy_zones.push(ZoneDef {
        name: "buy".into(),
        team: Team::Spectator,
        min: Vec3::new(-1536.0, 0.0, -2048.0),
        max: Vec3::new(1536.0, 400.0, 1024.0),
    });

    // Shooting range in front of the spawn, firing line at z = 600.
    b.solid(
        Vec3::new(-700.0, 0.0, 560.0),
        Vec3::new(700.0, 40.0, 576.0),
        "wood",
    );
    b.label(0.0, 60.0, 568.0, "Ligne de tir");
    let rows = [
        (-300.0, TargetKind::Static, "10 m"),
        (-900.0, TargetKind::Strafe, "Mobiles 20 m"),
        (-1400.0, TargetKind::Crouch, "Accroupis 30 m"),
        (-1900.0, TargetKind::Static, "40 m"),
    ];
    for (z, kind, text) in rows {
        for (i, x) in [-360.0, -120.0, 120.0, 360.0].into_iter().enumerate() {
            let (armor, helmet) = match i {
                0 => (0, false),
                1 => (100, false),
                _ => (100, true),
            };
            b.target(x, z, 180.0, kind, armor, helmet);
        }
        b.label(-520.0, 120.0, z, text);
    }
    b.label(-360.0, 100.0, -200.0, "Sans armure");
    b.label(-120.0, 100.0, -200.0, "Kevlar");
    b.label(240.0, 100.0, -200.0, "Kevlar + casque");

    // Wallbang station (west): a wall per material and thickness, target behind.
    let mats = [
        "wood",
        "plaster",
        "metal_thin",
        "brick",
        "concrete",
        "metal_thick",
    ];
    for (i, m) in mats.iter().enumerate() {
        for (j, t) in [8.0f32, 16.0, 32.0].into_iter().enumerate() {
            let x = -1440.0 + j as f32 * 140.0;
            let z = 300.0 - i as f32 * 220.0;
            b.solid(Vec3::new(x, 0.0, z), Vec3::new(x + 96.0, 128.0, z + t), m);
            b.target(x + 48.0, z - 90.0, 0.0, TargetKind::Static, 0, false);
            b.label(x + 48.0, 150.0, z + t * 0.5, &format!("{m} {t} u"));
        }
    }

    // Movement course (east).
    for i in 0..6 {
        let h = 16.0 * (i + 1) as f32;
        let z = 400.0 - i as f32 * 32.0;
        b.solid(
            Vec3::new(800.0, 0.0, z - 32.0),
            Vec3::new(960.0, h, z),
            "concrete",
        );
    }
    b.label(880.0, 140.0, 400.0, "Escalier 16 u");
    b.solid(
        Vec3::new(1050.0, 0.0, 200.0),
        Vec3::new(1210.0, 24.0, 400.0),
        "metal_thin",
    );
    b.label(1130.0, 60.0, 300.0, "Marche 24 u (bloque)");
    b.ramp(
        Vec3::new(1300.0, 0.0, 100.0),
        Vec3::new(1460.0, 64.0, 211.0),
        RampDir::NegZ,
        "concrete",
    );
    b.solid(
        Vec3::new(1300.0, 0.0, -60.0),
        Vec3::new(1460.0, 64.0, 100.0),
        "concrete",
    );
    b.label(1380.0, 120.0, 150.0, "Rampe 30°");
    b.ramp(
        Vec3::new(800.0, 0.0, -150.0),
        Vec3::new(960.0, 96.0, -70.0),
        RampDir::NegZ,
        "metal_thin",
    );
    b.label(880.0, 140.0, -110.0, "Pente 50° (glisse)");
    for (i, h) in [32.0, 48.0, 56.0, 64.0].into_iter().enumerate() {
        let x = 1050.0 + i as f32 * 110.0;
        b.solid(
            Vec3::new(x, 0.0, -300.0),
            Vec3::new(x + 80.0, h, -220.0),
            "crate",
        );
        b.label(x + 40.0, h + 30.0, -260.0, &format!("Saut {h} u"));
    }
    // Crouch tunnel: 60 units high (only the 54-unit crouched hull fits).
    b.solid(
        Vec3::new(800.0, 60.0, -700.0),
        Vec3::new(960.0, 128.0, -450.0),
        "concrete_dark",
    );
    b.solid(
        Vec3::new(760.0, 0.0, -700.0),
        Vec3::new(800.0, 128.0, -450.0),
        "concrete_dark",
    );
    b.solid(
        Vec3::new(960.0, 0.0, -700.0),
        Vec3::new(1000.0, 128.0, -450.0),
        "concrete_dark",
    );
    b.label(880.0, 150.0, -575.0, "Tunnel accroupi");
    // Long strafe lane for counter-strafe practice, marked every 64 u.
    for i in 0..9 {
        let x = 1050.0 + i as f32 * 48.0;
        b.solid(
            Vec3::new(x, 0.0, -900.0),
            Vec3::new(x + 4.0, 2.0, -700.0),
            "target_orange",
        );
    }
    b.label(1250.0, 80.0, -800.0, "Contre-strafe");

    // Spray wall at the far end.
    b.solid(
        Vec3::new(-600.0, 0.0, -2000.0),
        Vec3::new(600.0, 320.0, -1984.0),
        "plaster",
    );
    b.label(0.0, 340.0, -1990.0, "Mur de spray");
    b.map
}

/// Symmetric 1v1 arena.
fn duel_lab() -> MapDef {
    let mut b = Builder::new(
        "ps_duel_lab",
        "Duel Lab",
        &["duel", "deathmatch", "training"],
        Vec3::new(-1024.0, -32.0, -1536.0),
        Vec3::new(1024.0, 1536.0, 1536.0),
    );
    b.map.sky_color = [0.7, 0.62, 0.55];
    b.perimeter(320.0, "brick");
    b.shell("sand", 320.0);
    // Symmetric cover (mirrored on z and x).
    b.solid(
        Vec3::new(-96.0, 0.0, -96.0),
        Vec3::new(96.0, 256.0, 96.0),
        "concrete",
    );
    for s in [-1.0f32, 1.0] {
        let mirror = |z0: f32, z1: f32| if s > 0.0 { (z0, z1) } else { (-z1, -z0) };
        let (a, c) = mirror(400.0, 432.0);
        b.solid(
            Vec3::new(-600.0, 0.0, a),
            Vec3::new(-300.0, 96.0, c),
            "brick",
        );
        b.solid(
            Vec3::new(300.0, 0.0, -c),
            Vec3::new(600.0, 96.0, -a),
            "brick",
        );
        let (a, c) = mirror(700.0, 780.0);
        b.solid(Vec3::new(120.0, 0.0, a), Vec3::new(200.0, 64.0, c), "crate");
        b.solid(
            Vec3::new(-200.0, 0.0, -c),
            Vec3::new(-120.0, 64.0, -a),
            "crate",
        );
        let (a, c) = mirror(200.0, 208.0);
        b.solid(Vec3::new(500.0, 0.0, a), Vec3::new(800.0, 128.0, c), "wood");
        b.solid(
            Vec3::new(-800.0, 0.0, -c),
            Vec3::new(-500.0, 128.0, -a),
            "wood",
        );
    }
    b.spawn(Team::Attack, 0.0, 0.0, 1300.0, 0.0);
    b.spawn(Team::Defense, 0.0, 0.0, -1300.0, 180.0);
    for (x, z, yaw) in [
        (-700.0, 1200.0, 0.0),
        (700.0, 1200.0, 0.0),
        (-700.0, -1200.0, 180.0),
        (700.0, -1200.0, 180.0),
        (-800.0, 0.0, 270.0),
        (800.0, 0.0, 90.0),
    ] {
        b.dm(x, 0.0, z, yaw);
    }
    b.map.buy_zones.push(ZoneDef {
        name: "buy".into(),
        team: Team::Spectator,
        min: Vec3::new(-1024.0, 0.0, -1536.0),
        max: Vec3::new(1024.0, 400.0, 1536.0),
    });
    b.map
}

fn main() {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("maps"));
    std::fs::create_dir_all(&out).expect("create output dir");
    let cfg = ron::ser::PrettyConfig::new()
        .depth_limit(3)
        .struct_names(false);
    for map in [citadel(), training_lab(), duel_lab()] {
        let text = ron::ser::to_string_pretty(&map, cfg.clone()).expect("serialize map");
        let path = out.join(format!("{}.ron", map.name));
        let header = format!(
            "// Generated by tools/mapgen — do not edit by hand.\n// {} ({} brushes)\n",
            map.display_name,
            map.brushes.len()
        );
        std::fs::write(&path, header + &text).expect("write map");
        println!(
            "{} : {} brushes, {} spawns",
            path.display(),
            map.brushes.len(),
            map.spawns.len()
        );
    }
}
