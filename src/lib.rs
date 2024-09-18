#![warn(unused_crate_dependencies)]

use config::read_config_file;
use core::{
    api::{create_http_client, read_client_identity},
    reqwest::{Client, Identity},
};
use log::error;
use pocket_relay_client_shared as core;
use std::{path::Path, sync::Mutex};
use ui::{confirm_message, error_message};
use windows_sys::Win32::{
    Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
        },
        SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH},
        Threading::{
            GetCurrentProcessId, GetCurrentThreadId, OpenThread, ResumeThread, SuspendThread,
            THREAD_QUERY_INFORMATION, THREAD_SUSPEND_RESUME,
        },
    },
};

pub mod config;
pub mod game;
pub mod hooks;
pub mod servers;
pub mod ui;
pub mod update;

/// Constant storing the application version
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Handles the plugin being attached to the game
fn attach() {
    // Suspend all game threads so the user has a chance to connect to a server
    unsafe { suspend_all_threads() };

    // Debug allocates a console window to display output
    #[cfg(debug_assertions)]
    {
        unsafe { windows_sys::Win32::System::Console::AllocConsole() };
    }

    // Initialize logging
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

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

// Threads that were suspended
static SUSPENDED_THREADS: Mutex<Vec<u32>> = Mutex::new(Vec::new());

unsafe fn suspend_all_threads() {
    let current_thread_id = GetCurrentThreadId();
    let target_process_id = GetCurrentProcessId();

    let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
    if snapshot == INVALID_HANDLE_VALUE {
        return;
    }

    let mut thread_entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    thread_entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;

    // Read the first thread entry
    if Thread32First(snapshot, &mut thread_entry) == 0 {
        return;
    }

    let mut suspended_threads = Vec::new();

    loop {
        // Suspend threads that aren't the current thread
        if thread_entry.th32OwnerProcessID == target_process_id
            && thread_entry.th32ThreadID != current_thread_id
        {
            let thread_handle = unsafe {
                OpenThread(
                    THREAD_SUSPEND_RESUME | THREAD_QUERY_INFORMATION,
                    0,
                    thread_entry.th32ThreadID,
                )
            };

            if thread_handle != 0 {
                SuspendThread(thread_handle);
                CloseHandle(thread_handle);

                suspended_threads.push(thread_entry.th32ThreadID);
            }
        }

        // Read the next thread
        if Thread32Next(snapshot, &mut thread_entry) == 0 {
            break;
        }
    }

    CloseHandle(snapshot);

    // Store the threads we suspended
    if let Ok(mut value) = SUSPENDED_THREADS.lock() {
        *value = suspended_threads;
    }
}

unsafe fn resume_all_threads() {
    // Get the suspended threads
    let suspended_threads = match SUSPENDED_THREADS.lock() {
        Ok(mut value) => value.split_off(0),
        Err(_) => return,
    };

    // Resume the threads that were suspended
    for thread_id in suspended_threads {
        let thread_handle = OpenThread(
            THREAD_SUSPEND_RESUME | THREAD_QUERY_INFORMATION,
            0,
            thread_id,
        );

        if thread_handle != 0 {
            ResumeThread(thread_handle);
            CloseHandle(thread_handle);
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
