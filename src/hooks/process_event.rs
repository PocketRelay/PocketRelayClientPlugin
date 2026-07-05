use super::mem::use_memory;
use crate::{
    game::{
        core::{FString, UFunction, UObject, UObjectExt},
        sfxgame::{FSFXOnlineMOTDInfo, USFXOnlineComponentUI},
    },
    hooks::mem::find_pattern,
};
use log::{debug, warn};
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
// const PROCESS_EVENT_OFFSET: usize = 0x00453120;

/// Address to start matching from
const PROCESS_EVENT_START_OFFSET: usize = 0x401000;
/// Address to end matching at
const PROCESS_EVENT_END_OFFSET: usize = 0xFFFFFF;
/// Mask to use while matching the opcodes below
const PROCESS_EVENT_MASK: &str = "xxxxxxxxxxxx?xxxxxxxxxxxx?xxxxxxxxxxxxx?xxxxxx?x????????x?xx?x?x?x?xx?xx?xxxxxxxxxxx?xx?x?x?x?xxxxxxxx?xxxx?x?x?xx?x?x?x?xxxxxxxx?xxx?xx?xx?x?x?x?xx?xx?x?xx?x?xxxx?xxxxxxxxx?x?x";
/// Op codes to match against
const PROCESS_EVENT_OP_CODES: &[u8] = &[
    0x55, // push ebp
    0x8B, 0xEC, // mov ebp, esp
    0x6A, 0xFF, // push 0xFF
    0x68, 0xC8, 0x43, 0x1A, 0x01, // push 0x1A43C8
    0x64, 0xA1, 0x00, 0x00, 0x00, 0x00, // mov eax, [fs:0x0]
    0x50, // push eax
    0x83, 0xEC, 0x48, // sub esp, 0x48
    0xA1, 0x80, 0x5B, 0x90, 0x01, // mov eax, [0x1905B80]
    0x33, 0xC5, // xor eax, ebp
    0x89, 0x45, 0xEC, // mov [ebp-0x14], eax
    0x53, // push ebx
    0x56, // push esi
    0x57, // push edi
    0x50, // push eax
    0x8D, 0x45, 0xF4, // lea eax, [ebp-0xC]
    0x64, 0xA3, 0x00, 0x00, 0x00, 0x00, // mov [fs:0x0], eax
    0x8B, 0xF1, // mov esi, ecx
    0x89, 0x75, 0xE8, // mov [ebp-0x18], esi
    0x8B, 0x5D, 0x08, // mov ebx, [ebp+0x8]
    0xF7, 0x83, 0x88, 0x00, 0x00, 0x00, // test dword ptr [ebx+0x88], 0
    0x02, 0x04, 0x00, 0x00, // add [ebx+0x4], al
    0x0F, 0x84, 0x21, 0x02, 0x00, 0x00, // je 0x222
    0x83, 0x7B, 0x04, 0xFF, // cmp dword ptr [ebx+0x4], 0xFF
    0x75, 0x13, // jnz 0x13
    0x6A, 0x01, // push 0x1
    0x6A, 0x01, // push 0x1
    0x68, 0x30, 0x71, 0x6A, 0x01, // push 0x1A6730
    0x33, 0xC9, // xor ecx, ecx
    0x8D, 0x55, 0xE0, // lea edx, [ebp-0x20]
    0xE8, 0xC4, 0x79, 0x05, 0x00, // call 0x5A79C4
    0x8B, 0x06, // mov eax, [esi]
    0x8B, 0x50, 0x44, // mov edx, [eax+0x44]
    0x8B, 0xCE, // mov ecx, esi
    0xFF, 0xD2, // call edx
    0x85, 0xC0, // test eax, eax
    0x0F, 0x85, 0xF7, 0x01, 0x00, 0x00, // jne 0x1F7
    0x66, 0x39, 0x83, // cmp word ptr [ebx+0x83], ax
    0x8C, 0x00, 0x00, 0x00, // cmp word ptr [ebx], 0
    0x0F, 0x85, 0xEA, 0x01, 0x00, 0x00, // jne 0x1EAC
    0xF7, 0x83, 0x88, 0x00, 0x00, 0x00, // test dword ptr [ebx+0x88], 0
    0x00, 0x04, 0x00, 0x00, // add [ebx+0x4], al
    0x8B, 0x7D, 0x0C, // mov edi, [ebp+0xC]
    0x74, 0x18, // je 0x18
];

/// Hooks the game [ProcessEvent] function to use [fake_process_event] instead
/// to allow processing events that occur in the game
#[allow(clippy::missing_safety_doc)]
pub unsafe fn hook_process_event() {
    const JMP: u8 =  0xE9 /* jmp */;
    const JMP_SIZE: usize = 5; // Size of a near jump instruction in x86

    let Some(target) = find_pattern(
        PROCESS_EVENT_START_OFFSET,
        PROCESS_EVENT_END_OFFSET,
        PROCESS_EVENT_MASK,
        PROCESS_EVENT_OP_CODES,
    ) else {
        warn!("Failed to find process_event hook position");
        return;
    };

    debug!("Found ProcessEvent @ {:#016x}", target as usize);

    // let target = PROCESS_EVENT_OFFSET as *const u8 as *mut u8;
    let hook = fake_process_event as *const u8;

    let mut original_bytes: [u8; JMP_SIZE] = [0; JMP_SIZE];

    // Store the original function bytes that will be replaced with a jump
    std::ptr::copy_nonoverlapping(target, original_bytes.as_mut_ptr(), original_bytes.len());

    debug!("store original instructions {:?}", original_bytes);

    // Determine the offset to jump to the hooked function
    let relative_offset = hook as i32 - (target as i32 + JMP_SIZE as i32);

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
    let jump_back_offset = target as i32 - (trampoline as i32 + JMP_SIZE as i32);

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
