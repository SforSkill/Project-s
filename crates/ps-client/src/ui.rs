//! egui interface: main menu, HUD, scoreboard, buy menu, chat, settings,
//! network graph and Shot Inspector.

use crate::character::settings::{CharacterSettings, ColorMode};
use crate::character::style::{PaletteId, Quality};
use crate::game::{Hud, Screen, active_weapon};
use crate::net::{Clock, GameDataRes, LocalGame, Net};
use crate::settings::{FOV, Settings, vertical_fov};
use crate::theme;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use egui::{Align2, Color32, FontId, Pos2, RichText, Stroke, Vec2};
use ps_rules::Phase;
use ps_sim_core::Team;
use ps_sim_net::proto::ClientMsg;
use ps_weapons::WeaponKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuTab {
    #[default]
    Play,
    Join,
    Settings,
    About,
}

#[derive(Resource)]
pub struct MenuState {
    pub tab: MenuTab,
    pub game: LocalGame,
    pub address: String,
    pub dpi: f32,
}

impl Default for MenuState {
    fn default() -> Self {
        Self {
            tab: MenuTab::Play,
            game: LocalGame {
                map: "ps_citadel".into(),
                profile: "competitive".into(),
                bots: 0,
                fill_bots: true,
                difficulty: 0.5,
            },
            address: String::new(),
            dpi: 800.0,
        }
    }
}

/// HUD colours of the teams: the same palettes as the character bodies
/// (side or ally/enemy colours, as chosen in the settings).
struct TeamColors<'a> {
    cs: &'a CharacterSettings,
    viewer: Team,
    free_for_all: bool,
}

impl TeamColors<'_> {
    fn get(&self, t: Team) -> Color32 {
        if t == Team::Spectator {
            return Color32::from_gray(200);
        }
        let [r, g, b] = self
            .cs
            .palette_in(t, self.viewer, self.free_for_all)
            .ui_rgb();
        Color32::from_rgb(r, g, b)
    }
}

fn team_name(t: Team) -> &'static str {
    match t {
        Team::Attack => "Attaquants",
        Team::Defense => "Défenseurs",
        Team::Spectator => "Spectateurs",
    }
}

fn profile_label(p: &str) -> String {
    match p {
        "competitive" => "Compétitif 5v5 (MR15, bombe)".into(),
        "competitive_mr12" => "Compétitif MR12".into(),
        "competitive_mr16" => "Compétitif MR16".into(),
        "duel" => "Duel 1v1".into(),
        "deathmatch" => "Deathmatch".into(),
        "training" => "Entraînement".into(),
        o => o.into(),
    }
}

fn mmss(t: f32) -> String {
    if !t.is_finite() || t < 0.0 {
        return "--:--".into();
    }
    let s = t.ceil() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

fn palette_combo(ui: &mut egui::Ui, id: &str, v: &mut PaletteId) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(v.label())
        .show_ui(ui, |ui| {
            for p in PaletteId::ALL {
                let [r, g, b] = p.ui_rgb();
                ui.selectable_value(
                    v,
                    p,
                    RichText::new(format!("■ {}", p.label())).color(Color32::from_rgb(r, g, b)),
                );
            }
        });
}

/// Characters: quality and colours (accessibility). Every quality level
/// keeps the same silhouette and contour; colours never change hitboxes.
pub(crate) fn character_settings_ui(ui: &mut egui::Ui, c: &mut CharacterSettings) {
    ui.label("Qualité des personnages");
    ui.horizontal(|ui| {
        for q in Quality::ALL {
            ui.selectable_value(&mut c.quality, q, q.label());
        }
    });
    ui.end_row();
    ui.label("Couleurs des joueurs");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut c.mode, ColorMode::Sides, "Par camp");
        ui.selectable_value(&mut c.mode, ColorMode::Relative, "Alliés / ennemis");
    });
    ui.end_row();
    match c.mode {
        ColorMode::Sides => {
            ui.label("Défenseurs / attaquants");
            ui.horizontal(|ui| {
                palette_combo(ui, "pal_def", &mut c.defense);
                palette_combo(ui, "pal_att", &mut c.attack);
            });
        }
        ColorMode::Relative => {
            ui.label("Alliés / ennemis");
            ui.horizontal(|ui| {
                palette_combo(ui, "pal_ally", &mut c.allies);
                palette_combo(ui, "pal_enemy", &mut c.enemies);
            });
        }
    }
    ui.end_row();
}

