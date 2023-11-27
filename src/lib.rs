#![allow(clippy::missing_safety_doc)]

use std::path::Path;

use config::read_config_file;
use log::error;
use native_windows_gui::error_message;
use pocket_relay_client_shared::{
    api::{create_http_client, read_client_identity},
    reqwest::{Client, Identity},
};
use ui::show_confirm;
use windows_sys::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};

pub mod config;
pub mod constants;
pub mod hooks;
pub mod pattern;
pub mod servers;
pub mod ui;
pub mod update;

#[no_mangle]
#[allow(non_snake_case, unused_variables)]
unsafe extern "system" fn DllMain(dll_module: usize, call_reason: u32, _: *mut ()) -> bool {
    match call_reason {
        DLL_PROCESS_ATTACH => {
            #[cfg(debug_assertions)]
            {
                use windows_sys::Win32::System::Console::AllocConsole;
                AllocConsole();
            }

            env_logger::builder()
                .filter_level(log::LevelFilter::Debug)
                .init();

            // Handles the DLL being attached to the game
            unsafe { hooks::hook() };

            // Spawn UI and prepare task set
            std::thread::spawn(|| {
                let config = read_config_file();

                // Load the client identity
                let mut identity: Option<Identity> = None;
                let identity_file = Path::new("pocket-relay-identity.p12");
                if identity_file.exists() && identity_file.is_file()  
                 && show_confirm(
                    "Found client identity",
                    "Detected client identity pocket-relay-identity.p12, would you like to use this identity?",
                )
                {
                    identity = match read_client_identity(identity_file) {
                        Ok(value) => Some(value),
                        Err(err) => {
                            error!("Failed to set client identity: {}", err);
                            error_message("Failed to set client identity", &err.to_string());
                            None
                        }
                    };
                }

                let client: Client =
                    create_http_client(identity).expect("Failed to create HTTP client");

                // Initialize the UI
                ui::init(config, client);
            });
        }
        DLL_PROCESS_DETACH => {
            #[cfg(debug_assertions)]
            {
                use windows_sys::Win32::System::Console::FreeConsole;
                FreeConsole();
            }
        }
        _ => {}
    }

    true
}
