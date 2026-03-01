use std::path::Path;
use std::time::Instant;

use crate::analysis::{CollectionResult, EffectCollector};
use crate::contract::{ContractLoader, FfiContract};
use crate::parse::{FfiPatterns, Language, LanguageParser, ParseError, SwiftParser};
use crate::report::VerificationResult;
use crate::rules::BranchConsistency;
use crate::rules::{RuleRegistry, Violation};

pub struct Verifier {
    parser: Box<dyn LanguageParser>,
    patterns: FfiPatterns,
    rules: RuleRegistry,
    contract: Option<FfiContract>,
}

#[derive(Debug)]
pub enum VerifyError {
    Parse(ParseError),
    Io(std::io::Error),
    UnsupportedLanguage(String),
}

impl From<ParseError> for VerifyError {
    fn from(err: ParseError) -> Self {
        Self::Parse(err)
    }
}

impl From<std::io::Error> for VerifyError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "parse error: {}", e),
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::UnsupportedLanguage(lang) => write!(f, "unsupported language: {}", lang),
        }
    }
}

impl std::error::Error for VerifyError {}

impl Verifier {
    pub fn for_language(language: Language) -> Result<Self, VerifyError> {
        let (parser, patterns): (Box<dyn LanguageParser>, FfiPatterns) = match language {
            Language::Swift => (Box::new(SwiftParser::new()?), FfiPatterns::swift()),
            Language::Kotlin => return Err(VerifyError::UnsupportedLanguage("Kotlin".into())),
        };

        Ok(Self {
            parser,
            patterns,
            rules: RuleRegistry::with_defaults(),
            contract: None,
        })
    }

    pub fn for_path(path: &Path) -> Result<Self, VerifyError> {
        let language = Language::from_path(path).ok_or_else(|| {
            VerifyError::UnsupportedLanguage(
                path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
            )
        })?;
        Self::for_language(language)
    }

    pub fn swift() -> Result<Self, VerifyError> {
        Self::for_language(Language::Swift)
    }

    pub fn with_parser_and_patterns<P: LanguageParser + 'static>(
        parser: P,
        patterns: FfiPatterns,
    ) -> Self {
        Self {
            parser: Box::new(parser),
            patterns,
            rules: RuleRegistry::with_defaults(),
            contract: None,
        }
    }

    pub fn with_rules(mut self, rules: RuleRegistry) -> Self {
        self.rules = rules;
        self
    }

    pub fn with_contract(mut self, contract: FfiContract) -> Self {
        self.contract = Some(contract);
        self
    }

    pub fn with_auto_contract(mut self, source: &str, prefix: &str) -> Self {
        self.contract = Some(ContractLoader::from_source_with_patterns(
            source,
            prefix,
            &self.patterns,
        ));
        self
    }

    pub fn verify_file(&mut self, path: &Path) -> Result<VerificationResult, VerifyError> {
        let content = std::fs::read_to_string(path)?;
        self.verify_source(path, &content)
    }

    pub fn verify_source(
        &mut self,
        path: &Path,
        source: &str,
    ) -> Result<VerificationResult, VerifyError> {
        let start = Instant::now();

        let contract = self.contract.clone().unwrap_or_else(|| {
            ContractLoader::from_source_with_patterns(source, "boltffi", &self.patterns)
        });

        let units = self.parser.parse_source(path, source)?;
        let branch_rule = BranchConsistency;

        let all_violations: Vec<Violation> = units
            .iter()
            .flat_map(|unit| {
                let CollectionResult { trace, divergences } =
                    EffectCollector::collect_with_flow(unit);
                let mut violations = self.rules.check_all_with_contract(&trace, &contract);
                violations.extend(branch_rule.check_divergences(&divergences));
                violations
            })
            .collect();

        let duration = start.elapsed();

        if all_violations.is_empty() {
            Ok(VerificationResult::verified(
                units.len(),
                self.rules.rule_count() + 1,
                duration,
            ))
        } else {
            Ok(VerificationResult::failed(all_violations, duration))
        }
    }

    pub fn language(&self) -> &str {
        self.parser.language_name()
    }
}

impl Default for Verifier {
    fn default() -> Self {
        Self::swift().expect("failed to create verifier")
    }
}

#[cfg(all(test, not(miri)))]
mod tests {
    use super::*;

    fn verify_swift(source: &str) -> VerificationResult {
        let mut verifier = Verifier::swift().unwrap();
        verifier
            .verify_source(Path::new("test.swift"), source)
            .unwrap()
    }

    #[test]
    fn test_verify_balanced_alloc_free() {
        let source = r#"
public func test() {
    let ptr = UnsafeMutablePointer<Int32>.allocate(capacity: 10)
    defer { ptr.deallocate() }
}
"#;
        let result = verify_swift(source);
        assert!(result.is_verified(), "Should verify: balanced alloc/free");
    }

    #[test]
    fn test_verify_detects_memory_leak() {
        let source = r#"
public func test() {
    let ptr = UnsafeMutablePointer<Int32>.allocate(capacity: 10)
}
"#;
        let result = verify_swift(source);
        assert!(result.is_failed(), "Should detect memory leak");
        assert!(result.error_count() > 0);
    }

    #[test]
    fn test_verify_balanced_retain_release() {
        let source = r#"
public func test() {
    let obj = MyObject()
    let handle = Unmanaged.passRetained(obj).toOpaque()
    Unmanaged<MyObject>.fromOpaque(handle).release()
}
"#;
        let result = verify_swift(source);
        assert!(
            result.is_verified(),
            "Should verify: balanced retain/release"
        );
    }

    #[test]
    fn test_verify_multiple_functions() {
        let source = r#"
public func allocatesCorrectly() {
    let ptr = UnsafeMutablePointer<Int32>.allocate(capacity: 10)
    defer { ptr.deallocate() }
}

public func alsoCorrect() {
    let ptr = UnsafeMutablePointer<Double>.allocate(capacity: 5)
    defer { ptr.deallocate() }
}
"#;
        let result = verify_swift(source);
        assert!(result.is_verified(), "Should verify both functions");
    }

    #[test]
    fn test_verify_defer_in_branch() {
        let source = r#"
public func test(condition: Bool) {
    if condition {
        let ptr = UnsafeMutablePointer<Int32>.allocate(capacity: 10)
        defer { ptr.deallocate() }
    }
}
"#;
        let result = verify_swift(source);
        assert!(
            result.is_verified(),
            "Should verify: alloc+defer in same branch"
        );
    }
}
