use super::*;
use std::sync::atomic::{AtomicU32, Ordering};

fn temp_cfg() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "ps-console-test-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Runs lines against fresh builtin registry; returns settings, output, actions.
fn run(
    settings: &mut Settings,
    info: &Info,
    dir: &Path,
    lines: &[&str],
) -> (Vec<Line>, Vec<Action>) {
    let reg = Registry::new(commands::builtin());
    let mut ctx = Ctx::new(&reg, settings, info, dir.to_path_buf());
    for l in lines {
        reg.execute(&mut ctx, l);
    }
    (ctx.out, ctx.actions)
}

fn errors(out: &[Line]) -> usize {
    out.iter().filter(|l| l.kind == LineKind::Error).count()
}

#[test]
fn tokenizer_handles_quotes_comments_and_errors() {
    assert_eq!(tokenize("a  b").unwrap(), ["a", "b"]);
    assert_eq!(tokenize("say \"x y\" z").unwrap(), ["say", "x y", "z"]);
    assert_eq!(tokenize("a // comment").unwrap(), ["a"]);
    assert_eq!(tokenize("   ").unwrap(), Vec::<String>::new());
    assert_eq!(tokenize("a \"\"").unwrap(), ["a", ""]);
    assert!(tokenize("a \"b").is_err());
}

#[test]
fn set_get_alias_and_persistence_flag() {
    let mut s = Settings::default();
    let d = temp_cfg();
    let (out, _) = run(
        &mut s,
        &Info::default(),
        &d,
        &["sensitivity 3,5", "sensibility"],
    );
    assert_eq!(errors(&out), 0);
    assert_eq!(s.sensitivity, 3.5);
    assert!(out.last().unwrap().text.starts_with("sensitivity = 3.5"));
    // Case-insensitive, alias writes the same field.
    run(&mut s, &Info::default(), &d, &["SENSIBILITY 1"]);
    assert_eq!(s.sensitivity, 1.0);
    // Persistent change flags the settings for saving.
    let reg = Registry::new(commands::builtin());
    let info = Info::default();
    let mut ctx = Ctx::new(&reg, &mut s, &info, d);
    reg.execute(&mut ctx, "m_yaw 0.03");
    assert!(ctx.dirty);
    let mut ctx2 = Ctx::new(&reg, &mut s, &info, PathBuf::new());
    reg.execute(&mut ctx2, "m_yaw");
    assert!(!ctx2.dirty, "a read does not save");
}

#[test]
fn bounds_and_types_are_validated() {
    let mut s = Settings::default();
    let d = temp_cfg();
    let before = s.clone();
    let (out, _) = run(
        &mut s,
        &Info::default(),
        &d,
        &[
            "sensitivity 0",
            "sensitivity 11",
            "sensitivity abc",
            "sensitivity nan",
            "sensitivity 1 2",
            "cl_crosshaircolor 1 2",
            "cl_crosshaircolor 1 2 300",
            "cl_crosshairdot maybe",
            "fps_max 10",
            "fps_max 5000",
            "volume 1.5",
        ],
    );
    assert_eq!(errors(&out), 11, "{out:?}");
    assert_eq!(s, before);
    run(
        &mut s,
        &Info::default(),
        &d,
        &[
            "fps_max 144",
            "cl_crosshaircolor 1 2 3",
            "cl_crosshairdot on",
        ],
    );
    assert_eq!(s.fps_limit, 144);
    assert_eq!(s.crosshair.color, [1, 2, 3]);
    assert!(s.crosshair.dot);
}

#[test]
fn audio_commands_share_the_menu_fields() {
    let mut s = Settings::default();
    let d = temp_cfg();
    run(
        &mut s,
        &Info::default(),
        &d,
        &[
            "snd_master_volume 0.5",
            "snd_game_volume 0.25",
            "snd_ui_volume 0",
            "snd_mute 1",
        ],
    );
    assert_eq!(s.master_volume, 0.5);
    assert_eq!(s.game_volume, 0.25);
    assert_eq!(s.ui_volume, 0.0);
    assert!(s.muted);
    let (out, _) = run(
        &mut s,
        &Info::default(),
        &d,
        &["snd_music_volume 1", "snd_voice_volume"],
    );
    assert_eq!(errors(&out), 2);
    assert!(out.iter().all(|l| l.text.contains("non disponible")));
    assert_eq!(s.master_volume, 0.5);
}