fn settings_ui(ui: &mut egui::Ui, s: &mut Settings, dpi: &mut f32) {
    egui::Grid::new("settings_grid")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label("Pseudo");
            ui.text_edit_singleline(&mut s.name);
            ui.end_row();
            ui.label("Sensibilité");
            ui.add(
                egui::Slider::new(&mut s.sensitivity, 0.05..=10.0)
                    .logarithmic(true)
                    .max_decimals(3),
            );
            ui.end_row();
            ui.label("m_yaw / m_pitch");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut s.m_yaw)
                        .speed(0.0005)
                        .range(0.001..=0.1)
                        .max_decimals(4),
                );
                ui.add(
                    egui::DragValue::new(&mut s.m_pitch)
                        .speed(0.0005)
                        .range(0.001..=0.1)
                        .max_decimals(4),
                );
            });
            ui.end_row();
            ui.label("DPI de la souris (calcul)");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(dpi).range(100.0..=32000.0).speed(50.0));
                ui.label(format!("= {:.1} cm / 360°", s.cm_per_360(*dpi)));
            });
            ui.end_row();
            ui.label("Sensibilité en zoom (ratio)");
            ui.add(egui::Slider::new(&mut s.zoom_sensitivity, 0.2..=2.0));
            ui.end_row();
            ui.label("Inverser l'axe Y");
            ui.checkbox(&mut s.invert_y, "");
            ui.end_row();
            ui.label("FOV de l'arme");
            ui.add(egui::Slider::new(&mut s.viewmodel_fov, 54.0..=90.0).suffix("°"));
            ui.end_row();
            ui.label("Position de l'arme (x, y, z)");
            ui.horizontal(|ui| {
                for v in &mut s.viewmodel_offset {
                    ui.add(egui::DragValue::new(v).speed(0.1).range(-10.0..=10.0));
                }
            });
            ui.end_row();
            ui.label("Afficher l'arme");
            ui.checkbox(&mut s.show_viewmodel, "");
            ui.end_row();
            character_settings_ui(ui, &mut s.characters);
            ui.label("Volume");
            ui.add(egui::Slider::new(&mut s.volume, 0.0..=1.0));
            ui.end_row();
            ui.label("Réticule : taille / écart / épaisseur");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut s.crosshair.size)
                        .range(0.0..=30.0)
                        .speed(0.1),
                );
                ui.add(
                    egui::DragValue::new(&mut s.crosshair.gap)
                        .range(-5.0..=30.0)
                        .speed(0.1),
                );
                ui.add(
                    egui::DragValue::new(&mut s.crosshair.thickness)
                        .range(0.5..=8.0)
                        .speed(0.1),
                );
            });
            ui.end_row();
            ui.label("Réticule : point, contour, couleur");
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.crosshair.dot, "point");
                ui.checkbox(&mut s.crosshair.outline, "contour");
                ui.color_edit_button_srgb(&mut s.crosshair.color);
            });
            ui.end_row();
            ui.label("Limite d'images/s (0 = aucune)");
            ui.add(
                egui::DragValue::new(&mut s.fps_limit)
                    .range(0..=1000)
                    .speed(5.0),
            );
            ui.end_row();
            ui.label("Synchronisation verticale");
            ui.checkbox(&mut s.vsync, "");
            ui.end_row();
            ui.label("Plein écran (F11 ou Alt+Entrée)");
            ui.checkbox(&mut s.fullscreen, "");
            ui.end_row();
            ui.label("Graphe réseau");
            ui.checkbox(&mut s.net_graph, "");
            ui.end_row();
            ui.label("Variante réseau (labo)");
            ui.horizontal(|ui| {
                for v in ["A", "B", "C", "D"] {
                    ui.selectable_value(&mut s.net_variant, v.to_string(), v);
                }
            });
            ui.end_row();
            ui.label("Extrapolation adverse max");
            ui.horizontal(|ui| {
                for ms in [0.0, 15.0, 30.0] {
                    ui.selectable_value(&mut s.extrapolation_ms, ms, format!("{ms} ms"));
                }
            });
            ui.end_row();
            ui.label("Échelle de l'interface");
            ui.add(egui::Slider::new(&mut s.ui_scale, 0.75..=2.0));
            ui.end_row();
        });
    ui.label(RichText::new("Les touches se modifient dans settings.toml (dossier ProjetS de la configuration utilisateur). Variante et extrapolation s'appliquent à la prochaine connexion.").small().weak());
}

/// Home screen: full-screen Graphite, a rail on the left (monogram, title, tick
/// ruler, navigation, version) and the selected tab on the right. Returns true
/// when "Quitter" is clicked.
fn main_menu(
    ctx: &egui::Context,
    menu: &mut MenuState,
    logo: &theme::Logo,
    content: impl FnOnce(&mut egui::Ui, &mut MenuState),
) -> bool {
    const RAIL: f32 = 300.0;
    let mut quit = false;
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("main_menu"))
        .order(egui::Order::Background)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let p = ui.painter();
            p.rect_filled(screen, 0.0, theme::GRAPHITE);
            let rail = egui::Rect::from_min_size(screen.min, Vec2::new(RAIL, screen.height()));
            p.rect_filled(rail, 0.0, theme::ABYSSE);
            p.line_segment(
                [rail.right_top(), rail.right_bottom()],
                Stroke::new(1.0, theme::LIGNE),
            );

            let inner = rail.shrink2(Vec2::new(28.0, 32.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                ui.add(egui::Image::new(&logo.0).fit_to_exact_size(Vec2::new(68.0, 80.6)));
                ui.add_space(14.0);
                ui.label(
                    RichText::new("PROJET S")
                        .font(theme::title(30.0))
                        .color(theme::CRAIE),
                );
                ui.label(
                    RichText::new("FPS TACTIQUE COMPÉTITIF")
                        .font(theme::title(10.0))
                        .color(theme::ACIER),
                );
                ui.add_space(14.0);
                theme::tick_ruler(ui, inner.width());
                ui.add_space(26.0);
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (tab, label) in [
                        (MenuTab::Play, "Jouer"),
                        (MenuTab::Join, "Rejoindre"),
                        (MenuTab::Settings, "Paramètres"),
                        (MenuTab::About, "À propos"),
                    ] {
                        if theme::nav_item(ui, label, menu.tab == tab).clicked() {
                            menu.tab = tab;
                        }
                    }
                });
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.label(
                        RichText::new(format!("v{} · alpha · 128 Hz", env!("CARGO_PKG_VERSION")))
                            .monospace()
                            .color(theme::ACIER),
                    );
                    ui.add_space(6.0);
                    if theme::nav_item(ui, "Quitter", false).clicked() {
                        quit = true;
                    }
                });
            });

            let area = egui::Rect::from_min_max(egui::pos2(rail.right(), screen.top()), screen.max);
            let width = (area.width() - 112.0).clamp(320.0, 680.0);
            let body = egui::Rect::from_min_size(
                egui::pos2(area.left() + 56.0, area.top() + 64.0),
                Vec2::new(width, area.height() - 104.0),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
                ui.set_width(width);
                content(ui, menu);
            });
        });
    quit
}

