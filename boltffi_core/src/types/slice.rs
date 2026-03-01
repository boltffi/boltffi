use core::{marker::PhantomData, slice};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FfiSlice<'a, T> {
    ptr: *const T,
    len: usize,
    _marker: PhantomData<&'a [T]>,
}

impl<'a, T> FfiSlice<'a, T> {
    pub fn from_slice(data: &'a [T]) -> Self {
        Self {
            ptr: data.as_ptr(),
            len: data.len(),
            _marker: PhantomData,
        }
    }

    pub fn as_slice(&self) -> &'a [T] {
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
        unsafe { slice::from_raw_parts(self.ptr, self.len) }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<'a, T> From<&'a [T]> for FfiSlice<'a, T> {
    fn from(data: &'a [T]) -> Self {
        Self::from_slice(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_roundtrip() {
        let data = [1u32, 2, 3, 4, 5];
        let ffi_slice = FfiSlice::from_slice(&data);
        assert_eq!(ffi_slice.len(), 5);
        assert_eq!(ffi_slice.as_slice(), &data);
    }

    #[test]
    fn empty_slice() {
        let data: [u32; 0] = [];
        let ffi_slice = FfiSlice::from_slice(&data);
        assert!(ffi_slice.is_empty());
        assert_eq!(ffi_slice.as_slice(), &data);
    }
}
