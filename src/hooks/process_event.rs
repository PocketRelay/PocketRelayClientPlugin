use super::mem::use_memory;
use crate::game::{
    core::{FString, UFunction, UObject, UObjectExt},
    sfxgame::{FSFXOnlineMOTDInfo, USFXOnlineComponentUI},
};
use log::debug;
use serde::{Deserialize, Serialize};
use std::os::raw::c_void;
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
};

type ProcessEvent =
    unsafe extern "thiscall" fn(*mut UObject, *mut UFunction, *mut c_void, *mut c_void);

// Original function for ProcessEvent
static mut PROCESS_EVENT_ORIGINAL: Option<ProcessEvent> = None;

/// Memory address the process event function is stored at
const PROCESS_EVENT_OFFSET: usize = 0x00453120;

/// Hooks the game [ProcessEvent] function to use [fake_process_event] instead
/// to allow processing events that occur in the game
#[allow(clippy::missing_safety_doc)]
pub unsafe fn hook_process_event() {
    const JMP: u8 =  0xE9 /* jmp */;
    const JMP_SIZE: usize = 5; // Size of a near jump instruction in x86

    let target = PROCESS_EVENT_OFFSET as *const u8 as *mut u8;
    let hook = fake_process_event as *const u8;

    let mut original_bytes: [u8; JMP_SIZE] = [0; JMP_SIZE];

    // Store the original jump instruction
    std::ptr::copy_nonoverlapping(target, original_bytes.as_mut_ptr(), original_bytes.len());

    debug!("store original jump instruction {:?}", original_bytes);

    // Determine the offset to jump to the hooked function
    let relative_offset = hook as i32 - target as i32 - JMP_SIZE as i32;

    debug!("relative offset {:#016x}", relative_offset);

    use_memory(target, JMP_SIZE, |mem| {
        // Set the jump instruction
        *mem = JMP;

        // Set the jump offset
        let jump_addr = mem.byte_add(1).cast::<i32>();
        *jump_addr = relative_offset.to_le();
    });

    // Calculate the address of the original function after the JMP instruction
    let trampoline_size = JMP_SIZE;
    let trampoline = VirtualAlloc(
        std::ptr::null_mut(),
        trampoline_size,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_EXECUTE_READWRITE,
    );

    if trampoline.is_null() {
        panic!("Failed to allocate memory for trampoline");
    }

    // Determine the offset to jump back
    let jump_back_offset = target as i32 - trampoline as i32;

    debug!("jump back offset {:#016x}", jump_back_offset);

    {
        // Write the original jump instruction to the start of the trampoline
        let mem = trampoline.cast::<u8>();
        std::ptr::copy_nonoverlapping(original_bytes.as_ptr(), mem, original_bytes.len());

        // Write the jump back from the trampoline
        let mem = mem.byte_add(JMP_SIZE);
        *mem = JMP;

        // Write the jump offset
        let jump_addr = mem.byte_add(1).cast::<i32>();
        *jump_addr = jump_back_offset.to_le();
    }

    // Save the original function pointer, adjusted to skip the JMP instruction
    PROCESS_EVENT_ORIGINAL = Some(std::mem::transmute::<*mut c_void, ProcessEvent>(trampoline));
}

/// JSON structure for a system terminal message the server can
/// send to have displayed in the in-game terminal
#[derive(Deserialize, Serialize)]
pub struct SystemTerminalMessage {
    /// Title displayed on the terminal
    title: String,
    /// Message displayed on the terminal
    message: String,
    /// Message displayed at the top of the terminal (Can be empty for a default image)
    image: String,
    /// Type of message (Where it appears)
    ty: u8,
    /// Unique tracking ID for the message can be used to replace a message
    tracking_id: i32,
    /// Priority of the message for ordering
    priority: i32,
}

/// Calls the original ProcessEvent function
///
/// # Safety
///
/// Memory for the process event call should always point to
/// the valid ProcessEvent function as long as the binary
/// offset doesn't change
pub unsafe fn process_event(
    this: *mut UObject,
    func: *mut UFunction,
    params: *mut c_void,
    result: *mut c_void,
) {
    let original_fn: ProcessEvent =
        PROCESS_EVENT_ORIGINAL.expect("Process event hook called before it was hooked");

    // Call the original function
    original_fn(this, func, params, result);
}

/// Structure of the parameters for SFXGame.SFXOnlineComponentUI.OnDisplayNotification
#[repr(C)]
#[allow(non_camel_case_types)]
struct OnDisplayNotificationParams {
    info: FSFXOnlineMOTDInfo,
}

/// Handles incoming notification display calls, adds additional logic to
/// check for special JSON payload messages send by Pocket Relay to display
/// custom messages
fn process_on_display_notification(
    this: &mut USFXOnlineComponentUI,
    params: &OnDisplayNotificationParams,
) -> bool {
    // Get the info data
    let info = &params.info;

    // Extract the message
    let original_message = &info.message.to_string();

    // Split the payload at new lines
    let lines = original_message.lines();

    // Find a system message line
    let system_message = lines
        .into_iter()
        // Find a system message line
        .find_map(|line| line.strip_prefix("[SYSTEM_TERMINAL]"));

    let system_message = match system_message {
        Some(value) => value,
        // No system message found
        None => return false,
    };

    // Parse the system message
    let message = match serde_json::from_str::<SystemTerminalMessage>(system_message) {
        Ok(value) => value,
        // Ignore malformed system message
        Err(_) => return false,
    };

    // Send custom message instead
    unsafe {
        this.event_on_display_notification(FSFXOnlineMOTDInfo {
            title: FString::from_string(message.title),
            message: FString::from_string(message.message),
            image: FString::from_string(message.image),
            tracking_id: message.tracking_id,
            priority: message.priority,
            bw_ent_id: 0,
            offer_id: 0,
            ty: message.ty,
        });
    }

    true
}

/// Hooked ProcessEvent function that allows extending the games
/// behavior by listing for specific events
///
/// # Safety
///
/// Checks are made on pointers that are used, most events are forwarded
/// directly to the original function.
#[no_mangle]
pub unsafe extern "thiscall" fn fake_process_event(
    object: *mut UObject,
    func: *mut UFunction,
    params: *mut c_void,
    result: *mut c_void,
) {
    // Ensure func is not null
    let func_ref = match func.as_ref() {
        Some(value) => value,
        None => {
            process_event(object, func, params, result);
            return;
        }
    };

    // Find the full name of the function that was called
    let name = func_ref.as_object_ref().get_full_name();

    // Hook existing display notification event code
    if name.contains("Function SFXGame.SFXOnlineComponentUI.OnDisplayNotification") {
        // Cast the types
        let this = object.cast::<USFXOnlineComponentUI>().as_mut();
        let params = params.cast::<OnDisplayNotificationParams>().as_mut();

        // Try handle a notification
        if let (Some(this), Some(params)) = (this, params) {
            if process_on_display_notification(this, params) {
                return;
            }
        }
    }

    process_event(object, func, params, result);
}
