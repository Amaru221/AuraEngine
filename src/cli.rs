//! Command line interface and forwarding to a running instance.

use std::path::PathBuf;

use crate::config::{Config, ParentMode};
use crate::engine::mpv;
use crate::platform::{host, ipc, win};
use crate::util;

const COMMANDS: &[&str] = &[
    "next",
    "prev",
    "pause",
    "resume",
    "toggle-pause",
    "stop",
    "switcher",
    "show",
    "quit",
    "settings",
    "library",
    "explore",
    "monitors",
    "about",
    "status",
    "doctor",
    "help",
    "apply",
    "rescan",
    "install-mpv",
    "set-wallpaper",
    "search-motionbgs",
    "download-motionbgs",
    "motionbgs",
];

pub fn is_cli_invocation(args: &[String]) -> bool {
    let Some(first) = args.first() else {
        return false;
    };
    if first == "--hidden" || first == "--autostart" {
        return false;
    }
    if first == "--help" || first == "-h" || first == "--version" || first == "-V" {
        return true;
    }
    if COMMANDS.contains(&first.as_str()) {
        return true;
    }
    // A bare path means "apply this wallpaper".
    let path = PathBuf::from(first);
    path.exists() && crate::config::is_supported_media(&path)
}

pub fn extract_action(args: &[String]) -> Option<String> {
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--action" => return iter.next().cloned(),
            "--apply" => {
                if let Some(path) = iter.next() {
                    let monitor = args
                        .iter()
                        .position(|a| a == "--monitor")
                        .and_then(|i| args.get(i + 1))
                        .cloned()
                        .unwrap_or_else(|| "*".into());
                    return Some(format!("apply|{path}|{monitor}"));
                }
            }
            value if value.starts_with("--") => {}
            value => {
                if PathBuf::from(value).exists() {
                    return Some(format!("apply|{value}|*"));
                }
            }
        }
    }
    None
}

pub fn run(args: &[String]) {
    util::attach_console();
    let first = args.first().map(String::as_str).unwrap_or("help");

    match first {
        "--help" | "-h" | "help" => print_help(),
        "--version" | "-V" => util::console_out(&format!("Aura Engine {}", env!("CARGO_PKG_VERSION"))),
        "status" => print_status(),
        "doctor" => print_doctor(),
        "install-mpv" => install_mpv(),
        "motionbgs" => {
            let category = args.get(1).map(String::as_str).unwrap_or("all");
            forward(&format!("motionbgs|{category}"));
        }
        "search-motionbgs" => {
            let (query, category) = parse_motionbgs_args(&args[1..]);
            match crate::online::search(
                crate::online::Provider::MotionBgs,
                &query,
                1,
                "",
                category,
            ) {
                Ok(items) => {
                    util::console_out(&format!("{} results", items.len()));
                    for item in items.iter().take(15) {
                        util::console_out(&format!(
                            "{} | {} | {} | {}",
                            item.id, item.title, item.resolution, item.full_url
                        ));
                    }
                }
                Err(err) => util::console_err(&format!("MotionBGS search failed: {err:#}")),
            }
        }
        "download-motionbgs" => {
            let (query, category) = parse_motionbgs_args(&args[1..]);
            if query.trim().is_empty() && category == crate::online::MotionBgsCategory::All {
                util::console_err(
                    "usage: aura download-motionbgs <query> [--category anime|games|4k|superhero|nature]",
                );
                return;
            }
            match crate::online::search(
                crate::online::Provider::MotionBgs,
                &query,
                1,
                "",
                category,
            ) {
                Ok(items) => {
                    if let Some(item) = items.first() {
                        // Keep stdout clean: scripts consume the final path.
                        util::console_err(&format!("Downloading: {}", item.title));
                        match crate::online::download(item, &util::online_dir(), None) {
                            Ok(path) => util::console_out(&path.display().to_string()),
                            Err(err) => util::console_err(&format!("Download failed: {err:#}")),
                        }
                    } else {
                        util::console_err("No results");
                    }
                }
                Err(err) => util::console_err(&format!("MotionBGS search failed: {err:#}")),
            }
        }
        "set-wallpaper" => {
            let path = args.get(1).cloned().unwrap_or_default();
            if path.is_empty() {
                util::console_err("usage: aura set-wallpaper <image>");
                return;
            }
            let index = args.get(2).and_then(|value| value.parse::<usize>().ok());
            match crate::platform::static_wallpaper::set_wallpaper(
                index,
                std::path::Path::new(&path),
                crate::config::Scaling::Fill,
            ) {
                Ok(()) => util::console_out("Static wallpaper applied."),
                Err(err) => util::console_err(&format!("SetWallpaper failed: {err:#}")),
            }
        }
        "apply" => {
            let path = args.get(1).cloned().unwrap_or_default();
            if path.is_empty() {
                util::console_err("usage: aura apply <file> [--monitor \\\\.\\DISPLAY1]");
                return;
            }
            let monitor = args
                .iter()
                .position(|a| a == "--monitor")
                .and_then(|i| args.get(i + 1))
                .cloned()
                .unwrap_or_else(|| "*".into());
            forward(&format!("apply|{path}|{monitor}"));
        }
        "quit" => {
            if ipc::send("quit") {
                util::console_out("Aura Engine is shutting down.");
            } else {
                util::console_out("Aura Engine is not running.");
            }
        }
        command => forward(command),
    }
}

fn forward(command: &str) {
    if ipc::send(command) {
        util::console_out(&format!("Sent '{command}' to Aura Engine."));
        return;
    }

    // Nothing running: start a hidden instance with the requested action.
    let exe = win::exe_path();
    util::console_out("Starting Aura Engine...");
    util::spawn_detached(
        &exe,
        &[
            "--hidden".to_string(),
            "--action".to_string(),
            command.to_string(),
        ],
        false,
    );
}

