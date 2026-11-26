//! Map presentation: brushes merged into one mesh per material, lights, sky.
//!
//! Game space is Y-up in Source units (inches); the renderer works in metres:
//! every position is multiplied by [`SCALE`].

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use engine_physics::map::{BrushDef, MapDef, RampDir};
use engine_physics::material::MaterialTable;
use std::collections::BTreeMap;

pub const SCALE: f32 = 0.0254;

/// Marker for entities that belong to the loaded map (despawned on map change).
#[derive(Component)]
pub struct MapEntity;

#[derive(Default)]
struct MeshBuilder {
    pos: Vec<[f32; 3]>,
    normal: Vec<[f32; 3]>,
    color: Vec<[f32; 4]>,
    uv: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

impl MeshBuilder {
    /// Adds a planar polygon (convex, any winding): the winding is fixed so the
    /// face points away from `inside`.
    fn poly(&mut self, pts: &[Vec3], inside: Vec3, shade: f32) {
        if pts.len() < 3 {
            return;
        }
        let mut n = (pts[1] - pts[0]).cross(pts[2] - pts[0]).normalize_or_zero();
        let c = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;
        let flip = n.dot(c - inside) < 0.0;
        if flip {
            n = -n;
        }
        // Light per-face shading and a 64-unit checker so distances read well.
        let base = self.pos.len() as u32;
        for p in pts {
            let (u, v) = if n.y.abs() > 0.7 {
                (p.x, p.z)
            } else if n.x.abs() > 0.7 {
                (p.z, p.y)
            } else {
                (p.x, p.y)
            };
            let checker = if ((u / 64.0).floor() + (v / 64.0).floor()) as i64 % 2 == 0 {
                1.0
            } else {
                0.93
            };
            let s = shade * checker;
            self.pos.push((*p * SCALE).to_array());
            self.normal.push(n.to_array());
            self.color.push([s, s, s, 1.0]);
            self.uv.push([u / 64.0, v / 64.0]);
        }
        for i in 1..pts.len() as u32 - 1 {
            if flip {
                self.idx.extend_from_slice(&[base, base + i + 1, base + i]);
            } else {
                self.idx.extend_from_slice(&[base, base + i, base + i + 1]);
            }
        }
    }

    fn cuboid(&mut self, min: Vec3, max: Vec3, shade: f32) {
        let c = (min + max) * 0.5;
        let p = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        let (a, b) = (min, max);
        self.poly(
            &[
                p(a.x, b.y, a.z),
                p(b.x, b.y, a.z),
                p(b.x, b.y, b.z),
                p(a.x, b.y, b.z),
            ],
            c,
            shade,
        );
        self.poly(
            &[
                p(a.x, a.y, a.z),
                p(b.x, a.y, a.z),
                p(b.x, a.y, b.z),
                p(a.x, a.y, b.z),
            ],
            c,
            shade * 0.7,
        );
        self.poly(
            &[
                p(a.x, a.y, a.z),
                p(a.x, b.y, a.z),
                p(a.x, b.y, b.z),
                p(a.x, a.y, b.z),
            ],
            c,
            shade * 0.85,
        );
        self.poly(
            &[
                p(b.x, a.y, a.z),
                p(b.x, b.y, a.z),
                p(b.x, b.y, b.z),
                p(b.x, a.y, b.z),
            ],
            c,
            shade * 0.85,
        );
        self.poly(
            &[
                p(a.x, a.y, a.z),
                p(b.x, a.y, a.z),
                p(b.x, b.y, a.z),
                p(a.x, b.y, a.z),
            ],
            c,
            shade * 0.92,
        );
        self.poly(
            &[
                p(a.x, a.y, b.z),
                p(b.x, a.y, b.z),
                p(b.x, b.y, b.z),
                p(a.x, b.y, b.z),
            ],
            c,
            shade * 0.92,
        );
    }

    /// Wedge rising from `min.y` (low side) to `max.y` towards `high`.
    fn ramp(&mut self, min: Vec3, max: Vec3, high: RampDir, shade: f32) {
        // Corners of the footprint, the high edge raised.
        let corners = [
            Vec3::new(min.x, 0.0, min.z),
            Vec3::new(max.x, 0.0, min.z),
            Vec3::new(max.x, 0.0, max.z),
            Vec3::new(min.x, 0.0, max.z),
        ];
        let is_high = |c: Vec3| match high {
            RampDir::PosX => c.x == max.x,
            RampDir::NegX => c.x == min.x,
            RampDir::PosZ => c.z == max.z,
            RampDir::NegZ => c.z == min.z,
        };
        let bottom: Vec<Vec3> = corners.iter().map(|c| Vec3::new(c.x, min.y, c.z)).collect();
        let top: Vec<Vec3> = corners
            .iter()
            .map(|c| Vec3::new(c.x, if is_high(*c) { max.y } else { min.y }, c.z))
            .collect();
        let inside = (min + max) * 0.5 - Vec3::Y * (max.y - min.y) * 0.25;
        self.poly(&bottom, inside, shade * 0.7);
        self.poly(&top, inside, shade);
        for i in 0..4 {
            let j = (i + 1) % 4;
            let mut face = vec![bottom[i], bottom[j]];
            if top[j].y > bottom[j].y {
                face.push(top[j]);
            }
            if top[i].y > bottom[i].y {
                face.push(top[i]);
            }
            if face.len() >= 3 {
                self.poly(&face, inside, shade * 0.85);
            }
        }
    }

