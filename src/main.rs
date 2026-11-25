#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rodon::core::Core;
use std::sync::Arc;

fn main() {
    let raw: Vec<_> = std::env::args_os().collect();
    if raw.get(1).is_some_and(|s| s == "--finish-rodon-update") {
        if let Err(e) = rodon::services::updater::finish_update(&raw[2..]) {
            rodon::ui::fatal_dialog(&format!("Update failed: {e}"));
        }
        return;
    }

    let mut args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{} {}", rodon::APP_NAME, rodon::VERSION);
        return;
    }

    let smoke = args.iter().any(|a| a == "--smoke-test");
    let fixture = if smoke { Some(tempfile::tempdir().expect("smoke profile")) } else { None };
    let data_override = fixture.as_ref().map(|d| d.path().display().to_string()).or_else(|| rodon::paths::take_data_dir_arg(&mut args));
    let data_dir = rodon::paths::init_data_dir(data_override).to_path_buf();

    if smoke {
        let settings = rodon::store::Settings {
            check_updates: false,
            validate_cookies_on_startup: false,
            window_grid_hotkey_enabled: false,
            splash: false,
            ..Default::default()
        };
        settings.save(&data_dir).expect("smoke preferences");
    }
    rodon::logging::init(rodon::store::Settings::load(&data_dir).log_retention);

    // Headless MCP server for AI tools (Claude, Codex) spawned over stdio.
    if args.iter().any(|a| a == "--mcp") {
        match open_core(&data_dir, mcp_password()) {
            Ok(core) => rodon::api::mcp::serve_stdio(core),
            Err(err) => eprintln!("Rodon could not open its vault: {err}"),
        }
        return;
    }

    let core = match open_core(&data_dir, None) {
        Ok(core) => core,
        Err(err) if err.code == "VAULT_LOCKED" => match rodon::ui::unlock_prompt(&data_dir) {
            Some(core) => core,
            None => return,
        },
        Err(err) => {
            rodon::log_error!("Startup failed: {err}");
            rodon::ui::fatal_dialog(&err.message);
            return;
        }
    };

    let services = rodon::services::Services::new(Arc::clone(&core));
    if let Err(err) = rodon::ui::run(core, services, smoke) {
        rodon::log_error!("UI exited with error: {err}");
        if smoke {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
    if smoke {
        rodon::logging::close();
        if !rodon::ui::SMOKE_PASSED.load(std::sync::atomic::Ordering::Acquire) {
            std::process::exit(1);
        }
    }
    if rodon::paths::WIPE_ON_EXIT.load(std::sync::atomic::Ordering::Acquire) {
        rodon::logging::close();
        if let Err(e) = rodon::paths::wipe(&data_dir) {
            rodon::ui::fatal_dialog(&format!("Removing local data failed: {e}"));
        }
    }
}

fn open_core(data_dir: &std::path::Path, password: Option<String>) -> rodon::error::AppResult<Arc<Core>> {
    Core::open(data_dir, password.as_deref())
}

/// A password for the MCP server's vault, from `RODON_VAULT_PASSWORD` if set.
fn mcp_password() -> Option<String> {
    std::env::var("RODON_VAULT_PASSWORD").ok().filter(|p| !p.is_empty())
}
