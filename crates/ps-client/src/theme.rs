//! Visual identity of the interface (docs/brand.md): colours, fonts, the
//! monogram and the few widgets drawn by hand (faceted buttons, tick ruler).
//!
//! Brand rules that shape this file: flat matte fills, 45° facets, a 128 Hz
//! tick ruler as the only ornament; no glow, shadow or gradient.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, Response,
    Sense, Shadow, Shape, Stroke, TextStyle, Ui, Vec2,
};
use std::sync::Arc;

pub const GRAPHITE: Color32 = Color32::from_rgb(0x12, 0x14, 0x17);
pub const CRAIE: Color32 = Color32::from_rgb(0xEC, 0xE8, 0xE1);
pub const AMETHYSTE: Color32 = Color32::from_rgb(0x9B, 0x6B, 0xFF);
pub const AMETHYSTE_CLAIR: Color32 = Color32::from_rgb(0xB7, 0x96, 0xFF);
pub const AMETHYSTE_SOMBRE: Color32 = Color32::from_rgb(0x74, 0x46, 0xE0);
pub const MINIUM: Color32 = Color32::from_rgb(0xF0, 0x61, 0x2A);
pub const ACIER: Color32 = Color32::from_rgb(0x8A, 0x90, 0x99);
/// Surfaces derived from Graphite (panels, fields, lines).
pub const ABYSSE: Color32 = Color32::from_rgb(0x0C, 0x0D, 0x10);
pub const SURFACE: Color32 = Color32::from_rgb(0x1A, 0x1D, 0x21);
pub const SURFACE_HAUTE: Color32 = Color32::from_rgb(0x24, 0x28, 0x2E);
pub const LIGNE: Color32 = Color32::from_rgb(0x2E, 0x33, 0x3A);

/// Font family of the titles: Archivo Expanded ExtraBold, always in capitals.
pub fn title_family() -> FontFamily {
    FontFamily::Name("titre".into())
}

pub fn title(size: f32) -> FontId {
    FontId::new(size, title_family())
}

/// Monogram texture, inserted by [`apply`] once the theme is installed: its
/// presence also means the title font can be used.
#[derive(Resource)]
pub struct Logo(pub egui::TextureHandle);

/// Installs the theme over the first two egui frames: fonts first, then (once
/// egui has loaded them, a frame later) the styles that use the title font.
pub fn apply(mut contexts: EguiContexts, mut commands: Commands, mut step: Local<u8>) -> Result {
    if *step >= 2 {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    *step += 1;
    if *step == 1 {
        ctx.set_fonts(fonts());
        return Ok(());
    }
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, visuals());
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        s.text_styles = [
            (TextStyle::Heading, title(20.0)),
            (TextStyle::Body, FontId::proportional(15.0)),
            (TextStyle::Button, FontId::proportional(15.0)),
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
        s.spacing.item_spacing = Vec2::new(8.0, 7.0);
        s.spacing.window_margin = egui::Margin::same(16);
    });
    commands.insert_resource(Logo(ctx.load_texture(
        "logo",
        logo_image(),
        egui::TextureOptions::LINEAR,
    )));
    Ok(())
}

fn fonts() -> FontDefinitions {
    let mut f = FontDefinitions::default();
    for (name, bytes) in [
        (
            "archivo",
            include_bytes!("../../../assets/fonts/Archivo-Regular.ttf") as &[u8],
        ),
        (
            "archivo-expanded",
            &include_bytes!("../../../assets/fonts/ArchivoExpanded-ExtraBold.ttf")[..],
        ),
        (
            "jetbrains-mono",
            &include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf")[..],
        ),
    ] {
        f.font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    // The egui fonts stay as fallbacks (symbols, emoji).
    let fallbacks = f
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for (family, first) in [
        (FontFamily::Proportional, "archivo"),
        (FontFamily::Monospace, "jetbrains-mono"),
    ] {
        f.families
            .entry(family)
            .or_default()
            .insert(0, first.into());
    }
    let mut titles = vec!["archivo-expanded".to_string()];
    titles.extend(fallbacks);
    f.families.insert(title_family(), titles);
    f
}

fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    let w = |bg: Color32, weak: Color32, line: Color32, fg: Color32| egui::style::WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: weak,
        bg_stroke: Stroke::new(1.0, line),
        corner_radius: CornerRadius::ZERO,
        fg_stroke: Stroke::new(1.0, fg),
        expansion: 0.0,
    };
    v.widgets.noninteractive = w(GRAPHITE, SURFACE, LIGNE, CRAIE);
    v.widgets.inactive = w(SURFACE_HAUTE, SURFACE, LIGNE, CRAIE);
    v.widgets.hovered = w(SURFACE_HAUTE, SURFACE_HAUTE, AMETHYSTE_CLAIR, CRAIE);
    v.widgets.active = w(AMETHYSTE_SOMBRE, AMETHYSTE_SOMBRE, AMETHYSTE, CRAIE);
    v.widgets.open = w(SURFACE_HAUTE, SURFACE_HAUTE, LIGNE, CRAIE);
    v.selection.bg_fill = AMETHYSTE_SOMBRE;
    v.selection.stroke = Stroke::new(1.0, CRAIE);
    v.hyperlink_color = AMETHYSTE_CLAIR;
    v.weak_text_color = Some(ACIER);
    v.faint_bg_color = SURFACE;
    v.extreme_bg_color = ABYSSE;
    v.code_bg_color = SURFACE;
    v.error_fg_color = MINIUM;
    v.window_fill = GRAPHITE;
    v.panel_fill = GRAPHITE;
    v.window_stroke = Stroke::new(1.0, LIGNE);
    v.window_corner_radius = CornerRadius::ZERO;
    v.menu_corner_radius = CornerRadius::ZERO;
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.slider_trailing_fill = true;
    v
}

