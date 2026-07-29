use boltffi_binding::FutureMobility;
use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, format_ident, quote};

fn symbol(mobility: FutureMobility, operation: &str) -> Ident {
    let runtime = match mobility {
        FutureMobility::CrossThread => "rust_future",
        FutureMobility::ThreadBound => "rust_thread_bound_future",
    };
    format_ident!("{runtime}_{operation}", span = Span::call_site())
}

pub(crate) fn invalid(mobility: FutureMobility, rust_return_type: impl ToTokens) -> TokenStream {
    let function = symbol(mobility, "invalid_arg");
    quote! {
        ::boltffi::__private::rustfuture::#function::<#rust_return_type>()
    }
}

pub(crate) fn start(mobility: FutureMobility, future: impl ToTokens) -> TokenStream {
    let function = symbol(mobility, "new");
    quote! {
        ::boltffi::__private::rustfuture::#function(#future)
    }
}

pub(crate) fn poll(mobility: FutureMobility, rust_return_type: impl ToTokens) -> TokenStream {
    let function = symbol(mobility, "poll");
    quote! {
        ::boltffi::__private::rustfuture::#function::<#rust_return_type>(
            handle,
            callback,
            callback_data
        )
    }
}

pub(crate) fn complete(mobility: FutureMobility, rust_return_type: impl ToTokens) -> TokenStream {
    let function = symbol(mobility, "complete");
    quote! {
        ::boltffi::__private::rustfuture::#function::<#rust_return_type>(handle)
    }
}

pub(crate) fn panic_message(
    mobility: FutureMobility,
    rust_return_type: impl ToTokens,
) -> TokenStream {
    let function = symbol(mobility, "panic_message");
    quote! {
        ::boltffi::__private::rustfuture::#function::<#rust_return_type>(handle)
    }
}

pub(crate) fn cancel(mobility: FutureMobility, rust_return_type: impl ToTokens) -> TokenStream {
    let function = symbol(mobility, "cancel");
    quote! {
        ::boltffi::__private::rustfuture::#function::<#rust_return_type>(handle)
    }
}

pub(crate) fn free(mobility: FutureMobility, rust_return_type: impl ToTokens) -> TokenStream {
    let function = symbol(mobility, "free");
    quote! {
        ::boltffi::__private::rustfuture::#function::<#rust_return_type>(handle)
    }
}

pub(crate) fn poll_sync(mobility: FutureMobility, rust_return_type: impl ToTokens) -> TokenStream {
    let function = symbol(mobility, "poll_sync");
    quote! {
        ::boltffi::__private::rustfuture::#function::<#rust_return_type>(handle)
    }
}
