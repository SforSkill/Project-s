//! Developer console: a registry of commands, each with typed and bounded
//! arguments, a permission category and a persistence mode. The registry and the
//! execution are plain Rust (no Bevy), so they are unit-tested; `ui.rs` is the
//! egui layer and `commands.rs` the built-in commands.
//!
//! The console never owns a setting: variables read and write the same
//! [`Settings`] as the menus, so a change made here shows up at once in the
//! settings screen and the other way round. Nothing here touches health, speed,
//! damage, hitboxes, tick rate, sanctions, permissions or positions: those stay
//! on the server, which is authoritative. Server-side commands go through the
//! existing `ClientMsg::Host` message (the `host` command).

pub mod commands;
pub mod ui;

use crate::settings::Settings;
use bevy::prelude::Resource;
use std::path::{Path, PathBuf};

/// Who may run a command, and what it may reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Console and session control (help, quit, connect...). Not allowed from a cfg file.
    Client,
    /// Player settings (sensitivity, volume, crosshair...). Allowed from a cfg file.
    Config,
    /// Diagnostics (fps counter, network graph, ping).
    Dev,
    /// Sent to the server as a host command; needs a session, never from a cfg file.
    ServerAdmin,
    /// Only registered for tests (`Ctx::test_mode`).
    #[allow(
        dead_code,
        reason = "gate checked by the tests; no built-in command uses it"
    )]
    TestOnly,
}

impl Permission {
    pub fn label(self) -> &'static str {
        match self {
            Permission::Client => "Client",
            Permission::Config => "Config",
            Permission::Dev => "Dev",
            Permission::ServerAdmin => "ServerAdmin",
            Permission::TestOnly => "TestOnly",
        }
    }
}

/// Whether a successful change is written to `settings.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persist {
    Persistent,
    /// Applied but not saved (none of the built-in settings uses it yet).
    #[allow(dead_code, reason = "extension point, exercised by the tests")]
    Temporary,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArgKind {
    Float {
        min: f64,
        max: f64,
    },
    Int {
        min: i64,
        max: i64,
    },
    Bool,
    /// One word (or a quoted string).
    Word,
    /// The rest of the line, joined by spaces.
    Rest,
}

#[derive(Debug, Clone, Copy)]
pub struct ArgSpec {
    pub name: &'static str,
    pub kind: ArgKind,
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Float(f64),
    Int(i64),
    Bool(bool),
    Text(String),
}

impl Value {
    pub fn f32(&self) -> f32 {
        match self {
            Value::Float(v) => *v as f32,
            Value::Int(v) => *v as f32,
            _ => 0.0,
        }
    }

    pub fn int(&self) -> i64 {
        match self {
            Value::Int(v) => *v,
            Value::Float(v) => *v as i64,
            _ => 0,
        }
    }

    pub fn bool(&self) -> bool {
        matches!(self, Value::Bool(true))
    }

    pub fn text(&self) -> &str {
        match self {
            Value::Text(t) => t,
            _ => "",
        }
    }
}

pub type Getter = Box<dyn Fn(&Settings) -> String + Send + Sync>;
pub type Setter = Box<dyn Fn(&mut Settings, &[Value]) -> Result<(), String> + Send + Sync>;
pub type Runner = Box<dyn Fn(&mut Ctx, &[Value]) -> Result<String, String> + Send + Sync>;

pub enum Body {
    /// A setting: no argument shows the current value, otherwise sets it.
    Var {
        get: Getter,
        set: Setter,
    },
    Action(Runner),
    /// Known name, feature that does not exist: always answers with the reason.
    Unsupported(&'static str),
}

pub struct Command {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub help: &'static str,
    pub args: Vec<ArgSpec>,
    pub permission: Permission,
    pub persist: Persist,
    pub body: Body,
}

impl Command {
    /// `name <required> [optional]` with bounds.
    pub fn usage(&self) -> String {
        let mut u = self.name.to_string();
        for a in &self.args {
            let range = match a.kind {
                ArgKind::Float { min, max } => format!(" {}..{}", fmt_f64(min), fmt_f64(max)),
                ArgKind::Int { min, max } => format!(" {min}..{max}"),
                ArgKind::Bool => " 0|1".to_string(),
                _ => String::new(),
            };
            if a.optional {
                u.push_str(&format!(" [{}{range}]", a.name));
            } else {
                u.push_str(&format!(" <{}{range}>", a.name));
            }
        }
        u
    }

