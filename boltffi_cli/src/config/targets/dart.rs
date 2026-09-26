use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::target::RustTarget;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DartConfig {
    #[serde(default = "default_dart_output")]
    pub output: PathBuf,
    #[serde(default)]
    pub enabled: bool,
    #[serde(
        default,
        serialize_with = "DartConfig::serialize_native_targets",
        deserialize_with = "DartConfig::deserialize_native_targets"
    )]
    pub native_targets: Option<Vec<RustTarget>>,
    /// Cargo arguments for every cargo invocation that builds this target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_args: Vec<String>,
}

impl Default for DartConfig {
    fn default() -> Self {
        Self {
            output: default_dart_output(),
            enabled: false,
            native_targets: None,
            cargo_args: Vec::new(),
        }
    }
}

impl DartConfig {
    fn deserialize_native_targets<'de, D>(
        deserializer: D,
    ) -> Result<Option<Vec<RustTarget>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<Vec<String>>::deserialize(deserializer)?
            .map(|targets| {
                targets
                    .into_iter()
                    .map(|target| {
                        RustTarget::from_dart_native_name(&target).ok_or_else(|| {
                            <D::Error as serde::de::Error>::custom(format!(
                                "unsupported Dart native target {target}"
                            ))
                        })
                    })
                    .collect()
            })
            .transpose()
    }

    fn serialize_native_targets<S>(
        targets: &Option<Vec<RustTarget>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        targets
            .as_deref()
            .map(|targets| {
                targets
                    .iter()
                    .map(|target| {
                        target.dart_native_name().ok_or_else(|| {
                            <S::Error as serde::ser::Error>::custom(format!(
                                "unsupported Dart native target {}",
                                target.triple()
                            ))
                        })
                    })
                    .collect::<Result<Vec<_>, S::Error>>()
            })
            .transpose()?
            .serialize(serializer)
    }
}

fn default_dart_output() -> PathBuf {
    PathBuf::from("dist/dart")
}
