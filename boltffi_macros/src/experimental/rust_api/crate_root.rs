use std::collections::HashMap;

use boltffi_ast::{
    BaseTrait, ConstantDef, CustomTypeConverter, CustomTypeDef, EnumDef, FieldDef, FnSig,
    FunctionDef, MethodDef, PackageInfo, Path, PathRoot, PathSegment, RecordDef, ReturnDef,
    SourceContract, StreamDef, TraitBounds, TraitDef, TypeExpr, VariantPayload,
};

pub struct RootModuleTypes {
    crate_name: String,
    visible_paths: HashMap<String, Path>,
}

impl RootModuleTypes {
    pub fn new(package: &PackageInfo) -> Self {
        Self {
            crate_name: package.name.replace('-', "_"),
            visible_paths: HashMap::new(),
        }
    }

    pub fn with_visible_paths(
        package: &PackageInfo,
        paths: impl IntoIterator<Item = (String, Path)>,
    ) -> Self {
        Self {
            crate_name: package.name.replace('-', "_"),
            visible_paths: paths.into_iter().collect(),
        }
    }

    pub fn contract(&self, source: &SourceContract) -> SourceContract {
        let mut source = source.clone();
        source
            .records
            .iter_mut()
            .for_each(|record| self.record(record));
        source
            .enums
            .iter_mut()
            .for_each(|enumeration| self.enumeration(enumeration));
        source
            .functions
            .iter_mut()
            .for_each(|function| self.function(function));
        source
            .classes
            .iter_mut()
            .for_each(|class| self.class(class));
        source
            .traits
            .iter_mut()
            .for_each(|callback| self.callback(callback));
        source
            .streams
            .iter_mut()
            .for_each(|stream| self.stream(stream));
        source
            .constants
            .iter_mut()
            .for_each(|constant| self.constant(constant));
        source
            .customs
            .iter_mut()
            .for_each(|custom| self.custom(custom));
        source
    }
}

impl RootModuleTypes {
    fn record(&self, record: &mut RecordDef) {
        let self_type = TypeExpr::record(
            record.id.clone(),
            self.declaration_path(record.id.as_str(), record.name.spelling()),
        );
        record.fields.iter_mut().for_each(|field| self.field(field));
        record
            .methods
            .iter_mut()
            .for_each(|method| self.method_with_self(method, &self_type));
    }

    fn enumeration(&self, enumeration: &mut EnumDef) {
        let self_type = TypeExpr::enumeration(
            enumeration.id.clone(),
            self.declaration_path(enumeration.id.as_str(), enumeration.name.spelling()),
        );
        enumeration
            .variants
            .iter_mut()
            .for_each(|variant| match &mut variant.payload {
                VariantPayload::Unit => {}
                VariantPayload::Tuple(elements) => elements
                    .iter_mut()
                    .for_each(|element| self.type_expr(element)),
                VariantPayload::Struct(fields) => {
                    fields.iter_mut().for_each(|field| self.field(field));
                }
            });
        enumeration
            .methods
            .iter_mut()
            .for_each(|method| self.method_with_self(method, &self_type));
    }

    fn class(&self, class: &mut boltffi_ast::ClassDef) {
        let self_type = TypeExpr::class(
            class.id.clone(),
            self.declaration_path(class.id.as_str(), class.name.spelling()),
        );
        class
            .methods
            .iter_mut()
            .for_each(|method| self.method_with_self(method, &self_type));
    }

    fn function(&self, function: &mut FunctionDef) {
        function
            .parameters
            .iter_mut()
            .for_each(|parameter| self.parameter(parameter));
        self.return_def(&mut function.returns);
    }

    fn callback(&self, callback: &mut TraitDef) {
        callback
            .methods
            .iter_mut()
            .for_each(|method| self.method(method));
    }

    fn stream(&self, stream: &mut StreamDef) {
        self.type_expr(&mut stream.item_type);
    }

    fn constant(&self, constant: &mut ConstantDef) {
        self.type_expr(&mut constant.type_expr);
    }