    fn matches(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
            || self.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
    }
}

pub fn fmt_f64(v: f64) -> String {
    format!("{v}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Input,
    Info,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub kind: LineKind,
    pub text: String,
}

/// What a command asks the application to do (needs the network, the window...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Clear,
    Quit,
    Disconnect,
    Connect(String),
    Retry,
    /// Server command, sent as `ClientMsg::Host`.
    Host(String),
}

/// Read-only snapshot of the session, filled by the UI layer.
#[derive(Debug, Clone, Default)]
pub struct Info {
    pub connected: bool,
    pub server: Option<String>,
    pub ping_ms: Option<f32>,
    pub tick_hz: Option<f64>,
    pub fps: f32,
}

/// Maximum nesting of `exec`, lines read per file and commands per top-level call.
pub const MAX_EXEC_DEPTH: u8 = 3;
const MAX_CFG_LINES: usize = 500;
const MAX_CFG_BYTES: u64 = 64 * 1024;
const SCRIPT_BUDGET: u32 = 2000;

pub struct Ctx<'a> {
    pub registry: &'a Registry,
    pub settings: &'a mut Settings,
    pub info: &'a Info,
    /// Folder of the `.cfg` files (`<config>/cfg`).
    pub cfg_dir: PathBuf,
    pub actions: Vec<Action>,
    pub out: Vec<Line>,
    /// A persistent setting changed: the caller saves the settings.
    pub dirty: bool,
    depth: u8,
    budget: u32,
    pub test_mode: bool,
}

impl<'a> Ctx<'a> {
    pub fn new(
        registry: &'a Registry,
        settings: &'a mut Settings,
        info: &'a Info,
        cfg_dir: PathBuf,
    ) -> Self {
        Self {
            registry,
            settings,
            info,
            cfg_dir,
            actions: Vec::new(),
            out: Vec::new(),
            dirty: false,
            depth: 0,
            budget: SCRIPT_BUDGET,
            test_mode: false,
        }
    }

    fn say(&mut self, kind: LineKind, text: impl Into<String>) {
        self.out.push(Line {
            kind,
            text: text.into(),
        });
    }

    fn check(&self, p: Permission) -> Result<(), String> {
        let script = self.depth > 0;
        match p {
            Permission::Config | Permission::Dev => Ok(()),
            Permission::Client if script => Err("interdit dans un fichier cfg".into()),
            Permission::Client => Ok(()),
            Permission::ServerAdmin if script => Err("interdit dans un fichier cfg".into()),
            Permission::ServerAdmin if !self.info.connected => {
                Err("commande serveur : connectez-vous d'abord".into())
            }
            Permission::ServerAdmin => Ok(()),
            Permission::TestOnly if self.test_mode => Ok(()),
            Permission::TestOnly => Err("commande réservée aux tests".into()),
        }
    }
}

/// Splits a line into words: spaces separate, double quotes group, `//` starts a
/// comment (outside quotes).
pub fn tokenize(line: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut has = false;
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                quoted = false;
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            quoted = true;
            has = true;
        } else if c == '/' && chars.peek() == Some(&'/') {
            break;
        } else if c.is_whitespace() {
            if has {
                out.push(std::mem::take(&mut cur));
                has = false;
            }
        } else {
            cur.push(c);
            has = true;
        }
    }
    if quoted {
        return Err("guillemet non fermé".into());
    }
    if has {
        out.push(cur);
    }
    Ok(out)
}

