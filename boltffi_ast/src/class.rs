use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    ClassId, DeprecationInfo, DocComment, ExecutionKind, MethodDef, Receiver, Source, SourceName,
    SourceSpan, UserAttr,
};

/// A class-style Rust object exported through BoltFFI.
///
/// A class groups associated functions and methods around an owned
/// Rust value. Associated functions that return `Self` stay as methods here;
/// binding layers can present them as creation entry points later.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ClassDef {
    /// Stable class identity derived from the canonical Rust path.
    pub id: ClassId,
    /// Source class name.
    pub name: SourceName,
    /// Methods attached to the class.
    pub methods: Vec<MethodDef>,
    /// Thread-safety policy collected from exported class impl blocks.
    #[serde(default)]
    pub thread_safety: ClassThreadSafety,
    /// User attributes preserved from the class declaration.
    pub user_attrs: Vec<UserAttr>,
    /// Documentation attached to the class.
    pub doc: Option<DocComment>,
    /// Deprecation metadata attached to the class.
    pub deprecated: Option<DeprecationInfo>,
    /// Visibility and source location for diagnostics.
    pub source: Source,
    /// Span available during macro expansion.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub source_span: Option<SourceSpan>,
}

/// Thread-safety policy for an exported class.
///
/// Classes require `Send + Sync` unless every exported impl block declares
/// single-threaded access.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ClassThreadSafety {
    /// The Rust class type must implement `Send + Sync`.
    #[default]
    RequireSendSync,
    /// The class is exported without a `Send + Sync` assertion.
    UnsafeSingleThreaded,
}

/// An async instance method rejected by a single-threaded class contract.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SingleThreadedAsyncMethod {
    class: String,
    method: String,
}

impl SingleThreadedAsyncMethod {
    /// Creates a violation for the named class and method.
    pub fn new(class: impl Into<String>, method: impl Into<String>) -> Self {
        Self {
            class: class.into(),
            method: method.into(),
        }
    }

    /// Returns the class name.
    pub fn class(&self) -> &str {
        self.class.as_str()
    }

    /// Returns the method name.
    pub fn method(&self) -> &str {
        self.method.as_str()
    }
}

impl fmt::Display for SingleThreadedAsyncMethod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "async instance method `{}::{}` cannot be exported from `#[export(single_threaded)]`: its receiver would remain inside a future that foreign runtimes may resume on another thread; single-threaded classes support synchronous instance methods only",
            self.class, self.method
        )
    }
}

impl ClassThreadSafety {
    /// Merges policies collected from multiple impl blocks.
    pub const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::UnsafeSingleThreaded, Self::UnsafeSingleThreaded) => Self::UnsafeSingleThreaded,
            _ => Self::RequireSendSync,
        }
    }
}

impl ClassDef {
    /// Builds an empty class definition.
    ///
    /// The `id` parameter is the stable class ID. The `name` parameter is the
    /// canonical source name.
    ///
    /// Returns a class with no methods, attributes, or docs.
    pub fn new(id: ClassId, name: impl Into<SourceName>) -> Self {
        Self {
            id,
            name: name.into(),
            methods: Vec::new(),
            thread_safety: ClassThreadSafety::default(),
            user_attrs: Vec::new(),
            doc: None,
            deprecated: None,
            source: Source::exported(),
            source_span: None,
        }
    }

    /// Returns the invalid async instance method in a single-threaded export.
    pub fn single_threaded_async_method(&self) -> Option<SingleThreadedAsyncMethod> {
        if self.thread_safety != ClassThreadSafety::UnsafeSingleThreaded {
            return None;
        }

        self.methods
            .iter()
            .find(|method| {
                method.execution == ExecutionKind::Async && method.receiver != Receiver::None
            })
            .map(|method| {
                SingleThreadedAsyncMethod::new(self.name.spelling(), method.name.spelling())
            })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, to_value};

    use super::*;
    use crate::CanonicalName;

    #[test]
    fn missing_thread_safety_deserializes_to_required_send_sync() {
        let mut value = to_value(ClassDef::new(
            ClassId::new("demo::Engine"),
            CanonicalName::single("Engine"),
        ))
        .expect("class serializes");
        let Value::Object(fields) = &mut value else {
            panic!("serialized class must be an object");
        };
        fields.remove("thread_safety");

        let class = serde_json::from_value::<ClassDef>(value).expect("class deserializes");

        assert_eq!(class.thread_safety, ClassThreadSafety::RequireSendSync);
    }

    #[test]
    fn single_threaded_async_method_finds_every_instance_receiver() {
        [Receiver::Shared, Receiver::Mutable, Receiver::Owned]
            .into_iter()
            .for_each(|receiver| {
                let mut method = MethodDef::new(
                    crate::MethodId::new("load"),
                    CanonicalName::single("load"),
                    receiver,
                );
                method.execution = ExecutionKind::Async;
                let mut class = ClassDef::new(
                    ClassId::new("demo::Engine"),
                    CanonicalName::single("Engine"),
                );
                class.thread_safety = ClassThreadSafety::UnsafeSingleThreaded;
                class.methods.push(method);

                assert_eq!(
                    class
                        .single_threaded_async_method()
                        .map(|violation| violation.method().to_owned()),
                    Some("load".to_owned())
                );
            });
    }

    #[test]
    fn single_threaded_async_method_allows_static_async_and_synchronous_instance_methods() {
        let mut static_async = MethodDef::new(
            crate::MethodId::new("load"),
            CanonicalName::single("load"),
            Receiver::None,
        );
        static_async.execution = ExecutionKind::Async;
        let synchronous = MethodDef::new(
            crate::MethodId::new("get"),
            CanonicalName::single("get"),
            Receiver::Shared,
        );
        let mut class = ClassDef::new(
            ClassId::new("demo::Engine"),
            CanonicalName::single("Engine"),
        );
        class.thread_safety = ClassThreadSafety::UnsafeSingleThreaded;
        class.methods = vec![static_async, synchronous];

        assert!(class.single_threaded_async_method().is_none());
    }
}
