use boltffi_ast::SourceName;

use crate::{NativeSymbol, SymbolId, SymbolName};

use super::LowerError;

pub const FFI_PREFIX: &str = "boltffi";

#[derive(Clone, Copy)]
pub enum SymbolOwner<'source> {
    Record(&'source str),
    Enum(&'source str),
    Class(&'source str),
    Callback(&'source str),
}

impl<'source> SymbolOwner<'source> {
    pub const fn record(source_id: &'source str) -> Self {
        Self::Record(source_id)
    }

    pub const fn enumeration(source_id: &'source str) -> Self {
        Self::Enum(source_id)
    }

    pub const fn class(source_id: &'source str) -> Self {
        Self::Class(source_id)
    }

    pub const fn callback(source_id: &'source str) -> Self {
        Self::Callback(source_id)
    }

    pub fn method_symbol_name(self, member: &SourceName) -> String {
        format!(
            "{}_method_{}_{}_{}",
            FFI_PREFIX,
            self.family(),
            symbol_path(self.source_id()),
            source_member_name(member)
        )
    }

    pub fn initializer_symbol_name(self, initializer: &SourceName) -> String {
        format!(
            "{}_init_{}_{}_{}",
            FFI_PREFIX,
            self.family(),
            symbol_path(self.source_id()),
            source_member_name(initializer)
        )
    }

    fn family(self) -> &'static str {
        match self {
            Self::Record(_) => "record",
            Self::Enum(_) => "enum",
            Self::Class(_) => "class",
            Self::Callback(_) => "callback",
        }
    }

    fn source_id(self) -> &'source str {
        match self {
            Self::Record(source_id)
            | Self::Enum(source_id)
            | Self::Class(source_id)
            | Self::Callback(source_id) => source_id,
        }
    }
}

pub struct SymbolAllocator {
    next: u32,
}

impl SymbolAllocator {
    pub fn new() -> Self {
        Self { next: 0 }
    }

    pub fn mint(&mut self, name: String) -> Result<NativeSymbol, LowerError> {
        let id = self.next_id();
        let parsed = SymbolName::parse(name)?;
        Ok(NativeSymbol::new(id, parsed))
    }

    pub fn mint_function(&mut self, function_id: &str) -> Result<NativeSymbol, LowerError> {
        self.mint(format!(
            "{}_function_{}",
            FFI_PREFIX,
            symbol_path(function_id)
        ))
    }

    pub fn mint_constant_accessor(
        &mut self,
        constant_id: &str,
    ) -> Result<NativeSymbol, LowerError> {
        self.mint(format!("{}_const_{}", FFI_PREFIX, symbol_path(constant_id)))
    }

    pub fn mint_class_release(&mut self, class_id: &str) -> Result<NativeSymbol, LowerError> {
        self.mint(format!(
            "{}_release_class_{}",
            FFI_PREFIX,
            symbol_path(class_id)
        ))
    }

    pub fn mint_method(
        &mut self,
        owner: SymbolOwner,
        member: &SourceName,
    ) -> Result<NativeSymbol, LowerError> {
        self.mint(owner.method_symbol_name(member))
    }

    pub fn mint_initializer(
        &mut self,
        owner: SymbolOwner,
        initializer: &SourceName,
    ) -> Result<NativeSymbol, LowerError> {
        self.mint(owner.initializer_symbol_name(initializer))
    }

    pub fn mint_callback_register(
        &mut self,
        callback_id: &str,
    ) -> Result<NativeSymbol, LowerError> {
        self.mint(format!(
            "{}_register_callback_{}",
            FFI_PREFIX,
            symbol_path(callback_id)
        ))
    }

    pub fn mint_callback_create_handle(
        &mut self,
        callback_id: &str,
    ) -> Result<NativeSymbol, LowerError> {
        self.mint(format!(
            "{}_create_callback_{}",
            FFI_PREFIX,
            symbol_path(callback_id)
        ))
    }

    pub fn mint_callback_complete(
        &mut self,
        callback_id: &str,
        slot: &CallbackSlot,
    ) -> Result<NativeSymbol, LowerError> {
        self.mint(format!(
            "{}_callback_{}_{}_complete",
            FFI_PREFIX,
            symbol_path(callback_id),
            slot.as_str()
        ))
    }

    pub fn mint_stream(
        &mut self,
        stream_id: &str,
        action: StreamLifecycle,
    ) -> Result<NativeSymbol, LowerError> {
        self.mint(format!(
            "{}_stream_{}_{}",
            FFI_PREFIX,
            symbol_path(stream_id),
            action.suffix()
        ))
    }

    pub fn mint_async_lifecycle(
        &mut self,
        start_symbol_name: &str,
        action: AsyncLifecycle,
    ) -> Result<NativeSymbol, LowerError> {
        let start_without_prefix = start_symbol_name
            .strip_prefix(&format!("{FFI_PREFIX}_"))
            .unwrap_or(start_symbol_name);
        self.mint(format!(
            "{}_async_{}_{}",
            FFI_PREFIX,
            start_without_prefix,
            action.suffix()
        ))
    }

    pub const fn next_group_id(&self) -> u32 {
        self.next
    }

    fn next_id(&mut self) -> SymbolId {
        let id = SymbolId::from_raw(self.next);
        self.next += 1;
        id
    }
}

fn source_member_name(name: &SourceName) -> String {
    name.parts()
        .map(|part| to_snake_case(part.as_str()))
        .collect::<Vec<_>>()
        .join("_")
}

#[derive(Clone, Copy)]
pub enum CallbackLocalLifecycle {
    Handle,
    Free,
    Clone,
}

impl CallbackLocalLifecycle {
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Handle => "handle",
            Self::Free => "free",
            Self::Clone => "clone",
        }
    }

    pub fn function_name(self, callback_id: &str) -> String {
        format!(
            "__{}_local_{}_{}",
            FFI_PREFIX,
            symbol_path(callback_id),
            self.suffix()
        )
    }
}