fn parse_bool(t: &str) -> Option<bool> {
    match t.to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "oui" | "yes" => Some(true),
        "0" | "false" | "off" | "non" | "no" => Some(false),
        _ => None,
    }
}

/// Types the tokens after the command name according to its argument list.
pub fn parse_args(cmd: &Command, tokens: &[String]) -> Result<Vec<Value>, String> {
    let usage = || format!("usage : {}", cmd.usage());
    let required = cmd.args.iter().filter(|a| !a.optional).count();
    let has_rest = cmd.args.iter().any(|a| a.kind == ArgKind::Rest);
    if tokens.len() < required {
        return Err(usage());
    }
    if !has_rest && tokens.len() > cmd.args.len() {
        return Err(format!("trop d'arguments ; {}", usage()));
    }
    let mut vals = Vec::new();
    for (i, spec) in cmd.args.iter().enumerate() {
        let Some(tok) = tokens.get(i) else { break };
        let v = match spec.kind {
            ArgKind::Float { min, max } => {
                let v: f64 = tok
                    .replace(',', ".")
                    .parse()
                    .map_err(|_| format!("« {tok} » n'est pas un nombre"))?;
                if !v.is_finite() || v < min || v > max {
                    return Err(format!(
                        "{} : valeur hors bornes ({}..{})",
                        spec.name,
                        fmt_f64(min),
                        fmt_f64(max)
                    ));
                }
                Value::Float(v)
            }
            ArgKind::Int { min, max } => {
                let v: i64 = tok
                    .parse()
                    .map_err(|_| format!("« {tok} » n'est pas un entier"))?;
                if v < min || v > max {
                    return Err(format!("{} : valeur hors bornes ({min}..{max})", spec.name));
                }
                Value::Int(v)
            }
            ArgKind::Bool => Value::Bool(
                parse_bool(tok).ok_or_else(|| format!("« {tok} » : attendu 0 ou 1 (on/off)"))?,
            ),
            ArgKind::Word => Value::Text(tok.clone()),
            ArgKind::Rest => Value::Text(tokens[i..].join(" ")),
        };
        vals.push(v);
    }
    Ok(vals)
}

/// Result of a TAB press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// New content of the input line.
    pub text: String,
    /// Every name the typed prefix could continue to (when there are several).
    pub candidates: Vec<String>,
}

pub struct Registry {
    cmds: Vec<Command>,
}

impl Registry {
    pub fn new(cmds: Vec<Command>) -> Self {
        Self { cmds }
    }

    pub fn commands(&self) -> &[Command] {
        &self.cmds
    }

    pub fn find(&self, name: &str) -> Option<&Command> {
        self.cmds.iter().find(|c| c.matches(name))
    }

    /// Names and aliases starting with `prefix`, sorted.
    pub fn names_with_prefix(&self, prefix: &str) -> Vec<&'static str> {
        let p = prefix.to_ascii_lowercase();
        let mut v: Vec<&'static str> = self
            .cmds
            .iter()
            .flat_map(|c| std::iter::once(c.name).chain(c.aliases.iter().copied()))
            .filter(|n| n.starts_with(&p))
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// TAB completion of the command word. Only the first word is completed.
    pub fn complete(&self, input: &str) -> Completion {
        let unchanged = || Completion {
            text: input.to_string(),
            candidates: Vec::new(),
        };
        if input.chars().any(char::is_whitespace) {
            return unchanged();
        }
        let names = self.names_with_prefix(input);
        match names.len() {
            0 => unchanged(),
            1 => Completion {
                text: format!("{} ", names[0]),
                candidates: Vec::new(),
            },
            _ => {
                let mut common = names[0].to_string();
                for n in &names[1..] {
                    let keep = common
                        .chars()
                        .zip(n.chars())
                        .take_while(|(a, b)| a == b)
                        .count();
                    common = common.chars().take(keep).collect();
                }
                Completion {
                    text: common,
                    candidates: names.iter().map(|s| s.to_string()).collect(),
                }
            }
        }
    }

