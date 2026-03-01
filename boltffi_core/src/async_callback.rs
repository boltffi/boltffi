use std::{cell::RefCell, collections::HashMap, task::Waker};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct CallbackRequestId(u32);

impl CallbackRequestId {
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncCallbackCompletionCode {
    Completed = 0,
    Cancelled = -1,
    Panicked = -2,
}

impl AsyncCallbackCompletionCode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            0 => Self::Completed,
            -1 => Self::Cancelled,
            _ => Self::Panicked,
        }
    }

    pub fn is_success(self) -> bool {
        matches!(self, Self::Completed)
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompleteResult {
    Accepted = 0,
    UnknownOrAlreadyCompleted = -1,
}

pub struct CompletionPayload {
    pub code: AsyncCallbackCompletionCode,
    pub data: Vec<u8>,
}

struct PendingRequest {
    waker: Option<Waker>,
    result: Option<CompletionPayload>,
}

struct RequestRegistry {
    next_id: u32,
    pending: HashMap<u32, PendingRequest>,
}

impl RequestRegistry {
    fn new() -> Self {
        Self {
            next_id: 1,
            pending: HashMap::new(),
        }
    }

    fn allocate(&mut self) -> CallbackRequestId {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        if self.next_id == 0 {
            self.next_id = 1;
        }
        self.pending.insert(id, PendingRequest { waker: None, result: None });
        CallbackRequestId(id)
    }

    fn set_waker(&mut self, id: CallbackRequestId, waker: Waker) {
        if let Some(request) = self.pending.get_mut(&id.0) {
            request.waker = Some(waker);
        }
    }

    fn complete(&mut self, id: CallbackRequestId, code: AsyncCallbackCompletionCode, data: Vec<u8>) -> CompleteResult {
        let Some(request) = self.pending.get_mut(&id.0) else {
            return CompleteResult::UnknownOrAlreadyCompleted;
        };

        if request.result.is_some() {
            return CompleteResult::UnknownOrAlreadyCompleted;
        }

        request.result = Some(CompletionPayload { code, data });

        if let Some(waker) = request.waker.take() {
            waker.wake();
        }

        CompleteResult::Accepted
    }

    fn take_result(&mut self, id: CallbackRequestId) -> Option<CompletionPayload> {
        self.pending.get_mut(&id.0)?.result.take()
    }

    fn remove(&mut self, id: CallbackRequestId) {
        self.pending.remove(&id.0);
    }

    fn cancel(&mut self, id: CallbackRequestId) -> bool {
        let Some(request) = self.pending.get_mut(&id.0) else {
            return false;
        };

        if request.result.is_some() {
            return false;
        }

        request.result = Some(CompletionPayload {
            code: AsyncCallbackCompletionCode::Cancelled,
            data: Vec::new(),
        });

        if let Some(waker) = request.waker.take() {
            waker.wake();
        }

        true
    }
}

thread_local! {
    static REGISTRY: RefCell<RequestRegistry> = RefCell::new(RequestRegistry::new());
}

pub fn allocate_request() -> CallbackRequestId {
    REGISTRY.with(|r| r.borrow_mut().allocate())
}

pub fn set_request_waker(id: CallbackRequestId, waker: Waker) {
    REGISTRY.with(|r| r.borrow_mut().set_waker(id, waker));
}

pub fn complete_request(id: CallbackRequestId, code: AsyncCallbackCompletionCode, data: Vec<u8>) -> CompleteResult {
    REGISTRY.with(|r| r.borrow_mut().complete(id, code, data))
}

pub fn take_request_result(id: CallbackRequestId) -> Option<CompletionPayload> {
    REGISTRY.with(|r| r.borrow_mut().take_result(id))
}

pub fn remove_request(id: CallbackRequestId) {
    REGISTRY.with(|r| r.borrow_mut().remove(id));
}

pub fn cancel_request(id: CallbackRequestId) -> bool {
    REGISTRY.with(|r| r.borrow_mut().cancel(id))
}

pub struct RequestGuard(pub CallbackRequestId);

impl Drop for RequestGuard {
    fn drop(&mut self) {
        remove_request(self.0);
    }
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn complete_request_from_ffi(request_id: u32, completion_code: i32, data_ptr: u32, data_len: u32, data_cap: u32) -> i32 {
    let id = CallbackRequestId(request_id);
    let code = AsyncCallbackCompletionCode::from_i32(completion_code);

    let data = if data_ptr != 0 && data_len > 0 {
        unsafe {
            let slice = std::slice::from_raw_parts(data_ptr as *const u8, data_len as usize);
            slice.to_vec()
        }
    } else {
        Vec::new()
    };

    if data_ptr != 0 && data_cap > 0 {
        crate::wasm::boltffi_wasm_free_impl(data_ptr as *mut u8, data_cap as usize);
    }

    complete_request(id, code, data) as i32
}
