use crate::{
    core::servers::has_server_tasks,
    hooks::mem::{find_pattern, use_memory},
};
use log::{debug, warn};
use std::{
    ffi::CStr,
    ptr::{addr_of_mut, null_mut},
};
use windows_sys::{
    core::PCSTR,
    Win32::Networking::WinSock::{gethostbyname, HOSTENT},
};

/// Address to start matching from
const HOST_LOOKUP_START_OFFSET: usize = 0x401000;
/// Address to end matching at
const HOST_LOOKUP_END_OFFSET: usize = 0xFFFFFF;
/// Mask to use while matching the opcodes below
const HOST_LOOKUP_MASK: &str = "x????xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
/// Op codes to match against
const HOST_LOOKUP_OP_CODES: &[u8] = &[
    0xE8, 0x8B, 0x9F, 0xF8, 0xFF, // call <JMP.&gethostbyname>
    0x85, 0xC0, // test eax,eax
    0x74, 0x2E, // je me3c.F652E7
    0x8B, 0x48, 0x0C, // mov ecx,dword ptr ds:[eax+C]
    0x8B, 0x01, // mov eax,dword ptr ds:[ecx]
    0x0F, 0xB6, 0x10, // movzx edx,byte ptr ds:[eax]
    0x0F, 0xB6, 0x48, 0x01, // movzx ecx,byte ptr ds:[eax+1]
    0xC1, 0xE2, 0x08, // shl edx,8
    0x0B, 0xD1, // or edx,ecx
    0x0F, 0xB6, 0x48, 0x02, // movzx ecx,byte ptr ds:[eax+2]
    0x0F, 0xB6, 0x40, 0x03, // movzx eax,byte ptr ds:[eax+3]
    0xC1, 0xE2, 0x08, // shl edx,8
    0x0B, 0xD1, // or edx,ecx
    0xC1, 0xE2, 0x08, // shl edx,8
    0x0B, 0xD0, // or edx,eax
    0x89, 0x56, 0x04, // mov dword ptr ds:[esi+4],edx
    0xC7, 0x06, 0x01, 0x00, 0x00, 0x00, // mov dword ptr ds:[esi],1
];

/// Static memory region for the host name bytes
static mut HOST_BYTES: [u8; 21] = *b"gosredirector.ea.com\0";
/// Static memory region storing the address bytes
static mut ADDRESS_BYTES: [i8; 5] = [127, 0, 0, 1, 0];
/// Static null terminated addresses array
static mut ADDRESSES_ARRAY: [*mut i8; 2] = [unsafe { ADDRESS_BYTES.as_mut_ptr() }, null_mut()];
/// Static HOSTENT structure
static mut HOST_ENT: HOSTENT = unsafe {
    HOSTENT {
        h_name: HOST_BYTES.as_mut_ptr(),
        h_aliases: null_mut(), /* Null aliases */
        h_addrtype: 2,         /* IPv4 addresses */
        h_length: 4,           /* 4 bytes for IPv4 */
        h_addr_list: ADDRESSES_ARRAY.as_mut_ptr(),
    }
};

/// Function used to override the normal functionality for `gethostbyname` and
/// replace lookups for gosredirector.ea.com with localhost redirects
///
/// ## Safety
///
/// This function safely passes memory to the os implementation of this function
/// only using a different pointer when required so it is considered safe
#[no_mangle]
pub unsafe extern "system" fn fake_gethostbyname(name: PCSTR) -> *mut HOSTENT {
    // Derive the safe name from the str bytes
    let str_name = CStr::from_ptr(name.cast());

    debug!("Got host lookup request: {:?}", str_name);

    // Only handle gosredirector.ea.com domains and don't use the override unless
    // there is running server tasks
    if str_name.to_bytes() == b"gosredirector.ea.com" && has_server_tasks() {
        debug!("Responding with localhost redirect");
        return addr_of_mut!(HOST_ENT);
    }

    // Use the actual function
    gethostbyname(name)
}

/// This hook is applied to the `gethostbyname` function within the game in order
/// to intercept IP address lookups for different domain names, allowing the client
/// to replace them with references to 127.0.0.1 instead
///
/// ## Safety
///
/// Reading program memory is *NOT* safe but its required for pattern matching, this
/// function mutates memory to replace function calls
pub unsafe fn hook_host_lookup() {
    let Some(addr) = find_pattern(
        HOST_LOOKUP_START_OFFSET,
        HOST_LOOKUP_END_OFFSET,
        HOST_LOOKUP_MASK,
        HOST_LOOKUP_OP_CODES,
    ) else {
        warn!("Failed to find gethostbyname hook position");
        return;
    };

    debug!("Found gethostbyname @ {:#016x}", addr as usize);

    // Initial -> f652b0

    // == Obtain the address from the call ????
    // call ???? (Obtain the relative call distance)
    let distance = *(addr.add(1 /* Skip call opcode */) as *const usize);

    // Relative jump -> EEF240 (jump to jmp in thunk table)
    let jmp_address = addr.add(5 /* Skip call opcode + address */ + distance);

    // == Address to the final ptr
    // jmp dword ptr ds:[????]
    let address = *(jmp_address.add(2 /* Skip ptr jmp opcode */) as *const usize);

    // Final pointer from the resolved address
    let addr = address as *const u8;

    use_memory(addr, 4, |addr| {
        // Replace the address with our faker function
        let ptr: *mut usize = addr as *mut usize;
        *ptr = fake_gethostbyname as usize;
    });
}
