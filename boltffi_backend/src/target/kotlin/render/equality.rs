use askama::Template as AskamaTemplate;

use crate::{
    core::Result,
    target::kotlin::{
        KotlinHost,
        syntax::{Expression, Identifier, Statement, TypeName},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/equality.kt", escape = "none")]
struct EqualityTemplate<'equality> {
    equality: &'equality StructuralEquality,
}

/// How a generated `equals` compares one property.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Comparison {
    /// `==`, as the data class would.
    Value,
    /// Boxed `equals`, the total order a data class uses for `Float` and `Double`.
    Float {
        nullable: bool,
    },
    /// `contentEquals`, which also accepts nullable arrays.
    Array,
    /// Element-wise over a `List` whose elements compare by content.
    List(Box<Comparison>),
    Map {
        key: Box<Comparison>,
        value: Box<Comparison>,
    },
    Tuple(Vec<(Identifier, Comparison)>),
    Result {
        ok: Box<Comparison>,
        error: Box<Comparison>,
    },
    Nullable(Box<Comparison>),
}

impl Comparison {
    pub fn map(key: Self, value: Self) -> Self {
        if key.by_content() || value.by_content() {
            Self::Map {
                key: Box::new(key),
                value: Box::new(value),
            }
        } else {
            Self::Value
        }
    }

    pub fn sequence(element: Self) -> Self {
        if element.by_content() {
            Self::List(Box::new(element))
        } else {
            Self::Value
        }
    }

    pub fn tuple(fields: Vec<(Identifier, Self)>) -> Self {
        if fields.iter().any(|(_, comparison)| comparison.by_content()) {
            Self::Tuple(fields)
        } else {
            Self::Value
        }
    }

    pub fn result(ok: Self, error: Self) -> Self {
        if ok.by_content() || error.by_content() {
            Self::Result {
                ok: Box::new(ok),
                error: Box::new(error),
            }
        } else {
            Self::Value
        }
    }

    pub fn optional(self) -> Self {
        match self {
            Self::Value | Self::Array => self,
            Self::Float { .. } => Self::Float { nullable: true },
            comparison => Self::Nullable(Box::new(comparison)),
        }
    }

    fn by_content(&self) -> bool {
        !matches!(self, Self::Value | Self::Float { .. })
    }
}

/// `equals` and `hashCode` for a data class holding arrays, which the
/// generated members would compare by identity. `toString` still prints arrays by identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructuralEquality {
    owner: TypeName,
    equals: Vec<Expression>,
    hash_codes: Vec<Expression>,
}

