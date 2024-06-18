use std::os::raw::c_void;

use serde::{Deserialize, Serialize};
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
};

use crate::game::{
    core::{FString, UFunction, UObject},
    sfxgame::{FSFXOnlineMOTDInfo, USFXOnlineComponentUI},
};

use super::mem::use_memory;

type ProcessEvent =
    unsafe extern "thiscall" fn(*mut UObject, *mut UFunction, *mut c_void, *mut c_void);

// Original function for ProcessEvent
static mut PROCESS_EVENT_ORIGINAL: Option<ProcessEvent> = None;

const JMP_SIZE: usize = 5; // Size of a near jump instruction in x86

/// Memory address the process event function is stored at
const PROCESS_EVENT_OFFSET: usize = 0x00453120;

/// Hooks the game [ProcessEvent] function to use [fake_process_event] instead
/// to allow processing events that occur in the game
#[allow(clippy::missing_safety_doc)]
pub unsafe fn hook_process_event() {
    const JMP: u8 =  0xE9 /* jmp */;

    let target = PROCESS_EVENT_OFFSET as *const u8 as *mut u8;
    let hook = fake_process_event as *const u8;

    let mut original_bytes: [u8; JMP_SIZE] = [0; JMP_SIZE];

    // Store the original jump instruction
    std::ptr::copy_nonoverlapping(target, original_bytes.as_mut_ptr(), JMP_SIZE);

    // Determine the offset to jump to the hooked function
    let relative_offset = hook as isize - target as isize - JMP_SIZE as isize;

    use_memory(target, JMP_SIZE, |mem| {
        // Set the jump instruction
        *mem = JMP;

        // Set the jump offset
        let jump_addr = mem.byte_add(1).cast::<isize>();
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
    let jump_back_offset = target.add(JMP_SIZE) as isize - trampoline as isize - JMP_SIZE as isize;

    {
        // Write the original jump instruction to the start of the trampoline
        let mem = trampoline.cast::<u8>();
        std::ptr::copy_nonoverlapping(original_bytes.as_ptr(), mem, JMP_SIZE);

        // Write the jump back from the trampoline
        let mem = mem.add(JMP_SIZE);
        *mem = JMP;

        // Write the jump offset
        let jump_addr = mem.byte_add(1).cast::<isize>();
        *jump_addr = jump_back_offset.to_le();
    }

    // Save the original function pointer, adjusted to skip the JMP instruction
    PROCESS_EVENT_ORIGINAL = Some(std::mem::transmute::<*mut c_void, ProcessEvent>(trampoline));
}

/// JSON structure for a system terminal message the server can
/// send to have displayed in the in-game terminal
#[derive(Deserialize, Serialize)]
pub struct SystemTerminalMessage {
    title: String,
    message: String,
    image: String,
    ty: u8,
    tracking_id: i32,
    priority: i32,
}

/// # Safety
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

#[allow(clippy::missing_safety_doc)]
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
        /// Structure of the parameters
        #[derive(Debug, Clone, Copy)]
        #[repr(C)]
        #[allow(non_camel_case_types)]
        struct Params {
            info: FSFXOnlineMOTDInfo,
        }

        let original_params = &params
            .cast::<Params>()
            .as_mut()
            .expect("OnDisplayNotification params were null")
            .info;

        let original_message = &original_params.message.to_string();

        // Handle system messages
        if let Some(message) = original_message.strip_prefix("[SYSTEM_TERMINAL]") {
            let value = serde_json::from_str::<SystemTerminalMessage>(
                // Stip all non JSON data from the end of the payload
                message.trim_end_matches(|value| value != '}'),
            );

            if let Ok(message) = value {
                // Get mutable reference to type
                let this = object
                    .cast::<USFXOnlineComponentUI>()
                    .as_mut()
                    .expect("USFXOnlineComponentUI class was null");

                // Send custom message instead
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

                return;
            }
        }
    }

    process_event(object, func, params, result);
}
