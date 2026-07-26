use crate::wire::{WireBuffer, WireEncode};
use core::mem::{self, ManuallyDrop};

#[repr(C)]
pub struct FfiBuf {
    ptr: *mut u8,
    len: usize,
    cap: usize,
    align: usize,
}

impl FfiBuf {
    pub const fn empty() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
            len: 0,
            cap: 0,
            align: 1,
        }
    }

    pub fn from_vec<T>(vec: Vec<T>) -> Self {
        let mut vec = ManuallyDrop::new(vec);
        let len = vec.len() * mem::size_of::<T>();
        let cap = vec.capacity() * mem::size_of::<T>();
        let align = mem::align_of::<T>();
        let ptr = vec.as_mut_ptr() as *mut u8;
        Self {
            ptr,
            len,
            cap,
            align,
        }
    }

    pub fn wire_encode<V: WireEncode>(value: &V) -> Self {
        Self::from_vec(WireBuffer::new(value).into_bytes())
    }

    pub fn wire_encode_owned_string(value: impl Into<String>) -> Self {
        Self::wire_encode_owned_bytes(value.into().into_bytes())
    }

    pub fn wire_encode_owned_bytes(value: impl Into<Vec<u8>>) -> Self {
        let mut value = value.into();
        let byte_count = value.len();
        value.reserve_exact(core::mem::size_of::<u32>());
        unsafe {
            value.set_len(byte_count + core::mem::size_of::<u32>());
            core::ptr::copy(
                value.as_ptr(),
                value.as_mut_ptr().add(core::mem::size_of::<u32>()),
                byte_count,
            );
        }
        value[..core::mem::size_of::<u32>()].copy_from_slice(&(byte_count as u32).to_le_bytes());
        Self::from_vec(value)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn cap(&self) -> usize {
        self.cap
    }

    pub fn align(&self) -> usize {
        self.align
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }

    pub unsafe fn as_byte_slice(&self) -> &[u8] {
        if self.ptr.is_null() || self.len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
        }
    }

    pub unsafe fn into_vec<T>(self) -> Vec<T> {
        if self.ptr.is_null() {
            return Vec::new();
        }
        debug_assert_eq!(self.align, mem::align_of::<T>());
        let elem_len = self.len / mem::size_of::<T>();
        let elem_cap = self.cap / mem::size_of::<T>();
        let ptr = self.ptr as *mut T;
        mem::forget(self);
        unsafe { Vec::from_raw_parts(ptr, elem_len, elem_cap) }
    }
}

impl Drop for FfiBuf {
    fn drop(&mut self) {
        if !self.ptr.is_null()
            && self.cap > 0
            && let Ok(layout) = core::alloc::Layout::from_size_align(self.cap, self.align)
        {
            unsafe { std::alloc::dealloc(self.ptr, layout) };
        }
    }
}

impl Default for FfiBuf {
    fn default() -> Self {
        Self::empty()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn boltffi_free_buf(buf: FfiBuf) {
    drop(buf);
}

#[unsafe(no_mangle)]
pub extern "C" fn boltffi_buf_from_bytes(ptr: *const u8, len: usize) -> FfiBuf {
    if ptr.is_null() || len == 0 {
        return FfiBuf::empty();
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    FfiBuf::from_vec(bytes.to_vec())
}

#[unsafe(no_mangle)]
pub extern "C" fn boltffi_buf_with_len(len: usize) -> FfiBuf {
    if len == 0 {
        return FfiBuf::empty();
    }
    FfiBuf::from_vec(vec![0u8; len])
}

#[cfg(target_arch = "wasm32")]
impl FfiBuf {
    pub fn into_packed(self) -> u64 {
        let len = self.len;
        if len == 0 {
            return 0;
        }
        if self.cap == len && self.align == 1 {
            let ptr = self.ptr;
            mem::forget(self);
            return ((len as u64) << 32) | (ptr as u64);
        }

        let bytes = unsafe { self.as_byte_slice() }.to_vec().into_boxed_slice();
        drop(self);
        let ptr = Box::into_raw(bytes) as *mut u8;
        ((len as u64) << 32) | (ptr as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buf_from_u8_vec() {
        let data = vec![1u8, 2, 3, 4, 5];
        let ffi_buf = FfiBuf::from_vec(data);
        assert_eq!(ffi_buf.len(), 5);
        assert_eq!(ffi_buf.align, 1);
        let recovered: Vec<u8> = unsafe { ffi_buf.into_vec() };
        assert_eq!(recovered, vec![1u8, 2, 3, 4, 5]);
    }

    #[test]
    fn buf_from_i32_vec() {
        let data = vec![10i32, 20, 30];
        let ffi_buf = FfiBuf::from_vec(data);
        assert_eq!(ffi_buf.len(), 12);
        assert_eq!(ffi_buf.align, 4);
        let recovered: Vec<i32> = unsafe { ffi_buf.into_vec() };
        assert_eq!(recovered, vec![10i32, 20, 30]);
    }

    #[test]
    fn buf_drop() {
        let data = vec![1u8, 2, 3];
        let ffi_buf = FfiBuf::from_vec(data);
        drop(ffi_buf);
    }

    #[test]
    fn buf_empty() {
        let buf = FfiBuf::empty();
        assert!(buf.is_empty());
        assert!(buf.as_ptr().is_null());
    }

    #[test]
    fn buf_with_len_is_rust_owned() {
        let buf = boltffi_buf_with_len(24);
        assert_eq!(buf.len(), 24);
        assert_eq!(unsafe { buf.as_byte_slice() }, &[0; 24]);
    }

    #[test]
    fn owned_string_preserves_wire_encoding() {
        let value = String::from("boltffi");
        let expected = FfiBuf::wire_encode(&value);
        let actual = FfiBuf::wire_encode_owned_string(value);

        assert_eq!(unsafe { actual.as_byte_slice() }, unsafe {
            expected.as_byte_slice()
        });
    }

    #[test]
    fn owned_bytes_preserve_wire_encoding() {
        let value = vec![1_u8, 2, 3, 4];
        let expected = FfiBuf::wire_encode(&value);
        let actual = FfiBuf::wire_encode_owned_bytes(value);

        assert_eq!(unsafe { actual.as_byte_slice() }, unsafe {
            expected.as_byte_slice()
        });
    }
}
