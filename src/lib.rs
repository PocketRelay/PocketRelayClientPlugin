#![allow(clippy::missing_safety_doc)]

use config::read_config_file;
use windows_sys::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};

pub mod api;
pub mod config;
pub mod constants;
pub mod hooks;
pub mod interface;
pub mod pattern;
pub mod servers;

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
                // Create tokio async runtime
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("Failed building the Runtime");

                let config = runtime.block_on(read_config_file());

                let handle = runtime.handle().clone();

                // Initialize the UI
                interface::init(handle, config);

                // Block for CTRL+C to keep servers alive when window closes
                let shutdown_signal = tokio::signal::ctrl_c();
                let _ = runtime.block_on(shutdown_signal);
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