/// Main menu, connection screen, escape menu.
pub fn menu(
    mut contexts: EguiContexts,
    mut screen: ResMut<Screen>,
    mut menu: ResMut<MenuState>,
    mut net: ResMut<Net>,
    mut settings: ResMut<Settings>,
    data: Res<GameDataRes>,
    clock: Res<Clock>,
    mut hud: ResMut<Hud>,
    mut exit: MessageWriter<AppExit>,
    logo: Option<Res<theme::Logo>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    ctx.set_zoom_factor(settings.ui_scale.clamp(0.5, 3.0));
    let now = clock.now();
    if *screen == Screen::Game && net.session.is_none() {
        *screen = Screen::Menu;
        *hud = Hud::default();
    }
    match *screen {
        Screen::Menu => {
            // The theme (title font, logo) is ready from the second egui frame.
            let Some(logo) = logo.as_deref() else {
                return Ok(());
            };
            let quit = main_menu(ctx, &mut menu, logo, |ui, menu| {
                if let Some(e) = &net.error {
                    ui.colored_label(theme::MINIUM, e);
                    ui.add_space(6.0);
                }
                match menu.tab {
                    MenuTab::Play => {
                        theme::section_title(ui, "Jouer en local");
                        ui.add_space(10.0);
                        let maps: Vec<(String, String, Vec<String>)> = data
                            .0
                            .maps
                            .iter()
                            .map(|(k, m)| (k.clone(), m.display_name.clone(), m.modes.clone()))
                            .collect();
                        egui::Grid::new("play_grid")
                            .num_columns(2)
                            .spacing([16.0, 8.0])
                            .show(ui, |ui| {
                                ui.label("Carte");
                                let cur = maps
                                    .iter()
                                    .find(|m| m.0 == menu.game.map)
                                    .map(|m| m.1.clone())
                                    .unwrap_or_default();
                                egui::ComboBox::from_id_salt("map")
                                    .selected_text(cur)
                                    .show_ui(ui, |ui| {
                                        for (k, n, _) in &maps {
                                            ui.selectable_value(&mut menu.game.map, k.clone(), n);
                                        }
                                    });
                                ui.end_row();
                                ui.label("Mode");
                                let modes: Vec<String> = maps
                                    .iter()
                                    .find(|m| m.0 == menu.game.map)
                                    .map(|m| m.2.clone())
                                    .unwrap_or_default();
                                let profiles: Vec<String> = data
                                    .0
                                    .rules
                                    .profiles
                                    .iter()
                                    .filter(|(_, p)| {
                                        modes.is_empty()
                                            || modes.iter().any(|m| {
                                                m == &format!("{:?}", p.mode).to_lowercase()
                                            })
                                    })
                                    .map(|(k, _)| k.clone())
                                    .collect();
                                if !profiles.contains(&menu.game.profile)
                                    && let Some(p) = profiles.first()
                                {
                                    menu.game.profile = p.clone();
                                }
                                egui::ComboBox::from_id_salt("profile")
                                    .selected_text(profile_label(&menu.game.profile))
                                    .show_ui(ui, |ui| {
                                        for p in &profiles {
                                            ui.selectable_value(
                                                &mut menu.game.profile,
                                                p.clone(),
                                                profile_label(p),
                                            );
                                        }
                                    });
                                ui.end_row();
                                ui.label("Compléter avec des bots");
                                ui.checkbox(&mut menu.game.fill_bots, "");
                                ui.end_row();
                                ui.label("Bots supplémentaires");
                                ui.add(egui::Slider::new(&mut menu.game.bots, 0..=15));
                                ui.end_row();
                                ui.label("Difficulté des bots");
                                ui.add(egui::Slider::new(&mut menu.game.difficulty, 0.0..=1.0));
                                ui.end_row();
                            });
                        ui.add_space(8.0);
                        if theme::primary_button(ui, "Lancer la partie").clicked() {
                            let g = menu.game.clone();
                            net.host(&g, &settings, data.0.clone(), now);
                            if net.session.is_some() {
                                *screen = Screen::Game;
                            }
                        }
                        ui.label(RichText::new("Un serveur local (128 Hz) démarre dans le jeu ; les autres joueurs du réseau local ne peuvent pas le rejoindre (lié à 127.0.0.1). Pour héberger, lancez projet-s-server.").small().weak());
                    }
                    MenuTab::Join => {
                        theme::section_title(ui, "Rejoindre un serveur");
                        ui.add_space(10.0);
                        if menu.address.is_empty() {
                            menu.address = settings.last_server.clone();
                        }
                        ui.horizontal(|ui| {
                            ui.label("Adresse (hôte:port)");
                            ui.text_edit_singleline(&mut menu.address);
                        });
                        if theme::primary_button(ui, "Se connecter").clicked() {
                            let a = menu.address.trim().to_string();
                            settings.last_server = a.clone();
                            settings.save();
                            net.connect(&a, &settings, data.0.clone(), now);
                            if net.session.is_some() {
                                *screen = Screen::Game;
                            }
                        }
                        ui.label(
                            RichText::new("Port par défaut du serveur dédié : 27015/UDP.")
                                .small()
                                .weak(),
                        );
                    }
                    MenuTab::Settings => {
                        theme::section_title(ui, "Paramètres");
                        ui.add_space(10.0);
                        let mut dpi = menu.dpi;
                        egui::ScrollArea::vertical()
                            .max_height(ui.available_height() - 60.0)
                            .show(ui, |ui| settings_ui(ui, &mut settings, &mut dpi));
                        menu.dpi = dpi;
                        if ui.button("Enregistrer").clicked() {
                            settings.save();
                        }
                    }
                    MenuTab::About => {
                        theme::section_title(ui, "À propos");
                        ui.add_space(10.0);
                        ui.label("Projet S : FPS tactique compétitif, alpha jouable.");
                        ui.label("Déplacements, armes, réseau et règles : code original. Aucun contenu de Valve ou de Riot.");
                        ui.label(RichText::new("Commandes : ZQSD/WASD selon les touches physiques, Espace saut, Ctrl accroupi, Maj marcher, 1-5 armes, Q dernière arme, R recharger, E utiliser, G lâcher, B achat, Tab scores, Y/U chat, F3 inspecteur de tirs, Échap menu.").small());
                    }
                }
            });
            if quit {
                settings.save();
                exit.write(AppExit::Success);
            }
        }
        Screen::Game => {
            let Some(s) = &mut net.session else {
                return Ok(());
            };
            if !s.client.in_game() {
                let waited = now - s.started;
                let mut cancel = false;
                egui::Window::new("Connexion")
                    .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label(format!("Connexion à {} … ({waited:.0} s)", s.server));
                        ui.spinner();
                        cancel = ui.button("Annuler").clicked();
                    });
                if cancel {
                    net.disconnect(now);
                }
                return Ok(());
            }
            if hud.escape_menu {
                let mut action = 0;
                let host = s.client.welcome.as_ref().is_some_and(|w| w.host);
                egui::Window::new("Menu").anchor(Align2::CENTER_CENTER, [0.0, 0.0]).collapsible(false).resizable(false).show(ctx, |ui| {
                    if ui.button("Reprendre").clicked() {
                        action = 1;
                    }
                    if ui.button("Paramètres").clicked() {
                        action = 2;
                    }
                    ui.separator();
                    ui.label("Équipe");
                    ui.horizontal(|ui| {
                        if ui.button("Attaquants").clicked() {
                            s.client.send_reliable(ClientMsg::Team(Team::Attack));
                            action = 1;
                        }
                        if ui.button("Défenseurs").clicked() {
                            s.client.send_reliable(ClientMsg::Team(Team::Defense));
                            action = 1;
                        }
                        if ui.button("Spectateur").clicked() {
                            s.client.send_reliable(ClientMsg::Team(Team::Spectator));
                            action = 1;
                        }
                    });
                    if host {
                        ui.separator();
                        ui.label("Commandes de l'hôte");
                        ui.horizontal(|ui| {
                            for (label, cmd) in [("+1 bot", "bots +1"), ("Retirer les bots", "kickbots"), ("Démarrer", "start"), ("Recommencer", "restart")] {
                                if ui.button(label).clicked() {
                                    s.client.send_reliable(ClientMsg::Host(cmd.into()));
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut hud.host_cmd);
                            if ui.button("Envoyer").clicked() && !hud.host_cmd.trim().is_empty() {
                                s.client.send_reliable(ClientMsg::Host(hud.host_cmd.trim().to_string()));
                                hud.host_cmd.clear();
                            }
                        });
                        ui.label(RichText::new("bots N [attack|defense], difficulty 0..1, map NOM [profil], mode PROFIL, kick ID, say TEXTE, status").small().weak());
                    }
                    ui.separator();
                    if ui.button("Quitter la partie").clicked() {
                        action = 3;
                    }
                    if ui.button("Quitter le jeu").clicked() {
                        action = 4;
                    }
                });
                match action {
                    1 => hud.escape_menu = false,
                    2 => {
                        hud.settings_open = true;
                        hud.escape_menu = false;
                    }
                    3 => {
                        net.disconnect(now);
                        *screen = Screen::Menu;
                        *hud = Hud::default();
                    }
                    4 => {
                        net.disconnect(now);
                        settings.save();
                        exit.write(AppExit::Success);
                    }
                    _ => {}
                }
            }
            if hud.settings_open {
                let mut open = true;
                let mut dpi = menu.dpi;
                egui::Window::new("Paramètres")
                    .open(&mut open)
                    .collapsible(false)
                    .default_width(520.0)
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(520.0)
                            .show(ui, |ui| settings_ui(ui, &mut settings, &mut dpi));
                        if ui.button("Enregistrer").clicked() {
                            settings.save();
                        }
                    });
                menu.dpi = dpi;
                if !open {
                    settings.save();
                    hud.settings_open = false;
                }
            }
        }
    }
    Ok(())
}

