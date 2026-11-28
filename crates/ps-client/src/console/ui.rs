//! egui layer of the console: toggle key, output, input line with history and
//! completion, and the application of the actions commands ask for.

use super::{Action, Console, Ctx, Info, LineKind};
use crate::bindings::{self, Rebind};
use crate::game::{Hud, Screen};
use crate::net::{Clock, GameDataRes, Net};
use crate::settings::{Settings, config_dir};
use bevy::app::AppExit;
use bevy::input::ButtonInput;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use ps_sim_net::proto::ClientMsg;

fn snapshot(net: &Net, hud: &Hud) -> Info {
    let s = net.session.as_ref();
    Info {
        connected: s.is_some(),
        server: s.map(|s| s.server.to_string()),
        ping_ms: s
            .filter(|s| s.client.in_game())
            .map(|s| s.client.stats.rtt_ms),
        tick_hz: s.filter(|s| s.client.in_game()).map(|s| s.client.rate()),
        fps: hud.fps,
    }
}

/// Moves the text cursor to the end of the input line.
fn cursor_to_end(ctx: &egui::Context, id: egui::Id, len: usize) {
    let mut st = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    st.cursor.set_char_range(Some(egui::text::CCursorRange::one(
        egui::text::CCursor::new(len),
    )));
    st.store(ctx, id);
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut contexts: EguiContexts,
    mut con: ResMut<Console>,
    mut settings: ResMut<Settings>,
    mut net: ResMut<Net>,
    mut screen: ResMut<Screen>,
    mut hud: ResMut<Hud>,
    data: Res<GameDataRes>,
    clock: Res<Clock>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    rebind: Res<Rebind>,
    mut exit: MessageWriter<AppExit>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let toggle = bindings::just_pressed(&settings.keys.console, &keys, &mouse);
    let escape = keys.just_pressed(KeyCode::Escape) && rebind.0.is_none();
    let mut just_opened = false;
    if hud.console_open {
        if toggle || escape {
            hud.console_open = false;
        }
    } else if toggle && rebind.0.is_none() && ctx.memory(|m| m.focused().is_none()) {
        hud.console_open = true;
        just_opened = true;
    }
    if !hud.console_open {
        return Ok(());
    }

    let id = egui::Id::new("console_input");
    // History and completion keys are taken before the text field sees them.
    let up = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
    let down = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
    let tab = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
    let mut moved_cursor = false;
    {
        let Console {
            registry,
            input,
            history,
            output,
            ..
        } = &mut *con;
        if up && let Some(t) = history.prev(input) {
            *input = t;
            moved_cursor = true;
        }
        if down && let Some(t) = history.next() {
            *input = t;
            moved_cursor = true;
        }
        if tab {
            let c = registry.complete(input);
            if !c.candidates.is_empty() {
                output.push(super::Line {
                    kind: LineKind::Info,
                    text: c.candidates.join("  "),
                });
            }
            *input = c.text;
            moved_cursor = true;
        }
    }

    let screen_rect = ctx.content_rect();
    let mut submitted: Option<String> = None;
    egui::Area::new(egui::Id::new("console_area"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen_rect.min)
        .show(&ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(225))
                .inner_margin(8.0)
                .show(ui, |ui| {
                    let w = screen_rect.width() - 16.0;
                    ui.set_width(w);
                    egui::ScrollArea::vertical()
                        .max_height(screen_rect.height() * 0.45)
                        .min_scrolled_height(screen_rect.height() * 0.45)
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for l in &con.output {
                                let (prefix, col) = match l.kind {
                                    LineKind::Input => ("> ", egui::Color32::from_gray(150)),
                                    LineKind::Info => ("", egui::Color32::from_gray(225)),
                                    LineKind::Error => ("", egui::Color32::from_rgb(255, 120, 110)),
                                };
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("{prefix}{}", l.text))
                                            .monospace()
                                            .color(col),
                                    )
                                    .wrap(),
                                );
                            }
                        });
                    ui.separator();
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut con.input)
                            .id(id)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .lock_focus(true)
                            .hint_text("commande (help)"),
                    );
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submitted = Some(std::mem::take(&mut con.input));
                    }
                    resp.request_focus();
                    // Live hint: usage of the command being typed, or the names matching.
                    let typed = con.input.trim_start();
                    let first = typed.split_whitespace().next().unwrap_or("");
                    let hint = if first.is_empty() {
                        String::new()
                    } else if let Some(c) = con.registry.find(first) {
                        format!("{}  : {}", c.usage(), c.help)
                    } else {
                        let n = con.registry.names_with_prefix(first);
                        n.iter().take(8).copied().collect::<Vec<_>>().join("  ")
                    };
                    ui.label(egui::RichText::new(hint).small().weak().monospace());
                    if just_opened {
                        // The key that opened the console must not be typed into it.
                        con.input.clear();
                    }
                });
        });
    if moved_cursor {
        cursor_to_end(&ctx, id, con.input.chars().count());
    }

    if let Some(line) = submitted {
        run_line(
            &mut con,
            &mut settings,
            &mut net,
            &mut screen,
            &data,
            &clock,
            &hud,
            &mut exit,
            &line,
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_line(
    con: &mut Console,
    settings: &mut Settings,
    net: &mut Net,
    screen: &mut Screen,
    data: &GameDataRes,
    clock: &Clock,
    hud: &Hud,
    exit: &mut MessageWriter<AppExit>,
    line: &str,
) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    con.history.push(line);
    con.push(LineKind::Input, line);
    let info = snapshot(net, hud);
    let (out, actions, dirty) = {
        let mut ctx = Ctx::new(&con.registry, settings, &info, config_dir().join("cfg"));
        con.registry.execute(&mut ctx, line);
        (ctx.out, ctx.actions, ctx.dirty)
    };
    for l in out {
        con.push(l.kind, l.text);
    }
    if dirty {
        settings.save();
    }
    let now = clock.now();
    for a in actions {
        match a {
            Action::Clear => con.output.clear(),
            Action::Quit => {
                exit.write(AppExit::Success);
            }
            Action::Disconnect => net.disconnect(now),
            Action::Connect(addr) => connect(con, settings, net, screen, data, now, &addr),
            Action::Retry => {
                let last = settings.last_server.clone();
                connect(con, settings, net, screen, data, now, &last);
            }
            Action::Host(cmd) => {
                if let Some(s) = &mut net.session {
                    s.client.send_reliable(ClientMsg::Host(cmd));
                }
            }
        }
    }
}

fn connect(
    con: &mut Console,
    settings: &mut Settings,
    net: &mut Net,
    screen: &mut Screen,
    data: &GameDataRes,
    now: f64,
    addr: &str,
) {
    if settings.last_server != addr {
        settings.last_server = addr.to_string();
        settings.save();
    }
    net.connect(addr, settings, data.0.clone(), now);
    if net.session.is_some() {
        *screen = Screen::Game;
    } else if let Some(e) = &net.error {
        con.push(LineKind::Error, e.clone());
    }
}
