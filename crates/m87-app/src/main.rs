//! `m87` — lightweight configuration tool for the M87 Pro keyboard.

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use m87_hid::{list_interfaces, Keyboard, Role};
use m87_proto::layout::{self, KEYS};
use m87_proto::lighting::{Effect, Lighting, Rgb, MAX_LEVEL};
use m87_proto::plan::{self, Kind, Packet};
use m87_proto::keymap::{KeyAction, Layer, MEDIA, Mouse};
use m87_proto::settings::SLEEP_CHOICES;
use m87_proto::{hex, Link};

mod tui;

use m87_core::state;

use state::{LightMode, State};

#[derive(Parser)]
#[command(
    name = "m87",
    version,
    about = "Configure the Nodal Studio M87 Pro keyboard. Run without a command to open the full-screen interface.",
    after_help = "For the Nodal Studio M87 Pro only (USB 0C45:8006, 2.4 GHz receiver 0C45:FEFE). Unofficial: not affiliated with Nodal Studio or Sonix.
Copyright (c) 2026 rity73. Licensed under MIT or Apache-2.0, without warranty. Sends no information over the network.
Source: https://github.com/rity73/m87pro-tool"
)]
struct Cli {
    /// Print every packet sent to and received from the keyboard.
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// List the keyboard's USB interfaces found on this PC.
    Devices,
    /// Check that the keyboard answers.
    Ping,
    /// Show the battery level (2.4 GHz receiver only).
    Battery,
    /// List the lighting effects and the settings each one uses.
    Effects,
    /// List key names (for key-colors and remap).
    Keys,
    /// Turn on a lighting effect.
    Light(LightArgs),
    /// Give each key its own fixed color.
    KeyColors(KeyColorArgs),
    /// Change what a key does.
    Remap(RemapArgs),
    /// Show the key changes saved on this PC.
    KeyMap,
    /// Return every key on both layers to its factory function.
    ResetKeys(SendOnly),
    /// Show or change keyboard settings (Windows key lock, sleep timer, ...).
    Settings(SettingsArgs),
    /// List the actions a key can be given.
    Actions,
    /// Create, list or delete macros (key sequences played by the keyboard).
    #[command(subcommand)]
    Macro(MacroCmd),
    /// Save, load, export and import complete setups (lighting, keys, macros, settings).
    #[command(subcommand)]
    Profile(ProfileCmd),
    /// Erase all changes on the keyboard: default lighting, every key normal,
    /// no macros, default settings. Profiles saved on this PC are kept.
    FactoryReset {
        /// Confirm the reset. Without it nothing is sent.
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        send: SendOpts,
    },
}

#[derive(Subcommand)]
enum ProfileCmd {
    /// List the saved profiles.
    List,
    /// Save the current setup (the last one sent to the keyboard) as a profile.
    Save {
        /// Profile name: letters, digits, spaces, - or _.
        name: String,
    },
    /// Send a saved profile to the keyboard. This replaces everything on it.
    Load {
        name: String,
        #[command(flatten)]
        send: SendOpts,
    },
    /// Delete a saved profile from this PC.
    Delete { name: String },
    /// Write a profile, or the current setup, to a file you choose.
    Export {
        /// File to write, for example D:\backup\m87.txt
        file: std::path::PathBuf,
        /// Profile to export. Without it, the current setup is exported.
        #[arg(long)]
        name: Option<String>,
    },
    /// Add a profile from a file (for example one made with export).
    Import {
        file: std::path::PathBuf,
        /// Name for the new profile. Default: the file name.
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Subcommand)]
enum MacroCmd {
    /// Show the saved macros.
    List,
    /// Create or replace a macro and send all macros to the keyboard.
    ///
    /// STEPS is a comma-separated list: tap:A, down:LShift, up:LShift,
    /// click:left, mouse-down:right, mouse-up:right, wait:200 (milliseconds).
    /// Example: m87 macro set 1 "tap:H, tap:I, wait:100, tap:Enter"
    Set {
        /// Macro number, 1 to 100.
        #[arg(value_parser = clap::value_parser!(u8).range(1..=100))]
        number: u8,
        /// Steps, in quotes.
        steps: String,
        #[command(flatten)]
        send: SendOpts,
    },
    /// Delete a macro and send the remaining macros to the keyboard.
    Delete {
        #[arg(value_parser = clap::value_parser!(u8).range(1..=100))]
        number: u8,
        #[command(flatten)]
        send: SendOpts,
    },
}

