use log::debug;
use minhook::MinHook;
use pocket_relay_client_shared::servers::has_server_tasks;
use std::{
    ffi::{CStr, CString},
    os::raw::c_void,
    str::FromStr,
    sync::LazyLock,
};
use windows_sys::{
    core::PCSTR,
    Win32::{
        Networking::WinSock::HOSTENT,
        System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
    },
};

type GetHostByNameFn = unsafe extern "system" fn(name: PCSTR) -> *mut HOSTENT;

static ORIGINAL_GETHOSTBYNAME: LazyLock<(GetHostByNameFn, GetHostByNameFn)> =
    LazyLock::new(|| unsafe {
        let ws2_32 = GetModuleHandleA(b"ws2_32.dll\0".as_ptr());
        if ws2_32.is_null() {
            panic!("Failed to obtain handle for ws2_32.dll");
        }

        let target_addr = GetProcAddress(ws2_32, b"gethostbyname\0".as_ptr());
        let Some(target_fn) = target_addr else {
            panic!("Failed to locate address of gethostbyname() inside ws2_32.dll");
        };

        let target_fn: GetHostByNameFn = std::mem::transmute(target_fn);

        let original = MinHook::create_hook(target_fn as _, fake_gethostbyname as _)
            .expect("failed to create hook");
        let original: GetHostByNameFn = std::mem::transmute(original);
        (original, target_fn)
    });

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

    let (gethostbyname, _) = *ORIGINAL_GETHOSTBYNAME;

    // Only handle gosredirector.ea.com domains and don't use the override unless
    // there is running server tasks
    if str_name.to_bytes() == b"gosredirector.ea.com" && has_server_tasks() {
        debug!("Responding with localhost redirect");
        let localhost = CString::from_str("localhost").expect("host is a valid host");
        let host = gethostbyname(localhost.as_ptr().cast());
        return host;
    }

    // Use the actual function
    gethostbyname(name)
}

pub unsafe fn hook_host_lookup() {
    let (_, target) = *ORIGINAL_GETHOSTBYNAME;
    MinHook::enable_hook(target as *mut c_void).expect("failed to enable host lookup hook");
}
