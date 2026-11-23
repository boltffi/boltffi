use std::path::Path;
use std::process::Output;

use crate::cli::{CliError, Result};

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
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
        let mut collector = NativeLinkMetadataCollector::default();
        stdout.lines().chain(stderr.lines()).for_each(|line| {
            collector.observe(line);
        });
        collector.finish()
    }
}

/// Gathers [`NativeLinkMetadata`] line by line from the output of
/// `cargo rustc --message-format=json-render-diagnostics -- --print=native-static-libs`.
///
/// This lets the build that produces a static library also report how to link
/// it while its output streams, instead of needing a second `cargo rustc` just
/// for the metadata. Cargo replays both the `native-static-libs` note and the
/// `build-script-executed` messages for a fresh unit, so the metadata is there
/// even when nothing was recompiled.
#[derive(Debug, Default)]
pub struct NativeLinkMetadataCollector {
    native_static_libraries: Option<Vec<String>>,
    native_link_search_paths: Vec<String>,
}

impl NativeLinkMetadataCollector {
    /// Records the link metadata `line` carries. Returns whether `line` was a
    /// cargo JSON message, which a caller echoing the build should not print.
    pub fn observe(&mut self, line: &str) -> bool {
        if let Some(message) = parse_cargo_message(line) {
            if message.reason == "build-script-executed" {
                for path in message.linked_paths {
                    if !self.native_link_search_paths.contains(&path) {
                        self.native_link_search_paths.push(path);
                    }
                }
            }
            return true;
        }
        // rustc can print the note more than once; the last one wins
        if let Some(flags) = parse_native_static_libraries(line) {
            self.native_static_libraries = Some(flags);
        }
        false
    }

    pub fn finish(self) -> Result<NativeLinkMetadata> {
        let native_static_libraries =
            self.native_static_libraries
                .ok_or_else(|| CliError::CommandFailed {
                    command: "cargo rustc --print=native-static-libs did not emit link metadata"
                        .to_owned(),
                    status: None,
                })?;
        Ok(NativeLinkMetadata {
            native_static_libraries,
            native_link_search_paths: self.native_link_search_paths,
        })
    }
}

#[derive(serde::Deserialize)]
struct CargoMessage {
    reason: String,
    #[serde(default)]
    linked_paths: Vec<String>,
}

fn parse_cargo_message(line: &str) -> Option<CargoMessage> {
    line.starts_with('{')
        .then(|| serde_json::from_str(line).ok())
        .flatten()
}

pub fn parse_native_static_libraries(line: &str) -> Option<Vec<String>> {
    let sanitized = strip_ansi_escape_codes(line);
    let (_, flags) = sanitized.split_once("native-static-libs:")?;
    let parsed: Vec<String> = flags.split_whitespace().map(str::to_string).collect();

    (!parsed.is_empty()).then_some(parsed)
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
    use super::{NativeLinkMetadataCollector, parse_native_static_libraries};

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

    fn collect(output: &str) -> NativeLinkMetadataCollector {
        let mut collector = NativeLinkMetadataCollector::default();
        output.lines().for_each(|line| {
            collector.observe(line);
        });
        collector
    }

    #[test]
    fn collects_last_native_static_library_line_from_combined_output() {
        let metadata = collect(
            "Compiling demo\nnote: native-static-libs: -lSystem\nFinished\nnote: native-static-libs: -framework CoreFoundation -lSystem\n",
        )
        .finish()
        .expect("expected link metadata");

        assert_eq!(
            metadata.native_static_libraries,
            vec!["-framework", "CoreFoundation", "-lSystem"]
        );
    }

    #[test]
    fn collects_link_search_paths_from_build_script_messages() {
        let metadata = collect(
            r#"{"reason":"compiler-artifact","package_id":"path+file:///tmp/demo#0.1.0"}
{"reason":"build-script-executed","package_id":"path+file:///tmp/dep#0.1.0","linked_paths":["native=/tmp/out","framework=/tmp/frameworks","native=/tmp/out"]}
note: native-static-libs: -lc"#,
        )
        .finish()
        .expect("expected link metadata");

        assert_eq!(
            metadata.native_link_search_paths,
            vec![
                "native=/tmp/out".to_string(),
                "framework=/tmp/frameworks".to_string(),
            ]
        );
    }

    #[test]
    fn collector_without_a_native_static_library_note_fails() {
        let collector = collect(
            r#"{"reason":"build-script-executed","package_id":"path+file:///tmp/dep#0.1.0","linked_paths":["native=/tmp/out"]}
Finished `release` profile [optimized] target(s) in 0.03s"#,
        );

        assert!(collector.finish().is_err());
    }

    #[test]
    fn collector_reports_which_lines_are_cargo_messages() {
        let mut collector = NativeLinkMetadataCollector::default();

        assert!(collector.observe(r#"{"reason":"compiler-artifact","fresh":true}"#));
        assert!(!collector.observe("   Compiling demo v0.1.0 (/tmp/demo)"));
        assert!(!collector.observe("note: native-static-libs: -lc"));
        assert!(!collector.observe("{ not json"));
    }
}
