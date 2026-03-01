use proc_macro2::Span;
use syn::{Expr, ExprPath, ExprUnsafe, ItemFn, Path, Type, spanned::Spanned, visit::Visit};

#[derive(Debug)]
pub struct SafetyViolation {
    pub span: Span,
    pub kind: ViolationKind,
}

#[derive(Debug)]
pub enum ViolationKind {
    UnsafeBlock,
    MemForget,
    MemTransmute,
    RawPointerType,
    BoxIntoRaw,
    BoxLeak,
    VecIntoRawParts,
    ManuallyDrop,
    StaticMut,
}

impl ViolationKind {
    fn message(&self) -> &'static str {
        match self {
            Self::UnsafeBlock => "unsafe blocks are not allowed in #[boltffi::export] functions",
            Self::MemForget => "mem::forget is not allowed - it can violate ownership semantics",
            Self::MemTransmute => "mem::transmute is not allowed - it can violate type safety",
            Self::RawPointerType => "raw pointer types (*const/*mut) are not allowed - use safe references",
            Self::BoxIntoRaw => "Box::into_raw is not allowed - ownership must flow through return",
            Self::BoxLeak => "Box::leak is not allowed - it creates untracked references",
            Self::VecIntoRawParts => "Vec::into_raw_parts is not allowed - use normal Vec returns",
            Self::ManuallyDrop => "ManuallyDrop is not allowed - let BoltFFI handle drop semantics",
            Self::StaticMut => "static mut access is not allowed - use thread-safe patterns",
        }
    }

    pub fn to_compile_error(&self, span: Span) -> proc_macro2::TokenStream {
        let msg = self.message();
        quote::quote_spanned!(span => compile_error!(#msg);)
    }
}

struct SafetyScanner {
    violations: Vec<SafetyViolation>,
}

impl SafetyScanner {
    fn new() -> Self {
        Self { violations: Vec::new() }
    }

    fn add_violation(&mut self, span: Span, kind: ViolationKind) {
        self.violations.push(SafetyViolation { span, kind });
    }

    fn check_path(&mut self, path: &Path) {
        let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        let full_path = segments.join("::");

        let prohibited = [
            ("mem::forget", ViolationKind::MemForget),
            ("std::mem::forget", ViolationKind::MemForget),
            ("core::mem::forget", ViolationKind::MemForget),
            ("mem::transmute", ViolationKind::MemTransmute),
            ("std::mem::transmute", ViolationKind::MemTransmute),
            ("core::mem::transmute", ViolationKind::MemTransmute),
            ("ManuallyDrop", ViolationKind::ManuallyDrop),
            ("std::mem::ManuallyDrop", ViolationKind::ManuallyDrop),
            ("core::mem::ManuallyDrop", ViolationKind::ManuallyDrop),
        ];

        for (pattern, kind) in prohibited {
            if full_path.ends_with(pattern) || full_path == pattern {
                self.add_violation(path.span(), kind);
                return;
            }
        }

        if let Some(last) = segments.last() {
            match last.as_str() {
                "forget" if segments.iter().any(|s| s == "mem") => {
                    self.add_violation(path.span(), ViolationKind::MemForget);
                }
                "transmute" if segments.iter().any(|s| s == "mem") => {
                    self.add_violation(path.span(), ViolationKind::MemTransmute);
                }
                "into_raw" if segments.iter().any(|s| s == "Box") => {
                    self.add_violation(path.span(), ViolationKind::BoxIntoRaw);
                }
                "leak" if segments.iter().any(|s| s == "Box") => {
                    self.add_violation(path.span(), ViolationKind::BoxLeak);
                }
                "into_raw_parts" if segments.iter().any(|s| s == "Vec") => {
                    self.add_violation(path.span(), ViolationKind::VecIntoRawParts);
                }
                "new" if segments.iter().any(|s| s == "ManuallyDrop") => {
                    self.add_violation(path.span(), ViolationKind::ManuallyDrop);
                }
                _ => {}
            }
        }

        if segments.iter().any(|s| s == "ManuallyDrop") {
            self.add_violation(path.span(), ViolationKind::ManuallyDrop);
        }
    }

    fn check_type(&mut self, ty: &Type) {
        match ty {
            Type::Ptr(ptr) => {
                self.add_violation(ptr.span(), ViolationKind::RawPointerType);
            }
            Type::Path(type_path) => {
                let segments: Vec<_> = type_path.path.segments.iter().map(|s| s.ident.to_string()).collect();

                if segments.iter().any(|s| s == "ManuallyDrop") {
                    self.add_violation(type_path.span(), ViolationKind::ManuallyDrop);
                }
            }
            _ => {}
        }
    }
}

impl<'ast> Visit<'ast> for SafetyScanner {
    fn visit_expr_unsafe(&mut self, node: &'ast ExprUnsafe) {
        self.add_violation(node.span(), ViolationKind::UnsafeBlock);
    }

    fn visit_expr_path(&mut self, node: &'ast ExprPath) {
        self.check_path(&node.path);
        syn::visit::visit_expr_path(self, node);
    }

    fn visit_expr(&mut self, node: &'ast Expr) {
        if let Expr::MethodCall(method) = node {
            let method_name = method.method.to_string();
            match method_name.as_str() {
                "into_raw" => {
                    self.add_violation(method.span(), ViolationKind::BoxIntoRaw);
                }
                "leak" => {
                    self.add_violation(method.span(), ViolationKind::BoxLeak);
                }
                "into_raw_parts" => {
                    self.add_violation(method.span(), ViolationKind::VecIntoRawParts);
                }
                _ => {}
            }
        }
        syn::visit::visit_expr(self, node);
    }

    fn visit_type(&mut self, node: &'ast Type) {
        self.check_type(node);
        syn::visit::visit_type(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        if matches!(node.mutability, syn::StaticMutability::Mut(_)) {
            self.add_violation(node.span(), ViolationKind::StaticMut);
        }
        syn::visit::visit_item_static(self, node);
    }
}

pub fn scan_function(func: &ItemFn) -> Vec<SafetyViolation> {
    let mut scanner = SafetyScanner::new();
    scanner.visit_block(&func.block);
    scanner.violations
}

pub fn violations_to_compile_errors(violations: &[SafetyViolation]) -> proc_macro2::TokenStream {
    violations.iter().map(|v| v.kind.to_compile_error(v.span)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_scan(code: &str) -> Vec<SafetyViolation> {
        let func: ItemFn = syn::parse_str(code).expect("failed to parse");
        scan_function(&func)
    }

    #[test]
    fn test_safe_function_passes() {
        let violations = parse_and_scan(
            r#"
            fn safe_fn() -> Vec<u8> {
                vec![1, 2, 3]
            }
        "#,
        );
        assert!(violations.is_empty());
    }

    #[test]
    fn test_detects_unsafe_block() {
        let violations = parse_and_scan(
            r#"
            fn unsafe_fn() {
                unsafe { std::ptr::null::<u8>(); }
            }
        "#,
        );
        assert_eq!(violations.len(), 1);
        assert!(matches!(violations[0].kind, ViolationKind::UnsafeBlock));
    }

    #[test]
    fn test_detects_mem_forget() {
        let violations = parse_and_scan(
            r#"
            fn forget_fn() {
                let v = vec![1, 2, 3];
                std::mem::forget(v);
            }
        "#,
        );
        assert!(violations.iter().any(|v| matches!(v.kind, ViolationKind::MemForget)));
    }

    #[test]
    fn test_detects_raw_pointer() {
        let violations = parse_and_scan(
            r#"
            fn ptr_fn() {
                let p: *const u8 = std::ptr::null();
            }
        "#,
        );
        assert!(violations.iter().any(|v| matches!(v.kind, ViolationKind::RawPointerType)));
    }

    #[test]
    fn test_detects_box_into_raw() {
        let violations = parse_and_scan(
            r#"
            fn box_fn() {
                let b = Box::new(42);
                let _ = Box::into_raw(b);
            }
        "#,
        );
        assert!(violations.iter().any(|v| matches!(v.kind, ViolationKind::BoxIntoRaw)));
    }

    #[test]
    fn test_detects_method_into_raw() {
        let violations = parse_and_scan(
            r#"
            fn box_method_fn() {
                let b = Box::new(42);
                let _ = b.into_raw();
            }
        "#,
        );
        assert!(!violations.is_empty());
    }

    #[test]
    fn test_detects_manually_drop() {
        let violations = parse_and_scan(
            r#"
            fn manual_fn() {
                let md = std::mem::ManuallyDrop::new(42);
            }
        "#,
        );
        assert!(violations.iter().any(|v| matches!(v.kind, ViolationKind::ManuallyDrop)));
    }
}
