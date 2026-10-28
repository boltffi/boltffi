use boltffi_binding::{
    CallbackDecl, ClassDecl, ConstantDecl, ConstantValueDecl, CustomTypeDecl, DefaultValue,
    DirectValueType, DirectVectorElementType, Direction, EnumDecl, ExecutionDecl, ExportedCallable,
    FunctionDecl, HandlePresence, ImportedCallable, ParamDirection, ParamPlan, RecordDecl,
    ReturnPlan, StreamDecl, StreamItemPlan, TypeRef, Wasm32,
};

use crate::core::{
    CoverageMode, Diagnostic, Emitted, Error, FileLayout, FilePath, FilePlan, GeneratedOutput,
    RenderContext, RenderedDeclaration, Result,
};

use super::interop;
use super::name_style::Name;

fn unsupported(shape: &'static str) -> Error {
    Error::UnsupportedTarget {
        target: "dart_web",
        shape,
    }
}

mod call;
mod callback;
mod class;
mod constant;
mod custom_type;
mod function;
mod model;
mod module;
mod stream;

use call::{
    CallSignature, call_signature, callback_method_signature, direct_primitive, error_caught_value,
    error_exception_type, error_throw_expression, web_default,
};
use model::field_dart_name;

pub use callback::Callback;
pub use class::Class;
pub use constant::Constant;
pub use custom_type::CustomType;
pub use function::Function;
pub use model::{Enumeration, Record};
pub use module::Module;
pub use stream::Stream;

#[cfg(test)]
mod tests {
    use crate::target::dart::default_value::float_literal as render_float_literal;

    #[test]
    fn renders_finite_floats_with_debug_formatting() {
        assert_eq!(render_float_literal(1.5), "1.5");
        assert_eq!(render_float_literal(0.0), "0.0");
    }

    #[test]
    fn renders_nan_and_infinity_as_dart_double_constants() {
        assert_eq!(render_float_literal(f64::NAN), "double.nan");
        assert_eq!(render_float_literal(f64::INFINITY), "double.infinity");
        assert_eq!(
            render_float_literal(f64::NEG_INFINITY),
            "double.negativeInfinity"
        );
    }
}
