#![allow(clippy::missing_safety_doc)]

use config::read_config_file;
use core::{
    api::{create_http_client, read_client_identity},
    reqwest::{Client, Identity},
};
use log::error;
use native_windows_gui::error_message;
pub use pocket_relay_client_shared as core;
use std::path::Path;
use ui::show_confirm;
use windows_sys::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};

pub mod config;
pub mod hooks;
pub mod pattern;
pub mod servers;
pub mod ui;
pub mod update;

/// Constant storing the application version
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Handles the plugin being attached to the game
fn attach() {
    // Debug allocates a console window to display output
    #[cfg(debug_assertions)]
    {
        unsafe { windows_sys::Win32::System::Console::AllocConsole() };
    }

    // Initialize logging
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    // Apply the hooks
    unsafe { hooks::hook() };

    // Load the config file
    let config = read_config_file();

    // Load the client identity if one is present
    let identity = load_identity();

    // Create the internal HTTP client
    let client: Client = create_http_client(identity).expect("Failed to create HTTP client");

    // Start the UI in a new thread
    std::thread::spawn(move || {
        // Initialize the UI
        ui::init(config, client);
    });
}

/// Handles the plugin being detatched from the game, this handles
/// cleaning up any extra allocated resources
fn detatch() {
    // Debug console must be freed on detatch
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
    if identity_file.exists()
        && identity_file.is_file()
        && show_confirm(
          "Found client identity",
          "Detected client identity pocket-relay-identity.p12, would you like to use this identity?",
        )
    {
        match read_client_identity(identity_file) {
            Ok(value) => Some(value),
            Err(err) => {
                error!("Failed to set client identity: {}", err);
                error_message("Failed to set client identity", &err.to_string());
                None
            }
        }
    } else {
        None
    }
}

#[no_mangle]
#[allow(non_snake_case, unused_variables)]
unsafe extern "system" fn DllMain(dll_module: usize, call_reason: u32, _: *mut ()) -> bool {
    match call_reason {
        DLL_PROCESS_ATTACH => attach(),
        DLL_PROCESS_DETACH => detatch(),
        _ => {}
    }

    true
}
