use log::{debug, error, warn};
use std::ffi::c_void;
use windows_sys::Win32::{
    Foundation::{GetLastError, FALSE},
    System::Memory::{VirtualProtect, PAGE_PROTECTION_FLAGS, PAGE_READWRITE},
};

/// Represents a pattern that can be patched
pub struct Pattern {
    /// The name of the pattern
    pub name: &'static str,
    /// The address to start searching at
    pub start: usize,
    /// The address to end searching at
    pub end: usize,
    /// The string mask deciding which opcodes to use
    pub mask: &'static str,
    /// The opcode pattern to match
    pub op: &'static [u8],
}

impl Pattern {
    /// Attempts to apply a pattern
    ///
    /// # Arguments
    /// * pattern - The pattern to use
    /// * length - The length of memory to protect
    /// * action - The action to take on the memory
    pub unsafe fn apply<F>(&self, length: usize, action: F)
    where
        F: FnOnce(*mut u8),
    {
        let Some(addr) = self.find() else {
            warn!("Failed to find {} hook position", self.name);
            return;
        };

        debug!("Found {} @ {:#016x}", self.name, addr as usize);

        Self::use_memory(addr, length, action)
    }

    /// Attempts to apply a pattern with a transformed
    /// adddress
    ///
    /// # Arguments
    /// * pattern - The pattern to use
    /// * length - The length of memory to protect
    /// * transform - Transformer for transforming the located address
    /// * action - The action to take on the memory
    pub unsafe fn apply_with_transform<F, T, P>(&self, length: usize, transform: T, action: F)
    where
        T: FnOnce(*const u8) -> *const P,
        F: FnOnce(*mut P),
    {
        let Some(addr) = self.find() else {
            warn!("Failed to find {} hook position", self.name);
            return;
        };

        debug!("Found {} @ {:#016x}", self.name, addr as usize);

        // Transform the address
        let addr = transform(addr);
        Self::use_memory(addr, length, action)
    }

    /// Attempts to find a matching pattern anywhere between the start and
    /// end address
    unsafe fn find(&self) -> Option<*const u8> {
        (self.start..=self.end)
            .map(|addr| addr as *const u8)
            .find(|addr| self.compare_mask(*addr))
    }

    /// Compares the opcodes after the provided address using the provided
    /// opcode and pattern
    ///
    /// # Arguments
    /// * addr - The address to start matching from
    unsafe fn compare_mask(&self, addr: *const u8) -> bool {
        self.mask
            .chars()
            .enumerate()
            .zip(self.op)
            .all(|((offset, mask), op)| mask == '?' || *addr.add(offset) == *op)
    }

    /// Attempts to apply virtual protect READ/WRITE access
    /// over the memory at the provided address for the length
    /// provided.
    ///
    /// # Arguments
    /// * addr - The address to protect
    /// * length - The protected region
    /// * action - The aciton to execute on the memory
    unsafe fn use_memory<F, P>(addr: *const P, length: usize, action: F)
    where
        F: FnOnce(*mut P),
    {
        let mut old_protect: PAGE_PROTECTION_FLAGS = 0;

        // Protect the memory region
        if VirtualProtect(
            addr as *const c_void,
            length,
            PAGE_READWRITE,
            &mut old_protect,
        ) == FALSE
        {
            let error = GetLastError();

            error!(
                "Failed to protect memory region @ {:#016x} length {} error: {:#4x}",
                addr as usize, length, error
            );
            return;
        }

        action(addr.cast_mut());

        // Unprotect the memory region
        VirtualProtect(addr as *const c_void, length, old_protect, &mut old_protect);
    }
}

pub unsafe fn fill_bytes(mut ptr: *mut u8, bytes: &[u8]) {
    for byte in bytes {
        *ptr = *byte;
        ptr = ptr.add(1);
    }
}
