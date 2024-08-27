pub mod host_lookup;
pub mod mem;
pub mod process_event;

/// Applies all hooks
#[allow(clippy::missing_safety_doc)]
pub unsafe fn apply_hooks() {
    host_lookup::hook_host_lookup();
    process_event::hook_process_event();
}