    fn custom(&self, custom: &mut CustomTypeDef) {
        self.type_expr(&mut custom.repr);
        self.custom_converter(custom.id.as_str(), &mut custom.converters.into_ffi);
        self.custom_converter(custom.id.as_str(), &mut custom.converters.try_from_ffi);
    }

    fn method(&self, method: &mut MethodDef) {
        self.method_with_optional_self(method, None);
    }

    fn method_with_self(&self, method: &mut MethodDef, self_type: &TypeExpr) {
        self.method_with_optional_self(method, Some(self_type));
    }

    fn method_with_optional_self(&self, method: &mut MethodDef, self_type: Option<&TypeExpr>) {
        method
            .parameters
            .iter_mut()
            .for_each(|parameter| self.parameter_with_self(parameter, self_type));
        self.return_def_with_self(&mut method.returns, self_type);
    }

    fn field(&self, field: &mut FieldDef) {
        self.type_expr(&mut field.type_expr);
    }

    fn parameter(&self, parameter: &mut boltffi_ast::ParameterDef) {
        self.type_expr(&mut parameter.type_expr);
    }

    fn parameter_with_self(
        &self,
        parameter: &mut boltffi_ast::ParameterDef,
        self_type: Option<&TypeExpr>,
    ) {
        self.type_expr_with_self(&mut parameter.type_expr, self_type);
    }

    fn return_def(&self, return_def: &mut ReturnDef) {
        self.return_def_with_self(return_def, None);
    }

    fn return_def_with_self(&self, return_def: &mut ReturnDef, self_type: Option<&TypeExpr>) {
        if let ReturnDef::Value(type_expr) = return_def {
            self.type_expr_with_self(type_expr, self_type);
        }
    }

    fn type_expr(&self, type_expr: &mut TypeExpr) {
        self.type_expr_with_self(type_expr, None);
    }

    fn type_expr_with_self(&self, type_expr: &mut TypeExpr, self_type: Option<&TypeExpr>) {
        if matches!(type_expr, TypeExpr::SelfType) {
            if let Some(self_type) = self_type {
                *type_expr = self_type.clone();
            }
            return;
        }

        match type_expr {
            TypeExpr::Record { id, path } => self.root_declaration_path(id.as_str(), path),
            TypeExpr::Enum { id, path } => self.root_declaration_path(id.as_str(), path),
            TypeExpr::Class { id, path } => self.root_declaration_path(id.as_str(), path),
            TypeExpr::Custom { id, path } => self.custom_path(id.as_str(), path),
            TypeExpr::InternedString { pool_id, pool, .. } => {
                self.root_declaration_path(pool_id, pool);
            }
            TypeExpr::Dyn(bounds) | TypeExpr::ImplTrait(bounds) => {
                self.trait_bounds_with_self(bounds, self_type)
            }
            TypeExpr::Boxed(inner)
            | TypeExpr::Arc(inner)
            | TypeExpr::Vec(inner)
            | TypeExpr::Slice(inner)
            | TypeExpr::Option(inner) => self.type_expr_with_self(inner, self_type),
            TypeExpr::FnPtr(signature) => self.signature_with_self(signature, self_type),
            TypeExpr::Result { ok, err } => {
                self.type_expr_with_self(ok, self_type);
                self.type_expr_with_self(err, self_type);
            }
            TypeExpr::Tuple(elements) => {
                elements
                    .iter_mut()
                    .for_each(|element| self.type_expr_with_self(element, self_type));
            }
            TypeExpr::Map { key, value, .. } => {
                self.type_expr_with_self(key, self_type);
                self.type_expr_with_self(value, self_type);
            }
            TypeExpr::Primitive(_)
            | TypeExpr::Unit
            | TypeExpr::String
            | TypeExpr::Str
            | TypeExpr::Builtin(_)
            | TypeExpr::SelfType
            | TypeExpr::Parameter(_) => {}
        }
    }

    fn trait_bounds(&self, bounds: &mut TraitBounds) {
        self.trait_bounds_with_self(bounds, None);
    }

