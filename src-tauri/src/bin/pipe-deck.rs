use pipe_deck_lib::config::ConfigStore;
use pipe_deck_lib::core::engine::CoreEngine;
use serde_json::json;
use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            if error.starts_with("pipewire") || error.contains("routing") {
                ExitCode::from(2)
            } else {
                ExitCode::from(1)
            }
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_help();
        return Ok(());
    }

    // Deliberately handled before the generic engine setup below: cleanup
    // must still tear down what it can even if `refresh_graph()` would fail
    // (e.g. PipeWire already partway through being removed alongside the
    // package) — nothing it does depends on a fresh graph.
    if args[0] == "cleanup" {
        return handle_cleanup(&args[1..]);
    }

    let mut engine = CoreEngine::new();
    ConfigStore::new()
        .ensure_layout()
        .map_err(|error| error.to_string())?;
    let _ = pipe_deck_lib::core::rules::ensure_rules_migrated();
    engine.initialize_plugins();
    engine.refresh_graph().map_err(|error| error.to_string())?;

    match args[0].as_str() {
        "graph" => {
            let graph = engine.runtime_graph().clone();
            print_json(&graph)?;
        }
        "route" => handle_route(&mut engine, &args[1..])?,
        "profile" => handle_profile(&mut engine, &args[1..])?,
        "rules" => handle_rules(&mut engine, &args[1..])?,
        "plugins" => handle_plugins(&engine, &args[1..])?,
        other => return Err(format!("unknown command: {other}")),
    }

    Ok(())
}

/// Tears down everything Pipe Deck can leave behind past a package removal
/// (issue #169): live `pipe-deck-*` pactl modules, the background-restore
/// systemd unit, and stray effects drop-ins — always. With `--purge-config`,
/// also deletes the config directory and daemon state directory. Intended
/// to be run once before/instead of removing the package itself (see
/// `docs/project/Uninstall.md`), not wired to a package-manager hook
/// automatically — Tauri's bundler has no postrm/prerm hook mechanism today.
fn handle_cleanup(args: &[String]) -> Result<(), String> {
    let purge_config = args.iter().any(|arg| arg == "--purge-config");

    let engine = CoreEngine::new();
    let (removed_devices, device_errors) = engine.remove_all_virtual_devices();

    let removed_unit = pipe_deck_lib::daemon::uninstall_user_service_unit()?;

    let mut errors = device_errors;
    if let Err(error) = pipe_deck_lib::pipewire::filter_chain::cleanup_effects_conf_files() {
        errors.push(format!("failed to remove effects drop-ins: {error}"));
    }

    let mut purged_dirs = Vec::new();
    if purge_config {
        let config_dir = ConfigStore::new().config_dir().clone();
        for dir in [config_dir, pipe_deck_lib::daemon::state_dir()] {
            if !dir.exists() {
                continue;
            }
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => purged_dirs.push(dir.display().to_string()),
                Err(error) => errors.push(format!("failed to remove {}: {error}", dir.display())),
            }
        }
    }

    print_json(&json!({
        "removed_virtual_devices": removed_devices,
        "removed_daemon_unit": removed_unit.map(|path| path.display().to_string()),
        "purged_directories": purged_dirs,
        "errors": errors,
    }))?;

    if !errors.is_empty() {
        return Err(format!("{} cleanup step(s) failed — see errors above", errors.len()));
    }
    Ok(())
}

fn handle_route(engine: &mut CoreEngine, args: &[String]) -> Result<(), String> {
    if args.len() < 2 || args[0] != "set" {
        return Err("usage: pipe-deck route set --stream <id> --targets a,b".into());
    }
    let mut stream_id = None;
    let mut targets = Vec::new();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--stream" => {
                stream_id = args.get(index + 1).cloned();
                index += 2;
            }
            "--targets" | "--target" => {
                let value = args.get(index + 1).cloned().ok_or("missing targets")?;
                targets = value.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
                index += 2;
            }
            _ => index += 1,
        }
    }
    let stream_id = stream_id.ok_or("missing --stream")?;
    if targets.is_empty() {
        return Err("missing --targets".into());
    }
    let result = engine
        .set_stream_targets(&stream_id, &targets)
        .map_err(|error| error.to_string())?;
    print_json(&result)?;
    Ok(())
}

fn handle_profile(engine: &mut CoreEngine, args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err("usage: pipe-deck profile list|swap|save".into());
    }
    match args[0].as_str() {
        "list" => {
            let store = ConfigStore::new();
            let config = store.load_config().map_err(|error| error.to_string())?;
            print_json(&config.profile_index)?;
        }
        "swap" => {
            let profile_id = args.get(1).ok_or("usage: pipe-deck profile swap <id>")?;
            let result = engine
                .swap_profile(profile_id)
                .map_err(|error| error.to_string())?;
            print_json(&result)?;
        }
        "save" => {
            let name = args
                .iter()
                .position(|arg| arg == "--name")
                .and_then(|index| args.get(index + 1))
                .map(String::as_str)
                .unwrap_or("CLI Snapshot");
            let profile_id = name.to_lowercase().replace(' ', "-");
            let result = engine
                .save_profile_as(&profile_id, name)
                .map_err(|error| error.to_string())?;
            print_json(&result)?;
        }
        other => return Err(format!("unknown profile subcommand: {other}")),
    }
    Ok(())
}

fn handle_rules(engine: &mut CoreEngine, args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err("usage: pipe-deck rules list|simulate|apply".into());
    }
    match args[0].as_str() {
        "list" => {
            let store = ConfigStore::new();
            let config = store.load_config().map_err(|error| error.to_string())?;
            print_json(&config.rules)?;
        }
        "simulate" => {
            let results = engine.simulate_rules();
            print_json(&results)?;
        }
        "apply" => {
            engine
                .apply_desired_routing()
                .map_err(|error| error.to_string())?;
            println!("Rules applied.");
        }
        other => return Err(format!("unknown rules subcommand: {other}")),
    }
    Ok(())
}

fn handle_plugins(engine: &CoreEngine, args: &[String]) -> Result<(), String> {
    if args.is_empty() || args[0] == "list" {
        print_json(&engine.list_plugins())?;
        return Ok(());
    }
    if args[0] == "status" {
        print_json(&json!({ "plugins": engine.list_plugins() }))?;
        return Ok(());
    }
    Err("usage: pipe-deck plugins list|status".into())
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(&mut handle, value).map_err(|error| error.to_string())?;
    writeln!(handle).map_err(|error| error.to_string())?;
    Ok(())
}

fn print_help() {
    println!(
        "pipe-deck — Linux Audio Control Center CLI\n\n\
Commands:\n  \
graph                 Print runtime graph as JSON\n  \
route set --stream ID --targets a,b\n  \
profile list|swap <id>|save [--name NAME]\n  \
rules list|simulate|apply\n  \
plugins list|status\n  \
cleanup [--purge-config]   Unload virtual devices + remove the daemon unit;\n                             \
                           --purge-config also deletes config/state dirs\n"
    );
}
