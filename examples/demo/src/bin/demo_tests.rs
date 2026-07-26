use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Attribute, Ident, Item, LitStr, Token, Type, Visibility, bracketed, parenthesized};

// These identifiers intentionally match `boltffi_bindgen/src/render/*` folder names.
const TARGETS: &[&str] = &["swift", "kotlin", "java", "csharp", "typescript", "python"];

type AppResult<T> = Result<T, String>;

#[derive(Debug)]
struct DemoCase {
    id: String,
    justification: String,
    directions: String,
    exercises: Vec<String>,
    exclusions: BTreeMap<String, Exclusion>,
    source: PathBuf,
}

#[derive(Debug, Clone)]
struct Exclusion {
    reason: ExclusionReason,
    details: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ExclusionReason {
    ImplementationGap,
    CoverageGap,
}

impl ExclusionReason {
    const ALL: [Self; 2] = [Self::ImplementationGap, Self::CoverageGap];

    fn from_path(path: &syn::Path) -> syn::Result<Self> {
        let segments: Vec<_> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        match segments.as_slice() {
            [prefix, variant] if prefix == "ExclusionReason" => match variant.as_str() {
                "ImplementationGap" => Ok(Self::ImplementationGap),
                "CoverageGap" => Ok(Self::CoverageGap),
                _ => Err(syn::Error::new_spanned(
                    path,
                    "unknown ExclusionReason variant",
                )),
            },
            _ => Err(syn::Error::new_spanned(
                path,
                "expected ExclusionReason::ImplementationGap or ExclusionReason::CoverageGap",
            )),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::ImplementationGap => "implementation gap",
            Self::CoverageGap => "coverage gap",
        }
    }

    fn count_label(self, count: usize) -> &'static str {
        match (self, count) {
            (Self::ImplementationGap, 1) => "implementation gap",
            (Self::ImplementationGap, _) => "implementation gaps",
            (Self::CoverageGap, 1) => "coverage gap",
            (Self::CoverageGap, _) => "coverage gaps",
        }
    }
}

#[derive(Debug)]
struct DemoCaseArgs {
    id: String,
    justification: String,
    directions: String,
    exercises: Vec<String>,
    exclusions: Vec<(String, Exclusion)>,
}

#[derive(Debug)]
struct PlatformScan {
    name: &'static str,
    roots: Vec<PathBuf>,
    suffixes: &'static [&'static str],
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> AppResult<()> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        return Err(usage());
    };

    let mut repo_root = None;
    let mut allow_unknown_markers = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo-root" => {
                let Some(path) = args.next() else {
                    return Err("--repo-root requires a path".to_string());
                };
                repo_root = Some(PathBuf::from(path));
            }
            "--allow-unknown-markers" if command == "audit" => {
                allow_unknown_markers = true;
            }
            "--help" | "-h" => return Err(usage()),
            _ => return Err(format!("unexpected argument {arg:?}\n\n{}", usage())),
        }
    }

    let repo_root = match repo_root {
        Some(path) => path,
        None => find_repo_root(&env::current_dir().map_err(|error| error.to_string())?)?,
    };

    match command.as_str() {
        "report" => report(&repo_root),
        "audit" => audit(&repo_root, allow_unknown_markers),
        _ => Err(format!("unknown command {command:?}\n\n{}", usage())),
    }
}

fn usage() -> String {
    "usage: demo-tests <report|audit> [--repo-root PATH]\n       demo-tests audit [--allow-unknown-markers]".to_string()
}

