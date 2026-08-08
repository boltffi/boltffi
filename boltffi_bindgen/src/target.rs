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
    Ruby,
    CSharp,
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
            Target::Ruby => "ruby",
            Target::CSharp => "csharp",
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}
