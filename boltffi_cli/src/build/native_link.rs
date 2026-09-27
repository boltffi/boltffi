use std::path::Path;
use std::process::Output;

use crate::cli::{CliError, Result};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct NativeLinkMetadata {
    pub native_static_libraries: Vec<String>,
    pub native_link_search_paths: Vec<String>,
}

impl NativeLinkMetadata {
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).map_err(|source| CliError::ReadFailed {
            path: path.to_path_buf(),
            source,
        })?;
        serde_json::from_slice(&bytes).map_err(|source| CliError::CommandFailed {
            command: format!("invalid native link metadata {}: {source}", path.display()),
            status: None,
        })
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec(self).map_err(|source| CliError::CommandFailed {
            command: format!("serialize native link metadata: {source}"),
            status: None,
        })?;
        std::fs::write(path, bytes).map_err(|source| CliError::WriteFailed {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn from_output(output: &Output) -> Result<Self> {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}\n{stderr}");
        let native_static_libraries =
            extract_native_static_libraries(&combined).ok_or_else(|| CliError::CommandFailed {
                command: "cargo rustc --print=native-static-libs did not emit link metadata"
                    .to_owned(),
                status: None,
            })?;
        Ok(Self {
            native_static_libraries,
            native_link_search_paths: extract_link_search_paths(&stdout),
        })
    }
}

pub fn parse_native_static_libraries(line: &str) -> Option<Vec<String>> {
    let sanitized = strip_ansi_escape_codes(line);
    let (_, flags) = sanitized.split_once("native-static-libs:")?;
    let parsed: Vec<String> = flags.split_whitespace().map(str::to_string).collect();

    (!parsed.is_empty()).then_some(parsed)
}

pub fn extract_native_static_libraries(output: &str) -> Option<Vec<String>> {
    output
        .lines()
        .filter_map(parse_native_static_libraries)
        .next_back()
}

pub fn extract_link_search_paths(output: &str) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct BuildScriptExecutedMessage {
        reason: String,
        #[serde(default)]
        linked_paths: Vec<String>,
    }

    output
        .lines()
        .filter_map(|line| serde_json::from_str::<BuildScriptExecutedMessage>(line).ok())
        .filter(|message| message.reason == "build-script-executed")
        .flat_map(|message| message.linked_paths)
        .fold(Vec::new(), |mut paths, path| {
            if !paths.contains(&path) {
                paths.push(path);
            }
            paths
        })
}

fn strip_ansi_escape_codes(input: &str) -> String {
    let mut characters = input.chars();
    let mut output = String::with_capacity(input.len());
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            if characters.next() == Some('[') {
                characters
                    .by_ref()
                    .find(|character| ('\u{40}'..='\u{7e}').contains(character));
            }
        } else {
            output.push(character);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{
        extract_link_search_paths, extract_native_static_libraries, parse_native_static_libraries,
    };

    #[test]
    fn parses_native_static_library_flags_from_cargo_output() {
        let parsed = parse_native_static_libraries(
            "note: native-static-libs: -framework Security -lresolv -lc++",
        )
        .expect("expected static library flags");

        assert_eq!(parsed, vec!["-framework", "Security", "-lresolv", "-lc++"]);
    }

    #[test]
    fn parses_native_static_library_flags_from_ansi_colored_cargo_output() {
        let parsed =
            parse_native_static_libraries("note: native-static-libs: -lSystem -lc -lm\u{1b}[0m")
                .expect("expected static library flags");

        assert_eq!(parsed, vec!["-lSystem", "-lc", "-lm"]);
    }

    #[test]
    fn preserves_repeated_framework_prefixes_in_native_static_library_flags() {
        let parsed = parse_native_static_libraries(
            "note: native-static-libs: -framework Security -framework SystemConfiguration -lobjc",
        )
        .expect("expected static library flags");

        assert_eq!(
            parsed,
            vec![
                "-framework",
                "Security",
                "-framework",
                "SystemConfiguration",
                "-lobjc",
            ]
        );
    }

    #[test]
    fn extracts_last_native_static_library_line_from_combined_output() {
        let parsed = extract_native_static_libraries(
            "Compiling demo\nnote: native-static-libs: -lSystem\nFinished\nnote: native-static-libs: -framework CoreFoundation -lSystem\n",
        )
        .expect("expected static library flags");

        assert_eq!(parsed, vec!["-framework", "CoreFoundation", "-lSystem"]);
    }

    #[test]
    fn extracts_link_search_paths_from_build_script_messages() {
        let linked_paths = extract_link_search_paths(
            r#"{"reason":"compiler-artifact","package_id":"path+file:///tmp/demo#0.1.0"}
{"reason":"build-script-executed","package_id":"path+file:///tmp/dep#0.1.0","linked_paths":["native=/tmp/out","framework=/tmp/frameworks","native=/tmp/out"]}"#,
        );

        assert_eq!(
            linked_paths,
            vec![
                "native=/tmp/out".to_string(),
                "framework=/tmp/frameworks".to_string(),
            ]
        );
    }
}