    fn build(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normal)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.color)
        .with_inserted_indices(Indices::U32(self.idx))
    }
}

pub fn spawn_map(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    def: &MapDef,
    mats: &MaterialTable,
) {
    let mut per_mat: BTreeMap<String, MeshBuilder> = BTreeMap::new();
    for b in &def.brushes {
        match b {
            BrushDef::Box { min, max, mat, .. } => per_mat
                .entry(mat.clone())
                .or_default()
                .cuboid(*min, *max, 1.0),
            BrushDef::Ramp {
                min,
                max,
                high,
                mat,
            } => per_mat
                .entry(mat.clone())
                .or_default()
                .ramp(*min, *max, *high, 1.0),
            BrushDef::Clip { .. } => {}
        }
    }
    for (name, mb) in per_mat {
        if mb.idx.is_empty() || name == "clip" {
            continue;
        }
        let color = mats
            .id_of(&name)
            .map(|id| mats.get(id).color)
            .unwrap_or([0.6, 0.6, 0.6]);
        let glass = name.contains("glass");
        let material = StandardMaterial {
            base_color: if glass {
                Color::srgba(color[0], color[1], color[2], 0.35)
            } else {
                Color::linear_rgb(color[0], color[1], color[2])
            },
            perceptual_roughness: if glass { 0.1 } else { 0.85 },
            alpha_mode: if glass {
                AlphaMode::Blend
            } else {
                AlphaMode::Opaque
            },
            ..default()
        };
        commands.spawn((
            Mesh3d(meshes.add(mb.build())),
            MeshMaterial3d(materials.add(material)),
            MapEntity,
        ));
    }
    // Bomb sites and buy zones: translucent floor markers.
    let zone = |commands: &mut Commands,
                meshes: &mut Assets<Mesh>,
                materials: &mut Assets<StandardMaterial>,
                min: Vec3,
                max: Vec3,
                c: Color| {
        let size = (max - min).abs();
        let centre = Vec3::new(
            (min.x + max.x) * 0.5,
            min.y.min(max.y) + 0.6,
            (min.z + max.z) * 0.5,
        );
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x * SCALE, 0.01, size.z * SCALE))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: c,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_translation(centre * SCALE),
            bevy::light::NotShadowCaster,
            MapEntity,
        ));
    };
    for s in &def.bomb_sites {
        zone(
            commands,
            meshes,
            materials,
            s.min,
            s.max,
            Color::srgba(1.0, 0.25, 0.15, 0.18),
        );
    }
    for s in &def.buy_zones {
        zone(
            commands,
            meshes,
            materials,
            s.min,
            s.max,
            Color::srgba(0.2, 0.6, 1.0, 0.08),
        );
    }
    // Lighting: a sun with shadows and a soft fill from the sky colour.
    let centre = (def.bounds_min + def.bounds_max) * 0.5 * SCALE;
    commands.spawn((
        DirectionalLight {
            illuminance: 9000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_translation(centre + Vec3::new(20.0, 60.0, 30.0))
            .looking_at(centre, Vec3::Y),
        MapEntity,
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 1800.0,
            shadow_maps_enabled: false,
            color: Color::srgb(0.75, 0.82, 1.0),
            ..default()
        },
        Transform::from_translation(centre + Vec3::new(-30.0, 40.0, -20.0))
            .looking_at(centre, Vec3::Y),
        MapEntity,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_point_outwards() {
        let mut mb = MeshBuilder::default();
        mb.cuboid(Vec3::ZERO, Vec3::splat(10.0), 1.0);
        mb.ramp(
            Vec3::new(20.0, 0.0, 0.0),
            Vec3::new(40.0, 10.0, 10.0),
            RampDir::PosX,
            1.0,
        );
        assert_eq!(mb.idx.len() % 3, 0);
        for t in mb.idx.chunks(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from_array(mb.pos[i as usize]));
            let n = Vec3::from_array(mb.normal[t[0] as usize]);
            // Counter-clockwise winding seen from outside = geometric normal along n.
            assert!((b - a).cross(c - a).dot(n) > 0.0, "triangle {t:?}");
        }
    }
}