    /// Runs one line; the output is appended to `ctx.out`.
    pub fn execute(&self, ctx: &mut Ctx, line: &str) {
        let toks = match tokenize(line) {
            Ok(t) => t,
            Err(e) => return ctx.say(LineKind::Error, e),
        };
        let Some((name, args)) = toks.split_first() else {
            return;
        };
        let Some(cmd) = self.find(name) else {
            let near = self.names_with_prefix(&name.chars().take(3).collect::<String>());
            let hint = if near.is_empty() {
                String::new()
            } else {
                format!(" Peut-être : {}.", near.join(", "))
            };
            return ctx.say(
                LineKind::Error,
                format!("Commande inconnue : « {name} ». Tapez help.{hint}"),
            );
        };
        if let Err(e) = ctx.check(cmd.permission) {
            return ctx.say(LineKind::Error, format!("{} : {e}", cmd.name));
        }
        match &cmd.body {
            Body::Unsupported(why) => {
                ctx.say(
                    LineKind::Error,
                    format!("{} : non disponible ({why})", cmd.name),
                );
            }
            Body::Var { get, set } => {
                if args.is_empty() {
                    let v = get(ctx.settings);
                    return ctx.say(
                        LineKind::Info,
                        format!("{} = {v}  : {}\n{}", cmd.name, cmd.help, cmd.usage()),
                    );
                }
                let result = parse_args(cmd, args).and_then(|vals| set(ctx.settings, &vals));
                match result {
                    Ok(()) => {
                        if cmd.persist == Persist::Persistent {
                            ctx.dirty = true;
                        }
                        let v = get(ctx.settings);
                        ctx.say(LineKind::Info, format!("{} = {v}", cmd.name));
                    }
                    Err(e) => ctx.say(LineKind::Error, format!("{} : {e}", cmd.name)),
                }
            }
            Body::Action(run) => match parse_args(cmd, args).and_then(|vals| run(ctx, &vals)) {
                Ok(s) if s.is_empty() => {}
                Ok(s) => ctx.say(LineKind::Info, s),
                Err(e) => ctx.say(LineKind::Error, format!("{} : {e}", cmd.name)),
            },
        }
    }

    /// Runs a `.cfg` file line by line through the same registry.
    pub fn exec_file(&self, ctx: &mut Ctx, name: &str) -> Result<String, String> {
        if ctx.depth >= MAX_EXEC_DEPTH {
            return Err(format!(
                "exec imbriqués trop profonds (max {MAX_EXEC_DEPTH})"
            ));
        }
        let path = cfg_path(&ctx.cfg_dir, name)?;
        let meta = std::fs::metadata(&path).map_err(|_| "fichier introuvable".to_string())?;
        if meta.len() > MAX_CFG_BYTES {
            return Err("fichier trop gros".into());
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("lecture : {e}"))?;
        ctx.depth += 1;
        let mut ran = 0;
        for (i, line) in text.lines().take(MAX_CFG_LINES).enumerate() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') || t.starts_with("//") {
                continue;
            }
            if ctx.budget == 0 {
                ctx.say(LineKind::Error, "budget de commandes épuisé");
                break;
            }
            ctx.budget -= 1;
            let before = ctx.out.len();
            self.execute(ctx, t);
            for l in &mut ctx.out[before..] {
                if l.kind == LineKind::Error {
                    l.text = format!("{name}.cfg:{} : {}", i + 1, l.text);
                }
            }
            ran += 1;
        }
        ctx.depth -= 1;
        Ok(format!("{name}.cfg : {ran} commande(s) exécutée(s)"))
    }

    /// Writes every persistent setting variable as a `.cfg` file.
    pub fn write_config(&self, ctx: &mut Ctx, name: &str) -> Result<String, String> {
        let path = cfg_path(&ctx.cfg_dir, name)?;
        let mut text = String::from("// Project S : réglages exportés (exec pour recharger)\n");
        for c in &self.cmds {
            if let Body::Var { get, .. } = &c.body
                && c.permission == Permission::Config
                && c.persist == Persist::Persistent
            {
                text.push_str(&format!("{} {}\n", c.name, get(ctx.settings)));
            }
        }
        std::fs::create_dir_all(&ctx.cfg_dir).map_err(|e| format!("dossier cfg : {e}"))?;
        std::fs::write(&path, text).map_err(|e| format!("écriture : {e}"))?;
        Ok(format!("réglages écrits dans {name}.cfg"))
    }
}

