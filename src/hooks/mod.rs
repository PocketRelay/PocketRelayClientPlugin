use log::debug;

pub mod host_lookup;
pub mod mem;
pub mod process_event;

/// Applies all hooks
#[allow(clippy::missing_safety_doc)]
pub unsafe fn apply_hooks() {
    debug!("apply host lookup");
    host_lookup::hook_host_lookup();
    debug!("apply process event hook");
    process_event::hook_process_event();
    debug!("all hooks applied")
}
