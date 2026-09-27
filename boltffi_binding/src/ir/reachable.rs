//! Narrowing an aggregated contract to what the bindings' seed crates declare and reach.

use std::collections::HashSet;

use boltffi_ast::{
    BaseTrait, ConstantOwner, FnSig, MethodDef, ParameterDef, ReturnDef, SourceContract, TypeExpr,
    VariantPayload,
};

/// Keeps every declaration of the `seed_crates` and every declaration they reach through
/// a type. Other crates contribute only what the seeds name.
pub fn reachable_from<'seed>(
    contract: SourceContract,
    seed_crates: impl IntoIterator<Item = &'seed str>,
) -> SourceContract {
    let mut reached = Reached::default();
    let prefixes = seed_crates
        .into_iter()
        .map(|seed| format!("{seed}::"))
        .collect::<Vec<_>>();
    let owned = |id: &str| prefixes.iter().any(|prefix| id.starts_with(prefix));
    contract
        .records
        .iter()
        .map(|record| record.id.as_str())
        .chain(
            contract
                .enums
                .iter()
                .map(|enumeration| enumeration.id.as_str()),
        )
        .chain(contract.classes.iter().map(|class| class.id.as_str()))
        .chain(contract.traits.iter().map(|r#trait| r#trait.id.as_str()))
        .chain(contract.customs.iter().map(|custom| custom.id.as_str()))
        .chain(
            contract
                .functions
                .iter()
                .map(|function| function.id.as_str()),
        )
        .chain(contract.streams.iter().map(|stream| stream.id.as_str()))
        .chain(
            contract
                .constants
                .iter()
                .map(|constant| constant.id.as_str()),
        )
        .filter(|id| owned(id))
        .for_each(|id| reached.pending.push(id.to_owned()));
    while let Some(id) = reached.pending.pop() {
        if !reached.ids.insert(id.clone()) {
            continue;
        }
        reached.declaration(&contract, &id);
    }
    let keep = |id: &str| reached.ids.contains(id);
    let mut contract = contract;
    contract.records.retain(|record| keep(record.id.as_str()));
    contract
        .enums
        .retain(|enumeration| keep(enumeration.id.as_str()));
    contract.classes.retain(|class| keep(class.id.as_str()));
    contract.traits.retain(|r#trait| keep(r#trait.id.as_str()));
    contract.customs.retain(|custom| keep(custom.id.as_str()));
    contract
        .functions
        .retain(|function| keep(function.id.as_str()));
    contract.streams.retain(|stream| keep(stream.id.as_str()));
    contract
        .constants
        .retain(|constant| keep(constant.id.as_str()));
    contract
}

#[derive(Default)]
struct Reached {
    ids: HashSet<String>,
    pending: Vec<String>,
}

impl Reached {
    fn declaration(&mut self, contract: &SourceContract, id: &str) {
        contract
            .records
            .iter()
            .filter(|record| record.id.as_str() == id)
            .for_each(|record| {
                record
                    .fields
                    .iter()
                    .for_each(|field| self.ty(&field.type_expr));
                self.methods(&record.methods);
            });
        contract
            .enums
            .iter()
            .filter(|enumeration| enumeration.id.as_str() == id)
            .for_each(|enumeration| {
                enumeration
                    .variants
                    .iter()
                    .for_each(|variant| match &variant.payload {
                        VariantPayload::Unit => {}
                        VariantPayload::Tuple(types) => types.iter().for_each(|ty| self.ty(ty)),
                        VariantPayload::Struct(fields) => {
                            fields.iter().for_each(|field| self.ty(&field.type_expr))
                        }
                    });
                self.methods(&enumeration.methods);
            });
        contract
            .classes
            .iter()
            .filter(|class| class.id.as_str() == id)
            .for_each(|class| self.methods(&class.methods));
        contract
            .traits
            .iter()
            .filter(|r#trait| r#trait.id.as_str() == id)
            .for_each(|r#trait| self.methods(&r#trait.methods));
        contract
            .customs
            .iter()
            .filter(|custom| custom.id.as_str() == id)
            .for_each(|custom| self.ty(&custom.repr));
        contract
            .functions
            .iter()
            .filter(|function| function.id.as_str() == id)
            .for_each(|function| self.callable(&function.parameters, &function.returns));
        contract
            .streams
            .iter()
            .filter(|stream| stream.id.as_str() == id)
            .for_each(|stream| {
                self.ty(&stream.item_type);
                if let Some(owner) = &stream.owner {
                    self.pending.push(owner.as_str().to_owned());
                }
            });
        contract
            .streams
            .iter()
            .filter(|stream| {
                stream
                    .owner
                    .as_ref()
                    .is_some_and(|owner| owner.as_str() == id)
            })
            .for_each(|stream| self.pending.push(stream.id.as_str().to_owned()));
        contract
            .constants
            .iter()
            .filter(|constant| constant.id.as_str() == id)
            .for_each(|constant| {
                self.ty(&constant.type_expr);
                if let Some(owner) = &constant.owner {
                    self.pending.push(owner_id(owner).to_owned());
                }
            });
        contract
            .constants
            .iter()
            .filter(|constant| {
                constant
                    .owner
                    .as_ref()
                    .is_some_and(|owner| owner_id(owner) == id)
            })
            .for_each(|constant| self.pending.push(constant.id.as_str().to_owned()));
    }

    fn methods(&mut self, methods: &[MethodDef]) {
        methods
            .iter()
            .for_each(|method| self.callable(&method.parameters, &method.returns));
    }

    fn callable(&mut self, parameters: &[ParameterDef], returns: &ReturnDef) {
        parameters
            .iter()
            .for_each(|parameter| self.ty(&parameter.type_expr));
        if let ReturnDef::Value(ty) = returns {
            self.ty(ty);
        }
    }

    fn signature(&mut self, signature: &FnSig) {
        signature.parameters.iter().for_each(|ty| self.ty(ty));
        if let ReturnDef::Value(ty) = &signature.returns {
            self.ty(ty);
        }
    }

    fn ty(&mut self, ty: &TypeExpr) {
        match ty {
            TypeExpr::Record { id, .. } => self.pending.push(id.as_str().to_owned()),
            TypeExpr::Enum { id, .. } => self.pending.push(id.as_str().to_owned()),
            TypeExpr::Class { id, .. } => self.pending.push(id.as_str().to_owned()),
            TypeExpr::Custom { id, .. } => self.pending.push(id.as_str().to_owned()),
            TypeExpr::Boxed(inner)
            | TypeExpr::Arc(inner)
            | TypeExpr::Vec(inner)
            | TypeExpr::Slice(inner)
            | TypeExpr::Option(inner) => self.ty(inner),
            TypeExpr::Result { ok, err } => {
                self.ty(ok);
                self.ty(err);
            }
            TypeExpr::Map { key, value, .. } => {
                self.ty(key);
                self.ty(value);
            }
            TypeExpr::Tuple(elements) => elements.iter().for_each(|element| self.ty(element)),
            TypeExpr::FnPtr(signature) => self.signature(signature),
            TypeExpr::Dyn(bounds) | TypeExpr::ImplTrait(bounds) => match &bounds.base {
                BaseTrait::Named { id, .. } => self.pending.push(id.as_str().to_owned()),
                BaseTrait::Function(function) => self.signature(&function.signature),
            },
            _ => {}
        }
    }
}

fn owner_id(owner: &ConstantOwner) -> &str {
    match owner {
        ConstantOwner::Record(id) => id.as_str(),
        ConstantOwner::Enum(id) => id.as_str(),
        ConstantOwner::Class(id) => id.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName, ClassDef, ConstExpr, ConstantDef, ConstantOwner, FunctionDef, PackageInfo,
        ParameterDef, Path, Primitive, RecordDef, ReturnDef, SourceContract, StreamDef, TypeExpr,
    };

    use super::reachable_from;

    fn name(spelling: &str) -> CanonicalName {
        CanonicalName::single(spelling)
    }

    fn function(id: &str, parameter: Option<TypeExpr>, returns: Option<TypeExpr>) -> FunctionDef {
        let mut function = FunctionDef::new(id.into(), name("f"));
        function.parameters = parameter
            .map(|type_expr| vec![ParameterDef::value(name("value"), type_expr)])
            .unwrap_or_default();
        function.returns = returns.map_or(ReturnDef::Void, ReturnDef::value);
        function
    }

    fn ids(contract: &SourceContract) -> Vec<String> {
        let mut ids = contract
            .records
            .iter()
            .map(|record| record.id.as_str().to_owned())
            .chain(
                contract
                    .classes
                    .iter()
                    .map(|class| class.id.as_str().to_owned()),
            )
            .chain(
                contract
                    .functions
                    .iter()
                    .map(|function| function.id.as_str().to_owned()),
            )
            .chain(
                contract
                    .streams
                    .iter()
                    .map(|stream| stream.id.as_str().to_owned()),
            )
            .chain(
                contract
                    .constants
                    .iter()
                    .map(|constant| constant.id.as_str().to_owned()),
            )
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }

    #[test]
    fn keeps_the_root_and_what_it_reaches_through_types() {
        let point = TypeExpr::record("shapes::Point".into(), Path::single("Point"));
        let widget = TypeExpr::class("shapes::Widget".into(), Path::single("Widget"));
        let mut contract = SourceContract::new(PackageInfo::new("app", None));
        contract.functions = vec![
            function("app::norm", Some(point), None),
            function("app::make", None, Some(widget)),
            function("shapes::origin", None, None),
            function("other::hello", None, None),
        ];
        contract.records = vec![
            RecordDef::new("shapes::Point".into(), name("point")),
            RecordDef::new("other::Loose".into(), name("loose")),
        ];
        contract.classes = vec![
            ClassDef::new("shapes::Widget".into(), name("widget")),
            ClassDef::new("other::Gadget".into(), name("gadget")),
        ];
        let mut ticks = StreamDef::new(
            "shapes::Widget::ticks".into(),
            name("ticks"),
            TypeExpr::Primitive(Primitive::U32),
        );
        ticks.owner = Some("shapes::Widget".into());
        contract.streams = vec![ticks];
        let mut unit = ConstantDef::new(
            "shapes::Point::UNIT".into(),
            name("unit"),
            TypeExpr::Primitive(Primitive::F64),
            ConstExpr::Raw("1.0".to_owned()),
        );
        unit.owner = Some(ConstantOwner::Record("shapes::Point".into()));
        contract.constants = vec![unit];

        assert_eq!(
            ids(&reachable_from(contract, ["app"])),
            [
                "app::make",
                "app::norm",
                "shapes::Point",
                "shapes::Point::UNIT",
                "shapes::Widget",
                "shapes::Widget::ticks",
            ]
        );
    }

    #[test]
    fn local_seeds_keep_everything_and_other_crates_only_what_is_reached() {
        let point = TypeExpr::record("registry::Point".into(), Path::single("Point"));
        let mut contract = SourceContract::new(PackageInfo::new("app", None));
        contract.functions = vec![
            function("app::norm", Some(point), None),
            function("shapes::origin", None, None),
            function("registry::helper", None, None),
        ];
        contract.records = vec![
            RecordDef::new("registry::Point".into(), name("point")),
            RecordDef::new("registry::Loose".into(), name("loose")),
        ];

        assert_eq!(
            ids(&reachable_from(contract, ["app", "shapes"])),
            ["app::norm", "registry::Point", "shapes::origin"]
        );
    }
}
