use std::{ffi::c_void, sync::Arc};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CallbackHandle {
    handle: u64,
    vtable: *const c_void,
}

unsafe impl Send for CallbackHandle {}
unsafe impl Sync for CallbackHandle {}

impl CallbackHandle {
    pub const NULL: Self = Self {
        handle: 0,
        vtable: std::ptr::null(),
    };

    #[inline]
    pub const fn new(handle: u64, vtable: *const c_void) -> Self {
        Self { handle, vtable }
    }

    #[inline]
    pub fn handle(&self) -> u64 {
        self.handle
    }

    #[inline]
    pub fn vtable(&self) -> *const c_void {
        self.vtable
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        self.handle == 0 || self.vtable.is_null()
    }

    #[inline]
    #[cfg(target_arch = "wasm32")]
    pub fn from_wasm_handle(handle: u32) -> Self {
        Self {
            handle: handle as u64,
            vtable: std::ptr::null(),
        }
    }
}

pub trait FromCallbackHandle {
    unsafe fn arc_from_callback_handle(handle: CallbackHandle) -> Arc<Self>;
    unsafe fn box_from_callback_handle(handle: CallbackHandle) -> Box<Self>;
}

pub trait CallbackForeignType {
    type Foreign: FromCallbackHandle;
}

impl Default for CallbackHandle {
    fn default() -> Self {
        Self::NULL
    }
}

impl std::fmt::Debug for CallbackHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallbackHandle").field("handle", &self.handle).field("vtable", &self.vtable).finish()
    }
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn boltffi_create_callback_handle(js_handle: u32) -> u32 {
    js_handle
}

#[cfg(target_arch = "wasm32")]
pub struct WasmCallbackOwner {
    handle: u32,
    free_fn: unsafe extern "C" fn(u32),
}

#[cfg(target_arch = "wasm32")]
impl WasmCallbackOwner {
    #[inline]
    pub fn new(handle: u32, free_fn: unsafe extern "C" fn(u32)) -> Self {
        Self { handle, free_fn }
    }

    #[inline]
    pub fn handle(&self) -> u32 {
        self.handle
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for WasmCallbackOwner {
    fn drop(&mut self) {
        unsafe { (self.free_fn)(self.handle) }
    }
}

#[cfg(target_arch = "wasm32")]
unsafe impl Send for WasmCallbackOwner {}

#[cfg(target_arch = "wasm32")]
unsafe impl Sync for WasmCallbackOwner {}
