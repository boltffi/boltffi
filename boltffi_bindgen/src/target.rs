use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Swift,
    Kotlin,
    KotlinMultiplatform,
    Java,
    TypeScript,
    Header,
    Dart,
    Python,
    CSharp,
    C,
    Ruby,
}

impl Target {
    pub const fn name(self) -> &'static str {
        match self {
            Target::Swift => "swift",
            Target::Kotlin => "kotlin",
            Target::KotlinMultiplatform => "kotlin_multiplatform",
            Target::Java => "java",
            Target::TypeScript => "typescript",
            Target::Header => "header",
            Target::Dart => "dart",
            Target::Python => "python",
            Target::CSharp => "csharp",
            Target::C => "c",
            Target::Ruby => "ruby",
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::Target;

    #[test]
    fn c_target_name_is_c() {
        assert_eq!(Target::C.name(), "c");
    }
}
