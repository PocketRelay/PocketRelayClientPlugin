use std::{
    char::decode_utf16,
    ffi::CStr,
    fmt::{Debug, Display},
    marker::PhantomData,
    os::raw::{c_char, c_int, c_uchar, c_uint, c_ulong, c_ushort, c_void},
};

/// Static memory address for the game objects
static GAME_OBJECT_OFFSET: u32 = 0x01AB5634;

type GameObjectsArray = TArray<*mut UObject>;

/// Obtains a reference to the [TArray] containing the game objects
pub fn game_objects_ref() -> &'static mut TArray<*mut UObject> {
    unsafe {
        (GAME_OBJECT_OFFSET as *const GameObjectsArray as *mut GameObjectsArray)
            .as_mut()
            .expect("Game objects pointer was null")
    }
}

pub fn get_function_object(index: usize) -> Option<*mut UFunction> {
    let fn_object = *game_objects_ref().get(index)?;
    let fn_ptr = fn_object.cast::<UFunction>() as *mut _;
    Some(fn_ptr)
}

/// Array type
#[repr(C)]
pub struct TArray<T> {
    /// Pointer to the data within the array
    data: *mut T,
    /// Number of items currently present
    count: c_int,
    /// Allocated capacity for underlying array memory
    capacity: c_int,
    /// Phantom type of the array generic type
    _type: PhantomData<::std::cell::UnsafeCell<T>>,
}

impl<T> Clone for TArray<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        let mut out = TArray::new();
        for value in self.iter() {
            out.push(value.clone());
        }
        out
    }
}

pub struct TArrayIter<'a, T> {
    arr: &'a TArray<T>,
    index: usize,
}

impl<'a, T> Iterator for TArrayIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.arr.len() {
            let item = self.arr.get(self.index).expect("TArray item was null");
            self.index += 1;
            Some(item)
        } else {
            None
        }
    }
}

impl<T> TArray<T> {
    /// Gets a pointer to specific element by index
    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len() {
            return None;
        }

        // Get a pointer to the data at the provided index
        let item = unsafe { self.data.add(index) };
        unsafe { item.as_ref() }
    }

    pub fn len(&self) -> usize {
        self.count as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.capacity as usize
    }

    pub fn clone_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        let mut out = Vec::with_capacity(self.len());
        for i in 0..self.len() {
            if let Some(value) = self.get(i) {
                out.push(value.clone())
            }
        }
        out
    }

    pub const fn new() -> Self {
        TArray {
            data: std::ptr::null_mut(),
            count: 0,
            capacity: 0,
            _type: PhantomData,
        }
    }

    fn grow(&mut self) {
        let new_capacity = if self.capacity == 0 {
            1
        } else {
            self.capacity * 2
        };
        let new_data = unsafe {
            let layout = std::alloc::Layout::array::<T>(new_capacity as usize).unwrap();
            let new_data = std::alloc::alloc(layout) as *mut T;
            if new_data.is_null() {
                panic!("Allocation failed");
            }
            new_data
        };

        // Copy old data to the new allocation
        unsafe {
            if !self.data.is_null() {
                std::ptr::copy_nonoverlapping(self.data, new_data, self.count as usize);
                std::alloc::dealloc(
                    self.data as *mut u8,
                    std::alloc::Layout::array::<T>(self.capacity as usize).unwrap(),
                );
            }
            self.data = new_data;
        }

        self.capacity = new_capacity;
    }

    pub fn push(&mut self, value: T) {
        if self.count == self.capacity {
            self.grow();
        }

        unsafe {
            let ptr = self.data.add(self.count as usize);
            ptr.write(value);
        }

        self.count += 1;
    }

    pub fn iter(&self) -> TArrayIter<'_, T> {
        TArrayIter {
            arr: self,
            index: 0,
        }
    }
}

impl<T> Debug for TArray<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T> From<Vec<T>> for TArray<T> {
    fn from(value: Vec<T>) -> Self {
        let length = value.len() as c_int;
        let capacity = value.capacity() as c_int;
        let value = value.leak();

        let data = value.as_mut_ptr();

        Self {
            data,
            count: length,
            capacity,
            _type: PhantomData,
        }
    }
}

#[repr(C)]
pub struct FString(TArray<i16>);

impl Default for FString {
    fn default() -> Self {
        Self(TArray::from(vec![0]))
    }
}

impl FString {
    pub fn from_string(mut value: String) -> FString {
        // String must be null terminated
        if !value.ends_with('\0') {
            value.push('\0')
        }

        let value = value
            .encode_utf16()
            .map(|value| value as i16)
            .collect::<Vec<_>>();
        FString(TArray::from(value))
    }

    pub fn from_str_with_null(value: &str) -> FString {
        // String must be null terminated
        if !value.ends_with('\0') {
            panic!("FString::from_str missing null terminator \"{value}\"");
        }

        let value = value
            .encode_utf16()
            .map(|value| value as i16)
            .collect::<Vec<_>>();
        FString(TArray::from(value))
    }
}

impl<T> Drop for TArray<T> {
    fn drop(&mut self) {
        if !self.data.is_null() {
            // Create a Vec from the raw parts so Rust can clean it up
            unsafe {
                Vec::from_raw_parts(self.data, self.count as usize, self.capacity as usize);
            }
        }
    }
}

