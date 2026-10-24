#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod cli;
mod config;
mod engine;
mod hotkeys;
mod i18n;
mod online;
mod platform;
mod scanner;
mod theme;
mod tray;
mod ui;
mod util;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    util::init_logging();
    log::info!("starting pid={} args={args:?}", std::process::id());
    platform::win::init_process();

    if cli::is_cli_invocation(&args) {
        cli::run(&args);
        return;
    }

    let hidden = args
        .iter()
        .any(|a| a == "--hidden" || a == "--autostart");
    let action = cli::extract_action(&args);

    // A second launch forwards its command to the running instance.
    let command = action.clone().unwrap_or_else(|| "show".to_string());
    if platform::ipc::send(&command) {
        return;
    }

    platform::ipc::start_server();

    if let Err(err) = app::run(hidden, action) {
        log::error!("fatal: {err:#}");
        util::console_err(&format!("Aura Engine failed to start: {err:#}"));
    }
}
