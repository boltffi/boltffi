use crate::types::FfiBuf;

/// Packs and unpacks `Vec<Self>` using the ABI representation chosen for `Self`.
///
/// This trait models sequence transport separately from [`crate::Passable`], which only
/// describes the ABI form of a single value.
pub trait VecTransport: Sized {
    /// Packs an owned vector into an FFI-owned byte buffer.
    fn pack_vec(vec: Vec<Self>) -> FfiBuf;

    /// Reconstructs a vector from raw bytes received through the ABI.
    ///
    /// # Safety
    ///
    /// `ptr` must point to `byte_len` readable bytes that contain values encoded
    /// according to the transport rules for `Self`.
    unsafe fn unpack_vec(ptr: *const u8, byte_len: usize) -> Vec<Self>;
}

macro_rules! impl_vec_direct {
    ($($ty:ty),* $(,)?) => {
        $(
            impl VecTransport for $ty {
                fn pack_vec(vec: Vec<$ty>) -> FfiBuf {
                    FfiBuf::from_vec(vec)
                }

                unsafe fn unpack_vec(ptr: *const u8, byte_len: usize) -> Vec<$ty> {
                    let element_count = byte_len / core::mem::size_of::<$ty>();
                    unsafe { core::slice::from_raw_parts(ptr as *const $ty, element_count) }.to_vec()
                }
            }
        )*
    };
}

impl_vec_direct!(
    i8, i16, i32, i64, u16, u32, u64, f32, f64, bool, usize, isize
);

impl VecTransport for u8 {
    fn pack_vec(vec: Vec<u8>) -> FfiBuf {
        FfiBuf::from_vec(vec)
    }

    unsafe fn unpack_vec(ptr: *const u8, byte_len: usize) -> Vec<u8> {
        unsafe { core::slice::from_raw_parts(ptr, byte_len) }.to_vec()
    }
}
