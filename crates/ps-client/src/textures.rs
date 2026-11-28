//! Surface textures of the map: matte detail maps (luminance only) multiplied
//! by the material colour of Project-s-content. Generated from CC0 sources by
//! `tools/textures.py` (list in `assets/textures/SOURCES.md`) and embedded in
//! the binary, so the content packages stay unchanged.

use bevy::asset::RenderAssetUsages;
use bevy::image::{
    CompressedImageFormats, Image, ImageAddressMode, ImageSampler, ImageSamplerDescriptor,
    ImageType,
};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use std::collections::BTreeMap;

macro_rules! texture {
    ($name:literal, $tile:expr) => {
        (
            $name,
            include_bytes!(concat!("../../../assets/textures/", $name, ".png")) as &[u8],
            $tile,
        )
    };
}

/// Material name, PNG, world size of one repetition in game units (1 u = 2.54 cm).
const TEXTURES: &[(&str, &[u8], f32)] = &[
    texture!("concrete", 128.0),
    texture!("concrete_dark", 128.0),
    texture!("brick", 48.0),
    texture!("plaster", 128.0),
    texture!("wood", 64.0),
    texture!("crate", 64.0),
    texture!("metal_thin", 64.0),
    texture!("metal_thick", 128.0),
    texture!("sand", 128.0),
    texture!("grass", 96.0),
    texture!("tile", 72.0),
];

/// Repetition size of the untextured materials (keeps the old 64-unit checker).
pub const DEFAULT_TILE: f32 = 64.0;

/// Texture handle and repetition size per material name.
#[derive(Resource, Default)]
pub struct SurfaceTextures(pub BTreeMap<&'static str, (Handle<Image>, f32)>);

pub fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let map = TEXTURES
        .iter()
        .map(|&(name, png, tile)| (name, (images.add(decode(png)), tile)))
        .collect();
    commands.insert_resource(SurfaceTextures(map));
}

/// Decodes an embedded PNG into a repeating, anisotropic, mipmapped texture.
fn decode(png: &[u8]) -> Image {
    let sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        anisotropy_clamp: 8,
        ..ImageSamplerDescriptor::linear()
    });
    let img = Image::from_buffer(
        png,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        sampler,
        RenderAssetUsages::RENDER_WORLD,
    )
    .expect("surface texture embedded in the binary");
    with_mipmaps(img)
}

/// Appends a box-filtered mip chain (averaged in linear space) so distant
/// surfaces stay calm instead of shimmering. Bevy does not build mips for PNGs.
fn with_mipmaps(mut img: Image) -> Image {
    if img.texture_descriptor.format != TextureFormat::Rgba8UnormSrgb {
        return img;
    }
    let Some(base) = img.data.take() else {
        return img;
    };
    let to_lin: Vec<f32> = (0..=255u8)
        .map(|v| {
            let c = v as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        })
        .collect();
    let to_srgb = |c: f32| {
        let s = if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0).round().clamp(0.0, 255.0) as u8
    };
    let (mut w, mut h) = (img.width() as usize, img.height() as usize);
    let mut data = base.clone();
    let mut prev = base;
    let mut levels = 1;
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                let src = [
                    (2 * x, 2 * y),
                    (2 * x + 1, 2 * y),
                    (2 * x, 2 * y + 1),
                    (2 * x + 1, 2 * y + 1),
                ]
                .map(|(sx, sy)| (sy.min(h - 1) * w + sx.min(w - 1)) * 4);
                for c in 0..4 {
                    let o = (y * nw + x) * 4 + c;
                    next[o] = if c < 3 {
                        to_srgb(
                            src.iter()
                                .map(|&i| to_lin[prev[i + c] as usize])
                                .sum::<f32>()
                                / 4.0,
                        )
                    } else {
                        (src.iter().map(|&i| prev[i + c] as u32).sum::<u32>() / 4) as u8
                    };
                }
            }
        }
        data.extend_from_slice(&next);
        prev = next;
        (w, h) = (nw, nh);
        levels += 1;
    }
    img.texture_descriptor.mip_level_count = levels;
    img.data = Some(data);
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_texture_decodes_with_a_full_mip_chain() {
        for &(name, png, tile) in TEXTURES {
            let img = decode(png);
            assert_eq!(
                img.texture_descriptor.format,
                TextureFormat::Rgba8UnormSrgb,
                "{name}"
            );
            assert_eq!((img.width(), img.height()), (512, 512), "{name}");
            assert_eq!(img.texture_descriptor.mip_level_count, 10, "{name}");
            let bytes: usize = (0..10).map(|l| (512usize >> l).pow(2) * 4).sum();
            assert_eq!(img.data.as_ref().map(Vec::len), Some(bytes), "{name}");
            assert!(tile > 0.0, "{name}");
        }
    }

    #[test]
    fn detail_maps_keep_the_material_shade() {
        // Mean of the last mip (1×1), linear: ~0.9, as tools/textures.py targets.
        for &(name, png, _) in TEXTURES {
            let img = decode(png);
            let data = img.data.unwrap();
            let v = data[data.len() - 4] as f32 / 255.0;
            let lin = ((v + 0.055) / 1.055).powf(2.4);
            assert!((0.8..=0.97).contains(&lin), "{name}: {lin}");
        }
    }

    #[test]
    fn textured_materials_exist_in_the_content() {
        // Content checkout: inside the repository (CI) or next to it (local).
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        let toml = ["Project-s-content", "../Project-s-content"]
            .iter()
            .find_map(|d| {
                std::fs::read_to_string(format!("{root}/{d}/configs/materials.toml")).ok()
            });
        let Some(toml) = toml else { return };
        for &(name, _, _) in TEXTURES {
            assert!(
                toml.contains(&format!("name = \"{name}\"")),
                "unknown material {name}"
            );
        }
    }
}