impl Debug for FString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self, f)
    }
}

impl Display for FString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let out = decode_utf16(self.0.iter().map(|value| *value as u16))
            .try_fold(String::new(), |mut accu, value| {
                if let Ok(value) = value {
                    accu.push(value);
                    Some(accu)
                } else {
                    None
                }
            })
            .unwrap();
        f.write_str(&out)
    }
}

#[repr(C)]
pub struct UObjectVTable(c_void);

#[repr(C, packed(4))]
pub struct UObject {
    pub vtable_: *const UObjectVTable,
    pub object_internal_integer: c_int,
    pub object_flags: FQWord,
    pub hash_next: FPointer,
    pub hash_outer_next: FPointer,
    pub state_frame: FPointer,
    pub linker: *mut UObject,
    pub linker_index: FPointer,
    pub net_index: c_int,
    pub outer: *mut UObject,
    pub name: FName,
    pub class: *mut UClass,
    pub object_archetype: *mut UObject,
}

impl UObject {
    pub fn cast<T>(&self) -> *const T {
        self as *const UObject as *const T
    }

    /// Collects the full name of the object
    pub fn get_full_name(&self) -> String {
        match unsafe { (self.class.as_ref(), self.outer.as_ref()) } {
            (Some(class), Some(outer)) => {
                let class_name = class.get_name().to_str().expect("Class name invalid utf8");
                let outer_name = outer.get_name().to_str().expect("Class name invalid utf8");
                let this_name = self.get_name().to_str().expect("Class name invalid utf8");

                if let Some(outer) = unsafe { outer.outer.as_ref() } {
                    let outer_outer_name =
                        outer.get_name().to_str().expect("Class name invalid utf8");

                    format!(
                        "{} {}.{}.{}",
                        class_name, outer_outer_name, outer_name, this_name
                    )
                } else {
                    format!("{} {}.{}", class_name, outer_name, this_name)
                }
            }
            _ => "(null)".to_string(),
        }
    }

    pub fn get_name(&self) -> &CStr {
        self.name.get_name()
    }
}

#[repr(C)]
pub struct FQWord {
    pub a: c_int,
    pub b: c_int,
}

#[repr(C)]
pub struct FPointer {
    pub dummy: c_int,
}

#[repr(C)]
pub struct FName {
    pub name_entry: *mut FNameEntry,
    pub name_index: c_uint,
}

impl FName {
    /// Gets the name from the entry, name is stored
    /// in the name char
    pub fn get_name(&self) -> &CStr {
        unsafe {
            self.name_entry
                .as_ref()
                .expect("Name entry pointer was null")
                .get_name()
        }
    }
}

// Name entry
#[repr(C)]
pub struct FNameEntry {
    // Unknown block of data
    pub unknown_data00: [c_uchar; 8usize],
    // Name array data
    pub name: [c_char; 16usize],
}

impl FNameEntry {
    /// Gets the name from the entry, name is stored
    /// in the name char
    pub fn get_name(&self) -> &CStr {
        unsafe { CStr::from_ptr(self.name.as_ptr()) }
    }
}

#[repr(C)]
pub struct UClass {
    pub _base: UState,
    pub unknown_data00: [c_uchar; 188usize],
}

impl UClass {
    pub fn get_name(&self) -> &CStr {
        self._base.get_name()
    }

    pub fn as_object_ref(&self) -> &UObject {
        self._base.as_object_ref()
    }
}

#[repr(C)]
pub struct UState {
    pub _base: UStruct,
    pub unknown_data00: [c_uchar; 36usize],
}

impl UState {
    pub fn get_name(&self) -> &CStr {
        self._base.get_name()
    }

    pub fn as_object_ref(&self) -> &UObject {
        self._base.as_object_ref()
    }
}

#[repr(C)]
pub struct UStruct {
    pub _base: UField,
    pub unknown_data00: [c_uchar; 64usize],
}

impl UStruct {
    pub fn get_name(&self) -> &CStr {
        self._base.get_name()
    }

    pub fn as_object_ref(&self) -> &UObject {
        self._base.as_object_ref()
    }
}

#[repr(C, packed(4))]
pub struct UField {
    pub _base: UObject,
    pub super_field: *mut UField,
    pub next: *mut UField,
}

impl UField {
    pub fn get_name(&self) -> &CStr {
        self._base.get_name()
    }

    pub fn as_object_ref(&self) -> &UObject {
        &self._base
    }
}

#[repr(C, packed(4))]
pub struct UFunction {
    pub _base: UStruct,
    pub func: *mut c_void,
    pub function_flags: c_ulong,
    pub i_native: c_ushort,
    pub unknown_data00: [c_uchar; 8usize],
}

impl UFunction {
    pub fn get_name(&self) -> &CStr {
        self._base.get_name()
    }

    pub fn as_object_ref(&self) -> &UObject {
        self._base.as_object_ref()
    }
}

#[repr(C)]
pub struct FScriptDelegate {
    pub unknown_data_00: [::std::os::raw::c_uchar; 12usize],
}
