use std::{
    char::decode_utf16,
    ffi::CStr,
    fmt::{Debug, Display},
    marker::PhantomData,
    mem::ManuallyDrop,
    os::raw::{c_char, c_int, c_uchar, c_uint, c_ulong, c_ushort, c_void},
    str::FromStr,
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

/// Gets a function object by its index in the game objects array
pub fn get_function_object(index: usize) -> Option<*mut UFunction> {
    let fn_object = *game_objects_ref().get(index)?;
    let fn_ptr = fn_object.cast::<UFunction>() as *mut _;
    Some(fn_ptr)
}

pub trait AsObjectRef {
    fn as_object_ref(&self) -> &UObject;
}

pub trait GetObjectName {
    fn get_object_name(&self) -> &CStr;
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

impl<T> TArray<T> {
    /// Constructor to create a new array
    pub const fn new() -> Self {
        TArray {
            data: std::ptr::null_mut(),
            count: 0,
            capacity: 0,
            _type: PhantomData,
        }
    }

    /// Constructs a [TArray] with an initial capacity
    pub fn with_capacity(capacity: usize) -> Self {
        let layout = std::alloc::Layout::array::<T>(capacity).unwrap();
        let data = unsafe { std::alloc::alloc(layout) as *mut T };
        if data.is_null() {
            panic!("Allocation failed");
        }

        TArray {
            data,
            count: 0,
            capacity: capacity as i32,
            _type: PhantomData,
        }
    }

    /// Gets a reference to specific element by index
    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len() {
            return None;
        }

        // Get a pointer to the data at the provided index
        let item = unsafe { self.data.add(index) };

        let item = match unsafe { item.as_ref() } {
            Some(value) => value,
            // Will only occur if array was created from an invalid data ptr
            None => panic!("Array item at index {index} was a nullptr"),
        };

        Some(item)
    }

    /// Returns the length of the array
    pub fn len(&self) -> usize {
        self.count as usize
    }

    /// Returns where the array is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the allocated capacity
    pub fn capacity(&self) -> usize {
        self.capacity as usize
    }

    /// Pushes a new item onto the array, grows the array
    /// capacity if there is not enough room
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

    /// Creates a reference iterator for the values within the array
    pub fn iter(&self) -> TArrayIter<'_, T> {
        TArrayIter {
            arr: self,
            index: 0,
        }
    }

    /// Creates a [Vec] from the array, they are the same type
    /// just have a different memory structure.
    ///
    /// # Safety
    ///
    /// Safe as long as the fields of this structure are correct this
    /// type uses [ManuallyDrop] to prevent freeing the array memory
    /// since its not owned by Rust
    pub unsafe fn as_vec(&self) -> ManuallyDrop<Vec<T>> {
        ManuallyDrop::new(Vec::from_raw_parts(
            self.data,
            self.count as usize,
            self.capacity as usize,
        ))
    }

    /// Grows the capacity of the underlying allocated memory
    fn grow(&mut self) {
        let new_capacity = if self.capacity == 0 {
            1
        } else {
            self.capacity * 2
        };

        // Allocate array memory the new capacity
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

                // Deallocate the old memory
                std::alloc::dealloc(
                    self.data as *mut u8,
                    std::alloc::Layout::array::<T>(self.capacity as usize).unwrap(),
                );
            }
            self.data = new_data;
        }

        self.capacity = new_capacity;
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

impl<A> FromIterator<A> for TArray<A> {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let (lower_bound, upper_bound) = iter.size_hint();
        let mut array = TArray::with_capacity(upper_bound.unwrap_or(lower_bound));
        for value in iter {
            array.push(value)
        }
        array
    }
}

impl<T> Clone for TArray<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        self.iter().cloned().collect()
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

        // Leak the array memory to allow the array take ownership over it
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

/// Iterator for a [TArray]
pub struct TArrayIter<'a, T> {
    arr: &'a TArray<T>,
    index: usize,
}

impl<'a, T> Iterator for TArrayIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        // Reached end of array
        if self.index >= self.arr.len() {
            return None;
        }

        let item = match self.arr.get(self.index) {
            Some(value) => value,
            None => panic!("Array item at index {} was a nullptr", self.index),
        };

        self.index += 1;

        Some(item)
    }
}

/// Unreal engine UTF-16 string based on a [TArray] of [u16] the string
/// values present are null terminated
#[repr(C)]
pub struct FString(TArray<u16>);

impl Default for FString {
    fn default() -> Self {
        Self(TArray::from(vec![0]))
    }
}

impl FString {
    /// Creates a new [FString] from a rust [String]
    pub fn from_string(mut value: String) -> FString {
        // String must be null terminated
        if !value.ends_with('\0') {
            value.push('\0')
        }

        Self::from_str_with_null(&value)
    }