    fn trait_bounds_with_self(&self, bounds: &mut TraitBounds, self_type: Option<&TypeExpr>) {
        match &mut bounds.base {
            BaseTrait::Named { id, path } => self.root_declaration_path(id.as_str(), path),
            BaseTrait::Function(function) => {
                self.signature_with_self(&mut function.signature, self_type)
            }
        }
    }

    fn signature(&self, signature: &mut FnSig) {
        self.signature_with_self(signature, None);
    }

    fn signature_with_self(&self, signature: &mut FnSig, self_type: Option<&TypeExpr>) {
        signature
            .parameters
            .iter_mut()
            .for_each(|parameter| self.type_expr_with_self(parameter, self_type));
        self.return_def_with_self(&mut signature.returns, self_type);
    }

    fn declaration_path(&self, id: &str, spelling: &str) -> Path {
        let mut path = Path::single(spelling);
        self.root_declaration_path(id, &mut path);
        path
    }

    fn root_declaration_path(&self, id: &str, path: &mut Path) {
        if let Some(visible_path) = self.visible_paths.get(id) {
            *path = visible_path.clone();
            return;
        }
        let segments = id.split("::").collect::<Vec<_>>();
        if segments.first().copied() != Some(self.crate_name.as_str()) {
            return;
        }
        let segments = segments.into_iter().skip(1).map(PathSegment::new).collect();
        *path = Path::new(PathRoot::Crate, segments);
    }

    fn custom_path(&self, id: &str, path: &mut Path) {
        let same_leaf = id
            .rsplit("::")
            .next()
            .zip(path.segments.last())
            .is_some_and(|(id_leaf, path_leaf)| id_leaf == path_leaf.name.as_str());
        if same_leaf {
            self.root_declaration_path(id, path);
        }
    }

    fn custom_converter(&self, id: &str, converter: &mut CustomTypeConverter) {
        if let CustomTypeConverter::TraitMethod(converter) = converter {
            self.custom_path(id, &mut converter.receiver);
        }
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        BaseTrait, CanonicalName, FunctionDef, FunctionId, PackageInfo, ParameterDef, PathRoot,
        RecordDef, RecordId, ReturnDef, SourceContract, TraitBounds, TraitId, TypeExpr,
    };

    use super::RootModuleTypes;

    #[test]
    fn rewrites_root_declared_type_paths_to_crate_root() {
        let mut contract = SourceContract::new(PackageInfo::new("demo", None));
        contract.records.push(RecordDef::new(
            RecordId::new("demo::records::Point"),
            CanonicalName::single("Point"),
        ));
        let mut function = FunctionDef::new(
            FunctionId::new("demo::distance"),
            CanonicalName::single("distance"),
        );
        function.parameters.push(ParameterDef::value(
            CanonicalName::single("point"),
            TypeExpr::record(
                RecordId::new("demo::records::Point"),
                boltffi_ast::Path::single("Point"),
            ),
        ));
        contract.functions.push(function);

        let rooted = RootModuleTypes::new(&contract.package).contract(&contract);
        let TypeExpr::Record { path, .. } = &rooted.functions[0].parameters[0].type_expr else {
            panic!("expected record type");
        };

        assert_eq!(path.root, PathRoot::Crate);
        assert_eq!(
            path.segments
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>(),
            vec!["records", "Point"]
        );
    }

    #[test]
    fn rewrites_interned_string_pool_paths_to_crate_root() {
        let mut contract = SourceContract::new(PackageInfo::new("demo", None));
        let mut function = FunctionDef::new(
            FunctionId::new("demo::api::browser"),
            CanonicalName::single("browser"),
        );
        function.returns = ReturnDef::value(TypeExpr::interned_string(
            boltffi_ast::Path::single("InternedString"),
            "demo::pools::BrowserName",
            boltffi_ast::Path::single("BrowserName"),
            vec!["Chrome".to_owned()],
        ));
        contract.functions.push(function);

        let rooted = RootModuleTypes::new(&contract.package).contract(&contract);
        let ReturnDef::Value(TypeExpr::InternedString { pool, pool_id, .. }) =
            &rooted.functions[0].returns
        else {
            panic!("expected interned string return");
        };
        assert_eq!(pool_id, "demo::pools::BrowserName");
        assert_eq!(pool.root, PathRoot::Crate);
        assert_eq!(
            pool.segments
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>(),
            vec!["pools", "BrowserName"]
        );
    }

