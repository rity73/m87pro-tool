//! `m87` — lightweight configuration tool for the M87 Pro keyboard.

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use m87_hid::{list_interfaces, Keyboard, Role};
use m87_proto::layout::{self, KEYS};
use m87_proto::lighting::{Effect, Lighting, Rgb, MAX_LEVEL};
use m87_proto::plan::{self, Kind, Packet};
use m87_proto::{hex, Link};

mod tui;

#[derive(Parser)]
#[command(name = "m87", version, about = "Configure the M87 Pro keyboard. Run without a command to open the full-screen interface.")]
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
    /// List key names for use with `key-colors --key`.
    Keys,
    /// Turn on a lighting effect.
    Light(LightArgs),
    /// Give each key its own fixed color.
    KeyColors(KeyColorArgs),
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
                let lit = colors.iter().filter(|(_, c)| *c != Rgb::default()).count();
                println!("Applied: per-key colors ({lit} of {} keys lit)", KEYS.len());
            }
        }
    }
    Ok(())
}
