//! The engine poses are drawn around the game hitboxes: the skinned skin of
//! the liquid characters must stay close to the `ps-hitreg` capsules, or a
//! player would be hit (or missed) where nothing is drawn.

use super::anim::{AnimInput, Animator, Hold};
use bevy::math::Vec3;
use ps_engine_character::mesh::{self, Lod, Parts};
use ps_engine_character::rig::{self, Pose};

fn run(inp: AnimInput, secs: f32) -> Animator {
    let mut a = Animator::default();
    let mut t = 0.0;
    while t < secs {
        a.update(&inp, 1.0 / 128.0);
        t += 1.0 / 128.0;
    }
    a
}

fn skinned(pose: &Pose, m: &mesh::MeshData) -> Vec<Vec3> {
    let mats: Vec<_> = (0..rig::JOINT_COUNT).map(|j| pose.skin_matrix(j)).collect();
    m.positions
        .iter()
        .zip(m.joints.iter().zip(&m.weights))
        .map(|(p, (j, w))| {
            let p = Vec3::from_array(*p);
            (0..4)
                .map(|k| mats[j[k] as usize].transform_point3(p) * w[k])
                .sum()
        })
        .collect()
}

fn capsule_dist(p: Vec3, c: &ps_hitreg::Capsule) -> f32 {
    let ab = c.b - c.a;
    let t = ((p - c.a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    (p - (c.a + ab * t)).length() - c.radius
}

/// Share of skin vertices within `margin` of a hitbox.
fn coverage(pose: &Pose, duck: f32, margin: f32) -> f32 {
    let m = mesh::humanoid(Lod::Mid, Parts::FullBody);
    let hb = ps_hitreg::hitboxes(&ps_hitreg::Pose {
        origin: Vec3::ZERO,
        yaw: 0.0,
        duck,
    });
    let pts = skinned(pose, &m);
    let inside = pts
        .iter()
        .filter(|p| hb.capsules.iter().any(|c| capsule_dist(**p, c) < margin))
        .count();
    inside as f32 / pts.len() as f32
}

#[test]
fn skin_stays_in_hitboxes() {
    let cases = [
        ("debout", AnimInput::default(), 0.0),
        (
            "accroupi",
            AnimInput {
                duck: 1.0,
                ..Default::default()
            },
            1.0,
        ),
        (
            "course",
            AnimInput {
                velocity: Vec3::new(0.0, 0.0, -250.0),
                ..Default::default()
            },
            0.0,
        ),
        (
            "pistolet",
            AnimInput {
                hold: Hold::Pistol,
                ..Default::default()
            },
            0.0,
        ),
    ];
    for (name, inp, duck) in cases {
        let a = run(inp, 1.3);
        let c = coverage(&a.pose, duck, 3.0);
        assert!(
            c > 0.9,
            "{name}: {:.1}% de la peau près des hitboxes",
            c * 100.0
        );
    }
}
