mod callback;
mod class;
mod enumeration;
mod function;
mod record;
mod stream;

use std::collections::{BTreeMap, BTreeSet};

use boltffi_binding::{Bindings, Decl, DeclarationRef, Native};

use crate::core::capabilities::BindingCapabilityAnalysis;
use crate::core::{
    CoverageReport, DeclarationLabel, HostCapabilities, Result, UnsupportedDeclaration,
};

pub use self::class::ClassShape;
use super::JavaHost;
pub use callback::CallbackShape;
pub use function::{FunctionShape, ReceiverSupport};
pub use record::RecordShape;
pub use stream::StreamShape;

pub struct Selection {
    bindings: Bindings<Native>,
    coverage: CoverageReport,
}

enum Decision {
    Accepted,
    Rejected(String),
}

impl Selection {
    pub fn new(host: &JavaHost, bindings: &Bindings<Native>) -> Result<Self> {
        let capabilities = host.capabilities();
        let capability_analysis = BindingCapabilityAnalysis::new(bindings);
        let (requested, mut reasons) = bindings.decls().iter().fold(
            (BTreeSet::new(), BTreeMap::new()),
            |(mut requested, mut reasons), declaration| {
                match Decision::for_declaration(declaration, &capabilities, &capability_analysis) {
                    Decision::Accepted => {
                        requested.insert(declaration.id());
                    }
                    Decision::Rejected(reason) => {
                        reasons.insert(declaration.id(), reason);
                    }
                }
                (requested, reasons)
            },
        );
        let retained = bindings.dependency_closed(&requested).map_err(|_| {
            JavaHost::broken_bridge_contract(
                "Java admission selects ids from the active binding contract",
            )
        })?;
        let retained_ids = retained
            .decls()
            .iter()
            .map(Decl::id)
            .collect::<BTreeSet<_>>();
        requested
            .difference(&retained_ids)
            .copied()
            .for_each(|declaration| {
                reasons.insert(
                    declaration,
                    "required declaration is unsupported".to_owned(),
                );
            });
        let coverage =
            bindings
                .decls()
                .iter()
                .fold(CoverageReport::new(), |mut coverage, declaration| {
                    if let Some(reason) = reasons.remove(&declaration.id()) {
                        coverage.push(UnsupportedDeclaration::new(
                            DeclarationLabel::from_ref(DeclarationRef::from(declaration)),
                            reason,
                        ));
                    }
                    coverage
                });
        Ok(Self {
            bindings: retained,
            coverage,
        })
    }

    pub fn into_parts(self) -> (Bindings<Native>, CoverageReport) {
        (self.bindings, self.coverage)
    }
}

impl Decision {
    fn for_declaration(
        declaration: &Decl<Native>,
        capabilities: &HostCapabilities,
        capability_analysis: &BindingCapabilityAnalysis,
    ) -> Self {
        let requirements = capability_analysis
            .declaration_requirements(declaration.id())
            .expect("capability analysis must include every binding declaration");
        for capability in requirements.iter() {
            let status = capabilities.status(capability);
            if !status.renderable_in_partial() {
                return Self::Rejected(status.reason().to_owned());
            }
        }
        match DeclarationRef::from(declaration) {
            DeclarationRef::Function(function) => FunctionShape::classify(function)
                .unsupported_reason()
                .map_or(Self::Accepted, |reason| Self::Rejected(reason.to_owned())),
            DeclarationRef::Record(record) => RecordShape::classify(record)
                .unsupported_reason()
                .map_or(Self::Accepted, |reason| Self::Rejected(reason.to_owned())),
            DeclarationRef::Enum(enumeration) => EnumShape::classify(enumeration)
                .unsupported_reason()
                .map_or(Self::Accepted, |reason| Self::Rejected(reason.to_owned())),
            DeclarationRef::Class(class) => ClassShape::classify(class)
                .unsupported_reason()
                .map_or(Self::Accepted, |reason| Self::Rejected(reason.to_owned())),
            DeclarationRef::Callback(callback) => CallbackShape::classify(callback)
                .unsupported_reason()
                .map_or(Self::Accepted, |reason| Self::Rejected(reason.to_owned())),
            DeclarationRef::Stream(stream) => StreamShape::classify(stream)
                .unsupported_reason()
                .map_or(Self::Accepted, |reason| Self::Rejected(reason.to_owned())),
            _ => Self::Accepted,
        }
    }
}
pub use enumeration::EnumShape;
