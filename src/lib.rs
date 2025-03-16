#![warn(unused_crate_dependencies)]

use config::read_config_file;
use core::{
    api::{create_http_client, read_client_identity},
    reqwest::{Client, Identity},
};
use log::error;
use pocket_relay_client_shared as core;
use std::path::Path;
use ui::{confirm_message, error_message};
use windows_sys::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};

pub mod config;
pub mod game;
pub mod hooks;
pub mod servers;
pub mod threads;
pub mod ui;
pub mod update;

/// Constant storing the application version
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Handles the plugin being attached to the game
fn attach() {
    // Suspend all game threads so the user has a chance to connect to a server
    threads::suspend_all_threads();

    // Debug allocates a console window to display output
    #[cfg(debug_assertions)]
    {
        unsafe { windows_sys::Win32::System::Console::AllocConsole() };
    }

    // Initialize logging
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    log_panics::init();

    // Apply hooks
    unsafe { hooks::apply_hooks() };

    // Load the config file
    let config = read_config_file();

    // Load the client identity if one is present
    let identity = load_identity();

    // Create the internal HTTP client
    let client: Client = create_http_client(identity).expect("Failed to create HTTP client");

    std::thread::spawn(|| {
        // Initialize the UI
        ui::init(config, client);
    });
}

/// Handles the plugin being detached from the game, this handles
/// cleaning up any extra allocated resources
fn detach() {
    // Debug console must be freed on detach
    #[cfg(debug_assertions)]
    {
        unsafe {
            windows_sys::Win32::System::Console::FreeConsole();
        }
    }
}

/// Attempts to load an identity file if one is present
fn load_identity() -> Option<Identity> {
    // Load the client identity
    let identity_file = Path::new("pocket-relay-identity.p12");

    // Handle no identity or user declining identity
    if !identity_file.exists() || !confirm_message(
        "Found client identity",
        "Detected client identity pocket-relay-identity.p12, would you like to use this identity?",
    ) {
        return None;
    }

    // Read the client identity
    match read_client_identity(identity_file) {
        Ok(value) => Some(value),
        Err(err) => {
            error!("Failed to set client identity: {}", err);
            error_message("Failed to set client identity", &err.to_string());
            None
        }
    }
}

/// Windows DLL entrypoint for the plugin
#[no_mangle]
#[allow(non_snake_case)]
extern "stdcall" fn DllMain(_hmodule: isize, reason: u32, _: *mut ()) -> bool {
    match reason {
        // Handle attaching
        DLL_PROCESS_ATTACH => attach(),
        // Handle detaching
        DLL_PROCESS_DETACH => detach(),
        _ => {}
    }

    true
}