impl Parse for DemoCaseArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let id = input.parse::<LitStr>()?.value();
        let mut justification = None;
        let mut directions = None;
        let mut exercises = Vec::new();
        let mut exclusions = Vec::new();

        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }

            let key = input.call(Ident::parse_any)?;
            match key.to_string().as_str() {
                "justification" => {
                    input.parse::<Token![=]>()?;
                    if justification
                        .replace(input.parse::<LitStr>()?.value())
                        .is_some()
                    {
                        return Err(
                            input.error("demo_case justification was provided more than once")
                        );
                    }
                }
                "directions" => {
                    input.parse::<Token![=]>()?;
                    if directions
                        .replace(input.parse::<LitStr>()?.value())
                        .is_some()
                    {
                        return Err(
                            input.error("demo_case directions were provided more than once")
                        );
                    }
                }
                "description" => {
                    return Err(input.error(
                        "demo_case description has been removed; provide justification and directions",
                    ));
                }
                "summary" => {
                    return Err(input.error(
                        "demo_case uses the case id as its summary; provide justification and directions only",
                    ));
                }
                "exercise" => {
                    if input.peek(Token![=]) {
                        input.parse::<Token![=]>()?;
                        exercises.push(input.parse::<LitStr>()?.value());
                    } else {
                        let content;
                        parenthesized!(content in input);
                        exercises.push(content.parse::<LitStr>()?.value());
                    }
                }
                "exercises" => {
                    input.parse::<Token![=]>()?;
                    let content;
                    bracketed!(content in input);
                    let values = Punctuated::<LitStr, Token![,]>::parse_terminated(&content)?;
                    exercises.extend(values.into_iter().map(|value| value.value()));
                }
                "exclude" => {
                    let content;
                    parenthesized!(content in input);
                    let platform = content.call(Ident::parse_any)?.to_string();
                    content.parse::<Token![,]>()?;

                    let mut reason = None;
                    let mut details = None;
                    while !content.is_empty() {
                        let key = content.call(Ident::parse_any)?;
                        match key.to_string().as_str() {
                            "reason" => {
                                content.parse::<Token![=]>()?;
                                if content.peek(LitStr) {
                                    return Err(content.error(
                                        "exclude reason is now an ExclusionReason; move prose into details = \"...\"",
                                    ));
                                }
                                let path = content.parse::<syn::Path>()?;
                                if reason.replace(ExclusionReason::from_path(&path)?).is_some() {
                                    return Err(
                                        content.error("exclude reason was provided more than once")
                                    );
                                }
                            }
                            "details" => {
                                content.parse::<Token![=]>()?;
                                if details
                                    .replace(content.parse::<LitStr>()?.value())
                                    .is_some()
                                {
                                    return Err(content
                                        .error("exclude details were provided more than once"));
                                }
                            }
                            _ => {
                                return Err(syn::Error::new_spanned(
                                    key,
                                    "unknown exclude argument",
                                ));
                            }
                        }
                        if !content.is_empty() {
                            content.parse::<Token![,]>()?;
                        }
                    }
                    exclusions.push((
                        platform,
                        Exclusion {
                            reason: reason.ok_or_else(|| {
                                content.error("exclude must include reason = ExclusionReason::...")
                            })?,
                            details: required_string(&content, "exclude details", details)?,
                        },
                    ));
                }
                _ => return Err(syn::Error::new_spanned(key, "unknown demo_case argument")),
            }
        }

        Ok(Self {
            id,
            justification: required_string(input, "justification", justification)?,
            directions: required_string(input, "directions", directions)?,
            exercises,
            exclusions,
        })
    }
}

fn required_string(
    input: ParseStream<'_>,
    name: &'static str,
    value: Option<String>,
) -> syn::Result<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| input.error(format!("demo_case must include non-empty {name}")))
}

fn report(repo_root: &Path) -> AppResult<()> {
    let cases = load_demo_cases(repo_root)?;
    let markers = collect_platform_markers(repo_root)?;

    println!("demo tests: {}", cases.len());
    println!();
    println!("by id:");
    for case in cases.values() {
        let supported = supported_platforms(case, &markers);
        println!("  {}", case.id);
        println!("    justification: {}", case.justification);
        println!("    directions: {}", case.directions);
        println!("    exercises: {}", case.exercises.join(", "));
        println!("    supported: {}", display_list(&supported));
        if !case.exclusions.is_empty() {
            println!("    excluded:");
            for (platform, exclusion) in &case.exclusions {
                println!(
                    "      {platform}: {} - {}",
                    exclusion.reason.label(),
                    exclusion.details
                );
            }
        }
    }

    println!();
    println!("by target:");
    for platform in TARGETS {
        let mut supported: Vec<&str> = cases
            .keys()
            .filter(|case_id| {
                markers
                    .get(*platform)
                    .is_some_and(|platform_markers| platform_markers.contains(*case_id))
            })
            .map(String::as_str)
            .collect();
        supported.sort_unstable();

        println!(
            "  {platform}: {} covered; {}",
            supported.len(),
            display_exclusion_counts(&exclusion_counts(cases.values(), platform))
        );
        for case_id in supported {
            println!("    {case_id}");
        }
    }

    Ok(())
}