#[test]
fn unknown_command_and_permissions() {
    let mut s = Settings::default();
    let d = temp_cfg();
    let (out, _) = run(
        &mut s,
        &Info::default(),
        &d,
        &["nope", "host kick x", "ping"],
    );
    assert_eq!(errors(&out), 3);
    let connected = Info {
        connected: true,
        ..Info::default()
    };
    let (out, actions) = run(
        &mut s,
        &connected,
        &d,
        &["host \"say hello\"", "disconnect", "quit"],
    );
    assert_eq!(errors(&out), 0);
    assert_eq!(
        actions,
        [
            Action::Host("say hello".into()),
            Action::Disconnect,
            Action::Quit
        ]
    );
    // TestOnly refuses unless the context is in test mode.
    let reg = Registry::new(vec![Command {
        name: "t",
        aliases: &[],
        help: "",
        args: vec![],
        permission: Permission::TestOnly,
        persist: Persist::Temporary,
        body: Body::Action(Box::new(|_, _| Ok("ok".into()))),
    }]);
    let info = Info::default();
    let mut ctx = Ctx::new(&reg, &mut s, &info, d);
    reg.execute(&mut ctx, "t");
    assert_eq!(errors(&ctx.out), 1);
    ctx.test_mode = true;
    reg.execute(&mut ctx, "t");
    assert_eq!(ctx.out.last().unwrap().text, "ok");
}

#[test]
fn temporary_commands_do_not_flag_a_save() {
    let mut s = Settings::default();
    let reg = Registry::new(vec![Command {
        name: "tmp",
        aliases: &[],
        help: "",
        args: vec![ArgSpec {
            name: "v",
            kind: ArgKind::Bool,
            optional: false,
        }],
        permission: Permission::Config,
        persist: Persist::Temporary,
        body: Body::Var {
            get: Box::new(|s| s.invert_y.to_string()),
            set: Box::new(|s, v| {
                s.invert_y = v[0].bool();
                Ok(())
            }),
        },
    }]);
    let info = Info::default();
    let mut ctx = Ctx::new(&reg, &mut s, &info, PathBuf::new());
    reg.execute(&mut ctx, "tmp 1");
    assert!(!ctx.dirty);
    assert!(s.invert_y);
}

#[test]
fn registry_has_unique_names_and_aliases() {
    let reg = Registry::new(commands::builtin());
    let mut seen = std::collections::HashSet::new();
    for c in reg.commands() {
        for n in std::iter::once(c.name).chain(c.aliases.iter().copied()) {
            assert!(seen.insert(n.to_ascii_lowercase()), "doublon : {n}");
        }
    }
    // The crosshair has one command per field.
    for n in [
        "cl_crosshairsize",
        "cl_crosshairgap",
        "cl_crosshairthickness",
        "cl_crosshairdot",
        "cl_crosshairoutline",
        "cl_crosshaircolor",
    ] {
        assert!(reg.find(n).is_some(), "{n}");
    }
    // No command reaches the simulation.
    for banned in [
        "health", "speed", "damage", "hitbox", "tickrate", "ban", "teleport",
    ] {
        assert!(
            reg.commands().iter().all(|c| !c.name.contains(banned)),
            "{banned}"
        );
    }
}

#[test]
fn completion() {
    let reg = Registry::new(commands::builtin());
    assert_eq!(reg.complete("disc").text, "disconnect ");
    let c = reg.complete("snd_");
    assert!(c.candidates.len() >= 4);
    assert_eq!(c.text, "snd_");
    let c = reg.complete("viewmodel_off");
    assert_eq!(c.text, "viewmodel_offset_");
    assert!(c.candidates.contains(&"viewmodel_offset_x".to_string()));
    assert_eq!(reg.complete("zzz").text, "zzz");
    assert_eq!(reg.complete("volume 0").text, "volume 0");
    assert_eq!(reg.complete("SENSIB").text, "sensibility ");
}