#[derive(Args)]
struct SendOnly {
    #[command(flatten)]
    send: SendOpts,
}

#[derive(Clone, Copy, ValueEnum)]
enum LayerArg {
    /// Normal typing.
    Base,
    /// While Fn is held down.
    Fn,
}

#[derive(Args)]
struct RemapArgs {
    /// Key to change (see `m87 keys`), for example CapsLock.
    key: String,
    /// New action (see `m87 actions`), for example key:Ctrl, media:mute, disabled, default.
    action: String,
    /// Layer to change.
    #[arg(short, long, value_enum, default_value_t = LayerArg::Base)]
    layer: LayerArg,
    #[command(flatten)]
    send: SendOpts,
}

#[derive(Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}

#[derive(Args)]
struct SettingsArgs {
    /// Block the Windows key.
    #[arg(long, value_enum)]
    block_win: Option<OnOff>,
    /// Block Alt+Tab.
    #[arg(long, value_enum)]
    block_alt_tab: Option<OnOff>,
    /// Block Alt+F4.
    #[arg(long, value_enum)]
    block_alt_f4: Option<OnOff>,
    /// Turn the keyboard off after no use for: never, 1min, 5min, 30min.
    #[arg(long, value_parser = ["never", "1min", "5min", "30min"])]
    sleep: Option<String>,
    /// Key response time level, 1 (fastest) to 5 (most protection against double presses).
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=5))]
    response: Option<u8>,
    /// Raw Fn switch value (meaning not yet confirmed).
    #[arg(long)]
    fn_switch: Option<u8>,
    #[command(flatten)]
    send: SendOpts,
}

#[derive(Args)]
struct SendOpts {
    /// Show the packets without sending them.
    #[arg(long)]
    dry_run: bool,
    /// Connection to assume with --dry-run when no keyboard is found.
    #[arg(long, value_enum, default_value_t = LinkArg::Wireless)]
    link: LinkArg,
}

#[derive(Clone, Copy, ValueEnum)]
enum LinkArg {
    Usb,
    Wireless,
}

#[derive(Args)]
struct LightArgs {
    /// Effect name (see `m87 effects`).
    effect: String,
    /// Single color, as RRGGBB or a name (red, orange, yellow, green, cyan, blue, purple, white).
    #[arg(short, long, conflicts_with = "rainbow")]
    color: Option<String>,
    /// Use all colors instead of a single color.
    #[arg(short, long)]
    rainbow: bool,
    /// Brightness, 0 (dimmest) to 5 (brightest).
    #[arg(short, long, default_value_t = MAX_LEVEL, value_parser = clap::value_parser!(u8).range(0..=5))]
    brightness: u8,
    /// Speed, 0 to 5.
    #[arg(short, long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(0..=5))]
    speed: u8,
    /// Direction code (effect-specific; usually 0 to 3).
    #[arg(short, long, default_value_t = 0)]
    direction: u8,
    #[command(flatten)]
    send: SendOpts,
}

#[derive(Args)]
struct KeyColorArgs {
    /// Color for every key, as RRGGBB or a name. Keys without a color stay dark.
    #[arg(short, long)]
    all: Option<String>,
    /// Color for one key, as KEY=COLOR (for example Esc=red). Can be repeated.
    #[arg(short, long = "key", value_name = "KEY=COLOR")]
    keys: Vec<String>,
    /// Brightness, 0 (dimmest) to 5 (brightest).
    #[arg(short, long, default_value_t = MAX_LEVEL, value_parser = clap::value_parser!(u8).range(0..=5))]
    brightness: u8,
    #[command(flatten)]
    send: SendOpts,
}