fn audit(repo_root: &Path, allow_unknown_markers: bool) -> AppResult<()> {
    let cases = load_demo_cases(repo_root)?;
    let markers = collect_platform_markers(repo_root)?;
    let exports = collect_demo_exports(repo_root)?;
    let mut errors = Vec::new();

    if cases.is_empty() {
        errors.push("no Rust demo cases found".to_string());
    }

    for case in cases.values() {
        for exercise in &case.exercises {
            if !exports.contains(exercise) {
                errors.push(format!(
                    "{}: case {:?} exercises unknown export {:?}",
                    case.source.display(),
                    case.id,
                    exercise
                ));
            }
        }
    }

    let known_case_ids: BTreeSet<&str> = cases.keys().map(String::as_str).collect();
    if !allow_unknown_markers {
        for (platform, platform_markers) in &markers {
            for marker in platform_markers {
                if !known_case_ids.contains(marker.as_str()) {
                    errors.push(format!(
                        "{platform}: marker references unknown case {marker:?}"
                    ));
                }
            }
        }
    }

    for case in cases.values() {
        for platform in TARGETS {
            let present = markers
                .get(*platform)
                .is_some_and(|platform_markers| platform_markers.contains(&case.id));
            let excluded = case.exclusions.contains_key(*platform);
            if excluded && present {
                errors.push(format!(
                    "{platform}: case {:?} is excluded but has a documentation marker",
                    case.id
                ));
            } else if !excluded && !present {
                errors.push(format!(
                    "{platform}: missing documentation marker for case {:?}",
                    case.id
                ));
            }
        }
    }

    print_summary(&cases, &markers);

    if errors.is_empty() {
        println!();
        println!("demo test audit passed");
        Ok(())
    } else {
        eprintln!();
        eprintln!("demo test audit failures:");
        for error in errors {
            eprintln!("  {error}");
        }
        Err("demo test audit failed".to_string())
    }
}

