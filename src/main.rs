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
mod update;
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

    // A second launch forwards its command to the running instance. The
    // single-instance mutex makes the common "first launch" path skip the
    // pipe probe completely instead of waiting before showing the window.
    let command = action.clone().unwrap_or_else(|| "show".to_string());
    if platform::win::is_primary_instance() {
        platform::ipc::start_server();
    } else if platform::ipc::send(&command) {
        return;
    } else {
        // The previous instance disappeared between the mutex check and the
        // pipe connection; become the server ourselves.
        platform::ipc::start_server();
    }

    if let Err(err) = app::run(hidden, action) {
        log::error!("fatal: {err:#}");
        util::console_err(&format!("Aura Engine failed to start: {err:#}"));
    }
}