#[test]
fn history_navigation() {
    let mut h = History::default();
    assert_eq!(h.prev("x"), None);
    h.push("a");
    h.push("a");
    h.push("  ");
    h.push("b");
    assert_eq!(h.items(), ["a", "b"]);
    assert_eq!(h.prev("draft").as_deref(), Some("b"));
    assert_eq!(h.prev("").as_deref(), Some("a"));
    assert_eq!(h.prev("").as_deref(), Some("a"));
    assert_eq!(h.next().as_deref(), Some("b"));
    assert_eq!(h.next().as_deref(), Some("draft"));
    assert_eq!(h.next(), None);
    for i in 0..150 {
        h.push(&format!("c{i}"));
    }
    assert_eq!(h.items().len(), 100);
}

#[test]
fn cfg_names_cannot_escape_the_folder() {
    let d = Path::new("cfg");
    assert_eq!(cfg_path(d, "autoexec").unwrap(), d.join("autoexec.cfg"));
    assert_eq!(cfg_path(d, "my-cfg_1.cfg").unwrap(), d.join("my-cfg_1.cfg"));
    for bad in [
        "",
        "..",
        "../x",
        "..\\x",
        "a/b",
        "a\\b",
        "/etc/passwd",
        "C:\\x",
        "C:x",
        "x.cfg.cfg",
        "a.b",
        "x\0",
        "con",
        "NUL.cfg",
        "com1",
        "LPT9",
        ".cfg",
        "é",
        &"a".repeat(65),
    ] {
        assert!(cfg_path(d, bad).is_err(), "{bad:?}");
    }
}

#[test]
fn exec_writeconfig_round_trip_and_limits() {
    let d = temp_cfg();
    let mut s = Settings {
        sensitivity: 4.0,
        ..Settings::default()
    };
    s.crosshair.color = [9, 8, 7];
    s.muted = true;
    let (out, _) = run(&mut s, &Info::default(), &d, &["writeconfig mine"]);
    assert_eq!(errors(&out), 0, "{out:?}");
    let mut t = Settings::default();
    let (out, _) = run(&mut t, &Info::default(), &d, &["exec mine"]);
    assert_eq!(errors(&out), 0, "{out:?}");
    assert_eq!(t.sensitivity, 4.0);
    assert_eq!(t.crosshair.color, [9, 8, 7]);
    assert!(t.muted);
    assert_eq!(t.master_volume, s.master_volume);

    // Traversal is refused, a missing file reports an error.
    let (out, _) = run(
        &mut t,
        &Info::default(),
        &d,
        &["exec ../mine", "exec absent", "writeconfig a/b"],
    );
    assert_eq!(errors(&out), 3);

    // Session and server commands are refused from a file; the rest still runs.
    std::fs::write(
        d.join("bad.cfg"),
        "// c\nquit\nconnect 1.2.3.4\nhost x\nsensitivity 1.5\nsensitivity 99\n",
    )
    .unwrap();
    let info = Info {
        connected: true,
        ..Info::default()
    };
    let (out, actions) = run(&mut t, &info, &d, &["exec bad"]);
    assert!(actions.is_empty(), "{actions:?}");
    assert_eq!(errors(&out), 4, "{out:?}");
    assert!(out.iter().any(|l| l.text.starts_with("bad.cfg:2")));
    assert_eq!(t.sensitivity, 1.5);

    // Recursion is bounded (depth and budget): the file runs itself.
    std::fs::write(d.join("loop.cfg"), "exec loop\nexec loop\nexec loop\n").unwrap();
    let (out, _) = run(&mut t, &Info::default(), &d, &["exec loop"]);
    assert!(errors(&out) > 0);
    assert!(out.len() < 5000);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn help_lists_supported_commands_only() {
    let mut s = Settings::default();
    let (out, _) = run(
        &mut s,
        &Info::default(),
        Path::new("x"),
        &["help", "help volume"],
    );
    assert!(out[0].text.contains("sensitivity"));
    assert!(!out[0].text.contains("snd_music_volume"));
    assert!(out[1].text.contains("snd_master_volume"));
}