    #[test]
    fn keeps_dependency_declared_type_paths_in_source_form() {
        let mut contract = SourceContract::new(PackageInfo::new("demo", None));
        let mut function = FunctionDef::new(
            FunctionId::new("demo::format"),
            CanonicalName::single("format"),
        );
        function.returns = ReturnDef::value(TypeExpr::impl_trait(
            TraitId::new("demo_multicrate_model::ForeignLabeler"),
            boltffi_ast::Path::single("ForeignLabeler"),
        ));
        contract.functions.push(function);

        let rooted = RootModuleTypes::new(&contract.package).contract(&contract);
        let ReturnDef::Value(TypeExpr::ImplTrait(TraitBounds {
            base: BaseTrait::Named { path, .. },
            ..
        })) = &rooted.functions[0].returns
        else {
            panic!("expected impl trait return");
        };

        assert_eq!(path.root, PathRoot::Relative);
        assert_eq!(
            path.segments
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>(),
            vec!["ForeignLabeler"]
        );
    }

    #[test]
    fn rewrites_dependency_declared_type_paths_when_visible_from_root() {
        let mut contract = SourceContract::new(PackageInfo::new("demo", None));
        let mut function = FunctionDef::new(
            FunctionId::new("demo::format"),
            CanonicalName::single("format"),
        );
        function.returns = ReturnDef::value(TypeExpr::impl_trait(
            TraitId::new("demo_multicrate_model::ForeignLabeler"),
            boltffi_ast::Path::single("ForeignLabeler"),
        ));
        contract.functions.push(function);

        let rooted = RootModuleTypes::with_visible_paths(
            &contract.package,
            [(
                "demo_multicrate_model::ForeignLabeler".to_owned(),
                boltffi_ast::Path::new(
                    PathRoot::Relative,
                    vec![
                        boltffi_ast::PathSegment::new("demo_multicrate_session"),
                        boltffi_ast::PathSegment::new("ForeignLabeler"),
                    ],
                ),
            )],
        )
        .contract(&contract);
        let ReturnDef::Value(TypeExpr::ImplTrait(TraitBounds {
            base: BaseTrait::Named { path, .. },
            ..
        })) = &rooted.functions[0].returns
        else {
            panic!("expected impl trait return");
        };

        assert_eq!(path.root, PathRoot::Relative);
        assert_eq!(
            path.segments
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>(),
            vec!["demo_multicrate_session", "ForeignLabeler"]
        );
    }

    #[test]
    fn rewrites_same_leaf_custom_type_paths_when_visible_from_root() {
        let mut contract = SourceContract::new(PackageInfo::new("demo", None));
        let mut function = FunctionDef::new(
            FunctionId::new("demo::format"),
            CanonicalName::single("format"),
        );
        function.parameters.push(ParameterDef::value(
            CanonicalName::single("code"),
            TypeExpr::custom(
                boltffi_ast::CustomTypeId::new("demo_multicrate_model::ForeignCode"),
                boltffi_ast::Path::single("ForeignCode"),
            ),
        ));
        contract.functions.push(function);

        let rooted = RootModuleTypes::with_visible_paths(
            &contract.package,
            [(
                "demo_multicrate_model::ForeignCode".to_owned(),
                boltffi_ast::Path::new(
                    PathRoot::Relative,
                    vec![
                        boltffi_ast::PathSegment::new("demo_multicrate_session"),
                        boltffi_ast::PathSegment::new("ForeignCode"),
                    ],
                ),
            )],
        )
        .contract(&contract);
        let TypeExpr::Custom { path, .. } = &rooted.functions[0].parameters[0].type_expr else {
            panic!("expected custom type");
        };

        assert_eq!(path.root, PathRoot::Relative);
        assert_eq!(
            path.segments
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>(),
            vec!["demo_multicrate_session", "ForeignCode"]
        );
    }
}
