//! Built-in console commands. Settings variables read and write the shared
//! [`Settings`] through small accessors; nothing here reaches the simulation.

use super::{Action, ArgKind, ArgSpec, Body, Command, Ctx, Permission, Persist, Value};
use crate::settings::Settings;

fn spec(name: &'static str, kind: ArgKind, optional: bool) -> ArgSpec {
    ArgSpec {
        name,
        kind,
        optional,
    }
}

fn base(
    name: &'static str,
    aliases: &'static [&'static str],
    help: &'static str,
    permission: Permission,
    args: Vec<ArgSpec>,
    body: Body,
) -> Command {
    Command {
        name,
        aliases,
        help,
        args,
        permission,
        persist: Persist::Persistent,
        body,
    }
}

fn action(
    name: &'static str,
    aliases: &'static [&'static str],
    help: &'static str,
    permission: Permission,
    args: Vec<ArgSpec>,
    run: impl Fn(&mut Ctx, &[Value]) -> Result<String, String> + Send + Sync + 'static,
) -> Command {
    base(
        name,
        aliases,
        help,
        permission,
        args,
        Body::Action(Box::new(run)),
    )
}

fn float_var(
    name: &'static str,
    aliases: &'static [&'static str],
    help: &'static str,
    (min, max): (f64, f64),
    get: fn(&Settings) -> f32,
    set: fn(&mut Settings, f32),
) -> Command {
    base(
        name,
        aliases,
        help,
        Permission::Config,
        vec![spec("valeur", ArgKind::Float { min, max }, false)],
        Body::Var {
            get: Box::new(move |s| get(s).to_string()),
            set: Box::new(move |s, v| {
                set(s, v[0].f32());
                Ok(())
            }),
        },
    )
}

fn bool_var(
    name: &'static str,
    aliases: &'static [&'static str],
    help: &'static str,
    permission: Permission,
    get: fn(&Settings) -> bool,
    set: fn(&mut Settings, bool),
) -> Command {
    base(
        name,
        aliases,
        help,
        permission,
        vec![spec("valeur", ArgKind::Bool, false)],
        Body::Var {
            get: Box::new(move |s| u8::from(get(s)).to_string()),
            set: Box::new(move |s, v| {
                set(s, v[0].bool());
                Ok(())
            }),
        },
    )
}

fn unsupported(name: &'static str, help: &'static str, why: &'static str) -> Command {
    base(
        name,
        &[],
        help,
        Permission::Config,
        vec![],
        Body::Unsupported(why),
    )
}

fn show_help(ctx: &mut Ctx, args: &[Value]) -> Result<String, String> {
    if let Some(a) = args.first() {
        let c = ctx
            .registry
            .find(a.text())
            .ok_or_else(|| format!("commande inconnue : {}", a.text()))?;
        let al = if c.aliases.is_empty() {
            String::new()
        } else {
            format!("\nalias : {}", c.aliases.join(", "))
        };
        return Ok(format!(
            "{}\n{}{al}\npermission : {}, {}",
            c.usage(),
            c.help,
            c.permission.label(),
            if c.persist == Persist::Persistent {
                "persistant"
            } else {
                "temporaire"
            }
        ));
    }
    let mut lines = vec!["Commandes (help <commande> pour le détail) :".to_string()];
    for c in ctx.registry.commands() {
        if matches!(c.body, Body::Unsupported(_)) {
            continue;
        }
        lines.push(format!("  {:<28} {}", c.usage(), c.help));
    }
    Ok(lines.join("\n"))
}

fn status(ctx: &mut Ctx, _: &[Value]) -> Result<String, String> {
    let i = ctx.info;
    let mut l = vec![format!("version {}", env!("CARGO_PKG_VERSION"))];
    if i.connected {
        l.push(format!(
            "connecté à {}",
            i.server.as_deref().unwrap_or("serveur local")
        ));
        if let Some(p) = i.ping_ms {
            l.push(format!("ping {p:.0} ms"));
        }
        if let Some(t) = i.tick_hz {
            l.push(format!("tick {t:.0} Hz"));
        }
    } else {
        l.push("non connecté".into());
    }
    if i.fps > 0.0 {
        l.push(format!("{:.0} images/s", i.fps));
    }
    Ok(l.join("\n"))
}

fn crosshair_float(
    name: &'static str,
    help: &'static str,
    range: (f64, f64),
    get: fn(&Settings) -> f32,
    set: fn(&mut Settings, f32),
) -> Command {
    float_var(name, &[], help, range, get, set)
}