fn load_demo_cases(repo_root: &Path) -> AppResult<BTreeMap<String, DemoCase>> {
    let src_root = repo_root.join("examples/demo/src");
    let mut cases = BTreeMap::new();
    for path in rust_files(&src_root)? {
        let module = module_name_for_source(&src_root, &path)?;
        let text =
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let file =
            syn::parse_file(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        collect_item_cases(&file.items, &module, &path, &mut cases)?;
    }
    Ok(cases)
}

fn collect_item_cases(
    items: &[Item],
    module: &str,
    path: &Path,
    cases: &mut BTreeMap<String, DemoCase>,
) -> AppResult<()> {
    for item in items {
        match item {
            Item::Fn(item_fn) => {
                let inferred = make_export_id(module, &item_fn.sig.ident.to_string(), None);
                collect_attribute_cases(&item_fn.attrs, Some(inferred), path, cases)?;
            }
            Item::Enum(item_enum) => {
                let inferred = (is_public(&item_enum.vis) && has_data_attr(&item_enum.attrs))
                    .then(|| make_export_id(module, &item_enum.ident.to_string(), None));
                collect_attribute_cases(&item_enum.attrs, inferred, path, cases)?;
            }
            Item::Impl(item_impl) => {
                let owner = type_owner(&item_impl.self_ty);
                let impl_inferred = owner
                    .as_ref()
                    .map(|owner| make_export_id(module, owner, None));
                collect_attribute_cases(&item_impl.attrs, impl_inferred, path, cases)?;

                for impl_item in &item_impl.items {
                    if let syn::ImplItem::Fn(method) = impl_item {
                        let inferred = owner.as_ref().map(|owner| {
                            make_export_id(module, &method.sig.ident.to_string(), Some(owner))
                        });
                        collect_attribute_cases(&method.attrs, inferred, path, cases)?;
                    }
                }
            }
            Item::Struct(item_struct) => {
                let inferred = (is_public(&item_struct.vis) && has_data_attr(&item_struct.attrs))
                    .then(|| make_export_id(module, &item_struct.ident.to_string(), None));
                collect_attribute_cases(&item_struct.attrs, inferred, path, cases)?;
            }
            _ => collect_attribute_cases(item_attrs(item), None, path, cases)?,
        }
    }
    Ok(())
}

fn collect_attribute_cases(
    attrs: &[Attribute],
    inferred_exercise: Option<String>,
    path: &Path,
    cases: &mut BTreeMap<String, DemoCase>,
) -> AppResult<()> {
    for attr in attrs.iter().filter(|attr| is_demo_case_attr(attr)) {
        let args = attr
            .parse_args::<DemoCaseArgs>()
            .map_err(|error| format!("{}: invalid demo_case attribute: {error}", path.display()))?;
        let case = demo_case_from_args(args, inferred_exercise.clone(), path)?;
        if let Some(previous) = cases.insert(case.id.clone(), case) {
            return Err(format!(
                "{}: duplicate case id {:?}; first defined in {}",
                path.display(),
                previous.id,
                previous.source.display()
            ));
        }
    }
    Ok(())
}

fn demo_case_from_args(
    args: DemoCaseArgs,
    inferred_exercise: Option<String>,
    path: &Path,
) -> AppResult<DemoCase> {
    let id = args
        .id
        .strip_prefix("case:")
        .unwrap_or(&args.id)
        .to_string();
    if !is_case_id(&id) {
        return Err(format!("{}: invalid demo case id {:?}", path.display(), id));
    }
    if !has_should_segment(&id) {
        return Err(format!(
            "{}: case {id:?} must include a should_ scenario segment",
            path.display()
        ));
    }

    let justification = args.justification;
    let directions = args.directions;

    let mut exercises = args.exercises;
    if exercises.is_empty() {
        exercises.push(inferred_exercise.ok_or_else(|| {
            format!(
                "{}: case {id:?} is not on a function or method, so it must declare exercises",
                path.display()
            )
        })?);
    }
    for exercise in &exercises {
        if exercise.trim().is_empty() {
            return Err(format!(
                "{}: case {id:?} has an empty exercise",
                path.display()
            ));
        }
    }

    let mut exclusions = BTreeMap::new();
    for (platform, exclusion) in args.exclusions {
        if !TARGETS.contains(&platform.as_str()) {
            return Err(format!(
                "{}: case {id:?} excludes unknown target {platform:?}",
                path.display()
            ));
        }
        let details = exclusion.details.trim().to_string();
        if details.is_empty() {
            return Err(format!(
                "{}: case {id:?} exclusion for {platform:?} must include details",
                path.display()
            ));
        }
        let exclusion = Exclusion {
            reason: exclusion.reason,
            details,
        };
        if exclusions.insert(platform.clone(), exclusion).is_some() {
            return Err(format!(
                "{}: case {id:?} repeats exclusion for {platform:?}",
                path.display()
            ));
        }
    }

    Ok(DemoCase {
        id,
        justification,
        directions,
        exercises,
        exclusions,
        source: path.to_path_buf(),
    })
}

fn collect_demo_exports(repo_root: &Path) -> AppResult<BTreeSet<String>> {
    let src_root = repo_root.join("examples/demo/src");
    let mut exports = BTreeSet::new();
    for path in rust_files(&src_root)? {
        let module = module_name_for_source(&src_root, &path)?;
        let text =
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let file =
            syn::parse_file(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        collect_item_exports(&file.items, &module, &mut exports);
    }
    Ok(exports)
}

fn collect_item_exports(items: &[Item], module: &str, exports: &mut BTreeSet<String>) {
    for item in items {
        match item {
            Item::Const(item_const)
                if is_public(&item_const.vis) && has_export_attr(&item_const.attrs) =>
            {
                exports.insert(make_export_id(
                    module,
                    &item_const.ident.to_string(),
                    None,
                ));
            }
            Item::Enum(item_enum)
                if is_public(&item_enum.vis) && has_data_attr(&item_enum.attrs) =>
            {
                exports.insert(make_export_id(module, &item_enum.ident.to_string(), None));
            }
            Item::Fn(item_fn) if is_public(&item_fn.vis) && has_export_attr(&item_fn.attrs) => {
                exports.insert(make_export_id(module, &item_fn.sig.ident.to_string(), None));
            }
            Item::Impl(item_impl)
                if has_export_attr(&item_impl.attrs) || has_data_impl_attr(&item_impl.attrs) =>
            {
                if let Some(owner) = type_owner(&item_impl.self_ty) {
                    for impl_item in &item_impl.items {
                        if let syn::ImplItem::Fn(method) = impl_item {
                            if is_public(&method.vis) {
                                exports.insert(make_export_id(
                                    module,
                                    &method.sig.ident.to_string(),
                                    Some(&owner),
                                ));
                            }
                        }
                    }
                }
            }
            Item::Struct(item_struct)
                if is_public(&item_struct.vis) && has_data_attr(&item_struct.attrs) =>
            {
                exports.insert(make_export_id(module, &item_struct.ident.to_string(), None));
            }
            _ => {}
        }
    }
}

fn collect_platform_markers(repo_root: &Path) -> AppResult<BTreeMap<String, BTreeSet<String>>> {
    let mut markers = BTreeMap::new();
    for scan in platform_scans(repo_root) {
        let mut platform_markers = BTreeSet::new();
        for root in scan.roots {
            if root.exists() {
                for path in files_with_suffixes(&root, scan.suffixes)? {
                    let text = fs::read_to_string(&path)
                        .map_err(|error| format!("{}: {error}", path.display()))?;
                    platform_markers.extend(find_case_markers(&text));
                }
            }
        }
        markers.insert(scan.name.to_string(), platform_markers);
    }
    Ok(markers)
}

fn platform_scans(repo_root: &Path) -> Vec<PlatformScan> {
    vec![
        PlatformScan {
            name: "swift",
            roots: vec![repo_root.join("examples/platforms/apple/Tests")],
            suffixes: &["swift"],
        },
        PlatformScan {
            name: "kotlin",
            roots: vec![repo_root.join("examples/platforms/kotlin/src/test")],
            suffixes: &["kt"],
        },
        PlatformScan {
            name: "java",
            roots: vec![repo_root.join("examples/platforms/java")],
            suffixes: &["java"],
        },
        PlatformScan {
            name: "csharp",
            roots: vec![repo_root.join("examples/platforms/csharp/DemoTest")],
            suffixes: &["cs"],
        },
        PlatformScan {
            name: "typescript",
            roots: vec![repo_root.join("examples/platforms/wasm/tests")],
            suffixes: &["js", "mjs", "ts"],
        },
        PlatformScan {
            name: "python",
            roots: vec![repo_root.join("examples/platforms/python/tests")],
            suffixes: &["py"],
        },
    ]
}

fn find_case_markers(text: &str) -> BTreeSet<String> {
    let mut markers = BTreeSet::new();
    for line in text.lines().map(strip_line_comment) {
        collect_case_prefix_markers(line, &mut markers);
        collect_demo_case_call_markers(line, &mut markers);
    }
    markers
}

fn collect_case_prefix_markers(line: &str, markers: &mut BTreeSet<String>) {
    let mut remaining = line;
    while let Some(index) = remaining.find("case:") {
        let after_prefix = &remaining[index + "case:".len()..];
        let marker: String = after_prefix
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-'))
            .collect();
        if !marker.is_empty() {
            markers.insert(marker);
        }
        remaining = after_prefix;
    }
}

fn collect_demo_case_call_markers(line: &str, markers: &mut BTreeSet<String>) {
    for call in [
        "demoCase(\"",
        "DemoCase(\"",
        "self.demo_case(\"",
        "globalThis.demoCase(\"",
    ] {
        let mut remaining = line;
        while let Some(index) = remaining.find(call) {
            let after_call = &remaining[index + call.len()..];
            if let Some(end) = after_call.find('"') {
                let value = &after_call[..end];
                let marker = value.strip_prefix("case:").unwrap_or(value);
                if is_case_id(marker) {
                    markers.insert(marker.to_string());
                }
                remaining = &after_call[end + 1..];
            } else {
                break;
            }
        }
    }
}

fn strip_line_comment(line: &str) -> &str {
    let slash_index = line.find("//");
    let hash_index = line.find('#');
    match (slash_index, hash_index) {
        (Some(a), Some(b)) => &line[..a.min(b)],
        (Some(index), None) | (None, Some(index)) => &line[..index],
        (None, None) => line,
    }
}

fn supported_platforms(
    case: &DemoCase,
    markers: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<&'static str> {
    TARGETS
        .iter()
        .copied()
        .filter(|platform| {
            markers
                .get(*platform)
                .is_some_and(|platform_markers| platform_markers.contains(&case.id))
        })
        .collect()
}

fn print_summary(cases: &BTreeMap<String, DemoCase>, markers: &BTreeMap<String, BTreeSet<String>>) {
    println!("demo tests: {}", cases.len());
    println!("targets: {}", TARGETS.join(", "));
    println!();
    for platform in TARGETS {
        let expected = cases
            .values()
            .filter(|case| !case.exclusions.contains_key(*platform))
            .count();
        let documented = cases
            .keys()
            .filter(|case_id| {
                markers
                    .get(*platform)
                    .is_some_and(|platform_markers| platform_markers.contains(*case_id))
            })
            .count();
        println!(
            "{platform}: {documented}/{expected} expected cases documented; {}",
            display_exclusion_counts(&exclusion_counts(cases.values(), platform))
        );
    }
}

fn exclusion_counts<'a>(
    cases: impl Iterator<Item = &'a DemoCase>,
    platform: &str,
) -> BTreeMap<ExclusionReason, usize> {
    let mut counts = BTreeMap::new();
    for case in cases {
        if let Some(exclusion) = case.exclusions.get(platform) {
            *counts.entry(exclusion.reason).or_default() += 1;
        }
    }
    counts
}

fn display_exclusion_counts(counts: &BTreeMap<ExclusionReason, usize>) -> String {
    ExclusionReason::ALL
        .iter()
        .map(|reason| {
            let count = counts.get(reason).copied().unwrap_or_default();
            format!("{count} {}", reason.count_label(count))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        Item::Verbatim(_) | _ => &[],
    }
}

fn is_demo_case_attr(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "demo_case")
}

fn has_export_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "export")
    })
}

