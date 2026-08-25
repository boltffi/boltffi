use crate::wire::{WireBuffer, WireEncode};
use core::mem::{self, ManuallyDrop};

#[repr(C)]
pub struct FfiBuf {
    ptr: *mut u8,
    len: usize,
    cap: usize,
    align: usize,
}

unsafe impl Send for FfiBuf {}

impl FfiBuf {
    pub const fn empty() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
            len: 0,
            cap: 0,
            align: 1,
        }
    }

    pub fn from_vec<T: Send + 'static>(vec: Vec<T>) -> Self {
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

/// Decodes an owned, wire-encoded UTF-8 string buffer into an [`FfiString`].
///
/// The returned string is length-delimited and is **not** NUL-terminated.
#[unsafe(no_mangle)]
pub extern "C" fn boltffi_buf_into_string(buf: FfiBuf) -> crate::FfiString {
    let mut bytes = unsafe { buf.into_vec::<u8>() };
    let Some(length) = bytes
        .get(..core::mem::size_of::<u32>())
        .and_then(|prefix| prefix.try_into().ok())
        .map(u32::from_le_bytes)
        .and_then(|length| usize::try_from(length).ok())
    else {
        return crate::FfiString::default();
    };
    let prefix_len = core::mem::size_of::<u32>();
    if bytes.len() != prefix_len.saturating_add(length) {
        return crate::FfiString::default();
    }
    bytes.copy_within(prefix_len.., 0);
    bytes.truncate(length);
    String::from_utf8(bytes)
        .map(crate::FfiString::from)
        .unwrap_or_default()
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
    /// Packed form of an empty buffer.
    ///
    /// `into_packed` returns `0` whenever `len == 0`, and `FfiBuf::empty()` has
    /// `len == 0`, so `FfiBuf::default().into_packed()` is always this value.
    /// Error paths return the constant instead: `into_packed` is a real call in
    /// the wasm module, so spelling it out made every argument-decode failure
    /// site build an `FfiBuf` on the stack and call into it.
    pub const EMPTY_PACKED: u64 = 0;

    pub fn into_packed(self) -> u64 {
        let len = self.len;
        if len == 0 {
            return Self::EMPTY_PACKED;
        }
        if self.cap == len && self.align == 1 {
            let ptr = self.ptr;
            mem::forget(self);
            return ((len as u64) << 32) | (ptr as u64);
        }

        if self.align == 1 {
            // The host frees this with `Vec::from_raw_parts(ptr, len, len)`, so
            // a capacity above the length has to go before the pointer can
            // cross. `shrink_to_fit` asks the allocator to split the block in
            // place; dlmalloc does that without touching the payload, which is
            // the whole point of trying it before falling back to a copy.
            let mut bytes = unsafe { self.into_vec::<u8>() };
            bytes.shrink_to_fit();
            if bytes.capacity() == len {
                let mut bytes = ManuallyDrop::new(bytes);
                return ((len as u64) << 32) | (bytes.as_mut_ptr() as u64);
            }
            let boxed = bytes.into_boxed_slice();
            let ptr = Box::into_raw(boxed) as *mut u8;
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
    #[test]
    fn an_empty_buffer_has_no_length() {
        // `EMPTY_PACKED` is only correct because `into_packed` returns 0 for a
        // zero length and an empty buffer has one. That second half is what
        // this pins; the first is two visible lines in `into_packed`, and is
        // `wasm32`-gated so it cannot be asserted from a host test.
        assert_eq!(FfiBuf::empty().len, 0);
        assert_eq!(FfiBuf::default().len, 0);
    }

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

    #[test]
    fn encoded_string_buffer_converts_to_owned_ffi_string() {
        let encoded = FfiBuf::wire_encode(&String::from("not NUL-terminated"));
        let decoded = boltffi_buf_into_string(encoded);

        assert_eq!(decoded.as_str(), Some("not NUL-terminated"));
    }

    #[test]
    fn malformed_encoded_string_buffer_converts_to_empty_string() {
        let malformed = FfiBuf::from_vec(vec![5_u8, 0, 0, 0, b'x']);
        let decoded = boltffi_buf_into_string(malformed);

        assert!(decoded.is_empty());
    }
}
