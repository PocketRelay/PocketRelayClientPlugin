//! Module for memory manipulation and searching logic

use log::error;
use windows_sys::Win32::{
    Foundation::{GetLastError, FALSE},
    System::Memory::{VirtualProtect, PAGE_PROTECTION_FLAGS, PAGE_READWRITE},
};

/// Compares the opcodes after the provided address using the provided
/// opcode and pattern
///
/// ## Safety
///
/// Reading program memory is *NOT* safe but its required for pattern matching
///
/// ## Arguments
/// * addr     - The address to start matching from
/// * mask     - The mask to use when matching opcodes
/// * op_codes - The op codes to match against
unsafe fn compare_mask(addr: *const u8, mask: &'static str, op_codes: &'static [u8]) -> bool {
    mask.chars()
        .enumerate()
        // Merge the iterator with the opcodes for matching
        .zip(op_codes.iter().copied())
        // Compare the mask and memory at the address with the op codes
        .all(|((offset, mask), op)| mask == '?' || *addr.add(offset) == op)
}

/// Attempts to find a matching pattern anywhere between the start and
/// end offsets
///
/// ## Safety
///
/// Reading program memory is *NOT* safe but its required for pattern matching
///
/// ## Arguments
/// * start_offset - The address to start matching from
/// * end_offset   - The address to stop matching at
/// * mask         - The mask to use when matching opcodes
/// * op_codes     - The op codes to match against
pub unsafe fn find_pattern(
    start_offset: usize,
    end_offset: usize,
    mask: &'static str,
    op_codes: &'static [u8],
) -> Option<*const u8> {
    // Iterate between the offsets
    (start_offset..=end_offset)
        // Cast the address to a pointer type
        .map(|addr| addr as *const u8)
        // Compare the mask at the provided address
        .find(|addr| compare_mask(*addr, mask, op_codes))
}