/// `<cfg_dir>/<name>.cfg` for a plain name: letters, digits, `_` and `-` only
/// (an optional `.cfg` suffix is accepted). Separators, `..`, drive letters and
/// Windows device names are refused, so the result can never leave `cfg_dir`.
pub fn cfg_path(cfg_dir: &Path, name: &str) -> Result<PathBuf, String> {
    let base = name.strip_suffix(".cfg").unwrap_or(name);
    let ok = !base.is_empty()
        && base.len() <= 64
        && base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !ok {
        return Err("nom de fichier invalide (lettres, chiffres, _ et - seulement)".into());
    }
    let up = base.to_ascii_uppercase();
    let reserved = matches!(up.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((up.starts_with("COM") || up.starts_with("LPT"))
            && up.len() == 4
            && up.as_bytes()[3].is_ascii_digit());
    if reserved {
        return Err("nom réservé par le système".into());
    }
    Ok(cfg_dir.join(format!("{base}.cfg")))
}

/// Command history with ↑/↓ navigation. The line being typed is kept as a draft.
#[derive(Debug, Default)]
pub struct History {
    items: Vec<String>,
    pos: Option<usize>,
    draft: String,
}

impl History {
    const MAX: usize = 100;

    pub fn push(&mut self, line: &str) {
        let t = line.trim();
        self.pos = None;
        if t.is_empty() || self.items.last().is_some_and(|l| l == t) {
            return;
        }
        self.items.push(t.to_string());
        if self.items.len() > Self::MAX {
            self.items.remove(0);
        }
    }

    /// ↑ : older entry (the current input is kept as a draft on the first press).
    pub fn prev(&mut self, current: &str) -> Option<String> {
        if self.items.is_empty() {
            return None;
        }
        let p = match self.pos {
            None => {
                self.draft = current.to_string();
                self.items.len() - 1
            }
            Some(p) => p.saturating_sub(1),
        };
        self.pos = Some(p);
        Some(self.items[p].clone())
    }

    /// ↓ : newer entry, then the draft.
    pub fn next(&mut self) -> Option<String> {
        let p = self.pos?;
        if p + 1 < self.items.len() {
            self.pos = Some(p + 1);
            Some(self.items[p + 1].clone())
        } else {
            self.pos = None;
            Some(std::mem::take(&mut self.draft))
        }
    }

    #[cfg(test)]
    pub fn items(&self) -> &[String] {
        &self.items
    }
}

/// Console state (the registry is built once).
#[derive(Resource)]
pub struct Console {
    pub registry: Registry,
    pub input: String,
    pub output: Vec<Line>,
    pub history: History,
}

impl Console {
    const MAX_LINES: usize = 500;

    pub fn push(&mut self, kind: LineKind, text: impl Into<String>) {
        self.output.push(Line {
            kind,
            text: text.into(),
        });
        let extra = self.output.len().saturating_sub(Self::MAX_LINES);
        if extra > 0 {
            self.output.drain(..extra);
        }
    }
}

impl Default for Console {
    fn default() -> Self {
        let mut c = Console {
            registry: Registry::new(commands::builtin()),
            input: String::new(),
            output: Vec::new(),
            history: History::default(),
        };
        c.push(
            LineKind::Info,
            "Console développeur : help pour la liste des commandes, TAB pour compléter.",
        );
        c
    }
}

#[cfg(test)]
mod tests;