pub fn callback_wasm_import_free_name(callback_id: &str) -> String {
    wasm_callback_import_name("lifecycle", &symbol_path(callback_id), "free")
}

pub fn callback_wasm_import_clone_name(callback_id: &str) -> String {
    wasm_callback_import_name("lifecycle", &symbol_path(callback_id), "clone")
}

#[derive(Clone, Copy)]
pub enum StreamLifecycle {
    Subscribe,
    PopBatch,
    Wait,
    Poll,
    Unsubscribe,
    Free,
}

impl StreamLifecycle {
    const fn suffix(self) -> &'static str {
        match self {
            Self::Subscribe => "subscribe",
            Self::PopBatch => "pop_batch",
            Self::Wait => "wait",
            Self::Poll => "poll",
            Self::Unsubscribe => "unsubscribe",
            Self::Free => "free",
        }
    }
}

#[derive(Clone, Copy)]
pub enum AsyncLifecycle {
    Poll,
    PollSync,
    Complete,
    Cancel,
    Free,
    Panic,
}

impl AsyncLifecycle {
    const fn suffix(self) -> &'static str {
        match self {
            Self::Poll => "poll",
            Self::PollSync => "poll_sync",
            Self::Complete => "complete",
            Self::Cancel => "cancel",
            Self::Free => "free",
            Self::Panic => "panic_message",
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CallbackSlot(String);

impl CallbackSlot {
    pub fn from_method_name(method_name: &str) -> Self {
        Self(to_snake_case(method_name))
    }

    pub fn from_source_name(name: &SourceName) -> Self {
        Self(source_member_name(name))
    }

    pub fn local_method_name(&self, callback_id: &str) -> String {
        format!(
            "__{}_local_{}_{}",
            FFI_PREFIX,
            symbol_path(callback_id),
            self.as_str()
        )
    }

    pub fn wasm_import_method_name(&self, callback_id: &str) -> String {
        wasm_callback_import_name("method", &symbol_path(callback_id), self.as_str())
    }

    pub fn wasm_import_start_name(&self, callback_id: &str) -> String {
        wasm_callback_import_name("async_start", &symbol_path(callback_id), self.as_str())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub const WASM_CALLBACK_IMPORT_MODULE: &str = "env";

pub const VTABLE_FREE_SLOT_NAME: &str = "free";

pub const VTABLE_CLONE_SLOT_NAME: &str = "clone";

fn symbol_path(source_id: &str) -> String {
    source_id
        .split("::")
        .filter(|segment| !segment.is_empty())
        .map(to_snake_case)
        .collect::<Vec<_>>()
        .join("_")
}

pub fn wasm_callback_import_name(lane: &str, owner: &str, action: &str) -> String {
    format!("__{}_callback_{}_{}_{}", FFI_PREFIX, lane, owner, action)
}

pub fn wasm_closure_export_name(group_id: u32, signature: &str, action: &str) -> String {
    format!(
        "{}_closure_{}_{}_{}",
        FFI_PREFIX, group_id, signature, action
    )
}

pub fn to_snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let initial = String::with_capacity(name.len() + chars.len() / 2);
    chars
        .iter()
        .enumerate()
        .fold(initial, |mut result, (index, &character)| {
            if character.is_uppercase() && index > 0 {
                let previous = chars[index - 1];
                let next = chars.get(index + 1).copied();
                let previous_is_word = previous.is_lowercase() || previous.is_ascii_digit();
                let acronym_word_break = previous.is_uppercase()
                    && next.is_some_and(|character| character.is_lowercase());
                if previous_is_word || acronym_word_break {
                    result.push('_');
                }
            }
            if character == '-' {
                result.push('_');
            } else {
                result.extend(character.to_lowercase());
            }
            result
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_lowercases_camel_words() {
        assert_eq!(to_snake_case("MyRecord"), "my_record");
        assert_eq!(to_snake_case("Point"), "point");
    }

    #[test]
    fn snake_case_breaks_acronyms_before_following_word() {
        assert_eq!(to_snake_case("HTTPHeader"), "http_header");
        assert_eq!(to_snake_case("XMLParser"), "xml_parser");
        assert_eq!(to_snake_case("MyHTTPClient"), "my_http_client");
    }

    #[test]
    fn snake_case_collapses_pure_acronyms() {
        assert_eq!(to_snake_case("HTTP"), "http");
        assert_eq!(to_snake_case("URL"), "url");
    }

    #[test]
    fn snake_case_passes_through_lowercase() {
        assert_eq!(to_snake_case("point"), "point");
        assert_eq!(to_snake_case("my_record"), "my_record");
    }

    #[test]
    fn snake_case_treats_digit_then_upper_as_word_break() {
        assert_eq!(to_snake_case("Point2D"), "point2_d");
        assert_eq!(to_snake_case("Vector3"), "vector3");
    }

    #[test]
    fn callback_local_handle_name_uses_local_callback_namespace() {
        assert_eq!(
            CallbackLocalLifecycle::Handle.function_name("demo::progress::ProgressListener"),
            "__boltffi_local_demo_progress_progress_listener_handle"
        );
    }

    #[test]
    fn member_symbol_name_uses_owner_and_member() {
        let member = SourceName::from_canonical(boltffi_ast::CanonicalName::new(vec![
            boltffi_ast::NamePart::new("translate"),
        ]));

        assert_eq!(
            SymbolOwner::record("demo::MyRecord").method_symbol_name(&member),
            "boltffi_method_record_demo_my_record_translate"
        );
    }

    #[test]
    fn source_member_name_snake_cases_each_source_part() {
        let name = SourceName::from_canonical(boltffi_ast::CanonicalName::new(vec![
            boltffi_ast::NamePart::new("from"),
            boltffi_ast::NamePart::new("HTTPRequest"),
        ]));

        assert_eq!(source_member_name(&name), "from_http_request");
    }

    #[test]
    fn initializer_symbol_name_uses_initializer_lane() {
        let initializer = SourceName::from_canonical(boltffi_ast::CanonicalName::new(vec![
            boltffi_ast::NamePart::new("new"),
        ]));

        assert_eq!(
            SymbolOwner::record("demo::Point").initializer_symbol_name(&initializer),
            "boltffi_init_record_demo_point_new"
        );
    }

    #[test]
    fn constant_accessor_symbol_name_uses_const_lane() {
        let mut allocator = SymbolAllocator::new();
        let symbol = allocator
            .mint_constant_accessor("demo::MAGIC")
            .expect("valid symbol");

        assert_eq!(symbol.name().as_str(), "boltffi_const_demo_magic");
    }

    #[test]
    fn class_release_symbol_name_uses_release_lane() {
        let mut allocator = SymbolAllocator::new();
        let symbol = allocator
            .mint_class_release("demo::Engine")
            .expect("valid symbol");

        assert_eq!(symbol.name().as_str(), "boltffi_release_class_demo_engine");
    }

    #[test]
    fn symbol_paths_include_source_namespaces() {
        let member = SourceName::from_canonical(boltffi_ast::CanonicalName::new(vec![
            boltffi_ast::NamePart::new("fetch"),
        ]));

        assert_eq!(
            SymbolOwner::class("demo::nested::HTTPClient").method_symbol_name(&member),
            "boltffi_method_class_demo_nested_http_client_fetch"
        );
    }

    #[test]
    fn allocator_mints_fresh_ids() {
        let mut allocator = SymbolAllocator::new();
        let first = allocator
            .mint("boltffi_demo_one".to_owned())
            .expect("valid name");
        let second = allocator
            .mint("boltffi_demo_two".to_owned())
            .expect("valid name");
        assert_ne!(first.id(), second.id());
        assert_eq!(first.id().raw(), 0);
        assert_eq!(second.id().raw(), 1);
    }

    #[test]
    fn wasm_callback_import_name_uses_shared_callback_lane() {
        assert_eq!(
            wasm_callback_import_name("method", "demo_listener", "on_event"),
            "__boltffi_callback_method_demo_listener_on_event"
        );
    }

    #[test]
    fn async_lifecycle_symbol_names_append_runtime_suffixes() {
        let mut allocator = SymbolAllocator::new();

        assert_eq!(
            allocator
                .mint_async_lifecycle("boltffi_function_demo_spin", AsyncLifecycle::Poll)
                .expect("valid symbol")
                .name()
                .as_str(),
            "boltffi_async_function_demo_spin_poll"
        );
        assert_eq!(
            allocator
                .mint_async_lifecycle("boltffi_function_demo_spin", AsyncLifecycle::PollSync)
                .expect("valid symbol")
                .name()
                .as_str(),
            "boltffi_async_function_demo_spin_poll_sync"
        );
        assert_eq!(
            allocator
                .mint_async_lifecycle("boltffi_function_demo_spin", AsyncLifecycle::Complete)
                .expect("valid symbol")
                .name()
                .as_str(),
            "boltffi_async_function_demo_spin_complete"
        );
    }

    #[test]
    fn wasm_async_callback_names_use_start_import_and_complete_export() {
        let slot = CallbackSlot::from_method_name("onEvent");
        let mut allocator = SymbolAllocator::new();

        assert_eq!(
            slot.wasm_import_start_name("demo::Listener"),
            "__boltffi_callback_async_start_demo_listener_on_event"
        );
        assert_eq!(
            allocator
                .mint_callback_complete("demo::Listener", &slot)
                .expect("valid symbol")
                .name()
                .as_str(),
            "boltffi_callback_demo_listener_on_event_complete"
        );
    }
}