fn open(verbose: bool) -> Result<Keyboard> {
    let mut kb = Keyboard::open()?;
    if verbose {
        kb.set_logger(Box::new(|line| eprintln!("{line}")));
        eprintln!("Connected via {}", kb.link().label());
    }
    Ok(kb)
}

fn parse_color(s: &str) -> Result<Rgb> {
    Rgb::parse(s).with_context(|| format!("\"{s}\" is not a color. Use RRGGBB (for example FF8000) or a color name"))
}

/// Connection used for a dry run: the connected keyboard if any, else `--link`.
fn dry_run_link(opts: &SendOpts) -> Link {
    list_interfaces()
        .iter()
        .find(|i| i.role == Role::Config)
        .map(|i| i.link)
        .unwrap_or(match opts.link {
            LinkArg::Usb => Link::Wired,
            LinkArg::Wireless => Link::Dongle,
        })
}

/// Send (or, with --dry-run, print) the packets built for the connection.
fn send(opts: &SendOpts, verbose: bool, build: impl Fn(Link) -> Vec<Packet>) -> Result<bool> {
    if opts.dry_run {
        let link = dry_run_link(opts);
        let packets = build(link);
        println!("Dry run over {}: {} packet(s), nothing sent.", link.label(), packets.len());
        for p in &packets {
            let how = match p.kind {
                Kind::DongleAck => "write+wait ",
                Kind::Feature { read_back: true } => "feature+rd ",
                Kind::Feature { read_back: false } => "feature    ",
            };
            println!("  {how} {}", hex(&p.bytes));
        }
        return Ok(false);
    }
    let kb = open(verbose)?;
    kb.run(&build(kb.link()))?;
    Ok(true)
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let Some(command) = cli.command else {
        return tui::run();
    };
    match command {
        Command::Devices => {
            let list = list_interfaces();
            if list.is_empty() {
                println!("No M87 Pro keyboard or 2.4 GHz receiver was found.");
                println!("Connect the keyboard with a USB cable, or plug in its 2.4 GHz receiver.");
                return Ok(());
            }
            println!("{:<18} {:<5} {:<11} {:<18} {}", "CONNECTION", "PORT", "USAGE", "PURPOSE", "PRODUCT");
            for i in &list {
                println!(
                    "{:<18} {:<5} {:<11} {:<18} {}",
                    i.link.label(),
                    i.info.interface,
                    format!("{:04X}:{:04X}", i.info.usage_page, i.info.usage),
                    i.role.label(),
                    i.info.product
                );
                if cli.verbose {
                    println!(
                        "    report sizes: in {} / out {} / feature {}",
                        i.info.input_len, i.info.output_len, i.info.feature_len
                    );
                    println!("    {}", i.info.path);
                }
            }
            if !list.iter().any(|i| i.role == Role::Config) {
                println!("\nWarning: no settings port was recognized. Settings cannot be changed.");
            }
        }
        Command::Ping => {
            let kb = open(cli.verbose)?;
            match kb.link() {
                Link::Dongle => {
                    kb.ping()?;
                    println!("OK: the keyboard answered through the 2.4 GHz receiver.");
                }
                Link::Wired => println!("OK: the keyboard is connected by USB cable."),
            }
        }
        Command::Battery => {
            let kb = open(cli.verbose)?;
            println!("Battery: {}%", kb.battery()?);
        }
        Command::Effects => {
            println!("{:<12} {:<4} SETTINGS IT USES", "EFFECT", "ID");
            for e in Effect::ALL {
                let c = e.controls();
                let mut uses = Vec::new();
                if c.color {
                    uses.push("color");
                }
                if c.brightness {
                    uses.push("brightness");
                }
                if c.speed {
                    uses.push("speed");
                }
                if c.direction {
                    uses.push("direction");
                }
                let uses = if uses.is_empty() { "-".to_string() } else { uses.join(", ") };
                println!("{:<12} {:<4} {}", e.name(), e.id(), uses);
            }
        }
        Command::Keys => {
            for row in 0..6u8 {
                let names: Vec<&str> = KEYS.iter().filter(|k| k.row == row).map(|k| k.id).collect();
                println!("{}", names.join(" "));
            }
        }
        c @ (Command::Remap(_)
        | Command::KeyMap
        | Command::ResetKeys(_)
        | Command::Settings(_)
        | Command::Actions
        | Command::Macro(_)) => return run_m3(c, cli.verbose),
        c @ (Command::Profile(_) | Command::FactoryReset { .. }) => return run_m4(c, cli.verbose),
        Command::Light(a) => {
            let effect = Effect::from_name(&a.effect)
                .with_context(|| format!("unknown effect \"{}\". Run `m87 effects` to see the list", a.effect))?;
            let color = match &a.color {
                Some(c) => parse_color(c)?,
                None => Rgb::new(0xFF, 0, 0),
            };
            let l = Lighting {
                effect,
                color,
                rainbow: a.rainbow,
                brightness: a.brightness,
                speed: a.speed,
                direction: a.direction,
            };
            if send(&a.send, cli.verbose, |link| plan::set_effect(link, &l))? {
                let mut st = State::load()?;
                st.light_mode = LightMode::Effect;
                st.lighting = l;
                st.save()?;
                println!("Applied: {}", effect.name());
            }
        }
        Command::KeyColors(a) => {
            if a.all.is_none() && a.keys.is_empty() {
                bail!("give --all COLOR, one or more --key KEY=COLOR, or both");
            }
            let mut colors: Vec<(u8, Rgb)> = Vec::new();
            if let Some(c) = &a.all {
                let c = parse_color(c)?;
                colors.extend(KEYS.iter().map(|k| (k.index, c)));
            }
            for item in &a.keys {
                let (name, color) = item
                    .split_once('=')
                    .with_context(|| format!("\"{item}\" must look like KEY=COLOR, for example Esc=red"))?;
                let key = layout::find(name)
                    .with_context(|| format!("unknown key \"{name}\". Run `m87 keys` to see the names"))?;
                let c = parse_color(color)?;
                colors.retain(|(i, _)| *i != key.index);
                colors.push((key.index, c));
            }
            if send(&a.send, cli.verbose, |link| plan::set_key_colors(link, a.brightness, &colors))? {
                let mut st = State::load()?;
                st.light_mode = LightMode::KeyColors;
                st.key_brightness = a.brightness;
                st.key_colors = colors
                    .iter()
                    .filter(|(_, c)| *c != Rgb::default())
                    .filter_map(|(i, c)| KEYS.iter().find(|k| k.index == *i).map(|k| (k.id, *c)))
                    .collect();
                st.save()?;
                let lit = colors.iter().filter(|(_, c)| *c != Rgb::default()).count();
                println!("Applied: per-key colors ({lit} of {} keys lit)", KEYS.len());
            }
        }
    }
    Ok(())
}