fn has_data_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("data"))
}

fn has_data_impl_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("data")
            && attr
                .parse_args_with(Ident::parse_any)
                .is_ok_and(|identifier| identifier == "impl")
    })
}

fn is_public(visibility: &Visibility) -> bool {
    matches!(visibility, Visibility::Public(_))
}

fn type_owner(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(type_path) if type_path.qself.is_none() => type_path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        _ => None,
    }
}

fn make_export_id(module: &str, name: &str, owner: Option<&str>) -> String {
    match owner {
        Some(owner) => format!("{module}::{owner}::{name}"),
        None => format!("{module}::{name}"),
    }
}

fn module_name_for_source(src_root: &Path, path: &Path) -> AppResult<String> {
    let relative = path
        .strip_prefix(src_root)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if relative == Path::new("lib.rs") {
        return Ok("crate".to_string());
    }

    let mut parts: Vec<String> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect();
    if parts.last().is_some_and(|part| part == "mod.rs") {
        parts.pop();
    } else if let Some(last) = parts.last_mut() {
        if let Some(stripped) = last.strip_suffix(".rs") {
            *last = stripped.to_string();
        }
    }
    Ok(parts.join("::"))
}

fn rust_files(root: &Path) -> AppResult<Vec<PathBuf>> {
    files_with_suffixes(root, &["rs"])
}

fn files_with_suffixes(root: &Path, suffixes: &[&str]) -> AppResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files_with_suffixes(root, suffixes, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files_with_suffixes(
    root: &Path,
    suffixes: &[&str],
    files: &mut Vec<PathBuf>,
) -> AppResult<()> {
    let mut entries = fs::read_dir(root)
        .map_err(|error| format!("{}: {error}", root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", root.display()))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_files_with_suffixes(&path, suffixes, files)?;
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| suffixes.contains(&extension))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn find_repo_root(start: &Path) -> AppResult<PathBuf> {
    for candidate in start.ancestors() {
        if candidate.join("Cargo.toml").is_file() && candidate.join("examples/demo").is_dir() {
            return Ok(candidate.to_path_buf());
        }
    }
    Err(format!("could not find repo root from {}", start.display()))
}

fn is_case_id(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '.' | '-')
        })
}

fn has_should_segment(value: &str) -> bool {
    value
        .split('.')
        .any(|segment| segment.starts_with("should_"))
}

fn display_list(values: &[&str]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}
