//! Weapon models from CC0 sources (Quaternius, list in
//! `assets/weapons/SOURCES.md`), converted by `tools/weapons.py` to the
//! conventions of `ps-engine-character::weapon_mesh`: origin at the right-hand
//! grip, -z towards the muzzle, game units, matte gunmetal vertex colours.
//! Holds without a model here (grenade, bomb) keep the procedural mesh.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Mesh, PrimitiveTopology};
use bevy::prelude::Vec3;
use ps_engine_character::anim::Hold;

const RIFLE: &[u8] = include_bytes!("../../../../assets/weapons/rifle.bin");
const SNIPER: &[u8] = include_bytes!("../../../../assets/weapons/sniper.bin");
const PISTOL: &[u8] = include_bytes!("../../../../assets/weapons/pistol.bin");
const KNIFE: &[u8] = include_bytes!("../../../../assets/weapons/knife.bin");

fn model(h: Hold) -> Option<&'static [u8]> {
    match h {
        Hold::Rifle => Some(RIFLE),
        Hold::Sniper => Some(SNIPER),
        Hold::Pistol => Some(PISTOL),
        Hold::Knife => Some(KNIFE),
        Hold::Grenade | Hold::Bomb | Hold::Empty => None,
    }
}

/// Model of a hold, if one is shipped.
pub fn mesh(h: Hold) -> Option<Mesh> {
    let s = decode(model(h)?).expect("weapon model embedded in the binary");
    Some(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, s.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, s.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, s.colors),
    )
}

#[derive(Default)]
struct Solid {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
}

/// Reads the format written by `tools/weapons.py` (little-endian):
/// `PSW1`, u8 colour count, colours (3 f32, linear), u32 triangle count, then
/// per triangle a u8 colour index and 3 vertices of 3 f32.
fn decode(b: &[u8]) -> Option<Solid> {
    let mut r = Reader(b);
    if r.take(4)? != b"PSW1" {
        return None;
    }
    let colours: Vec<[f32; 4]> = (0..r.u8()?)
        .map(|_| Some([r.f32()?, r.f32()?, r.f32()?, 1.0]))
        .collect::<Option<_>>()?;
    let tris = r.u32()? as usize;
    let mut s = Solid::default();
    for _ in 0..tris {
        let col = *colours.get(r.u8()? as usize)?;
        let mut v = [Vec3::ZERO; 3];
        for p in &mut v {
            *p = Vec3::new(r.f32()?, r.f32()?, r.f32()?);
        }
        let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
        for p in v {
            s.positions.push(p.to_array());
            s.normals.push(n.to_array());
            s.colors.push(col);
        }
    }
    r.0.is_empty().then_some(s)
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.0.split_at_checked(n)?;
        self.0 = tail;
        Some(head)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bounds (min, max) of a decoded model, game units.
    fn bounds(h: Hold) -> (Vec3, Vec3) {
        let s = decode(model(h).unwrap()).unwrap();
        s.positions.iter().fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(lo, hi), p| (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p))),
        )
    }

    #[test]
    fn models_follow_the_weapon_mesh_conventions() {
        for (h, length) in [
            (Hold::Rifle, 33.0),
            (Hold::Sniper, 43.0),
            (Hold::Pistol, 7.6),
            (Hold::Knife, 12.0),
        ] {
            let (lo, hi) = bounds(h);
            // Real-world length along z, muzzle (or blade) towards -z.
            assert!(((hi.z - lo.z) - length).abs() < 0.01, "{h:?}: {lo} {hi}");
            assert!(-lo.z > hi.z, "{h:?}: the barrel must point to -z");
            // The grip (origin) lies inside the weapon, which is thin along x
            // (the sniper's bolt handle sticks out to the right).
            assert!(
                lo.cmplt(Vec3::ZERO).all() && hi.cmpgt(Vec3::ZERO).all(),
                "{h:?}"
            );
            assert!(hi.x - lo.x < 3.5, "{h:?}: {lo} {hi}");
        }
    }

    #[test]
    fn models_are_low_poly() {
        for h in [Hold::Rifle, Hold::Sniper, Hold::Pistol, Hold::Knife] {
            let m = mesh(h).unwrap();
            let n = m.count_vertices() / 3;
            assert!((100..2500).contains(&n), "{h:?}: {n} triangles");
        }
        assert!(mesh(Hold::Grenade).is_none() && mesh(Hold::Bomb).is_none());
    }

    #[test]
    fn truncated_data_is_rejected() {
        assert!(decode(&RIFLE[..RIFLE.len() - 1]).is_none());
        assert!(decode(b"PSW0").is_none());
    }
}
