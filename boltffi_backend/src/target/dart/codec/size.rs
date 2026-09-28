use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecSize, CustomTypeId, ElementCount, EnumId,
    MapKind, Native, Op, Primitive, RecordId, ValueRef,
};

use crate::core::{RenderContext, Result};

use super::{CStyleEnumRepresentation, ValueScope, primitive_size, value::binder_name};

pub struct Sizer<'context, 'bindings> {
    scope: ValueScope,
    context: &'context RenderContext<'bindings, Native>,
}

pub struct SizeExpression {
    source: String,
    value: SizeValue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SizeValue {
    String,
    Other,
}

impl<'context, 'bindings> Sizer<'context, 'bindings> {
    pub fn new(scope: ValueScope, context: &'context RenderContext<'bindings, Native>) -> Self {
        Self { scope, context }
    }

    fn value(&self, value: &ValueRef) -> Result<String> {
        self.scope.value(value)
    }
}

impl SizeExpression {
    pub fn into_source(self) -> String {
        self.source
    }

    fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            value: SizeValue::Other,
        }
    }

    fn string(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            value: SizeValue::String,
        }
    }

    fn custom_representation(mut self) -> Self {
        self.value = SizeValue::Other;
        self
    }

    fn result_error(self, binder: &str) -> String {
        match self.value {
            SizeValue::String => {
                format!("4 + $$convert.utf8.encode({binder}.message).length")
            }
            SizeValue::Other => self.source,
        }
    }
}

impl CodecSize for Sizer<'_, '_> {
    type Expr = Result<SizeExpression>;

    fn primitive(&mut self, primitive: Primitive, _: &ValueRef) -> Self::Expr {
        Ok(SizeExpression::new(primitive_size(primitive).to_string()))
    }

    fn string(&mut self, value: &ValueRef) -> Self::Expr {
        // Upper bound on UTF-8 byte length instead of an exact `utf8.encode(...).length`,
        // so sizing doesn't force a throwaway encode pass before the real one in `writeString`.
        // Matches the bound already used by `_m$wireEncodedSize()` for `BoltException`.
        Ok(SizeExpression::string(format!(
            "4 + (({}).length * 3)",
            self.value(value)?
        )))
    }

    fn interned_string(&mut self, _: &[String], _: &ValueRef) -> Self::Expr {
        unreachable!("InternedString codec size reached Dart renderer without host capability")
    }

    fn bytes(&mut self, value: &ValueRef) -> Self::Expr {
        Ok(SizeExpression::new(format!(
            "4 + {}.lengthInBytes",
            self.value(value)?
        )))
    }

    fn direct_record(&mut self, _: RecordId, value: &ValueRef) -> Self::Expr {
        Ok(SizeExpression::new(format!(
            "{}._m$wireEncodedSize()",
            self.value(value)?
        )))
    }

    fn encoded_record(&mut self, id: RecordId, value: &ValueRef) -> Self::Expr {
        self.direct_record(id, value)
    }

    fn c_style_enum(&mut self, id: EnumId, _: &ValueRef) -> Self::Expr {
        CStyleEnumRepresentation::resolve(id, self.context)
            .map(|representation| SizeExpression::new(representation.size().to_string()))
    }

    fn data_enum(&mut self, _: EnumId, value: &ValueRef) -> Self::Expr {
        Ok(SizeExpression::new(format!(
            "{}._m$wireEncodedSize()",
            self.value(value)?
        )))
    }

    fn class_handle(&mut self, _: ClassId, _: &ValueRef) -> Self::Expr {
        Ok(SizeExpression::new("8"))
    }

    fn callback_handle(&mut self, _: CallbackId, _: &ValueRef) -> Self::Expr {
        Ok(SizeExpression::new("16"))
    }

    fn custom<F>(&mut self, _: CustomTypeId, value: &ValueRef, representation: F) -> Self::Expr
    where
        F: FnOnce(&mut Self, &ValueRef) -> Self::Expr,
    {
        representation(self, value).map(SizeExpression::custom_representation)
    }

    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Self::Expr {
        match kind {
            BuiltinType::Duration | BuiltinType::SystemTime => Ok(SizeExpression::new("12")),
            BuiltinType::Uuid => Ok(SizeExpression::new("16")),
            BuiltinType::Url => Ok(SizeExpression::new(format!(
                "4 + (({}).toString().length * 3)",
                self.value(value)?
            ))),
        }
    }

    fn optional(&mut self, value: &ValueRef, binder: BinderId, inner: Self::Expr) -> Self::Expr {
        let value = self.value(value)?;
        let binder = binder_name(binder);
        let inner = inner?.source;
        // Constant inner sizes never read the binder, so skip the unwrap
        // local entirely; null-checked values are promoted without `!`.
        let source = if inner.contains(&binder) {
            format!(
                "1 + ({value} == null ? 0 : (() {{ final {binder} = {value}; return {inner}; }})())"
            )
        } else {
            format!("1 + ({value} == null ? 0 : {inner})")
        };
        Ok(SizeExpression::new(source))
    }

    fn sequence(
        &mut self,
        value: &ValueRef,
        _: &Op<ElementCount>,
        binder: BinderId,
        element: Self::Expr,
    ) -> Self::Expr {
        let value = self.value(value)?;
        let binder = binder_name(binder);
        let element = element?.source;
        let source = if element.contains(&binder) {
            format!("4 + {value}.fold<int>(0, (_l$size, {binder}) => _l$size + {element})")
        } else {
            format!("4 + {value}.length * {element}")
        };
        Ok(SizeExpression::new(source))
    }

    fn tuple(&mut self, _: &ValueRef, elements: Vec<Self::Expr>) -> Self::Expr {
        Ok(SizeExpression::new(
            elements
                .into_iter()
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(SizeExpression::into_source)
                .collect::<Vec<_>>()
                .join(" + "),
        ))
    }

    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Self::Expr,
        err: Self::Expr,
    ) -> Self::Expr {
        let binder = binder_name(binder);
        Ok(SizeExpression::new(format!(
            "1 + switch ({}) {{ $$BoltResult$Ok(value: final {}) => {}, $$BoltResult$Err(value: final {}) => {} }}",
            self.value(value)?,
            binder,
            ok?.source,
            binder,
            err?.result_error(&binder)
        )))
    }

    fn map(
        &mut self,
        _: MapKind,
        value: &ValueRef,
        key_binder: BinderId,
        key: Self::Expr,
        value_binder: BinderId,
        map_value: Self::Expr,
    ) -> Self::Expr {
        let value = self.value(value)?;
        let key_binder = binder_name(key_binder);
        let value_binder = binder_name(value_binder);
        let key = key?.source;
        let map_value = map_value?.source;
        let key_used = key.contains(&key_binder);
        let value_used = map_value.contains(&value_binder);
        let source = match (key_used, value_used) {
            (false, false) => format!("4 + {value}.length * ({key} + {map_value})"),
            _ => {
                let mut locals = String::new();
                if key_used {
                    locals.push_str(&format!("final {key_binder} = _l$entry.key; "));
                }
                if value_used {
                    locals.push_str(&format!("final {value_binder} = _l$entry.value; "));
                }
                format!(
                    "4 + {value}.entries.fold<int>(0, (_l$size, _l$entry) {{ {locals}return _l$size + {key} + {map_value}; }})"
                )
            }
        };
        Ok(SizeExpression::new(source))
    }
}
