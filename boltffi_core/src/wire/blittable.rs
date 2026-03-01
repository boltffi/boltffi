use crate::wire::constants::VEC_COUNT_SIZE;

pub unsafe trait Blittable: Copy + Sized {}

unsafe impl Blittable for i8 {}
unsafe impl Blittable for i16 {}
unsafe impl Blittable for i32 {}
unsafe impl Blittable for i64 {}
unsafe impl Blittable for u8 {}
unsafe impl Blittable for u16 {}
unsafe impl Blittable for u32 {}
unsafe impl Blittable for u64 {}
unsafe impl Blittable for f32 {}
unsafe impl Blittable for f64 {}
unsafe impl Blittable for bool {}
unsafe impl Blittable for isize {}
unsafe impl Blittable for usize {}

#[inline]
pub fn encode_blittable_slice<T: Blittable>(slice: &[T], buf: &mut [u8]) -> usize {
    let count = slice.len() as u32;
    buf[..VEC_COUNT_SIZE].copy_from_slice(&count.to_le_bytes());

    if slice.is_empty() {
        return VEC_COUNT_SIZE;
    }

    let byte_count = std::mem::size_of_val(slice);
    unsafe {
        std::ptr::copy_nonoverlapping(slice.as_ptr() as *const u8, buf.as_mut_ptr().add(VEC_COUNT_SIZE), byte_count);
    }
    VEC_COUNT_SIZE + byte_count
}

#[inline]
pub fn blittable_slice_wire_size<T: Blittable>(slice: &[T]) -> usize {
    VEC_COUNT_SIZE + std::mem::size_of_val(slice)
}

#[inline]
pub fn decode_blittable_slice<T: Blittable>(buf: &[u8]) -> Option<Vec<T>> {
    if buf.len() < VEC_COUNT_SIZE {
        return None;
    }

    let count = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if count == 0 {
        return Some(Vec::new());
    }

    let byte_count = count * std::mem::size_of::<T>();
    let required_len = VEC_COUNT_SIZE + byte_count;
    if buf.len() < required_len {
        return None;
    }

    let mut result = Vec::<T>::with_capacity(count);
    unsafe {
        std::ptr::copy_nonoverlapping(buf.as_ptr().add(VEC_COUNT_SIZE), result.as_mut_ptr() as *mut u8, byte_count);
        result.set_len(count);
    }
    Some(result)
}

#[inline]
pub fn encode_blittable<T: Blittable>(value: &T, buf: &mut [u8]) -> usize {
    let size = std::mem::size_of::<T>();
    unsafe {
        std::ptr::copy_nonoverlapping(value as *const T as *const u8, buf.as_mut_ptr(), size);
    }
    size
}

#[inline]
pub fn decode_blittable<T: Blittable>(buf: &[u8]) -> Option<T> {
    if buf.len() < std::mem::size_of::<T>() {
        return None;
    }
    Some(unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const T) })
}
