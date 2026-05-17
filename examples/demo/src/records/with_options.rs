use boltffi::*;

/// A user profile where some fields may not be set.
#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct UserProfile {
    pub name: String,
    pub age: u32,
    /// Contact email, if the user has provided one.
    pub email: Option<String>,
    /// Reputation score, absent for new users.
    pub score: Option<f64>,
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_options.user_profile.should_roundtrip_present_options",
    justification = "Ensure a UserProfile record with present optional fields crosses the wire and returns unchanged.",
    directions = "Call `records::with_options::echo_user_profile` through the generated binding and assert a UserProfile record with present optional fields crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with optional fields are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "records.with_options.user_profile.should_roundtrip_absent_options",
    justification = "Ensure a UserProfile record with absent optional fields crosses the wire and returns unchanged.",
    directions = "Call `records::with_options::echo_user_profile` through the generated binding and assert a UserProfile record with absent optional fields crosses the wire and returns unchanged.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java reaches the surrounding surface but still needs a round-trip assertion for UserProfile with absent options."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin reaches the surrounding surface but still needs a round-trip assertion for UserProfile with absent options."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with optional fields are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript reaches the surrounding surface but still needs a round-trip assertion for UserProfile with absent options."
    )
)]
#[demo_bench_macros::demo_case(
    "records.with_options.user_profile.should_roundtrip_mixed_options",
    justification = "Ensure a UserProfile record with one present option and one absent option crosses the wire and returns unchanged.",
    directions = "Call `records::with_options::echo_user_profile` through the generated binding and assert a UserProfile record with one present option and one absent option crosses the wire and returns unchanged.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java reaches the surrounding surface but still needs a round-trip assertion for UserProfile with mixed option presence."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin reaches the surrounding surface but still needs a round-trip assertion for UserProfile with mixed option presence."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with optional fields are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript reaches the surrounding surface but still needs a round-trip assertion for UserProfile with mixed option presence."
    )
)]
#[demo_bench_macros::demo_case(
    "records.with_options.user_profile.should_roundtrip_utf8_optional_string",
    justification = "Ensure a UserProfile record with UTF-8 text inside optional string fields crosses the wire and returns unchanged.",
    directions = "Call `records::with_options::echo_user_profile` through the generated binding and assert a UserProfile record with UTF-8 text inside optional string fields crosses the wire and returns unchanged.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java reaches the surrounding surface but still needs a round-trip assertion for UTF-8 inside UserProfile optional fields."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin reaches the surrounding surface but still needs a round-trip assertion for UTF-8 inside UserProfile optional fields."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with optional fields are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript reaches the surrounding surface but still needs a round-trip assertion for UTF-8 inside UserProfile optional fields."
    )
)]
pub fn echo_user_profile(profile: UserProfile) -> UserProfile {
    profile
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_options.user_profile.should_make_with_present_options",
    justification = "Ensure make_user_profile constructs a UserProfile with present email and score options.",
    directions = "Call `records::with_options::make_user_profile` through the generated binding and assert make_user_profile constructs a UserProfile with present email and score options.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with optional fields are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "records.with_options.user_profile.should_make_with_absent_options",
    justification = "Ensure make_user_profile constructs a UserProfile with absent email and score options.",
    directions = "Call `records::with_options::make_user_profile` through the generated binding and assert make_user_profile constructs a UserProfile with absent email and score options.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with optional fields are implemented for Python."
    )
)]
pub fn make_user_profile(
    name: String,
    age: u32,
    email: Option<String>,
    score: Option<f64>,
) -> UserProfile {
    UserProfile {
        name,
        age,
        email,
        score,
    }
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_options.user_profile.should_display_email_when_present",
    justification = "Ensure user_display_name includes the email address when a UserProfile email option is present.",
    directions = "Call `records::with_options::user_display_name` through the generated binding and assert user_display_name includes the email address when a UserProfile email option is present.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with optional fields are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "records.with_options.user_profile.should_display_name_when_email_absent",
    justification = "Ensure user_display_name falls back to the name when a UserProfile email option is absent.",
    directions = "Call `records::with_options::user_display_name` through the generated binding and assert user_display_name falls back to the name when a UserProfile email option is absent.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with optional fields are implemented for Python."
    )
)]
pub fn user_display_name(profile: UserProfile) -> String {
    match profile.email {
        Some(email) => format!("{} <{}>", profile.name, email),
        None => profile.name,
    }
}

#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SearchResult {
    pub query: String,
    pub total: u32,
    pub next_cursor: Option<String>,
    pub max_score: Option<f64>,
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_options.search_result.should_roundtrip_present_options",
    justification = "Ensure a SearchResult record with present cursor and score options crosses the wire and returns unchanged.",
    directions = "Call `records::with_options::echo_search_result` through the generated binding and assert a SearchResult record with present cursor and score options crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with optional fields are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "records.with_options.search_result.should_roundtrip_absent_options",
    justification = "Ensure a SearchResult record with absent cursor and score options crosses the wire and returns unchanged.",
    directions = "Call `records::with_options::echo_search_result` through the generated binding and assert a SearchResult record with absent cursor and score options crosses the wire and returns unchanged.",
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin reaches the surrounding surface but still needs a round-trip assertion for SearchResult with absent options."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with optional fields are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript reaches the surrounding surface but still needs a round-trip assertion for SearchResult with absent options."
    )
)]
pub fn echo_search_result(result: SearchResult) -> SearchResult {
    result
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_options.search_result.should_report_more_results_when_cursor_present",
    justification = "Ensure has_more_results returns true when a SearchResult carries a next cursor.",
    directions = "Call `records::with_options::has_more_results` through the generated binding and assert has_more_results returns true when a SearchResult carries a next cursor.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with optional fields are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "records.with_options.search_result.should_report_no_more_results_without_cursor",
    justification = "Ensure has_more_results returns false when a SearchResult has no next cursor.",
    directions = "Call `records::with_options::has_more_results` through the generated binding and assert has_more_results returns false when a SearchResult has no next cursor.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with optional fields are implemented for Python."
    )
)]
pub fn has_more_results(result: SearchResult) -> bool {
    result.next_cursor.is_some()
}
