//! # Threads
//!
//! Logic for managing threads, in this case it handles pausing and
//! resuming process threads on startup. This is what allows the user
//! to connect to a server before the game properly starts

use std::{mem::swap, sync::Mutex};
use windows_sys::Win32::{
    Foundation::{CloseHandle, FALSE, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
        },
        Threading::{
            GetCurrentProcessId, GetCurrentThreadId, OpenThread, ResumeThread, SuspendThread,
            THREAD_QUERY_INFORMATION, THREAD_SUSPEND_RESUME,
        },
    },
};

// Threads that were suspended
static SUSPENDED_THREADS: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// Suspends all threads on the process excluding the current thread. Suspended
/// threads are stored in [SUSPENDED_THREADS] and can be later resumed with
/// [resume_all_threads].
///
/// Should only be called on initial startup to prevent interrupting any network
/// connection threads.
pub fn suspend_all_threads() {
    let (current_thread_id, target_process_id) =
        unsafe { (GetCurrentThreadId(), GetCurrentProcessId()) };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return;
    }

    let mut thread_entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    thread_entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;

    // Read the first thread entry
    if unsafe { Thread32First(snapshot, &mut thread_entry) } == FALSE {
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
                    FALSE,
                    thread_entry.th32ThreadID,
                )
            };

            if thread_handle != 0 {
                unsafe {
                    SuspendThread(thread_handle);
                    CloseHandle(thread_handle);
                }

                suspended_threads.push(thread_entry.th32ThreadID);
            }
        }

        // Read the next thread
        if unsafe { Thread32Next(snapshot, &mut thread_entry) } == FALSE {
            break;
        }
    }

    unsafe {
        CloseHandle(snapshot);
    }

    // Store the threads we suspended
    if let Ok(mut value) = SUSPENDED_THREADS.lock() {
        *value = suspended_threads;
    }
}

/// Resumes all suspended threads
pub fn resume_all_threads() {
    // Get the suspended threads
    let suspended_threads = match SUSPENDED_THREADS.lock() {
        // Take the collection of locked threads
        Ok(mut value) => {
            // Swap the allocated threads list with an unallocated vec
            //
            // Reason: Allows us to take the allocated capacity so it doesn't
            // maintain it for the life of the program like split_off would
            let mut threads = Vec::new();
            swap(value.as_mut(), &mut threads);

            threads
        }

        // Lock is poisoned, shouldn't have reached a reusable point if
        // the main thread crashed
        Err(_) => return,
    };

    // Resume the threads that were suspended
    for thread_id in suspended_threads {
        let thread_handle = unsafe {
            OpenThread(
                THREAD_SUSPEND_RESUME | THREAD_QUERY_INFORMATION,
                0,
                thread_id,
            )
        };

        if thread_handle != 0 {
            unsafe {
                ResumeThread(thread_handle);
                CloseHandle(thread_handle);
            }
        }
    }
}