fn run_m4(command: Command, verbose: bool) -> Result<()> {
    match command {
        Command::Profile(ProfileCmd::List) => {
            let names = state::list_profiles();
            if names.is_empty() {
                println!("No profiles saved. Save the current setup with: m87 profile save <NAME>");
            }
            for n in names {
                println!("{n}");
            }
            println!("(Profiles are in {})", state::profile_dir().display());
        }
        Command::Profile(ProfileCmd::Save { name }) => {
            let existed = state::profile_path(&name)?.exists();
            let p = State::load()?.save_profile(&name)?;
            let verb = if existed { "Replaced" } else { "Saved" };
            println!("{verb} profile \"{}\" ({})", name.trim(), p.display());
        }
        Command::Profile(ProfileCmd::Load { name, send: opts }) => {
            let st = State::load_profile(&name)?;
            let plan_for = |link| st.full_plan(link).map_err(|e| anyhow::anyhow!(e));
            plan_for(Link::Dongle)?;
            if send(&opts, verbose, |link| plan_for(link).unwrap_or_default())? {
                st.save()?;
                println!("Applied: profile \"{}\" is now on the keyboard.", name.trim());
            }
        }
        Command::Profile(ProfileCmd::Delete { name }) => {
            let p = state::profile_path(&name)?;
            if !p.exists() {
                bail!("there is no profile named \"{}\". Run `m87 profile list` to see the names", name.trim());
            }
            std::fs::remove_file(&p).with_context(|| format!("could not delete {}", p.display()))?;
            println!("Deleted profile \"{}\".", name.trim());
        }
        Command::Profile(ProfileCmd::Export { file, name }) => {
            let st = match &name {
                Some(n) => State::load_profile(n)?,
                None => State::load()?,
            };
            st.write_to(&file)?;
            match name {
                Some(n) => println!("Exported profile \"{}\" to {}", n.trim(), file.display()),
                None => println!("Exported the current setup to {}", file.display()),
            }
        }
        Command::Profile(ProfileCmd::Import { file, name }) => {
            let st = State::read_file(&file)?;
            let name = match name {
                Some(n) => n,
                None => file
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(String::from)
                    .context("give the profile a name with --name")?,
            };
            let existed = state::profile_path(&name)?.exists();
            st.save_profile(&name)?;
            let verb = if existed { "replaced" } else { "added" };
            println!("Profile \"{}\" {verb}. Send it to the keyboard with: m87 profile load \"{}\"", name.trim(), name.trim());
        }
        Command::FactoryReset { yes, send: opts } => {
            if !yes && !opts.dry_run {
                println!("This erases every change on the keyboard:");
                println!("  - lighting goes back to the default effect");
                println!("  - every key does its normal function");
                println!("  - all macros are deleted");
                println!("  - settings go back to their defaults");
                println!("Profiles saved on this PC are kept.");
                println!("To do it, run: m87 factory-reset --yes");
                return Ok(());
            }
            if send(&opts, verbose, plan::factory_reset)? {
                State::default().save()?;
                println!("Applied: the keyboard is back to its factory settings.");
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn print_settings(st: &State) {
    let s = &st.settings;
    let onoff = |b: bool| if b { "on" } else { "off" };
    let sleep = SLEEP_CHOICES.iter().find(|(_, v)| *v == s.sleep).map(|(n, _)| *n).unwrap_or("unknown");
    println!("Block Windows key   {}", onoff(s.disable_win));
    println!("Block Alt+Tab       {}", onoff(s.disable_alt_tab));
    println!("Block Alt+F4        {}", onoff(s.disable_alt_f4));
    println!("Sleep after         {sleep}");
    println!("Response level      {}", s.response);
    println!("Fn switch (raw)     {}", s.fn_switch);
    println!("(Saved in {})", state::path().display());
}

fn run_m3(command: Command, verbose: bool) -> Result<()> {
    match command {
        Command::Actions => {
            println!("default                 the key's normal function");
            println!("disabled                the key does nothing");
            println!("key:<KEYS>              send keys, e.g. key:A, key:Ctrl+C, key:Ctrl+Shift+Esc, key:LShift, key:F13");
            println!("mouse:<FUNCTION>        {}", Mouse::ALL.iter().map(|m| m.name()).collect::<Vec<_>>().join(", "));
            println!("media:<FUNCTION>        {}", MEDIA.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", "));
            println!("Modifiers for key:      Ctrl, Shift, Alt, Win (left side); RCtrl, RShift, RAlt, RWin");
        }
        Command::KeyMap => {
            let st = State::load()?;
            if st.keys.is_empty() {
                println!("No key changes saved. Every key does its normal function.");
            }
            for ((layer, id), action) in &st.keys {
                let l = if *layer == 0 { "base" } else { "fn  " };
                println!("{l}  {id:<12} {}", action.to_text());
            }
        }
        Command::Remap(a) => {
            let key = layout::find(&a.key).with_context(|| format!("unknown key \"{}\". Run `m87 keys` to see the names", a.key))?;
            if key.id == "Fn" {
                bail!("the Fn key cannot be changed");
            }
            let action = KeyAction::parse(&a.action).map_err(|e| anyhow::anyhow!(e))?;
            let layer = match a.layer {
                LayerArg::Base => Layer::Base,
                LayerArg::Fn => Layer::Fn,
            };
            let mut st = State::load()?;
            st.set_key(layer, key.id, action);
            let entries = st.entries(layer);
            if send(&a.send, verbose, |link| plan::set_key_map(link, layer, &entries))? {
                st.save()?;
                println!("Applied: {} key {} now does {}", layer.name(), key.id, action.to_text());
            }
        }
        Command::ResetKeys(a) => {
            if send(&a.send, verbose, plan::reset_key_maps)? {
                let mut st = State::load()?;
                st.clear_keys();
                st.save()?;
                println!("Applied: every key does its normal function again.");
            }
        }
        Command::Settings(a) => {
            let mut st = State::load()?;
            let changed = a.block_win.is_some()
                || a.block_alt_tab.is_some()
                || a.block_alt_f4.is_some()
                || a.sleep.is_some()
                || a.response.is_some()
                || a.fn_switch.is_some();
            if !changed {
                print_settings(&st);
                return Ok(());
            }
            let on = |v: OnOff| matches!(v, OnOff::On);
            let s = &mut st.settings;
            if let Some(v) = a.block_win {
                s.disable_win = on(v);
            }
            if let Some(v) = a.block_alt_tab {
                s.disable_alt_tab = on(v);
            }
            if let Some(v) = a.block_alt_f4 {
                s.disable_alt_f4 = on(v);
            }
            if let Some(v) = &a.sleep {
                s.sleep = SLEEP_CHOICES.iter().find(|(n, _)| n == v).map(|(_, x)| *x).unwrap_or(2);
            }
            if let Some(v) = a.response {
                s.response = v;
            }
            if let Some(v) = a.fn_switch {
                s.fn_switch = v;
            }
            let settings = st.settings;
            if send(&a.send, verbose, |link| plan::set_settings(link, &settings))? {
                st.save()?;
                println!("Applied.");
                print_settings(&st);
            }
        }
        Command::Macro(MacroCmd::List) => {
            let st = State::load()?;
            if st.macros.is_empty() {
                println!("No macros saved. Create one with: m87 macro set 1 \"tap:H, tap:I\"");
            }
            for (n, m) in &st.macros {
                println!("{n:>3}  {}", m.to_text());
            }
        }
        Command::Macro(MacroCmd::Set { number, steps, send: opts }) => {
            let m = m87_proto::macros::Macro::parse(&steps).map_err(|e| anyhow::anyhow!(e))?;
            let mut st = State::load()?;
            st.macros.insert(number, m);
            let buf = st.macro_storage().map_err(|e| anyhow::anyhow!(e))?;
            if send(&opts, verbose, |link| plan::set_macros(link, &buf))? {
                st.save()?;
                println!("Applied: macro {number} saved in the keyboard. Assign it with: m87 remap <KEY> macro:{number}");
            }
        }
        Command::Macro(MacroCmd::Delete { number, send: opts }) => {
            let mut st = State::load()?;
            if st.macros.remove(&number).is_none() {
                bail!("there is no macro {number}");
            }
            let buf = st.macro_storage().map_err(|e| anyhow::anyhow!(e))?;
            if send(&opts, verbose, |link| plan::set_macros(link, &buf))? {
                st.save()?;
                println!("Applied: macro {number} deleted.");
                let users: Vec<String> = st
                    .keys
                    .iter()
                    .filter(|(_, a)| matches!(a, KeyAction::Macro { slot, .. } if *slot + 1 == number))
                    .map(|((l, id), _)| format!("{}.{id}", if *l == 0 { "base" } else { "fn" }))
                    .collect();
                if !users.is_empty() {
                    println!("Note: these keys still point to macro {number}: {}. Give them a new action with m87 remap.", users.join(", "));
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