/// `assets/brand/logo.png`: `logo.svg` rasterised at 432×512 (see docs/brand.md).
fn logo_image() -> egui::ColorImage {
    let img = bevy::image::Image::from_buffer(
        include_bytes!("../../../assets/brand/logo.png"),
        bevy::image::ImageType::Extension("png"),
        bevy::image::CompressedImageFormats::NONE,
        false,
        bevy::image::ImageSampler::Default,
        bevy::asset::RenderAssetUsages::MAIN_WORLD,
    )
    .expect("logo.png embedded in the binary");
    let size = [img.width() as usize, img.height() as usize];
    egui::ColorImage::from_rgba_unmultiplied(size, img.data.as_deref().unwrap_or_default())
}

/// Corners of a rectangle with its top-left and bottom-right corners cut at 45°.
pub fn faceted(r: Rect, cut: f32) -> Vec<Pos2> {
    vec![
        Pos2::new(r.left() + cut, r.top()),
        r.right_top(),
        Pos2::new(r.right(), r.bottom() - cut),
        Pos2::new(r.right() - cut, r.bottom()),
        r.left_bottom(),
        Pos2::new(r.left(), r.top() + cut),
    ]
}

/// Main call to action: Améthyste fill, faceted corners, title font.
pub fn primary_button(ui: &mut Ui, label: &str) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_uppercase(), title(15.0), GRAPHITE);
    let size = Vec2::new(galley.size().x + 56.0, 44.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if resp.is_pointer_button_down_on() {
        AMETHYSTE_SOMBRE
    } else if resp.hovered() {
        AMETHYSTE_CLAIR
    } else {
        AMETHYSTE
    };
    let p = ui.painter();
    p.add(Shape::convex_polygon(
        faceted(rect, 10.0),
        fill,
        Stroke::NONE,
    ));
    p.galley(rect.center() - galley.size() / 2.0, galley, GRAPHITE);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Navigation entry of the main menu: Améthyste bar on the selected one.
pub fn nav_item(ui: &mut Ui, label: &str, selected: bool) -> Response {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::click());
    let p = ui.painter();
    if selected || resp.hovered() {
        p.rect_filled(rect, 0.0, SURFACE);
    }
    if selected {
        p.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(4.0, rect.height())),
            0.0,
            AMETHYSTE,
        );
    }
    let col = if selected || resp.hovered() {
        CRAIE
    } else {
        ACIER
    };
    p.text(
        Pos2::new(rect.left() + 20.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label.to_uppercase(),
        title(13.0),
        col,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The brand ornament: one 128 Hz second, a tick per 1/32 s, a major tick
/// every 1/4 s, the first tick in Améthyste.
pub fn tick_ruler(ui: &mut Ui, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 14.0), Sense::hover());
    let p = ui.painter();
    p.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, LIGNE),
    );
    let n = 32;
    for i in 0..=n {
        let x = rect.left() + rect.width() * i as f32 / n as f32;
        let h = if i % 8 == 0 {
            rect.height()
        } else {
            rect.height() * 0.45
        };
        let col = if i == 0 { AMETHYSTE } else { ACIER };
        p.line_segment(
            [Pos2::new(x, rect.bottom()), Pos2::new(x, rect.bottom() - h)],
            Stroke::new(1.0, col),
        );
    }
}

/// Section title: capitals in the title font, Craie.
pub fn section_title(ui: &mut Ui, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .font(title(22.0))
            .color(CRAIE),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_decodes_with_transparent_background() {
        let img = logo_image();
        assert_eq!(img.size, [432, 512]);
        assert_eq!(img.pixels[0].a(), 0, "top-left corner must be transparent");
    }

    #[test]
    fn facets_are_cut_at_45_degrees() {
        let pts = faceted(
            Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 40.0)),
            10.0,
        );
        let d = pts[0] - pts[5];
        assert_eq!(d.x.abs(), d.y.abs());
    }
}
