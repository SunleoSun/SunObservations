use super::*;
use std::{cmp::Ordering, marker::PhantomData, ops::RangeInclusive};

#[doc(hidden)]
pub fn domain_error(message: impl Into<String>) -> AnalysisError {
    AnalysisError {
        message: message.into(),
    }
}

/// Structural input representation used only at the typed Domain boundary.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub enum DomainValue {
    Int(i128),
    UInt(u128),
    Real(f64),
    Bool(bool),
    Text(String),
    Struct(Vec<Self>),
    Enum(usize, Vec<Self>),
    Sequence(Vec<Self>),
}
impl DomainValue {
    pub(crate) fn cmp_scalar(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a.partial_cmp(b),
            (Self::UInt(a), Self::UInt(b)) => a.partial_cmp(b),
            (Self::Real(a), Self::Real(b)) => a.partial_cmp(b),
            (Self::Bool(a), Self::Bool(b)) => a.partial_cmp(b),
            (Self::Text(a), Self::Text(b)) => a.partial_cmp(b),
            (Self::Enum(a, _), Self::Enum(b, _)) => a.partial_cmp(b),
            _ => None,
        }
    }
    pub(crate) fn offset(&self, n: i128) -> Option<Self> {
        match self {
            Self::Int(a) => a.checked_add(n).map(Self::Int),
            Self::UInt(a) => {
                if n < 0 {
                    a.checked_sub(n.unsigned_abs()).map(Self::UInt)
                } else {
                    a.checked_add(n as u128).map(Self::UInt)
                }
            }
            Self::Real(a) => {
                let v = *a + n as f64;
                v.is_finite().then_some(Self::Real(v))
            }
            _ => None,
        }
    }
    pub(crate) fn interpolate(&self, other: &Self, n: u64, d: u64) -> Self {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => {
                let span = b.saturating_sub(*a);
                Self::Int(a.saturating_add(
                    (span / d as i128) * n as i128 + ((span % d as i128) * n as i128) / d as i128,
                ))
            }
            (Self::UInt(a), Self::UInt(b)) => {
                let span = b - a;
                Self::UInt(
                    a + (span / d as u128) * n as u128
                        + ((span % d as u128) * n as u128) / d as u128,
                )
            }
            (Self::Real(a), Self::Real(b)) => {
                Self::Real(a * (1. - n as f64 / d as f64) + b * (n as f64 / d as f64))
            }
            _ => self.clone(),
        }
    }
    pub(crate) fn key(&self) -> DomainValueKey {
        match self {
            Self::Int(v) => DomainValueKey::Int(*v),
            Self::UInt(v) => DomainValueKey::UInt(*v),
            Self::Real(v) => DomainValueKey::Real(if *v == 0. { 0 } else { v.to_bits() }),
            Self::Bool(v) => DomainValueKey::Bool(*v),
            Self::Enum(v, _) => DomainValueKey::Variant(*v),
            Self::Text(v) => DomainValueKey::Text(v.clone()),
            _ => DomainValueKey::Composite,
        }
    }
}
/// Schema owned by DomainInput; field and variant positions, not names, identify paths.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub enum DomainSchema {
    Integer { min: i128, max: i128 },
    Unsigned,
    Real { max: f64 },
    Bool,
    Text,
    Struct(Vec<(&'static str, Self)>),
    Enum(Vec<(&'static str, Vec<(&'static str, Self)>)>),
    Sequence(Box<Self>),
}
impl DomainSchema {
    pub(crate) fn accepts(&self, v: &DomainValue) -> bool {
        match (self, v) {
            (Self::Integer { min, max }, DomainValue::Int(v)) => v >= min && v <= max,
            (Self::Unsigned, DomainValue::UInt(_)) => true,
            (Self::Real { max }, DomainValue::Real(v)) => v.is_finite() && v.abs() <= *max,
            (Self::Bool, DomainValue::Bool(_)) | (Self::Text, DomainValue::Text(_)) => true,
            (Self::Enum(vs), DomainValue::Enum(i, payload)) => {
                vs.get(*i).is_some_and(|(_, fields)| {
                    payload.is_empty()
                        || (fields.len() == payload.len()
                            && fields.iter().zip(payload).all(|((_, s), v)| s.accepts(v)))
                })
            }
            (Self::Struct(fs), DomainValue::Struct(vs)) => {
                fs.len() == vs.len() && fs.iter().zip(vs).all(|((_, s), v)| s.accepts(v))
            }
            (Self::Sequence(s), DomainValue::Sequence(vs)) => vs.iter().all(|v| s.accepts(v)),
            _ => false,
        }
    }
}
/// A fully constructible Rust input and its compiler-checked field descriptors.
///
/// Relations preserve scalar types, including derived collection lengths.
/// ```compile_fail
/// use sun_observations::certifications::*;
/// #[derive(DomainInput)]
/// struct Input { count: usize, threshold: f64 }
/// let _ = domain!(Input {
///     count.in_range(1..=9), threshold.in_range(0.0..=1.0),
///     count.less_equal(threshold),
/// });
/// ```
///
/// Variant descriptors belong to the selected enum type, not just a matching name.
/// ```compile_fail
/// use sun_observations::certifications::*;
/// #[derive(DomainInput)] enum Mode { Fast { count: usize } }
/// #[derive(DomainInput)] enum Other { Fast { count: usize } }
/// #[derive(DomainInput)] struct Input { mode: Mode }
/// let _ = domain!(Input {
///     mode { all_enum_values(), Other::Fast { count.fixed_value(1) } },
/// });
/// ```
///
/// Arbitrary structs are not scalar relation operands.
/// ```compile_fail
/// use sun_observations::certifications::*;
/// #[derive(DomainInput)] struct Nested { n: usize }
/// #[derive(DomainInput)] struct Input { a: Nested, b: Nested }
/// let _ = domain!(Input { a { n.fixed_value(1) }, b { n.fixed_value(1) }, a.equal(b) });
/// ```
pub trait DomainInput: Sized {
    type Fields;
    fn schema() -> DomainSchema;
    fn domain_fields(field: DomainField<Self>) -> Self::Fields;
    fn to_domain_value(&self) -> DomainValue;
    fn from_domain_value(value: DomainValue) -> Result<Self, AnalysisError>;
}
macro_rules! integers { ($($t:ty),*) => {$ (impl DomainInput for $t {
    type Fields=(); fn schema()->DomainSchema {DomainSchema::Integer{min:<$t>::MIN as i128,max:<$t>::MAX as i128}}
    fn domain_fields(_:DomainField<Self>){}
    fn to_domain_value(&self)->DomainValue {DomainValue::Int(*self as i128)}
    fn from_domain_value(v:DomainValue)->Result<Self,AnalysisError> {if let DomainValue::Int(n)=v {Self::try_from(n).map_err(|_|domain_error("integer out of bounds"))}else{Err(domain_error("not an integer"))}}
} impl DomainScalar for $t {} impl DomainOrdered for $t {})*}; }
integers!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, usize);
impl DomainInput for u128 {
    type Fields = ();
    fn schema() -> DomainSchema {
        DomainSchema::Unsigned
    }
    fn domain_fields(_: DomainField<Self>) {}
    fn to_domain_value(&self) -> DomainValue {
        DomainValue::UInt(*self)
    }
    fn from_domain_value(value: DomainValue) -> Result<Self, AnalysisError> {
        if let DomainValue::UInt(n) = value {
            Ok(n)
        } else {
            Err(domain_error("not u128"))
        }
    }
}
impl DomainScalar for u128 {}
impl DomainOrdered for u128 {}
macro_rules! reals { ($($t:ty),*) => {$ (impl DomainInput for $t { type Fields=(); fn schema()->DomainSchema{DomainSchema::Real{max:<$t>::MAX as f64}} fn domain_fields(_:DomainField<Self>){} fn to_domain_value(&self)->DomainValue{DomainValue::Real(*self as f64)} fn from_domain_value(v:DomainValue)->Result<Self,AnalysisError>{if let DomainValue::Real(n)=v {if n.is_finite()&&n.abs()<=<$t>::MAX as f64 {Ok(n as Self)}else{Err(domain_error("nonfinite or out of bounds real"))}}else{Err(domain_error("not a real"))}} } impl DomainScalar for $t {} impl DomainOrdered for $t {})*}; }
reals!(f32, f64);
impl DomainInput for bool {
    type Fields = ();
    fn schema() -> DomainSchema {
        DomainSchema::Bool
    }
    fn domain_fields(_: DomainField<Self>) {}
    fn to_domain_value(&self) -> DomainValue {
        DomainValue::Bool(*self)
    }
    fn from_domain_value(v: DomainValue) -> Result<Self, AnalysisError> {
        if let DomainValue::Bool(v) = v {
            Ok(v)
        } else {
            Err(domain_error("not a bool"))
        }
    }
}
impl DomainScalar for bool {}
impl DomainInput for String {
    type Fields = ();
    fn schema() -> DomainSchema {
        DomainSchema::Text
    }
    fn domain_fields(_: DomainField<Self>) {}
    fn to_domain_value(&self) -> DomainValue {
        DomainValue::Text(self.clone())
    }
    fn from_domain_value(v: DomainValue) -> Result<Self, AnalysisError> {
        if let DomainValue::Text(v) = v {
            Ok(v)
        } else {
            Err(domain_error("not text"))
        }
    }
}
impl<T: DomainInput> DomainInput for Vec<T> {
    type Fields = ();
    fn schema() -> DomainSchema {
        DomainSchema::Sequence(Box::new(T::schema()))
    }
    fn domain_fields(_: DomainField<Self>) {}
    fn to_domain_value(&self) -> DomainValue {
        DomainValue::Sequence(self.iter().map(T::to_domain_value).collect())
    }
    fn from_domain_value(v: DomainValue) -> Result<Self, AnalysisError> {
        if let DomainValue::Sequence(v) = v {
            v.into_iter().map(T::from_domain_value).collect()
        } else {
            Err(domain_error("not a collection"))
        }
    }
}
/// Scalar relations deliberately exclude arbitrary structs and strings.
pub trait DomainScalar: DomainInput {}
/// Ordered numeric value spaces and relations.
pub trait DomainOrdered: DomainScalar {}
/// Enum choices, including payload variants, identified by their derived schema.
pub trait DomainEnum: DomainScalar {}