impl StructuralEquality {
    /// Returns `None` when no property is an array, leaving the data class members in place.
    pub fn new<'property>(
        owner: TypeName,
        properties: impl IntoIterator<Item = (&'property Identifier, &'property Comparison)>,
    ) -> Result<Option<Self>> {
        let properties = properties.into_iter().collect::<Vec<_>>();
        if !properties
            .iter()
            .any(|(_, comparison)| comparison.by_content())
        {
            return Ok(None);
        }
        let other = Expression::identifier(Identifier::parse("other")?);
        let (equals, hash_codes) = properties
            .into_iter()
            .map(|(name, comparison)| {
                Self::property(
                    Expression::property(Expression::this(), name.clone()),
                    Expression::property(other.clone(), name.clone()),
                    comparison,
                    0,
                )
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .unzip();
        Ok(Some(Self {
            owner,
            equals,
            hash_codes,
        }))
    }

    pub fn render(&self, prefix: &str) -> Result<String> {
        Ok(EqualityTemplate { equality: self }
            .render()?
            .trim()
            .lines()
            .map(|line| match line.is_empty() {
                true => String::new(),
                false => format!("{prefix}{line}"),
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }

    pub fn owner(&self) -> &TypeName {
        &self.owner
    }

    pub fn equals(&self) -> &[Expression] {
        &self.equals
    }

    pub fn hash_codes(&self) -> &[Expression] {
        &self.hash_codes
    }

    /// Renders the `equals` term and hash of one value; `depth` keeps nested lambda parameters distinct.
    fn property(
        this: Expression,
        other: Expression,
        comparison: &Comparison,
        depth: usize,
    ) -> Result<(Expression, Expression)> {
        let name = |base: &str| match depth {
            0 => Identifier::parse(base),
            _ => Identifier::parse(format!("{base}{depth}")),
        };
        let hash_code = Identifier::parse("hashCode")?;
        let content_equals = Identifier::parse("contentEquals")?;
        let content_hash_code = Identifier::parse("contentHashCode")?;
        Ok(match comparison {
            Comparison::Value => (this.clone().equal(other), this.convert(hash_code)),
            Comparison::Float { nullable: false } => (
                Expression::call(
                    this.clone(),
                    Identifier::parse("equals")?,
                    [other].into_iter().collect(),
                ),
                this.convert(hash_code),
            ),
            Comparison::Float { nullable: true } => (
                Expression::safe_call(
                    this.clone(),
                    Identifier::parse("equals")?,
                    [other.clone()].into_iter().collect(),
                )
                .or_else(other.equal(Expression::null()).parenthesized()),
                this.convert(hash_code),
            ),
            Comparison::Array => (
                Expression::call(this.clone(), content_equals, [other].into_iter().collect()),
                this.convert(content_hash_code),
            ),
            Comparison::List(element) => {
                let size = Identifier::parse("size")?;
                let index = Expression::identifier(name("index")?);
                let hash = name("hash")?;
                let item = name("element")?;
                let (element_equals, _) = Self::property(
                    this.clone().index(index.clone()),
                    other.clone().index(index),
                    element,
                    depth + 1,
                )?;
                let (_, element_hash) = Self::property(
                    Expression::identifier(item.clone()),
                    Expression::null(),
                    element,
                    depth + 1,
                )?;
                (
                    Expression::property(this.clone(), size.clone())
                        .equal(Expression::property(other, size))
                        .and(
                            Expression::property(this.clone(), Identifier::parse("indices")?)
                                .all(name("index")?, element_equals),
                        ),
                    this.fold(
                        Expression::integer(1),
                        hash.clone(),
                        item,
                        Expression::integer(31)
                            .multiply(Expression::identifier(hash))
                            .add(element_hash),
                    ),
                )
            }
            Comparison::Map {
                key: key_comparison,
                value,
            } => {
                let size = Identifier::parse("size")?;
                let entries = Identifier::parse("entries")?;
                let entry = Expression::identifier(name("entry")?);
                let key = Expression::property(entry.clone(), Identifier::parse("key")?);
                let hash = name("hash")?;
                let candidate = Expression::identifier(name("candidate")?);
                let other_value = if key_comparison.by_content() {
                    Expression::property(candidate.clone(), Identifier::parse("value")?)
                } else {
                    Expression::call(
                        other.clone(),
                        Identifier::parse("getValue")?,
                        [key.clone()].into_iter().collect(),
                    )
                };
                let (value_equals, value_hash) = Self::property(
                    Expression::property(entry.clone(), Identifier::parse("value")?),
                    other_value,
                    value,
                    depth + 1,
                )?;
                let (key_equals, key_hash) = Self::property(
                    key.clone(),
                    Expression::property(candidate, Identifier::parse("key")?),
                    key_comparison,
                    depth + 1,
                )?;
                let entries_equal = if key_comparison.by_content() {
                    let unmatched_name = name("unmatchedEntries")?;
                    let unmatched = Expression::identifier(unmatched_name.clone());
                    let candidates_name = name("candidates")?;
                    let candidates = Expression::identifier(candidates_name.clone());
                    let index_name = name("matchedIndex")?;
                    let index = Expression::identifier(index_name.clone());
                    let matches = key_equals.and(value_equals);
                    Expression::run(
                        vec![Statement::value(
                            unmatched_name,
                            Expression::property(other.clone(), entries.clone()).group_by_to(
                                Expression::invoke("mutableMapOf", [].into_iter().collect()),
                                name("entry")?,
                                key_hash.clone(),
                            ),
                        )],
                        Expression::property(this.clone(), entries.clone()).all(
                            name("entry")?,
                            unmatched.index(key_hash.clone()).let_or_else(
                                candidates_name,
                                Expression::run(
                                    vec![Statement::value(
                                        index_name,
                                        candidates
                                            .clone()
                                            .index_of_first(name("candidate")?, matches),
                                    )],
                                    Expression::conditional(
                                        index.clone().not_equal(Expression::integer(-1)),
                                        Expression::run(
                                            vec![Statement::expression(Expression::call(
                                                candidates,
                                                Identifier::parse("removeAt")?,
                                                [index].into_iter().collect(),
                                            ))],
                                            Expression::bool(true),
                                        ),
                                        Expression::bool(false),
                                    ),
                                ),
                                Expression::bool(false),
                            ),
                        ),
                    )
                } else {
                    Expression::property(this.clone(), entries.clone()).all(
                        name("entry")?,
                        Expression::call(
                            other.clone(),
                            Identifier::parse("containsKey")?,
                            [key].into_iter().collect(),
                        )
                        .and(value_equals),
                    )
                };
                (
                    Expression::property(this.clone(), size.clone())
                        .equal(Expression::property(other.clone(), size))
                        .and(entries_equal),
                    Expression::property(this, entries).fold(
                        Expression::integer(0),
                        hash.clone(),
                        name("entry")?,
                        Expression::identifier(hash).add(Expression::call(
                            key_hash,
                            Identifier::parse("xor")?,
                            [value_hash].into_iter().collect(),
                        )),
                    ),
                )
            }
            Comparison::Tuple(fields) => fields
                .iter()
                .map(|(field, comparison)| {
                    Self::property(
                        Expression::property(this.clone(), field.clone()),
                        Expression::property(other.clone(), field.clone()),
                        comparison,
                        depth,
                    )
                })
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .reduce(|(equals, hash), (next_equals, next_hash)| {
                    (
                        equals.and(next_equals),
                        Expression::integer(31)
                            .multiply(hash.parenthesized())
                            .add(next_hash),
                    )
                })
                .map(|(equals, hash)| (equals.parenthesized(), hash.parenthesized()))
                .ok_or_else(|| KotlinHost::unsupported("tuple comparison without fields"))?,
            Comparison::Result { ok, error } => {
                let ok_name = name("ok")?;
                let error_name = name("error")?;
                let other_name = name("otherResult")?;
                let other_result = Expression::identifier(other_name.clone());
                let (ok_equals, ok_hash) = Self::property(
                    Expression::identifier(ok_name.clone()),
                    Expression::property(other_result.clone(), Identifier::parse("value")?),
                    ok,
                    depth + 1,
                )?;
                let (error_equals, error_hash) = Self::property(
                    Expression::identifier(error_name.clone()),
                    Expression::property(other_result.clone(), Identifier::parse("error")?),
                    error,
                    depth + 1,
                )?;
                (
                    Expression::run(
                        vec![Statement::value(other_name, other)],
                        this.clone().result_fold(
                            ok_name.clone(),
                            other_result
                                .clone()
                                .type_check(TypeName::new("BoltFFIResult.Ok"))
                                .and(ok_equals),
                            error_name.clone(),
                            other_result
                                .type_check(TypeName::new("BoltFFIResult.Err"))
                                .and(error_equals),
                        ),
                    ),
                    this.result_fold(ok_name, ok_hash, error_name, error_hash),
                )
            }
            Comparison::Nullable(inner) => {
                let left = name("left")?;
                let right = name("right")?;
                let (inner_equals, inner_hash) = Self::property(
                    Expression::identifier(left.clone()),
                    Expression::identifier(right.clone()),
                    inner,
                    depth,
                )?;
                (
                    this.clone()
                        .let_or_else(
                            left.clone(),
                            other
                                .clone()
                                .let_or_else(right, inner_equals, Expression::bool(false)),
                            other.equal(Expression::null()).parenthesized(),
                        )
                        .parenthesized(),
                    this.let_or_else(left, inner_hash, Expression::integer(0))
                        .parenthesized(),
                )
            }
        })
    }
}
