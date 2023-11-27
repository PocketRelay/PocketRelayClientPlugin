use crate::pattern::Pattern;
use log::debug;
use pocket_relay_client_shared::servers::has_server_tasks;
use std::{ffi::CStr, ptr::null_mut};
use windows_sys::{
    core::PCSTR,
    Win32::Networking::WinSock::{gethostbyname, HOSTENT},
};

const HOSTNAME_LOOKUP_PATTERN: Pattern = Pattern {
    name: "gethostbyname",
    start: 0x401000,
    end: 0xFFFFFF,
    mask: "x????xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    op: &[
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
    ],
};

pub unsafe fn hook() {
    hook_host_lookup();
}

thread_local! {
    /// Stores the thread local copy of the HOSTENT structure
    static ADDRESSES: *mut HOSTENT = unsafe { allocate_addresses() };
}

/// Creates an allocates the HOSTENT structure that is used
/// for responding to hostname requests.
///
/// ## Safety
///
/// This function WILL leak memory if used outside of a static context,
/// it should only be used in the thread local above
unsafe fn allocate_addresses() -> *mut HOSTENT {
    let ip_bytes = [127, 0, 0, 1];
    let host_bytes = b"gosredirector.ea.com\0";

    // Create the address bytes
    let address_bytes: Box<[i8]> = ip_bytes
        .iter()
        .chain(host_bytes.iter())
        .map(|byte| *byte as i8)
        .collect();

    // Leak the memory so it won't get dropped automatically
    let address_bytes = Box::leak(address_bytes);

    // Create an leak an addresses array
    let addresses: &mut [*mut i8; 2] =
        Box::leak(Box::new([address_bytes.as_mut_ptr(), null_mut()]));

    // Create and leak the host name bytes
    let host_name: Box<[u8]> = host_bytes.to_vec().into_boxed_slice();
    let host_name: &mut [u8] = Box::leak(host_name);

    // Respond with the fake result
    let result = Box::new(HOSTENT {
        h_name: host_name.as_mut_ptr(),
        h_aliases: null_mut(), /* Null aliases */
        h_addrtype: 2,         /* IPv4 addresses */
        h_length: 4,           /* 4 bytes for IPv4 */
        h_addr_list: addresses.as_mut_ptr(),
    });

    Box::leak(result)
}

#[no_mangle]
pub unsafe extern "system" fn fake_gethostbyname(name: PCSTR) -> *mut HOSTENT {
    // Resolve the name
    let str_name = CStr::from_ptr(name.cast());

    debug!("Got Host Lookup Request {}", str_name.to_string_lossy());

    // Don't redirect to local when custom server is not set
    let is_official = !has_server_tasks();

    // We are only targetting gosredirecotr for host redirects
    // forward null responses aswell
    if str_name.to_bytes() != b"gosredirector.ea.com" || is_official {
        // Obtain the actual host lookup result
        return gethostbyname(name);
    }

    debug!("Responding with localhost redirect");

    ADDRESSES.with(|addr| *addr)
}

unsafe fn hook_host_lookup() {
    Pattern::apply_with_transform(
        &HOSTNAME_LOOKUP_PATTERN,
        4,
        |addr| {
            // Initial -> f652b0

            // == Obtain the address from the call ????
            // call ???? (Obtain the relative call distance)
            let distance = *(addr.add(1 /* Skip call opcode */) as *const usize);

            // Relative jump -> EEF240 (jump to jmp in thunk table)
            let jmp_address = addr.add(5 /* Skip call opcode + address */ + distance);

            // == Address to the final ptr
            // jmp dword ptr ds:[????]
            let address = *(jmp_address.add(2 /* Skip ptr jmp opcode */) as *const usize);

            // Invalid call at -> 019A4DF1

            address as *const u8
        },
        |addr| {
            // Replace the address with our faker function
            let ptr: *mut usize = addr as *mut usize;
            *ptr = fake_gethostbyname as usize;
        },
    );
}
