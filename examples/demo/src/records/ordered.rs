use boltffi::*;

/// A rank that sorts by tier, then score, as its derived `Ord` does.
#[data(ord)]
#[demo_bench_macros::demo_case(
    "records.ordered.rank.should_compare_by_rust_ord",
    justification = "Ensure a direct #[data(ord)] record compares and sorts in the target language as its derived Rust Ord does.",
    directions = "Compare and sort `records::ordered::Rank` values through the generated binding (its compareTo and the language's comparison operators or protocol) and assert they order by tier, then score.",
    exclude(
        ruby,
        reason = ExclusionReason::ImplementationGap,
        details = "the Ruby target does not bind record methods yet"
    )
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rank {
    /// Compared first.
    pub tier: u32,
    /// Compared when the tiers are equal.
    pub score: i32,
}

/// A version that sorts by major, minor and patch, then puts a release above its
/// pre-releases: an order the fields alone do not give.
#[data(ord)]
#[demo_bench_macros::demo_case(
    "records.ordered.semantic_version.should_compare_by_rust_ord",
    justification = "Ensure an encoded #[data(ord)] record compares and sorts in the target language by its hand-written Rust Ord, not by its fields.",
    directions = "Compare and sort `records::ordered::SemanticVersion` values through the generated binding and assert that a release sorts above its pre-release and pre-releases sort by name.",
    exclude(
        ruby,
        reason = ExclusionReason::ImplementationGap,
        details = "the Ruby target does not bind record methods yet"
    )
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// The pre-release, such as `beta`; `None` for a release.
    pub pre: Option<String>,
}

impl Ord for SemanticVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(_), None) => std::cmp::Ordering::Less,
                (Some(left), Some(right)) => left.cmp(right),
            })
    }
}

impl PartialOrd for SemanticVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
