use askama::Template as AskamaTemplate;

use crate::{
    core::Result,
    target::kotlin::syntax::{Expression, Identifier, TypeName},
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
    Float { nullable: bool },
    /// `contentEquals`, which also accepts nullable arrays.
    Array,
    /// Element-wise over a `List` whose elements compare by content.
    List(Box<Comparison>),
    /// Entry-wise over a `Map` whose values compare by content; keys use `==`.
    Map(Box<Comparison>),
    /// Null-safe `List` or `Map` comparison.
    Nullable(Box<Comparison>),
}

impl Comparison {
    /// Wraps an element comparison in a container one, or `Value` when the element needs no content comparison.
    pub fn container(element: Self, container: fn(Box<Self>) -> Self) -> Self {
        match element.by_content() {
            true => container(Box::new(element)),
            false => Self::Value,
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
            Comparison::Map(value) => {
                let size = Identifier::parse("size")?;
                let entries = Identifier::parse("entries")?;
                let entry = Expression::identifier(name("entry")?);
                let key = Expression::property(entry.clone(), Identifier::parse("key")?);
                let hash = name("hash")?;
                let (value_equals, value_hash) = Self::property(
                    Expression::property(entry.clone(), Identifier::parse("value")?),
                    Expression::call(
                        other.clone(),
                        Identifier::parse("getValue")?,
                        [key.clone()].into_iter().collect(),
                    ),
                    value,
                    depth + 1,
                )?;
                (
                    Expression::property(this.clone(), size.clone())
                        .equal(Expression::property(other.clone(), size))
                        .and(
                            Expression::property(this.clone(), entries.clone()).all(
                                name("entry")?,
                                Expression::call(
                                    other,
                                    Identifier::parse("containsKey")?,
                                    [key.clone()].into_iter().collect(),
                                )
                                .and(value_equals),
                            ),
                        ),
                    Expression::property(this, entries).fold(
                        Expression::integer(0),
                        hash.clone(),
                        name("entry")?,
                        Expression::identifier(hash).add(Expression::call(
                            key.convert(hash_code),
                            Identifier::parse("xor")?,
                            [value_hash].into_iter().collect(),
                        )),
                    ),
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