fn parse_motionbgs_args(args: &[String]) -> (String, crate::online::MotionBgsCategory) {
    let mut category = crate::online::MotionBgsCategory::All;
    let mut query = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--category" || arg == "-c" {
            if let Some(id) = iter.next() {
                category = crate::online::MotionBgsCategory::from_id(id)
                    .unwrap_or(crate::online::MotionBgsCategory::All);
            }
        } else {
            query.push(arg.clone());
        }
    }
    (query.join(" "), category)
}

fn install_mpv() {
    use crate::engine::installer::{self, InstallEvent};
    util::console_out("Resolving the latest portable mpv build...");
    let (tx, rx) = crossbeam_channel::unbounded();
    std::thread::spawn(move || installer::install_mpv(tx));

    let mut last_percent = u64::MAX;
    while let Ok(event) = rx.recv() {
        match event {
            InstallEvent::Stage(stage) => util::console_out(&stage),
            InstallEvent::Progress { downloaded, total } => {
                if total > 0 {
                    let percent = downloaded * 100 / total;
                    if percent != last_percent && percent % 5 == 0 {
                        util::console_out(&format!("  {percent}%"));
                        last_percent = percent;
                    }
                }
            }
            InstallEvent::Done(path) => {
                util::console_out(&format!("Installed: {}", path.display()));
                return;
            }
            InstallEvent::Failed(error) => {
                util::console_err(&format!("Installation failed: {error}"));
                return;
            }
        }
    }
}

fn print_status() {
    let cfg = Config::load();
    let running = ipc::exists();
    util::console_out(&format!(
        "Aura Engine {}\nInstance: {}\nConfig: {}\nData: {}\nmpv: {}\n",
        env!("CARGO_PKG_VERSION"),
        if running { "running" } else { "stopped" },
        util::config_path().display(),
        util::data_root().display(),
        mpv::resolve_mpv(cfg.mpv_path.as_deref())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "not installed".into())
    ));

    if !cfg.assignments.is_empty() {
        util::console_out("Assignments:");
        for (key, assignment) in &cfg.assignments {
            util::console_out(&format!(
                "  {key} -> {} ({:?}, {:?})",
                assignment.path.display(),
                assignment.kind,
                assignment.scaling
            ));
        }
    }
}

fn print_doctor() {
    let cfg = Config::load();
    let monitors = win::monitors();
    let (parent, parent_ok) = host::describe_parent(ParentMode::Auto);

    let mut report = String::new();
    report.push_str(&format!("Aura Engine {}\n", env!("CARGO_PKG_VERSION")));
    report.push_str(&format!("Windows: {}\n", std::env::consts::OS));
    report.push_str(&format!("Config: {}\n", util::config_path().display()));
    report.push_str(&format!("Data: {}\n", util::data_root().display()));
    report.push_str(&format!("Logs: {}\n", util::logs_dir().display()));
    report.push_str(&format!(
        "Single instance window: {}\n",
        if ipc::exists() { "present" } else { "absent" }
    ));
    report.push_str(&format!(
        "Desktop parent: {parent} ({})\n",
        if parent_ok { "ok" } else { "not found" }
    ));
    report.push_str(&format!(
        "mpv: {}\n",
        mpv::resolve_mpv(cfg.mpv_path.as_deref())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "not installed".into())
    ));
    if let Some(path) = mpv::resolve_mpv(cfg.mpv_path.as_deref()) {
        report.push_str(&format!(
            "mpv version: {}\n",
            mpv::version(&path).unwrap_or_else(|| "unknown".into())
        ));
    }
    report.push_str(&format!(
        "Autostart: {}\nMonitors: {}\n",
        if win::autostart_enabled() { "enabled" } else { "disabled" },
        monitors.len()
    ));
    for monitor in &monitors {
        report.push_str(&format!(
            "  {} | {} | {}x{} @ {}dpi | {}\n",
            monitor.key,
            monitor.label(),
            monitor.rect.width(),
            monitor.rect.height(),
            monitor.dpi,
            if monitor.primary { "primary" } else { "secondary" }
        ));
    }
    report.push_str("Desktop layers:\n");
    report.push_str(&host::describe_desktop());
    util::console_out(report.trim_end());
}

fn print_help() {
    util::console_out(
        r#"Aura Engine - native live wallpaper manager for Windows

USAGE:
  aura [command]
  aura <file> [--monitor \\.\DISPLAY1]

COMMANDS:
  next             Apply the next wallpaper in the library
  prev             Apply the previous wallpaper
  apply <file>     Apply a video or image
  pause            Pause playback
  resume           Resume playback
  toggle-pause     Toggle playback
  stop             Stop and restore the original wallpaper
  switcher         Open the quick switcher HUD
  show             Show the main window
  quit             Exit Aura Engine
  status           Print the current state
  doctor           Print diagnostics (monitors, mpv, desktop layer)
  motionbgs <categoria>       Abre Explorar en una categoria de MotionBGS
  search-motionbgs <query> [--category id]    Busca en MotionBGS
  download-motionbgs <query> [--category id]  Descarga el primer resultado
                              ids: all, anime, games, 4k, superhero, nature
  --version        Print version
  --help           Print this help

SHORTCUTS:
  Ctrl+Alt+W       Quick switcher
  Ctrl+Alt+Right   Next wallpaper
  Ctrl+Alt+Left    Previous wallpaper
  Ctrl+Alt+Space   Pause / resume
  Ctrl+Alt+S       Stop wallpaper
"#,
    );
}