/// In-game HUD.
pub fn hud(
    mut contexts: EguiContexts,
    screen: Res<Screen>,
    mut net: ResMut<Net>,
    settings: Res<Settings>,
    clock: Res<Clock>,
    mut hud: ResMut<Hud>,
    time: Res<Time>,
) -> Result {
    if *screen != Screen::Game {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let now = clock.now();
    let dt = time.delta_secs();
    hud.frame_ms.push(dt * 1000.0);
    if hud.frame_ms.len() > 240 {
        hud.frame_ms.remove(0);
    }
    hud.fps = if dt > 0.0 {
        hud.fps * 0.95 + 0.05 / dt
    } else {
        hud.fps
    };
    let Some(s) = &mut net.session else {
        return Ok(());
    };
    if !s.client.in_game() {
        return Ok(());
    }
    let c = &s.client;
    let Some(snap) = c.latest().cloned() else {
        return Ok(());
    };
    let data = c.data.clone();
    let screen_rect = ctx.content_rect();
    let centre = screen_rect.center();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("hud"),
    ));
    let white = Color32::from_rgb(240, 240, 240);
    let shadow = Color32::from_black_alpha(170);
    let big = FontId::proportional(30.0);
    let mid = FontId::proportional(18.0);
    let small = FontId::proportional(14.0);
    let text = |pos: Pos2, anchor: Align2, t: &str, f: &FontId, col: Color32| {
        painter.text(pos + Vec2::new(1.5, 1.5), anchor, t, f.clone(), shadow);
        painter.text(pos, anchor, t, f.clone(), col);
    };
    let me = snap.me.clone();
    let rules = c.welcome.as_ref().map(|w| w.rules.clone());
    let tc = TeamColors {
        cs: &settings.characters,
        viewer: me.as_ref().map_or(Team::Spectator, |m| m.team),
        free_for_all: rules
            .as_ref()
            .is_some_and(|r| r.mode == ps_rules::Mode::Deathmatch),
    };
    // Flash: fully white, then fading.
    if now < hud.flash_until {
        let a = if now < hud.flash_full_until {
            1.0
        } else {
            ((hud.flash_until - now) / (hud.flash_until - hud.flash_full_until).max(0.01)) as f32
        };
        painter.rect_filled(
            screen_rect,
            0.0,
            Color32::from_white_alpha((255.0 * a * hud.flash_alpha.clamp(0.0, 1.0)) as u8),
        );
    }
    // Scope overlay or crosshair.
    let weapon = me
        .as_ref()
        .and_then(active_weapon)
        .and_then(|w| data.catalog.try_get(w));
    let zoomed = me.as_ref().is_some_and(|m| m.alive && m.hand.zoom > 0)
        && weapon.is_some_and(|w| w.zoom_fov(1).is_some());
    if zoomed {
        let r = screen_rect.height() * 0.48;
        let black = Color32::BLACK;
        painter.rect_filled(
            egui::Rect::from_min_max(screen_rect.min, Pos2::new(centre.x - r, screen_rect.max.y)),
            0.0,
            black,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(Pos2::new(centre.x + r, screen_rect.min.y), screen_rect.max),
            0.0,
            black,
        );
        painter.circle_stroke(centre, r + 40.0, Stroke::new(80.0, black));
        painter.line_segment(
            [
                Pos2::new(centre.x - r, centre.y),
                Pos2::new(centre.x + r, centre.y),
            ],
            Stroke::new(1.0, black),
        );
        painter.line_segment(
            [
                Pos2::new(centre.x, centre.y - r),
                Pos2::new(centre.x, centre.y + r),
            ],
            Stroke::new(1.0, black),
        );
    } else if me.as_ref().is_some_and(|m| m.alive) {
        let ch = &settings.crosshair;
        let col = Color32::from_rgb(ch.color[0], ch.color[1], ch.color[2]);
        let t = ch.thickness;
        let lines = [
            (Vec2::new(1.0, 0.0)),
            Vec2::new(-1.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(0.0, -1.0),
        ];
        for d in lines {
            let a = centre + d * (ch.gap + 0.0);
            let b = centre + d * (ch.gap + ch.size);
            if ch.outline {
                painter.line_segment(
                    [a - d * 1.0, b + d * 1.0],
                    Stroke::new(t + 2.0, Color32::BLACK),
                );
            }
            painter.line_segment([a, b], Stroke::new(t, col));
        }
        if ch.dot {
            painter.circle_filled(centre, t, col);
        }
        // Where the next bullet goes with the current recoil (the camera shows half of it).
        if let (Some(m), Some(w)) = (&me, weapon) {
            let (rp, ry) = m.hand.recoil_now(w, c.server_time(now));
            if rp.abs() + ry.abs() > 0.05 {
                let f = screen_rect.height() * 0.5 / (vertical_fov(FOV) * 0.5).tan();
                let p = centre
                    + Vec2::new(
                        -(ry * 0.5).to_radians().tan() * f,
                        -(rp * 0.5).to_radians().tan() * f,
                    );
                painter.circle_filled(p, 2.0, Color32::from_rgba_unmultiplied(255, 80, 80, 200));
            }
        }
    }
    // Hit marker.
    let since_hit = now - hud.hit_marker;
    if since_hit < 0.18 {
        let col = if hud.hit_head {
            Color32::from_rgb(255, 80, 60)
        } else {
            white
        };
        for d in [
            Vec2::new(1.0, 1.0),
            Vec2::new(-1.0, 1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(-1.0, -1.0),
        ] {
            painter.line_segment([centre + d * 6.0, centre + d * 13.0], Stroke::new(2.0, col));
        }
    }
    // Damage direction indicators.
    for (from, t) in &hud.damage_from {
        let to = *from - hud.eye;
        let fwd = hud.view.forward_flat();
        let right = hud.view.right_flat();
        let ang = to.dot(right).atan2(to.dot(fwd));
        let a = (1.0 - (now - t) / 1.5).clamp(0.0, 1.0) as f32;
        let dir = Vec2::new(ang.sin(), -ang.cos());
        let p = centre + dir * 110.0;
        let n = Vec2::new(-dir.y, dir.x);
        painter.line_segment(
            [p - n * 22.0, p + n * 22.0],
            Stroke::new(
                5.0,
                Color32::from_rgba_unmultiplied(230, 40, 30, (220.0 * a) as u8),
            ),
        );
    }
    // Top: timer, scores, round.
    let (my_team, att_score, def_score) = {
        let att_is_squad0 = snap.squad0_side == Team::Attack;
        let (a, d) = if att_is_squad0 {
            (snap.scores[0], snap.scores[1])
        } else {
            (snap.scores[1], snap.scores[0])
        };
        (me.as_ref().map(|m| m.team).unwrap_or(Team::Spectator), a, d)
    };
    let has_rounds = rules.as_ref().is_some_and(|r| r.mode.has_rounds());
    let top = Pos2::new(centre.x, screen_rect.min.y + 12.0);
    let phase = match snap.phase {
        Phase::Warmup => "Échauffement",
        Phase::Freeze => "Achat",
        Phase::Live => "",
        Phase::RoundOver => "Fin de manche",
        Phase::HalfTime => "Mi-temps",
        Phase::GameOver => "Fin du match",
    };
    let timer = if snap.bomb.kind == 3 {
        format!("BOMBE {}", mmss(snap.bomb.time_left))
    } else {
        mmss(snap.time_left)
    };
    let timer = if timer == "--:--" {
        String::new()
    } else {
        timer
    };
    text(
        top + Vec2::new(0.0, 0.0),
        Align2::CENTER_TOP,
        &timer,
        &big,
        if snap.bomb.kind == 3 {
            Color32::from_rgb(255, 90, 70)
        } else {
            white
        },
    );
    if has_rounds {
        text(
            top + Vec2::new(-70.0, 4.0),
            Align2::RIGHT_TOP,
            &format!("{att_score}"),
            &big,
            tc.get(Team::Attack),
        );
        text(
            top + Vec2::new(70.0, 4.0),
            Align2::LEFT_TOP,
            &format!("{def_score}"),
            &big,
            tc.get(Team::Defense),
        );
        let label = if snap.round == 0 {
            phase.to_string()
        } else {
            format!("Manche {}  {phase}", snap.round)
        };
        text(
            top + Vec2::new(0.0, 38.0),
            Align2::CENTER_TOP,
            &label,
            &small,
            white,
        );
    } else if !phase.is_empty() {
        text(
            top + Vec2::new(0.0, 38.0),
            Align2::CENTER_TOP,
            phase,
            &small,
            white,
        );
    }
    // Banner (round end, bomb planted).
    if let Some((b, t)) = &hud.round_banner
        && now - t < 4.0
    {
        text(
            Pos2::new(centre.x, screen_rect.min.y + 110.0),
            Align2::CENTER_TOP,
            b,
            &mid,
            Color32::from_rgb(255, 230, 120),
        );
    }
    // Bottom: health, armour, money, ammo.
    let bl = Pos2::new(screen_rect.min.x + 24.0, screen_rect.max.y - 20.0);
    let br = Pos2::new(screen_rect.max.x - 24.0, screen_rect.max.y - 20.0);
    if let Some(m) = &me {
        if m.alive {
            let hp_col = if m.health <= 25 {
                Color32::from_rgb(255, 80, 60)
            } else {
                white
            };
            text(
                bl,
                Align2::LEFT_BOTTOM,
                &format!("✚ {}", m.health.max(0)),
                &big,
                hp_col,
            );
            let armor = if m.armor > 0 {
                format!("ARM {}{}", m.armor, if m.helmet { " +casque" } else { "" })
            } else {
                "ARM 0".into()
            };
            text(
                bl + Vec2::new(130.0, 0.0),
                Align2::LEFT_BOTTOM,
                &armor,
                &big,
                white,
            );
            if m.defuse_kit {
                text(
                    bl + Vec2::new(330.0, 0.0),
                    Align2::LEFT_BOTTOM,
                    "kit",
                    &mid,
                    tc.get(Team::Defense),
                );
            }
            if m.bomb {
                text(
                    bl + Vec2::new(390.0, 0.0),
                    Align2::LEFT_BOTTOM,
                    "BOMBE",
                    &mid,
                    tc.get(Team::Attack),
                );
            }
            if let Some(w) = weapon {
                let gun = match m.active {
                    ps_weapons::Slot::Primary => m.primary.map(|p| p.1),
                    ps_weapons::Slot::Secondary => m.secondary.map(|p| p.1),
                    _ => None,
                };
                let ammo = match gun {
                    Some(g) => format!("{} / {}", g.mag, g.reserve),
                    None => String::new(),
                };
                let reloading = m.hand.reload_until.is_some();
                text(
                    br,
                    Align2::RIGHT_BOTTOM,
                    &ammo,
                    &big,
                    if gun.is_some_and(|g| g.mag == 0) {
                        Color32::from_rgb(255, 80, 60)
                    } else {
                        white
                    },
                );
                text(
                    br + Vec2::new(0.0, -36.0),
                    Align2::RIGHT_BOTTOM,
                    &format!(
                        "{}{}",
                        w.name,
                        if reloading { " (rechargement)" } else { "" }
                    ),
                    &mid,
                    white,
                );
            } else if m.active == ps_weapons::Slot::Bomb {
                text(br, Align2::RIGHT_BOTTOM, "Bombe", &mid, white);
            }
            if !m.grenades.is_empty() {
                let g: Vec<String> = m
                    .grenades
                    .iter()
                    .filter_map(|w| data.catalog.try_get(*w))
                    .map(|w| w.name.clone())
                    .collect();
                text(
                    br + Vec2::new(0.0, -62.0),
                    Align2::RIGHT_BOTTOM,
                    &g.join(" · "),
                    &small,
                    Color32::from_gray(200),
                );
            }
            if let Some(since) = m.planting_since.or(m.defusing_since) {
                let total = rules
                    .as_ref()
                    .map(|r| {
                        if m.planting_since.is_some() {
                            r.bomb.plant_time
                        } else if m.defuse_kit {
                            r.bomb.defuse_time_kit
                        } else {
                            r.bomb.defuse_time
                        }
                    })
                    .unwrap_or(3.0);
                let f = ((c.server_time(now) - since) as f32 / total.max(0.1)).clamp(0.0, 1.0);
                let r = egui::Rect::from_center_size(
                    centre + Vec2::new(0.0, 80.0),
                    Vec2::new(220.0, 10.0),
                );
                painter.rect_filled(r, 2.0, Color32::from_black_alpha(160));
                painter.rect_filled(
                    egui::Rect::from_min_size(r.min, Vec2::new(r.width() * f, r.height())),
                    2.0,
                    Color32::from_rgb(255, 200, 60),
                );
                text(
                    r.center_top() + Vec2::new(0.0, -4.0),
                    Align2::CENTER_BOTTOM,
                    if m.planting_since.is_some() {
                        "Pose de la bombe"
                    } else {
                        "Désamorçage"
                    },
                    &small,
                    white,
                );
            }
        } else {
            let msg = match (m.respawn_in, m.spectating) {
                (Some(t), _) => format!("Réapparition dans {:.1} s", t.max(0.0)),
                (None, Some(id)) => format!(
                    "Spectateur : {}",
                    c.players
                        .iter()
                        .find(|p| p.id == id)
                        .map(|p| p.name.as_str())
                        .unwrap_or("?")
                ),
                _ => "Mort".into(),
            };
            text(
                Pos2::new(centre.x, screen_rect.max.y - 80.0),
                Align2::CENTER_BOTTOM,
                &msg,
                &mid,
                white,
            );
        }
        if has_rounds || m.money > 0 {
            text(
                Pos2::new(screen_rect.min.x + 24.0, screen_rect.min.y + 16.0),
                Align2::LEFT_TOP,
                &format!("$ {}", m.money),
                &mid,
                Color32::from_rgb(120, 230, 120),
            );
        }
        if m.can_buy && !hud.buy_menu {
            text(
                Pos2::new(screen_rect.min.x + 24.0, screen_rect.min.y + 42.0),
                Align2::LEFT_TOP,
                &format!("[{}] acheter", settings.keys.buy),
                &small,
                Color32::from_gray(190),
            );
        }
        if m.team == Team::Spectator {
            text(
                Pos2::new(centre.x, screen_rect.max.y - 110.0),
                Align2::CENTER_BOTTOM,
                "Spectateur : Échap pour choisir une équipe",
                &mid,
                white,
            );
        }
    }
    // Kill feed.
    for (i, k) in hud
        .kills
        .iter()
        .rev()
        .filter(|k| now - k.at < 7.0)
        .take(6)
        .enumerate()
    {
        let y = screen_rect.min.y + 16.0 + i as f32 * 24.0;
        let x = screen_rect.max.x - 16.0;
        let mut tags = k.weapon.clone();
        if k.penetrated {
            tags.push_str(" [mur]");
        }
        if k.headshot {
            tags.push_str(" [HS]");
        }
        let line_w = 380.0;
        if k.me_involved {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    Pos2::new(x - line_w, y - 2.0),
                    Vec2::new(line_w + 6.0, 22.0),
                ),
                3.0,
                Color32::from_rgba_unmultiplied(160, 30, 30, 120),
            );
        }
        let vr = painter.text(
            Pos2::new(x, y),
            Align2::RIGHT_TOP,
            &k.victim,
            small.clone(),
            tc.get(k.victim_team),
        );
        let wr = painter.text(
            Pos2::new(vr.min.x - 10.0, y),
            Align2::RIGHT_TOP,
            &tags,
            small.clone(),
            white,
        );
        painter.text(
            Pos2::new(wr.min.x - 10.0, y),
            Align2::RIGHT_TOP,
            &k.killer,
            small.clone(),
            tc.get(k.killer_team),
        );
    }
    // Notices and chat.
    for (i, (n, _)) in hud.notices.iter().rev().take(4).enumerate() {
        text(
            Pos2::new(centre.x, screen_rect.min.y + 140.0 + i as f32 * 20.0),
            Align2::CENTER_TOP,
            n,
            &small,
            Color32::from_rgb(255, 230, 160),
        );
    }
    let chat_base = Pos2::new(screen_rect.min.x + 24.0, screen_rect.max.y - 90.0);
    for (i, (line, t)) in hud.chat_log.iter().rev().take(6).enumerate() {
        if hud.chat_open || now - t < 10.0 {
            text(
                chat_base + Vec2::new(0.0, -(i as f32) * 18.0),
                Align2::LEFT_BOTTOM,
                line,
                &small,
                white,
            );
        }
    }
    if hud.chat_open {
        let mut send = None;
        egui::Area::new(egui::Id::new("chat_input"))
            .fixed_pos(chat_base + Vec2::new(0.0, 8.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(if hud.chat_team { "(équipe)" } else { "(tous)" });
                    let r =
                        ui.add(egui::TextEdit::singleline(&mut hud.chat_text).desired_width(360.0));
                    r.request_focus();
                    if r.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        send = Some(std::mem::take(&mut hud.chat_text));
                    }
                });
            });
        if let Some(t) = send {
            if !t.trim().is_empty() {
                s.client.send_reliable(ClientMsg::Chat {
                    team_only: hud.chat_team,
                    text: t.trim().chars().take(120).collect(),
                });
            }
            hud.chat_open = false;
        }
    }
    let c = &s.client;
    // Teammate names above their heads (projected with the camera's FOV).
    let tan_half = (vertical_fov(if zoomed {
        weapon
            .and_then(|w| w.zoom_fov(me.as_ref().map(|m| m.hand.zoom).unwrap_or(0)))
            .unwrap_or(FOV)
    } else {
        FOV
    }) * 0.5)
        .tan();
    for r in c.remote_players(now) {
        if !r.alive() || r.team != my_team || my_team == Team::Spectator {
            continue;
        }
        let rel = r.origin + Vec3::Y * 80.0 - hud.eye;
        let fwd = hud.view.forward();
        let right = hud.view.right_flat();
        let up = right.cross(fwd).normalize_or_zero();
        let z = rel.dot(fwd);
        if z < 1.0 {
            continue;
        }
        let f = screen_rect.height() * 0.5 / tan_half;
        let p = centre + Vec2::new(rel.dot(right) / z * f, -(rel.dot(up)) / z * f);
        let name = c
            .players
            .iter()
            .find(|p| p.id == r.id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        text(p, Align2::CENTER_BOTTOM, &name, &small, tc.get(r.team));
    }
    // Scoreboard.
    if hud.scoreboard {
        egui::Window::new("Tableau des scores")
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .default_width(720.0)
            .show(ctx, |ui| {
                let w = c.welcome.as_ref();
                ui.horizontal(|ui| {
                    ui.heading(w.map(|w| w.server_name.clone()).unwrap_or_default());
                    ui.label(format!(
                        "{} · {} · {} Hz",
                        w.map(|w| w.map.clone()).unwrap_or_default(),
                        profile_label(&w.map(|w| w.profile.clone()).unwrap_or_default()),
                        c.rate()
                    ));
                });
                for team in [Team::Attack, Team::Defense, Team::Spectator] {
                    let mut ps: Vec<_> = c.players.iter().filter(|p| p.team == team).collect();
                    if ps.is_empty() {
                        continue;
                    }
                    ps.sort_by(|a, b| b.score.cmp(&a.score).then(b.kills.cmp(&a.kills)));
                    ui.separator();
                    let score = match team {
                        Team::Attack if has_rounds => format!(" — {att_score}"),
                        Team::Defense if has_rounds => format!(" — {def_score}"),
                        _ => String::new(),
                    };
                    ui.label(
                        RichText::new(format!("{}{score}", team_name(team)))
                            .color(tc.get(team))
                            .strong(),
                    );
                    egui::Grid::new(format!("sb_{team:?}"))
                        .striped(true)
                        .num_columns(9)
                        .min_col_width(52.0)
                        .show(ui, |ui| {
                            for h in [
                                "Joueur", "Argent", "É", "A", "M", "HS", "Dégâts", "Score", "Ping",
                            ] {
                                ui.label(RichText::new(h).weak());
                            }
                            ui.end_row();
                            for p in ps {
                                let mut n = RichText::new(format!(
                                    "{}{}{}",
                                    p.name,
                                    if p.bot { " [BOT]" } else { "" },
                                    if p.alive || team == Team::Spectator {
                                        ""
                                    } else {
                                        " ✝"
                                    }
                                ));
                                if Some(p.id) == c.me() {
                                    n = n.strong().color(Color32::from_rgb(255, 240, 160));
                                }
                                ui.label(n);
                                ui.label(p.money.map(|m| format!("{m} $")).unwrap_or_default());
                                ui.label(p.kills.to_string());
                                ui.label(p.assists.to_string());
                                ui.label(p.deaths.to_string());
                                ui.label(p.headshots.to_string());
                                ui.label(p.damage.to_string());
                                ui.label(p.score.to_string());
                                ui.label(if p.bot {
                                    "BOT".into()
                                } else {
                                    format!("{}", p.ping_ms)
                                });
                                ui.end_row();
                            }
                        });
                }
            });
    }
    // Buy menu.
    if hud.buy_menu {
        let mut open = true;
        let money = me.as_ref().map(|m| m.money).unwrap_or(0);
        let can_buy = me.as_ref().is_some_and(|m| m.can_buy);
        let mut buy: Option<String> = None;
        egui::Window::new(format!("Achat — {money} $"))
            .open(&mut open)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if !can_buy {
                    ui.colored_label(
                        Color32::from_rgb(255, 140, 100),
                        "Achat impossible ici (zone d'achat, temps d'achat).",
                    );
                }
                ui.horizontal_top(|ui| {
                    for (title, kinds) in [
                        ("Pistolets", vec![WeaponKind::Pistol]),
                        ("Pistolets-mitrailleurs", vec![WeaponKind::Smg]),
                        (
                            "Fusils",
                            vec![
                                WeaponKind::Rifle,
                                WeaponKind::SniperLight,
                                WeaponKind::SniperHeavy,
                            ],
                        ),
                        ("Grenades", vec![WeaponKind::Grenade]),
                    ] {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(title).strong());
                            for w in data
                                .catalog
                                .weapons
                                .iter()
                                .filter(|w| kinds.contains(&w.kind))
                            {
                                let ok = can_buy && w.price <= money;
                                if ui
                                    .add_enabled(
                                        ok,
                                        egui::Button::new(format!("{} — {} $", w.name, w.price)),
                                    )
                                    .clicked()
                                {
                                    buy = Some(w.id.clone());
                                }
                            }
                        });
                    }
                    ui.vertical(|ui| {
                        let e = &data.catalog.equipment;
                        ui.label(RichText::new("Équipement").strong());
                        for (id, name, price) in [
                            ("kevlar", "Gilet", e.kevlar_price),
                            ("kevlar_helmet", "Gilet + casque", e.kevlar_helmet_price),
                            ("defuse_kit", "Kit de désamorçage", e.defuse_kit_price),
                        ] {
                            if ui
                                .add_enabled(
                                    can_buy && price <= money,
                                    egui::Button::new(format!("{name} — {price} $")),
                                )
                                .clicked()
                            {
                                buy = Some(id.into());
                            }
                        }
                    });
                });
            });
        if let Some(b) = buy {
            s.client.send_reliable(ClientMsg::Buy(b));
        }
        if !open {
            hud.buy_menu = false;
        }
    }
    let c = &s.client;
    // Network graph.
    if settings.net_graph {
        let st = &c.stats;
        let cs = c.conn_stats();
        let lines = [
            format!(
                "{:.0} i/s  ({:.1} ms)",
                hud.fps,
                hud.frame_ms.last().copied().unwrap_or(0.0)
            ),
            format!(
                "ping {:.0} ms  gigue {:.1} ms  pertes {:.1} %",
                st.rtt_ms,
                st.jitter_ms,
                st.loss * 100.0
            ),
            format!(
                "tick {} Hz  interp {:.2} ticks  tampon cmd {:.1}  échelle {:.3}",
                c.rate(),
                st.interp_ticks,
                st.cmd_buffer,
                st.time_scale
            ),
            format!(
                "instantanés {} ({} perdus)  {:.0} o/inst.",
                st.snapshots,
                st.snapshots_lost,
                if st.snapshots > 0 {
                    st.snapshot_bytes as f64 / st.snapshots as f64
                } else {
                    0.0
                }
            ),
            format!(
                "corrections {}  dernière {:.2} u  p95 {:.2} u  resync {}",
                st.corrections, st.last_correction, st.correction_p95, st.resyncs
            ),
            format!(
                "variante {:?}  extrapolation {:.1} ms  urgents {} ({:.1} ms)",
                c.cfg.variant, st.extrapolated_ms, st.urgent_events, st.urgent_delay_ms
            ),
            cs.map(|x| {
                format!(
                    "paquets env. {}  reçus {}",
                    x.packets_sent, x.packets_received
                )
            })
            .unwrap_or_default(),
        ];
        let base = Pos2::new(screen_rect.max.x - 16.0, screen_rect.max.y - 120.0);
        for (i, l) in lines.iter().enumerate() {
            text(
                base + Vec2::new(0.0, -(i as f32) * 16.0),
                Align2::RIGHT_BOTTOM,
                l,
                &FontId::monospace(12.0),
                Color32::from_rgb(200, 255, 200),
            );
        }
    }
    // Shot Inspector.
    if hud.inspector {
        egui::Window::new("Inspecteur de tirs (F3)").default_width(420.0).anchor(Align2::LEFT_CENTER, [12.0, 0.0]).show(ctx, |ui| {
            if c.shots.is_empty() {
                ui.label("Tirez pour voir le détail calculé par le serveur.");
            }
            egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
                for sh in c.shots.iter().rev().take(6) {
                    let w = data.catalog.try_get(sh.weapon).map(|w| w.name.clone()).unwrap_or_default();
                    ui.label(RichText::new(format!("tick {} · {w}", sh.tick)).strong());
                    ui.monospace(format!("recul  ({:+.2}°, {:+.2}°)  dispersion {:.2}° → ({:+.2}, {:+.2})", sh.recoil.0, sh.recoil.1, sh.spread_deg, sh.spread.0, sh.spread.1));
                    ui.monospace(format!("vitesse {:.0} u/s  au sol {}  rembobinage {:.1} ms{}", sh.speed, if sh.on_ground { "oui" } else { "non" }, sh.rewind_ms, if sh.rewind_clamped { " (borné)" } else { "" }));
                    for (m, thick, dmg, stopped) in &sh.surfaces {
                        ui.monospace(format!("  surface {m} : épaisseur {thick:.1} u, {}", if *stopped { "balle arrêtée".to_string() } else { format!("traversée, reste {dmg:.1} dégâts") }));
                    }
                    match &sh.hit {
                        Some((id, g, raw, hp, ar, killed)) => {
                            let n = c.players.iter().find(|p| p.id == *id).map(|p| p.name.clone()).unwrap_or(format!("#{id}"));
                            ui.monospace(format!("  touché {n} ({g:?}) : brut {raw:.1} → -{hp} PV, -{ar} armure{}", if *killed { ", tué" } else { "" }));
                        }
                        None => {
                            ui.monospace("  aucun joueur touché");
                        }
                    }
                    ui.separator();
                }
            });
        });
    }
    Ok(())
}