    /// Creates a new [FString] from a null terminated [str] slice
    ///
    /// Will panic if the [str] doesn't end with a null terminator
    /// use [Self::from_str] to append a null terminator when missing
    pub fn from_str_with_null(value: &str) -> FString {
        // String must be null terminated
        assert!(
            value.ends_with('\0'),
            "FString::from_str missing null terminator \"{value}\""
        );

        let value = value.encode_utf16().collect::<TArray<_>>();
        FString(value)
    }
}

impl FromStr for FString {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.ends_with('\0') {
            return Ok(Self::from_str_with_null(value));
        }

        let mut value = value.to_string();
        value.push('\0');
        Ok(Self::from_str_with_null(&value))
    }
}

impl Debug for FString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self, f)
    }
}

impl Display for FString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = String::with_capacity(self.0.len());
        let mut iter = decode_utf16(self.0.iter().copied());

        // Ignore decoding errors
        while let Some(Ok(value)) = iter.next() {
            // Stop at null terminators
            if value == '\0' {
                break;
            }

            out.push(value);
        }

        f.write_str(&out)
    }
}

#[repr(C, packed(4))]
pub struct UObject {
    /// Pointer to the object vtable
    pub vtable_: *const c_void,
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
    /// Cast the object to another type
    pub fn cast<T>(&self) -> *const T {
        self as *const UObject as *const T
    }

    /// Collects the full name of the object include the
    /// name of all outer classes
    pub fn get_full_name(&self) -> String {
        let (class, outer) = match unsafe { (self.class.as_ref(), self.outer.as_ref()) } {
            (Some(class), Some(outer)) => (class, outer),
            _ => return "(null)".to_string(),
        };

        let class_name = class
            .get_object_name()
            .to_str()
            .expect("Class name invalid utf8");
        let outer_name = outer
            .get_object_name()
            .to_str()
            .expect("Outer class name invalid utf8");
        let this_name = self
            .get_object_name()
            .to_str()
            .expect("This class name invalid utf8");

        let outer_outer = match unsafe { outer.outer.as_ref() } {
            Some(outer_outer) => outer_outer,

            // Class has no outer outer class
            None => return format!("{} {}.{}", class_name, outer_name, this_name),
        };

        let outer_outer_name = outer_outer
            .get_object_name()
            .to_str()
            .expect("Class name invalid utf8");

        format!(
            "{} {}.{}.{}",
            class_name, outer_outer_name, outer_name, this_name
        )
    }
}

impl GetObjectName for UObject {
    #[inline]
    fn get_object_name(&self) -> &CStr {
        self.name.get_name()
    }
}

/// Type representing a QWord
#[repr(C)]
pub struct FQWord {
    pub a: c_int,
    pub b: c_int,
}

/// Type representing a pointer
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

impl GetObjectName for UClass {
    #[inline]
    fn get_object_name(&self) -> &CStr {
        self._base.get_object_name()
    }
}

impl AsObjectRef for UClass {
    #[inline]
    fn as_object_ref(&self) -> &UObject {
        self._base.as_object_ref()
    }
}

#[repr(C)]
pub struct UState {
    pub _base: UStruct,
    pub unknown_data00: [c_uchar; 36usize],
}

impl GetObjectName for UState {
    #[inline]
    fn get_object_name(&self) -> &CStr {
        self._base.get_object_name()
    }
}

impl AsObjectRef for UState {
    #[inline]
    fn as_object_ref(&self) -> &UObject {
        self._base.as_object_ref()
    }
}

#[repr(C)]
pub struct UStruct {
    pub _base: UField,
    pub unknown_data00: [c_uchar; 64usize],
}

impl GetObjectName for UStruct {
    #[inline]
    fn get_object_name(&self) -> &CStr {
        self._base.get_object_name()
    }
}

impl AsObjectRef for UStruct {
    #[inline]
    fn as_object_ref(&self) -> &UObject {
        self._base.as_object_ref()
    }
}

#[repr(C, packed(4))]
pub struct UField {
    pub _base: UObject,
    pub super_field: *mut UField,
    pub next: *mut UField,
}

impl GetObjectName for UField {
    #[inline]
    fn get_object_name(&self) -> &CStr {
        self._base.get_object_name()
    }
}

impl AsObjectRef for UField {
    #[inline]
    fn as_object_ref(&self) -> &UObject {
        &self._base
    }
}

/// Object representing a function that can be called by the engine
#[repr(C, packed(4))]
pub struct UFunction {
    pub _base: UStruct,
    pub func: *mut c_void,
    pub function_flags: c_ulong,
    pub i_native: c_ushort,
    pub unknown_data00: [c_uchar; 8usize],
}

impl GetObjectName for UFunction {
    #[inline]
    fn get_object_name(&self) -> &CStr {
        self._base.get_object_name()
    }
}

impl AsObjectRef for UFunction {
    #[inline]
    fn as_object_ref(&self) -> &UObject {
        self._base.as_object_ref()
    }
}

#[repr(C)]
pub struct FScriptDelegate {
    pub unknown_data_00: [c_uchar; 12usize],
}
