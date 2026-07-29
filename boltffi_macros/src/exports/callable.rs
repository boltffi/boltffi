use boltffi_ffi_rules::callable::{CallableForm, ExecutionKind};
use syn::{FnArg, ImplItemFn, ItemFn};

#[derive(Clone, Copy)]
pub(crate) struct FunctionCallable<'a> {
    item: &'a ItemFn,
    execution_kind: ExecutionKind,
}

#[derive(Clone, Copy)]
pub(crate) struct MethodCallable<'a> {
    method: &'a ImplItemFn,
    form: CallableForm,
    execution_kind: ExecutionKind,
}

impl<'a> FunctionCallable<'a> {
    pub(crate) fn new(item: &'a ItemFn) -> Self {
        Self {
            item,
            execution_kind: Self::execution_kind_for(item),
        }
    }

    pub(crate) fn form(&self) -> CallableForm {
        CallableForm::Function
    }

    pub(crate) fn execution_kind(&self) -> ExecutionKind {
        self.execution_kind
    }

    pub(crate) fn item(&self) -> &'a ItemFn {
        self.item
    }

    fn execution_kind_for(item: &ItemFn) -> ExecutionKind {
        if item.sig.asyncness.is_some() {
            ExecutionKind::Async
        } else {
            ExecutionKind::Sync
        }
    }
}

impl<'a> MethodCallable<'a> {
    pub(crate) fn new(method: &'a ImplItemFn) -> Self {
        Self {
            method,
            form: Self::form_for(method),
            execution_kind: Self::execution_kind_for(method),
        }
    }

    pub(crate) fn form(&self) -> CallableForm {
        self.form
    }

    pub(crate) fn execution_kind(&self) -> ExecutionKind {
        self.execution_kind
    }

    pub(crate) fn method(&self) -> &'a ImplItemFn {
        self.method
    }

    fn form_for(method: &ImplItemFn) -> CallableForm {
        if method
            .sig
            .inputs
            .first()
            .is_some_and(|argument| matches!(argument, FnArg::Receiver(_)))
        {
            CallableForm::InstanceMethod
        } else {
            CallableForm::StaticMethod
        }
    }

    fn execution_kind_for(method: &ImplItemFn) -> ExecutionKind {
        if method.sig.asyncness.is_some() {
            ExecutionKind::Async
        } else {
            ExecutionKind::Sync
        }
    }
}