/// Structural step in an input schema, including homogeneous collection elements.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DomainPathStep {
    Field(usize),
    Variant(usize),
    Length,
    Each,
}
pub(crate) type InputPath = Vec<DomainPathStep>;
/// Typed schema accessor; only the derive-generated descriptors construct field paths.
#[derive(Debug)]
pub struct DomainField<T> {
    pub(crate) path: InputPath,
    marker: PhantomData<fn() -> T>,
}
impl<T> Clone for DomainField<T> {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            marker: PhantomData,
        }
    }
}
impl<T> DomainField<T> {
    #[doc(hidden)]
    pub fn assert_type(_: &Self) {}
    #[doc(hidden)]
    pub fn root() -> Self {
        Self {
            path: vec![],
            marker: PhantomData,
        }
    }
    #[doc(hidden)]
    pub fn child<U>(&self, step: DomainPathStep) -> DomainField<U> {
        let mut path = self.path.clone();
        path.push(step);
        DomainField {
            path,
            marker: PhantomData,
        }
    }
}
impl<T: DomainInput> DomainField<T> {
    pub fn fields(&self) -> T::Fields {
        T::domain_fields(self.clone())
    }
    pub fn fixed_value(&self, value: T) -> DomainSource {
        self.selected_values([value])
    }
    pub fn selected_values(&self, values: impl IntoIterator<Item = T>) -> DomainSource {
        DomainSource {
            path: self.path.clone(),
            space: ValueSpace::Selected(values.into_iter().map(|v| v.to_domain_value()).collect()),
        }
    }
    pub fn all_enum_values(&self) -> DomainSource
    where
        T: DomainEnum,
    {
        let values = if let DomainSchema::Enum(v) = T::schema() {
            (0..v.len()).map(|i| DomainValue::Enum(i, vec![])).collect()
        } else {
            vec![]
        };
        DomainSource {
            path: self.path.clone(),
            space: ValueSpace::Enum(values),
        }
    }
    pub fn selected_enum_values(&self, values: impl IntoIterator<Item = T>) -> DomainSource
    where
        T: DomainEnum,
    {
        DomainSource {
            path: self.path.clone(),
            space: ValueSpace::Enum(
                values
                    .into_iter()
                    .map(|v| {
                        let DomainValue::Enum(i, _) = v.to_domain_value() else {
                            return v.to_domain_value();
                        };
                        DomainValue::Enum(i, vec![])
                    })
                    .collect(),
            ),
        }
    }
    pub fn equal(&self, right: impl IntoDomainOperand<T>) -> DomainRelation
    where
        T: DomainScalar,
    {
        self.relation(RelationKind::Equal, right)
    }
    pub fn not_equal(&self, right: impl IntoDomainOperand<T>) -> DomainRelation
    where
        T: DomainScalar,
    {
        self.relation(RelationKind::NotEqual, right)
    }
    fn relation(&self, kind: RelationKind, right: impl IntoDomainOperand<T>) -> DomainRelation {
        DomainRelation {
            left: DomainOperand::Field(self.path.clone()),
            right: right.into_operand(),
            kind,
        }
    }
}
impl<T: DomainOrdered> DomainField<T> {
    pub fn in_range(&self, range: RangeInclusive<T>) -> DomainSource {
        self.one_of_ranges([range])
    }
    pub fn one_of_ranges(
        &self,
        ranges: impl IntoIterator<Item = RangeInclusive<T>>,
    ) -> DomainSource {
        let mut ranges: Vec<_> = ranges
            .into_iter()
            .map(|range| {
                (
                    range.start().to_domain_value(),
                    range.end().to_domain_value(),
                )
            })
            .collect();
        ranges.sort_by(|(a, _), (b, _)| a.cmp_scalar(b).unwrap_or(Ordering::Equal));
        DomainSource {
            path: self.path.clone(),
            space: ValueSpace::Ranges(ranges),
        }
    }
    pub fn less_than(&self, right: impl IntoDomainOperand<T>) -> DomainRelation {
        self.relation(RelationKind::LessThan, right)
    }
    pub fn less_equal(&self, right: impl IntoDomainOperand<T>) -> DomainRelation {
        self.relation(RelationKind::LessEqual, right)
    }
    pub fn greater_than(&self, right: impl IntoDomainOperand<T>) -> DomainRelation {
        self.relation(RelationKind::GreaterThan, right)
    }
    pub fn greater_equal(&self, right: impl IntoDomainOperand<T>) -> DomainRelation {
        self.relation(RelationKind::GreaterEqual, right)
    }
}
impl<T> DomainField<Vec<T>> {
    pub fn length(&self) -> DomainField<usize> {
        self.child(DomainPathStep::Length)
    }
    pub fn each(&self) -> DomainField<T> {
        self.child(DomainPathStep::Each)
    }
}
/// Typed conversion for a relation operand; incompatible field types fail to compile.
pub trait IntoDomainOperand<T> {
    fn into_operand(self) -> DomainOperand;
}
impl<T: DomainInput> IntoDomainOperand<T> for T {
    fn into_operand(self) -> DomainOperand {
        DomainOperand::Value(self.to_domain_value())
    }
}
impl<T> IntoDomainOperand<T> for DomainField<T> {
    fn into_operand(self) -> DomainOperand {
        DomainOperand::Field(self.path)
    }
}
impl<T> IntoDomainOperand<T> for &DomainField<T> {
    fn into_operand(self) -> DomainOperand {
        DomainOperand::Field(self.path.clone())
    }
}
#[doc(hidden)]
#[derive(Clone, Debug)]
pub enum DomainOperand {
    Field(InputPath),
    Value(DomainValue),
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum RelationKind {
    LessThan,
    LessEqual,
    GreaterThan,
    GreaterEqual,
    Equal,
    NotEqual,
}
/// Inspectable scalar comparison, also used for conditional activation.
#[derive(Clone, Debug)]
pub struct DomainRelation {
    pub(crate) left: DomainOperand,
    pub(crate) right: DomainOperand,
    pub(crate) kind: RelationKind,
}
impl DomainRelation {
    pub(crate) fn invalid_pairs(
        &self,
        left: &[DomainValue],
        right: &[DomainValue],
    ) -> Vec<(DomainValue, DomainValue)> {
        let mut pairs = vec![];
        for a in left {
            for b in right {
                let relation = Self {
                    left: DomainOperand::Value(a.clone()),
                    right: DomainOperand::Value(b.clone()),
                    kind: self.kind,
                };
                if relation.evaluate(&[]) == Some(false) && !pairs.contains(&(a.clone(), b.clone()))
                {
                    pairs.push((a.clone(), b.clone()));
                }
            }
        }
        pairs.sort_by(|(a, b), (c, d)| {
            Self::distance(a, b)
                .total_cmp(&Self::distance(c, d))
                .then_with(|| (a.key(), b.key()).cmp(&(c.key(), d.key())))
        });
        pairs
    }
    fn distance(a: &DomainValue, b: &DomainValue) -> f64 {
        match (a, b) {
            (DomainValue::Int(a), DomainValue::Int(b)) => a.abs_diff(*b) as f64,
            (DomainValue::UInt(a), DomainValue::UInt(b)) => a.abs_diff(*b) as f64,
            (DomainValue::Real(a), DomainValue::Real(b)) => (a - b).abs(),
            _ => 0.,
        }
    }
    pub(crate) fn negative_distance(
        &self,
        a: &DomainValue,
        b: &DomainValue,
        left: &[DomainValue],
        right: &[DomainValue],
    ) -> DomainNegativeDistance {
        let pairs = self.invalid_pairs(left, right);
        let Some(first) = pairs.first() else {
            return DomainNegativeDistance::Near;
        };
        let last = pairs.last().unwrap();
        let low = Self::distance(&first.0, &first.1);
        let high = Self::distance(&last.0, &last.1);
        let distance = Self::distance(a, b);
        if high <= low || distance <= low + (high - low) / 3. {
            DomainNegativeDistance::Near
        } else if distance < low + 2. * ((high - low) / 3.) {
            DomainNegativeDistance::Mid
        } else {
            DomainNegativeDistance::Far
        }
    }
    pub(crate) fn paths(&self) -> Vec<InputPath> {
        [&self.left, &self.right]
            .into_iter()
            .filter_map(|o| {
                if let DomainOperand::Field(p) = o {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    pub(crate) fn evaluate(&self, env: &[(InputPath, DomainValue)]) -> Option<bool> {
        fn value<'a>(
            o: &'a DomainOperand,
            env: &'a [(InputPath, DomainValue)],
        ) -> Option<&'a DomainValue> {
            match o {
                DomainOperand::Value(v) => Some(v),
                DomainOperand::Field(p) => env.iter().find(|(q, _)| p == q).map(|(_, v)| v),
            }
        }
        let a = value(&self.left, env)?;
        let b = value(&self.right, env)?;
        let c = a.cmp_scalar(b)?;
        Some(match self.kind {
            RelationKind::LessThan => c == Ordering::Less,
            RelationKind::LessEqual => c != Ordering::Greater,
            RelationKind::GreaterThan => c == Ordering::Greater,
            RelationKind::GreaterEqual => c != Ordering::Less,
            RelationKind::Equal => c == Ordering::Equal,
            RelationKind::NotEqual => c != Ordering::Equal,
        })
    }
}
#[derive(Clone, Debug)]
pub(crate) enum ValueSpace {
    Selected(Vec<DomainValue>),
    Enum(Vec<DomainValue>),
    Ranges(Vec<(DomainValue, DomainValue)>),
}
/// One constructive value space for an active input field.
#[derive(Clone, Debug)]
pub struct DomainSource {
    pub(crate) path: InputPath,
    pub(crate) space: ValueSpace,
}
#[derive(Clone, Debug)]
pub(crate) enum DomainRuleKind {
    Source(DomainSource),
    Relation(DomainRelation),
}
/// Typed DSL registration boundary. Conditions never contain opaque predicates.
pub trait IntoDomainRule {
    #[doc(hidden)]
    fn add_to(self, rules: &mut Vec<DomainRule>, gates: &[DomainRelation]);
}
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct DomainRule {
    pub(crate) kind: DomainRuleKind,
    pub(crate) gates: Vec<DomainRelation>,
}
impl IntoDomainRule for DomainSource {
    fn add_to(self, rules: &mut Vec<DomainRule>, gates: &[DomainRelation]) {
        rules.push(DomainRule {
            kind: DomainRuleKind::Source(self),
            gates: gates.to_vec(),
        })
    }
}
impl IntoDomainRule for DomainRelation {
    fn add_to(self, rules: &mut Vec<DomainRule>, gates: &[DomainRelation]) {
        rules.push(DomainRule {
            kind: DomainRuleKind::Relation(self),
            gates: gates.to_vec(),
        })
    }
}
/// Complete, inspectable input contract; compilation validates all reachable branches.
#[derive(Clone, Debug)]
pub struct Domain<T> {
    pub(crate) rules: Vec<DomainRule>,
    marker: PhantomData<fn() -> T>,
}
impl<T: DomainInput> Default for Domain<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T: DomainInput> Domain<T> {
    pub fn new() -> Self {
        Self {
            rules: vec![],
            marker: PhantomData,
        }
    }
    #[doc(hidden)]
    pub fn add(&mut self, rule: impl IntoDomainRule, gates: &[DomainRelation]) {
        rule.add_to(&mut self.rules, gates)
    }
    pub fn compile(self) -> Result<CompiledDomain<T>, AnalysisError> {
        CompiledDomain::compile(self)
    }
}

/// Position inside the valid distribution, not invalid-input severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DomainTercile {
    Low,
    Mid,
    High,
}
/// Ordered distance into an invalid region; separate from DomainTercile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DomainNegativeDistance {
    Near,
    Mid,
    Far,
}
/// Semantic region owned by one input constraint.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CaseDomainCoordinateSector {
    pub constraint: usize,
    pub region: DomainRegion,
}
/// Structural region identity. Disconnected range branches remain distinct.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DomainRegion {
    Range {
        branch: usize,
        tercile: DomainTercile,
    },
    Enum,
    Discrete(usize),
    Below,
    Above,
    Gap(usize),
    Relation,
    ExcludedVariant,
    OutsideSelection,
}
impl DomainRegion {
    pub(crate) fn is_negative(&self) -> bool {
        matches!(
            self,
            Self::Below
                | Self::Above
                | Self::Gap(_)
                | Self::Relation
                | Self::ExcludedVariant
                | Self::OutsideSelection
        )
    }
}
/// Representative position within a semantic sector.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CaseDomainCoordinateSlot {
    Anchor(usize),
    EnumVariant(usize),
    Stratum(u64),
    Combination,
    Negative(DomainNegativeDistance),
    ExcludedVariant(usize),
    Real(DomainValueKey),
}
/// Exact scalar identity, independent of formatted diagnostics.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DomainValueKey {
    Int(i128),
    UInt(u128),
    Real(u64),
    Bool(bool),
    Text(String),
    Variant(usize),
    Composite,
}
/// An active input dimension with bounded human-readable evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainCoordinateEntry {
    pub path: Vec<DomainPathStep>,
    pub sector: CaseDomainCoordinateSector,
    pub slot: CaseDomainCoordinateSlot,
    pub name: String,
    pub value: String,
}
/// Consistent active slots for one materialized input. Inactive branches are absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseDomainCoordinate {
    pub entries: Vec<DomainCoordinateEntry>,
    pub violated: Vec<usize>,
}
impl CaseDomainCoordinate {
    pub fn category(&self) -> CaseCategory {
        match self.violated.len() {
            0 => CaseCategory::Positive,
            1 => CaseCategory::SingleNegative,
            _ => CaseCategory::MultiNegative,
        }
    }
}
/// Execution categories share coverage and representative-failure prioritization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CaseCategory {
    Positive,
    SingleNegative,
    MultiNegative,
}
impl CaseCategory {
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Positive => 0,
            Self::SingleNegative => 1,
            Self::MultiNegative => 2,
        }
    }
}

#[path = "domain_compilation.rs"]
pub(crate) mod compilation;
pub use compilation::CompiledDomain;