pub fn builtin() -> Vec<Command> {
    use Permission::*;
    vec![
        // Console and session.
        action(
            "help",
            &["?"],
            "liste des commandes, ou détail d'une commande",
            Client,
            vec![spec("commande", ArgKind::Word, true)],
            show_help,
        ),
        action(
            "clear",
            &["cls"],
            "efface la console",
            Client,
            vec![],
            |c, _| {
                c.actions.push(Action::Clear);
                Ok(String::new())
            },
        ),
        action(
            "quit",
            &["exit"],
            "quitte le jeu",
            Client,
            vec![],
            |c, _| {
                c.actions.push(Action::Quit);
                Ok("au revoir".into())
            },
        ),
        action(
            "disconnect",
            &[],
            "quitte la partie en cours",
            Client,
            vec![],
            |c, _| {
                if !c.info.connected {
                    return Err("pas de partie en cours".into());
                }
                c.actions.push(Action::Disconnect);
                Ok("déconnexion".into())
            },
        ),
        action(
            "connect",
            &[],
            "rejoint un serveur (hôte[:port])",
            Client,
            vec![spec("adresse", ArgKind::Word, false)],
            |c, a| {
                let addr = crate::net::normalize_address(a[0].text())?;
                c.actions.push(Action::Connect(addr.clone()));
                Ok(format!("connexion à {addr}"))
            },
        ),
        action(
            "retry",
            &["reconnect"],
            "se reconnecte au dernier serveur",
            Client,
            vec![],
            |c, _| {
                let last = c.settings.last_server.clone();
                crate::net::normalize_address(&last)?;
                c.actions.push(Action::Retry);
                Ok(format!("connexion à {last}"))
            },
        ),
        action(
            "version",
            &[],
            "version du client",
            Client,
            vec![],
            |_, _| Ok(format!("Project S {}", env!("CARGO_PKG_VERSION"))),
        ),
        action(
            "status",
            &[],
            "état de la connexion",
            Client,
            vec![],
            status,
        ),
        action(
            "host",
            &[],
            "envoie une commande au serveur (il décide et peut refuser)",
            ServerAdmin,
            vec![spec("commande", ArgKind::Rest, false)],
            |c, a| {
                c.actions.push(Action::Host(a[0].text().to_string()));
                Ok(String::new())
            },
        ),
        // Files.
        action(
            "exec",
            &[],
            "exécute <nom>.cfg du dossier cfg de la configuration",
            Config,
            vec![spec("fichier", ArgKind::Word, false)],
            |c, a| {
                let registry = c.registry;
                let r = registry.exec_file(c, a[0].text());
                c.dirty = true;
                r
            },
        ),
        action(
            "writeconfig",
            &[],
            "écrit les réglages dans <nom>.cfg (config par défaut)",
            Config,
            vec![spec("fichier", ArgKind::Word, true)],
            |c, a| {
                let name = a.first().map_or("config", |v| v.text()).to_string();
                let registry = c.registry;
                registry.write_config(c, &name)
            },
        ),
        // Mouse.
        float_var(
            "sensitivity",
            &["sensibility"],
            "sensibilité de la souris",
            (0.05, 10.0),
            |s| s.sensitivity,
            |s, v| s.sensitivity = v,
        ),
        float_var(
            "m_yaw",
            &[],
            "degrés par unité de souris, horizontal",
            (0.001, 0.1),
            |s| s.m_yaw,
            |s, v| s.m_yaw = v,
        ),
        float_var(
            "m_pitch",
            &[],
            "degrés par unité de souris, vertical",
            (0.001, 0.1),
            |s| s.m_pitch,
            |s, v| s.m_pitch = v,
        ),
        float_var(
            "zoom_sensitivity_ratio",
            &[],
            "multiplicateur de sensibilité en zoom",
            (0.2, 2.0),
            |s| s.zoom_sensitivity,
            |s, v| s.zoom_sensitivity = v,
        ),
        // Audio.
        float_var(
            "volume",
            &["snd_master_volume"],
            "volume général (0 à 1)",
            (0.0, 1.0),
            |s| s.master_volume,
            |s, v| s.master_volume = v,
        ),
        float_var(
            "snd_game_volume",
            &[],
            "volume du jeu : armes, pas, grenades, bombe (0 à 1)",
            (0.0, 1.0),
            |s| s.game_volume,
            |s, v| s.game_volume = v,
        ),
        float_var(
            "snd_ui_volume",
            &[],
            "volume de l'interface et des retours de touche (0 à 1)",
            (0.0, 1.0),
            |s| s.ui_volume,
            |s, v| s.ui_volume = v,
        ),
        bool_var(
            "snd_mute",
            &[],
            "coupe tout le son",
            Config,
            |s| s.muted,
            |s, v| s.muted = v,
        ),
        unsupported(
            "snd_music_volume",
            "volume de la musique",
            "le jeu n'a pas de musique",
        ),
        unsupported(
            "snd_voice_volume",
            "volume des voix",
            "le jeu n'a pas de chat vocal",
        ),
        // Display and diagnostics.
        base(
            "fps_max",
            &[],
            "limite d'images par seconde (0 = aucune, sinon 30 à 1000)",
            Config,
            vec![spec("valeur", ArgKind::Int { min: 0, max: 1000 }, false)],
            Body::Var {
                get: Box::new(|s| s.fps_limit.to_string()),
                set: Box::new(|s, v| {
                    let n = v[0].int();
                    if n != 0 && n < 30 {
                        return Err("0 (aucune limite) ou 30 à 1000".into());
                    }
                    s.fps_limit = n as u32;
                    Ok(())
                }),
            },
        ),
        bool_var(
            "cl_showfps",
            &[],
            "affiche le compteur d'images par seconde",
            Dev,
            |s| s.show_fps,
            |s, v| s.show_fps = v,
        ),
        bool_var(
            "net_graph",
            &[],
            "affiche le graphe réseau",
            Dev,
            |s| s.net_graph,
            |s, v| s.net_graph = v,
        ),
        action(
            "ping",
            &[],
            "latence vers le serveur",
            Dev,
            vec![],
            |c, _| {
                c.info
                    .ping_ms
                    .map(|p| format!("ping {p:.0} ms"))
                    .ok_or_else(|| "non connecté".into())
            },
        ),
        // Viewmodel.
        float_var(
            "viewmodel_fov",
            &[],
            "champ de vision de l'arme",
            (54.0, 90.0),
            |s| s.viewmodel_fov,
            |s, v| s.viewmodel_fov = v,
        ),
        float_var(
            "viewmodel_offset_x",
            &[],
            "décalage de l'arme en x",
            (-10.0, 10.0),
            |s| s.viewmodel_offset[0],
            |s, v| s.viewmodel_offset[0] = v,
        ),
        float_var(
            "viewmodel_offset_y",
            &[],
            "décalage de l'arme en y",
            (-10.0, 10.0),
            |s| s.viewmodel_offset[1],
            |s, v| s.viewmodel_offset[1] = v,
        ),
        float_var(
            "viewmodel_offset_z",
            &[],
            "décalage de l'arme en z",
            (-10.0, 10.0),
            |s| s.viewmodel_offset[2],
            |s, v| s.viewmodel_offset[2] = v,
        ),
        unsupported(
            "viewmodel_opacity",
            "opacité de l'arme",
            "le rendu de l'arme n'a pas d'opacité réglable",
        ),
        // Crosshair: one command per field of `Crosshair`.
        crosshair_float(
            "cl_crosshairsize",
            "taille du réticule",
            (0.0, 30.0),
            |s| s.crosshair.size,
            |s, v| s.crosshair.size = v,
        ),
        crosshair_float(
            "cl_crosshairgap",
            "écart du réticule",
            (-5.0, 30.0),
            |s| s.crosshair.gap,
            |s, v| s.crosshair.gap = v,
        ),
        crosshair_float(
            "cl_crosshairthickness",
            "épaisseur du réticule",
            (0.0, 10.0),
            |s| s.crosshair.thickness,
            |s, v| s.crosshair.thickness = v,
        ),
        bool_var(
            "cl_crosshairdot",
            &[],
            "point central du réticule",
            Config,
            |s| s.crosshair.dot,
            |s, v| s.crosshair.dot = v,
        ),
        bool_var(
            "cl_crosshairoutline",
            &[],
            "contour du réticule",
            Config,
            |s| s.crosshair.outline,
            |s, v| s.crosshair.outline = v,
        ),
        base(
            "cl_crosshaircolor",
            &[],
            "couleur du réticule (rouge vert bleu, 0 à 255)",
            Config,
            ["rouge", "vert", "bleu"]
                .map(|n| spec(n, ArgKind::Int { min: 0, max: 255 }, false))
                .to_vec(),
            Body::Var {
                get: Box::new(|s| {
                    let c = s.crosshair.color;
                    format!("{} {} {}", c[0], c[1], c[2])
                }),
                set: Box::new(|s, v| {
                    s.crosshair.color = [v[0].int() as u8, v[1].int() as u8, v[2].int() as u8];
                    Ok(())
                }),
            },
        ),
    ]
}
