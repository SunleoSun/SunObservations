use super::*;
use std::collections::BTreeSet;

#[path = "domain_classification.rs"]
mod classification;
#[path = "domain_coverage.rs"]
mod coverage;

type Assignment = Vec<(InputPath, DomainValue)>;
const SEARCH_LIMIT: usize = 100_000;
const CONTEXT_LIMIT: usize = 4096;
const COLLECTION_LIMIT: usize = 1_000_000;
const TERCILE_STRATA: u64 = 64;
const RANGE_STRATA: u64 = 3 * TERCILE_STRATA;
const STRATIFIED_RESOLUTION: u64 = 1_000_003;
fn stratum_boundary(length: u128, bin: u128) -> u128 {
    let count = RANGE_STRATA as u128;
    length / count * bin + ((length % count) * bin).div_ceil(count)
}
fn push_value(values: &mut Vec<DomainValue>, value: DomainValue) {
    if !values.contains(&value) {
        values.push(value);
    }
}
fn normalize(schema: &DomainSchema, value: DomainValue) -> DomainValue {
    match (schema, value) {
        (DomainSchema::Real { max }, DomainValue::Real(v)) if *max == f32::MAX as f64 => {
            DomainValue::Real((v as f32) as f64)
        }
        (_, v) => v,
    }
}
fn adjacent(value: &DomainValue, schema: &DomainSchema, increasing: bool) -> Option<DomainValue> {
    let result = match value {
        DomainValue::UInt(v) => DomainValue::UInt(if increasing {
            v.checked_add(1)?
        } else {
            v.checked_sub(1)?
        }),
        DomainValue::Int(v) => DomainValue::Int(if increasing {
            v.checked_add(1)?
        } else {
            v.checked_sub(1)?
        }),
        DomainValue::Real(v) => {
            if matches!(schema,DomainSchema::Real{max} if *max==f32::MAX as f64) {
                let v = *v as f32;
                let next = if v == 0. {
                    if increasing {
                        f32::from_bits(1)
                    } else {
                        -f32::from_bits(1)
                    }
                } else {
                    let bits = v.to_bits();
                    f32::from_bits(if (v > 0.) == increasing {
                        bits + 1
                    } else {
                        bits - 1
                    })
                };
                DomainValue::Real(next as f64)
            } else {
                let next = if *v == 0. {
                    if increasing {
                        f64::from_bits(1)
                    } else {
                        -f64::from_bits(1)
                    }
                } else {
                    let bits = v.to_bits();
                    f64::from_bits(if (*v > 0.) == increasing {
                        bits + 1
                    } else {
                        bits - 1
                    })
                };
                DomainValue::Real(next)
            }
        }
        _ => return None,
    };
    schema.accepts(&result).then_some(result)
}
impl ValueSpace {
    fn contains(&self, value: &DomainValue) -> bool {
        match self {
            Self::Selected(v) => v.contains(value),
            Self::Enum(v) => v
                .iter()
                .any(|v| v.cmp_scalar(value) == Some(Ordering::Equal)),
            Self::Ranges(rs) => rs.iter().any(|(a, b)| {
                value.cmp_scalar(a).is_some_and(|c| c != Ordering::Less)
                    && value.cmp_scalar(b).is_some_and(|c| c != Ordering::Greater)
            }),
        }
    }
    fn anchors(&self) -> Vec<DomainValue> {
        match self {
            Self::Selected(v) | Self::Enum(v) => {
                let mut out = vec![];
                for v in v {
                    push_value(&mut out, v.clone());
                }
                out
            }
            Self::Ranges(rs) => {
                let mut out = vec![];
                if let (Some((a, _)), Some((_, b))) = (rs.first(), rs.last()) {
                    push_value(&mut out, a.clone());
                    push_value(&mut out, b.clone());
                }
                if !rs.is_empty() {
                    let mut lengths: Vec<_> = rs
                        .iter()
                        .map(|(a, b)| match (a, b) {
                            (DomainValue::Int(a), DomainValue::Int(b)) => {
                                (b.saturating_sub(*a).saturating_add(1)) as f64
                            }
                            (DomainValue::UInt(a), DomainValue::UInt(b)) => (b - a + 1) as f64,
                            (DomainValue::Real(a), DomainValue::Real(b)) => (b - a).max(0.),
                            _ => 0.,
                        })
                        .collect();
                    if !lengths.iter().sum::<f64>().is_finite() {
                        let maximum = lengths.iter().copied().fold(0.0, f64::max);
                        for length in &mut lengths {
                            *length /= maximum;
                        }
                    }
                    let total: f64 = lengths.iter().sum();
                    let mut center = total / 2.;
                    let mut chosen = rs[0].0.clone();
                    for ((a, b), length) in rs.iter().zip(lengths) {
                        if center < length || length == 0. {
                            chosen = match (a, b) {
                                (DomainValue::Int(a), DomainValue::Int(b)) => DomainValue::Int(
                                    a.saturating_add(center.floor() as i128).min(*b),
                                ),
                                (DomainValue::UInt(a), DomainValue::UInt(b)) => DomainValue::UInt(
                                    a.saturating_add(center.floor() as u128).min(*b),
                                ),
                                _ => a.interpolate(
                                    b,
                                    ((center / length.max(f64::MIN_POSITIVE)) * 1_000_000.) as u64,
                                    1_000_000,
                                ),
                            };
                            break;
                        }
                        center -= length;
                    }
                    push_value(&mut out, chosen);
                }
                for (a, b) in rs {
                    push_value(&mut out, a.clone());
                    push_value(&mut out, b.clone());
                    match (a, b) {
                        (DomainValue::UInt(a), DomainValue::UInt(b)) => {
                            let len = b - a + 1;
                            for k in [1, 2] {
                                let start = a + (len / 3) * k + (len % 3) * k / 3;
                                if start > *a && start <= *b {
                                    push_value(&mut out, DomainValue::UInt(start - 1));
                                    push_value(&mut out, DomainValue::UInt(start));
                                }
                            }
                            for k in 0..3 {
                                let lo = a + (len / 3) * k + (len % 3) * k / 3;
                                let end = (len / 3) * (k + 1) + (len % 3) * (k + 1) / 3;
                                if end > 0 {
                                    let hi = a + (end - 1);
                                    if lo <= hi && lo <= *b {
                                        push_value(&mut out, DomainValue::UInt(lo + (hi - lo) / 2));
                                    }
                                }
                            }
                        }
                        (DomainValue::Int(a), DomainValue::Int(b)) => {
                            let len = b.saturating_sub(*a).saturating_add(1);
                            for k in [1, 2] {
                                let start = a.saturating_add((len / 3) * k + (len % 3) * k / 3);
                                if start > *a && start <= *b {
                                    push_value(&mut out, DomainValue::Int(start - 1));
                                    push_value(&mut out, DomainValue::Int(start));
                                }
                            }
                            for k in 0..3 {
                                let lo = a.saturating_add((len / 3) * k + (len % 3) * k / 3);
                                let hi = if k == 2 {
                                    *b
                                } else {
                                    a.saturating_add((len / 3) * (k + 1) + (len % 3) * (k + 1) / 3)
                                        .saturating_sub(1)
                                };
                                if lo <= hi && lo <= *b {
                                    push_value(&mut out, DomainValue::Int(lo + (hi - lo) / 2));
                                }
                            }
                        }
                        _ => {
                            for (n, d) in [(1, 3), (2, 3), (1, 6), (3, 6), (5, 6)] {
                                push_value(&mut out, a.interpolate(b, n, d));
                            }
                        }
                    }
                }
                out
            }
        }
    }
    fn negative(
        &self,
        schema: &DomainSchema,
    ) -> Vec<(DomainValue, DomainRegion, DomainNegativeDistance)> {
        let mut out = vec![];
        let mut add = |v: DomainValue, region: DomainRegion, distance: DomainNegativeDistance| {
            let v = normalize(schema, v);
            if schema.accepts(&v) && !self.contains(&v) && !out.iter().any(|(old, _, _)| old == &v)
            {
                out.push((v, region, distance));
            }
        };
        match self {
            Self::Ranges(ranges) => {
                if let (Some((a, _)), Some((_, b))) = (ranges.first(), ranges.last()) {
                    if let Some(v) = adjacent(a, schema, false) {
                        add(v, DomainRegion::Below, DomainNegativeDistance::Near)
                    }
                    if let Some(v) = adjacent(b, schema, true) {
                        add(v, DomainRegion::Above, DomainNegativeDistance::Near)
                    }
                    for (factor, distance) in [
                        (1, DomainNegativeDistance::Mid),
                        (8, DomainNegativeDistance::Far),
                    ] {
                        match (a, b) {
                            (DomainValue::UInt(a), DomainValue::UInt(b)) => {
                                let span = (b - a).clamp(1, u128::MAX / 16) * factor as u128;
                                if let Some(v) = a.checked_sub(span) {
                                    add(DomainValue::UInt(v), DomainRegion::Below, distance)
                                }
                                if let Some(v) = b.checked_add(span) {
                                    add(DomainValue::UInt(v), DomainRegion::Above, distance)
                                }
                            }
                            (DomainValue::Int(a), DomainValue::Int(b)) => {
                                let span = b
                                    .saturating_sub(*a)
                                    .clamp(1, i128::MAX / 16)
                                    .saturating_mul(factor);
                                if let Some(v) = a.checked_sub(span) {
                                    add(DomainValue::Int(v), DomainRegion::Below, distance)
                                }
                                if let Some(v) = b.checked_add(span) {
                                    add(DomainValue::Int(v), DomainRegion::Above, distance)
                                }
                            }
                            (DomainValue::Real(a), DomainValue::Real(b)) => {
                                let epsilon = if matches!(schema,DomainSchema::Real{max} if *max==f32::MAX as f64)
                                {
                                    f32::EPSILON as f64
                                } else {
                                    f64::EPSILON
                                };
                                let span = (b - a).abs().max(a.abs().max(1.) * epsilon * 8.)
                                    * factor as f64;
                                add(DomainValue::Real(a - span), DomainRegion::Below, distance);
                                add(DomainValue::Real(b + span), DomainRegion::Above, distance)
                            }
                            _ => {}
                        }
                    }
                    for (i, pair) in ranges.windows(2).enumerate() {
                        let a = &pair[0].1;
                        let b = &pair[1].0;
                        for (value, distance) in [
                            (adjacent(a, schema, true), DomainNegativeDistance::Near),
                            (adjacent(b, schema, false), DomainNegativeDistance::Near),
                            (Some(a.interpolate(b, 1, 4)), DomainNegativeDistance::Mid),
                            (Some(a.interpolate(b, 3, 4)), DomainNegativeDistance::Mid),
                            (Some(a.interpolate(b, 1, 2)), DomainNegativeDistance::Far),
                        ] {
                            if let Some(value) = value {
                                if value.cmp_scalar(a) == Some(Ordering::Greater)
                                    && value.cmp_scalar(b) == Some(Ordering::Less)
                                {
                                    add(value, DomainRegion::Gap(i), distance)
                                }
                            }
                        }
                    }
                }
            }
            Self::Enum(values) => {
                if let DomainSchema::Enum(variants) = schema {
                    for tag in 0..variants.len() {
                        let value = DomainValue::Enum(tag, vec![]);
                        if !values
                            .iter()
                            .any(|v| v.cmp_scalar(&value) == Some(Ordering::Equal))
                        {
                            add(
                                value,
                                DomainRegion::ExcludedVariant(tag),
                                DomainNegativeDistance::Near,
                            )
                        }
                    }
                }
            }
            Self::Selected(values) => {
                for value in values {
                    for increasing in [false, true] {
                        if let Some(v) = adjacent(value, schema, increasing) {
                            add(
                                v,
                                DomainRegion::OutsideSelection,
                                DomainNegativeDistance::Near,
                            )
                        }
                    }
                    for n in [-2, 2, -8, 8] {
                        if let Some(v) = value.offset(n) {
                            add(
                                v,
                                DomainRegion::OutsideSelection,
                                if n.abs() == 2 {
                                    DomainNegativeDistance::Mid
                                } else {
                                    DomainNegativeDistance::Far
                                },
                            )
                        }
                    }
                    if let DomainValue::Bool(v) = value {
                        add(
                            DomainValue::Bool(!v),
                            DomainRegion::OutsideSelection,
                            DomainNegativeDistance::Near,
                        )
                    }
                }
            }
        }
        out
    }
    fn region(&self, value: &DomainValue) -> DomainRegion {
        match self {
            Self::Enum(_) => {
                if let DomainValue::Enum(i, _) = value {
                    if self.contains(value) {
                        DomainRegion::Discrete(*i)
                    } else {
                        DomainRegion::ExcludedVariant(*i)
                    }
                } else {
                    DomainRegion::OutsideSelection
                }
            }
            Self::Selected(vs) => vs
                .iter()
                .position(|v| v == value)
                .map(DomainRegion::Discrete)
                .unwrap_or(DomainRegion::OutsideSelection),
            Self::Ranges(rs) => {
                for (branch, (a, b)) in rs.iter().enumerate() {
                    if value.cmp_scalar(a).is_some_and(|c| c != Ordering::Less)
                        && value.cmp_scalar(b).is_some_and(|c| c != Ordering::Greater)
                    {
                        let tercile = match (a, b, value) {
                            (DomainValue::UInt(a), DomainValue::UInt(b), DomainValue::UInt(v)) => {
                                let len = b - a + 1;
                                let p = v - a;
                                if p < len / 3 {
                                    DomainTercile::Low
                                } else if p < (len / 3) * 2 + (len % 3) * 2 / 3 {
                                    DomainTercile::Mid
                                } else {
                                    DomainTercile::High
                                }
                            }
                            (DomainValue::Int(a), DomainValue::Int(b), DomainValue::Int(v)) => {
                                let len = b.saturating_sub(*a).saturating_add(1);
                                let p = v.saturating_sub(*a);
                                if p < len / 3 {
                                    DomainTercile::Low
                                } else if p < (len / 3) * 2 + (len % 3) * 2 / 3 {
                                    DomainTercile::Mid
                                } else {
                                    DomainTercile::High
                                }
                            }
                            _ => {
                                if value.cmp_scalar(&a.interpolate(b, 1, 3)) == Some(Ordering::Less)
                                {
                                    DomainTercile::Low
                                } else if value.cmp_scalar(&a.interpolate(b, 2, 3))
                                    == Some(Ordering::Less)
                                {
                                    DomainTercile::Mid
                                } else {
                                    DomainTercile::High
                                }
                            }
                        };
                        return DomainRegion::Range { branch, tercile };
                    }
                }
                if let Some((a, _)) = rs.first() {
                    if value.cmp_scalar(a) == Some(Ordering::Less) {
                        return DomainRegion::Below;
                    }
                }
                for (i, pair) in rs.windows(2).enumerate() {
                    if value.cmp_scalar(&pair[1].0) == Some(Ordering::Less) {
                        return DomainRegion::Gap(i);
                    }
                }
                DomainRegion::Above
            }
        }
    }
}
#[derive(Clone, Debug)]
struct Variable {
    path: InputPath,
    name: String,
    schema: DomainSchema,
    extras: Vec<DomainValue>,
}
#[derive(Clone, Debug)]
pub(crate) struct DomainTarget {
    forced: Assignment,
    violated: Vec<usize>,
    focus: Vec<InputPath>,
    pub(crate) depth: usize,
}
/// Feasible semantic coverage, computed once from validated input witnesses.
#[derive(Clone, Debug, Default)]
pub(crate) struct DomainCoverageCatalog {
    pub positive_anchors: BTreeSet<(CaseDomainCoordinateSector, CaseDomainCoordinateSlot)>,
    pub enum_values: BTreeSet<CaseDomainCoordinateSector>,
    pub pairwise_combinations: BTreeSet<(CaseDomainCoordinateSector, CaseDomainCoordinateSector)>,
    pub negative_sectors: BTreeSet<CaseDomainCoordinateSector>,
}
/// Validated input schema and constraint-owned exploration spaces. No executor or evidence is stored here.
#[derive(Clone, Debug)]
pub struct CompiledDomain<T> {
    rules: Vec<DomainRule>,
    variables: Vec<Variable>,
    schema: DomainSchema,
    contexts: Vec<Assignment>,
    targets: [Vec<DomainTarget>; 3],
    marker: PhantomData<fn() -> T>,
    catalog: DomainCoverageCatalog,
}
fn get<'a>(env: &'a [(InputPath, DomainValue)], path: &InputPath) -> Option<&'a DomainValue> {
    env.iter().find(|(p, _)| p == path).map(|(_, v)| v)
}
fn path_active(path: &InputPath, env: &[(InputPath, DomainValue)]) -> Option<bool> {
    let mut prefix = vec![];
    for step in path {
        match step {
            DomainPathStep::Variant(i) => {
                let DomainValue::Enum(tag, _) = get(env, &prefix)? else {
                    return Some(false);
                };
                if i != tag {
                    return Some(false);
                }
            }
            DomainPathStep::Each => {
                let mut length = prefix.clone();
                length.push(DomainPathStep::Length);
                if let Some(DomainValue::Int(0)) = get(env, &length) {
                    return Some(false);
                }
            }
            _ => {}
        }
        prefix.push(step.clone());
    }
    Some(true)
}
fn rule_active(rule: &DomainRule, env: &[(InputPath, DomainValue)]) -> Option<bool> {
    let paths = match &rule.kind {
        DomainRuleKind::Source(s) => vec![s.path.clone()],
        DomainRuleKind::Relation(r) => r.paths(),
    };
    let mut unknown = false;
    for p in paths {
        match path_active(&p, env) {
            Some(false) => return Some(false),
            None => unknown = true,
            _ => {}
        }
    }
    for gate in &rule.gates {
        match gate.evaluate(env) {
            Some(false) => return Some(false),
            None => unknown = true,
            _ => {}
        }
    }
    if unknown {
        None
    } else {
        Some(true)
    }
}
fn flatten(
    schema: &DomainSchema,
    path: InputPath,
    name: String,
    rules: &[DomainRule],
    out: &mut Vec<Variable>,
) {
    let complete=rules.iter().any(|r|matches!(&r.kind,DomainRuleKind::Source(s) if s.path==path && matches!(&s.space,ValueSpace::Selected(_))));
    match schema {
        DomainSchema::Struct(fields) if !complete => {
            for (i, (field, s)) in fields.iter().enumerate() {
                let mut p = path.clone();
                p.push(DomainPathStep::Field(i));
                flatten(
                    s,
                    p,
                    if name.is_empty() {
                        field.to_string()
                    } else {
                        format!("{name}.{field}")
                    },
                    rules,
                    out,
                )
            }
        }
        DomainSchema::Enum(variants) if !complete => {
            out.push(Variable {
                path: path.clone(),
                name: name.clone(),
                schema: schema.clone(),
                extras: vec![],
            });
            for (i, (variant, fields)) in variants.iter().enumerate() {
                for (j, (field, s)) in fields.iter().enumerate() {
                    let mut p = path.clone();
                    p.extend([DomainPathStep::Variant(i), DomainPathStep::Field(j)]);
                    flatten(s, p, format!("{name}::{variant}.{field}"), rules, out)
                }
            }
        }
        DomainSchema::Sequence(each) if !complete => {
            let mut length = path.clone();
            length.push(DomainPathStep::Length);
            out.push(Variable {
                path: length,
                name: format!("{name}.length"),
                schema: usize::schema(),
                extras: vec![],
            });
            let mut p = path;
            p.push(DomainPathStep::Each);
            flatten(each, p, format!("{name}.each"), rules, out)
        }
        _ => out.push(Variable {
            path,
            name,
            schema: schema.clone(),
            extras: vec![],
        }),
    }
}
impl<T: DomainInput> CompiledDomain<T> {
    pub(crate) fn compile(domain: Domain<T>) -> Result<Self, AnalysisError> {
        let schema = T::schema();
        let mut variables = vec![];
        flatten(
            &schema,
            vec![],
            String::new(),
            &domain.rules,
            &mut variables,
        );
        for rule in &domain.rules {
            if let DomainRuleKind::Source(source) = &rule.kind {
                let var = variables
                    .iter()
                    .find(|v| v.path == source.path)
                    .ok_or_else(|| {
                        domain_error(
                            "value source conflicts with a recursive or fixed parent domain",
                        )
                    })?;
                match &source.space {
                    ValueSpace::Ranges(rs) => {
                        if rs.is_empty() {
                            return Err(domain_error(format!("{} has empty ranges", var.name)));
                        }
                        for (a, b) in rs {
                            let width_valid = match (a, b) {
                                (DomainValue::UInt(a), DomainValue::UInt(b)) => {
                                    b.checked_sub(*a).and_then(|n| n.checked_add(1)).is_some()
                                }
                                (DomainValue::Int(a), DomainValue::Int(b)) => {
                                    b.checked_sub(*a).and_then(|n| n.checked_add(1)).is_some()
                                }
                                (DomainValue::Real(a), DomainValue::Real(b)) => (b - a).is_finite(),
                                _ => false,
                            };
                            if !width_valid
                                || !var.schema.accepts(a)
                                || !var.schema.accepts(b)
                                || a.cmp_scalar(b).is_none_or(|c| c == Ordering::Greater)
                            {
                                return Err(domain_error(format!(
                                    "{} has invalid range",
                                    var.name
                                )));
                            }
                        }
                        for pair in rs.windows(2) {
                            if pair[0].1.cmp_scalar(&pair[1].0) != Some(Ordering::Less) {
                                return Err(domain_error(format!(
                                    "{} ranges must be ordered and disjoint",
                                    var.name
                                )));
                            }
                        }
                    }
                    ValueSpace::Selected(vs) | ValueSpace::Enum(vs) => {
                        if vs.is_empty() || vs.iter().any(|v| !var.schema.accepts(v)) {
                            return Err(domain_error(format!(
                                "{} has an empty or invalid value specification",
                                var.name
                            )));
                        }
                    }
                }
            }
        }
        let mut ordered = vec![];
        let mut pending = variables;
        while !pending.is_empty() {
            let position = pending
                .iter()
                .position(|v| {
                    let mut deps = vec![];
                    for (i, step) in v.path.iter().enumerate() {
                        if matches!(step, DomainPathStep::Variant(_)) {
                            deps.push(v.path[..i].to_vec())
                        }
                        if matches!(step, DomainPathStep::Each) {
                            let mut p = v.path[..i].to_vec();
                            p.push(DomainPathStep::Length);
                            deps.push(p)
                        }
                    }
                    for r in &domain.rules {
                        if matches!(&r.kind,DomainRuleKind::Source(s) if s.path==v.path) {
                            for g in &r.gates {
                                deps.extend(g.paths())
                            }
                        }
                    }
                    deps.iter()
                        .all(|d| ordered.iter().any(|v: &Variable| &v.path == d))
                })
                .ok_or_else(|| {
                    domain_error("cyclic or unavailable conditional Domain dependency")
                })?;
            ordered.push(pending.remove(position));
        }
        let mut result = Self {
            rules: domain.rules,
            variables: ordered,
            schema,
            contexts: vec![],
            targets: std::array::from_fn(|_| vec![]),
            marker: PhantomData,
            catalog: DomainCoverageCatalog::default(),
        };
        for i in 0..result.variables.len() {
            let p = result.variables[i].path.clone();
            let mut extras = vec![];
            for r in &result.rules {
                if let DomainRuleKind::Source(s) = &r.kind {
                    if s.path == p {
                        for v in s.space.anchors() {
                            push_value(&mut extras, v)
                        }
                    }
                }
                for g in &r.gates {
                    if g.paths().contains(&p) {
                        for o in [&g.left, &g.right] {
                            if let DomainOperand::Value(v) = o {
                                push_value(&mut extras, v.clone());
                                for increasing in [false, true] {
                                    if let Some(v) =
                                        adjacent(v, &result.variables[i].schema, increasing)
                                    {
                                        push_value(&mut extras, v)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            result.variables[i].extras = extras;
        }
        for _ in 0..result.variables.len().max(1) {
            let old: Vec<_> = result.variables.iter().map(|v| v.extras.clone()).collect();
            for rule in &result.rules {
                if let DomainRuleKind::Relation(r) = &rule.kind {
                    for p in r.paths() {
                        let Some(i) = result.variables.iter().position(|v| v.path == p) else {
                            return Err(domain_error(
                                "relation refers to unavailable scalar field",
                            ));
                        };
                        for o in [&r.left, &r.right] {
                            let candidates = match o {
                                DomainOperand::Value(v) => vec![v.clone()],
                                DomainOperand::Field(q) => result
                                    .variables
                                    .iter()
                                    .position(|v| &v.path == q)
                                    .map(|j| old[j].clone())
                                    .unwrap_or_default(),
                            };
                            for v in candidates {
                                if result.variables[i].schema.accepts(&v) {
                                    push_value(&mut result.variables[i].extras, v.clone());
                                    for increasing in [false, true] {
                                        if let Some(v) =
                                            adjacent(&v, &result.variables[i].schema, increasing)
                                        {
                                            if result.variables[i].schema.accepts(&v) {
                                                push_value(&mut result.variables[i].extras, v)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        result
            .solve(&[], &[])?
            .ok_or_else(|| domain_error("unsatisfiable Domain"))?;
        let mut contexts: Vec<Assignment> = vec![vec![]];
        for var in &result.variables {
            let mut deps = vec![];
            for (i, step) in var.path.iter().enumerate() {
                if matches!(step, DomainPathStep::Variant(_)) {
                    deps.push(var.path[..i].to_vec())
                }
                if matches!(step, DomainPathStep::Each) {
                    let mut p = var.path[..i].to_vec();
                    p.push(DomainPathStep::Length);
                    deps.push(p)
                }
            }
            for r in &result.rules {
                if matches!(&r.kind,DomainRuleKind::Source(s) if s.path==var.path) {
                    for g in &r.gates {
                        deps.extend(g.paths())
                    }
                }
            }
            if matches!(var.schema, DomainSchema::Enum(_)) {
                deps.push(var.path.clone());
            }
            let mut index = 0;
            while index < deps.len() {
                let dep = &deps[index].clone();
                for r in &result.rules {
                    if matches!(&r.kind,DomainRuleKind::Source(s) if &s.path==dep) {
                        for g in &r.gates {
                            for p in g.paths() {
                                if !deps.contains(&p) {
                                    deps.push(p)
                                }
                            }
                        }
                    }
                }
                for (i, s) in dep.iter().enumerate() {
                    if matches!(s, DomainPathStep::Variant(_)) {
                        let p = dep[..i].to_vec();
                        if !deps.contains(&p) {
                            deps.push(p)
                        }
                    }
                }
                index += 1;
            }
            deps.sort();
            deps.dedup();
            deps.sort_by_key(|p| {
                result
                    .variables
                    .iter()
                    .position(|v| &v.path == p)
                    .unwrap_or(usize::MAX)
            });
            let mut group: Vec<Assignment> = vec![vec![]];
            for dep in deps {
                let variable = result
                    .variables
                    .iter()
                    .find(|v| v.path == dep)
                    .ok_or_else(|| domain_error("unknown conditional field"))?;
                let mut choices = vec![];
                for r in &result.rules {
                    if let DomainRuleKind::Source(s) = &r.kind {
                        if s.path == dep {
                            for v in s.space.anchors() {
                                push_value(&mut choices, v)
                            }
                        }
                    }
                }
                for v in &variable.extras {
                    push_value(&mut choices, v.clone());
                }
                if dep.last() == Some(&DomainPathStep::Length) {
                    choices.sort_by(|a, b| a.cmp_scalar(b).unwrap_or(Ordering::Equal));
                    let positive = choices
                        .iter()
                        .find(|v| matches!(v,DomainValue::Int(n) if *n>0))
                        .cloned();
                    choices.retain(|v| matches!(v, DomainValue::Int(0)));
                    if let Some(v) = positive {
                        choices.push(v)
                    }
                }
                let mut next = vec![];
                for context in group {
                    if path_active(&dep, &context) == Some(false) {
                        next.push(context);
                        continue;
                    }
                    for choice in &choices {
                        let mut c = context.clone();
                        c.push((dep.clone(), choice.clone()));
                        next.push(c);
                        if next.len() > CONTEXT_LIMIT {
                            return Err(domain_error(
                                "Domain conditional preflight exceeds bounded solver capacity",
                            ));
                        }
                    }
                }
                group = next;
            }
            for context in group {
                if !contexts.contains(&context) {
                    contexts.push(context);
                    if contexts.len() > CONTEXT_LIMIT {
                        return Err(domain_error(
                            "Domain conditional preflight exceeds bounded solver capacity",
                        ));
                    }
                }
            }
        }
        for context in contexts {
            if result.solve(&context, &[])?.is_some() {
                result.contexts.push(context);
            } else if result.context_is_reachable(&context) {
                return Err(domain_error("unsatisfiable conditional Domain branch"));
            }
        }
        result.build_targets()?;
        result.catalog = result.make_catalog()?;
        Ok(result)
    }
    fn context_is_reachable(&self, c: &Assignment) -> bool {
        c.iter().all(|(p, v)| {
            path_active(p, c) != Some(false)
                && self.rules.iter().any(|r| {
                    matches!(&r.kind,DomainRuleKind::Source(s) if &s.path==p && s.space.contains(v))
                        && r.gates.iter().all(|g| g.evaluate(c) != Some(false))
                })
        })
    }
    fn candidates(
        &self,
        var: &Variable,
        source: &DomainSource,
        negative: bool,
    ) -> Vec<DomainValue> {
        let mut values = vec![];
        if negative {
            for (v, _, _) in source.space.negative(&var.schema) {
                push_value(&mut values, v)
            }
        } else {
            for v in source.space.anchors() {
                let v = normalize(&var.schema, v);
                if var.schema.accepts(&v) && source.space.contains(&v) {
                    push_value(&mut values, v)
                }
            }
        }
        for v in &var.extras {
            let v = normalize(&var.schema, v.clone());
            if var.schema.accepts(&v) && source.space.contains(&v) != negative {
                push_value(&mut values, v)
            }
        }
        values
    }
    fn possible_operand(
        &self,
        operand: &DomainOperand,
        env: &Assignment,
        forced: &[(InputPath, DomainValue)],
        violated: &[usize],
    ) -> Option<Vec<DomainValue>> {
        let path = match operand {
            DomainOperand::Value(v) => return Some(vec![v.clone()]),
            DomainOperand::Field(p) => p,
        };
        if let Some(v) = get(env, path) {
            return Some(vec![v.clone()]);
        }
        let var = self.variables.iter().find(|v| &v.path == path)?;
        let sources: Vec<_> = self
            .rules
            .iter()
            .enumerate()
            .filter_map(|(id, r)| {
                if let DomainRuleKind::Source(s) = &r.kind {
                    (&s.path == path && rule_active(r, env) != Some(false)).then_some((id, s))
                } else {
                    None
                }
            })
            .collect();
        if sources.is_empty() {
            return None;
        }
        let mut all = vec![];
        for (id, source) in sources {
            let negative = violated.contains(&id);
            if let Some(v) = get(forced, path) {
                if source.space.contains(v) != negative && var.schema.accepts(v) {
                    push_value(&mut all, v.clone())
                }
            } else {
                for v in self.candidates(var, source, negative) {
                    push_value(&mut all, v)
                }
            }
        }
        Some(all)
    }
    fn solve(
        &self,
        forced: &[(InputPath, DomainValue)],
        violated: &[usize],
    ) -> Result<Option<Assignment>, AnalysisError> {
        let mut forced = forced.to_vec();
        let mut required: Vec<_> = forced.iter().map(|(p, _)| p.clone()).collect();
        for id in violated {
            match &self.rules[*id].kind {
                DomainRuleKind::Source(s) => required.push(s.path.clone()),
                DomainRuleKind::Relation(r) => required.extend(r.paths()),
            }
        }
        for path in required {
            for (i, step) in path.iter().enumerate() {
                if let DomainPathStep::Variant(tag) = step {
                    let parent = path[..i].to_vec();
                    let value = DomainValue::Enum(*tag, vec![]);
                    if get(&forced, &parent)
                        .is_some_and(|v| v.cmp_scalar(&value) != Some(Ordering::Equal))
                    {
                        return Ok(None);
                    }
                    if get(&forced, &parent).is_none() {
                        forced.push((parent, value))
                    }
                }
            }
        }
        fn dfs<T: DomainInput>(
            d: &CompiledDomain<T>,
            i: usize,
            env: &mut Assignment,
            forced: &[(InputPath, DomainValue)],
            violated: &[usize],
            steps: &mut usize,
        ) -> Result<Option<Assignment>, AnalysisError> {
            *steps += 1;
            if *steps > SEARCH_LIMIT {
                return Err(domain_error(
                    "Domain solver exhausted its bounded search; coverage cannot be certified",
                ));
            }
            let mut known = env.clone();
            for (p, v) in forced {
                if get(&known, p).is_none() {
                    known.push((p.clone(), v.clone()))
                }
            }
            if violated
                .iter()
                .any(|id| rule_active(&d.rules[*id], &known) == Some(false))
                || forced
                    .iter()
                    .any(|(p, _)| path_active(p, &known) == Some(false))
            {
                return Ok(None);
            }
            for (id, rule) in d.rules.iter().enumerate() {
                if let DomainRuleKind::Relation(r) = &rule.kind {
                    if rule_active(rule, &known) == Some(true) || violated.contains(&id) {
                        let desired = !violated.contains(&id);
                        if let Some(valid) = r.evaluate(&known) {
                            if valid != desired {
                                return Ok(None);
                            }
                        } else if let (Some(left), Some(right)) = (
                            d.possible_operand(&r.left, &known, forced, violated),
                            d.possible_operand(&r.right, &known, forced, violated),
                        ) {
                            if !left.iter().any(|a| {
                                right.iter().any(|b| {
                                    DomainRelation {
                                        left: DomainOperand::Value(a.clone()),
                                        right: DomainOperand::Value(b.clone()),
                                        kind: r.kind,
                                    }
                                    .evaluate(&[])
                                        == Some(desired)
                                })
                            }) {
                                return Ok(None);
                            }
                        }
                    }
                }
            }
            if i == d.variables.len() {
                if forced.iter().any(|(p, v)| get(env, p) != Some(v)) {
                    return Ok(None);
                }
                for id in violated {
                    let Some(rule) = d.rules.get(*id) else {
                        return Ok(None);
                    };
                    if rule_active(rule, env) != Some(true) {
                        return Ok(None);
                    }
                }
                return Ok(Some(env.clone()));
            }
            let var = &d.variables[i];
            if path_active(&var.path, env) == Some(false) {
                return dfs(d, i + 1, env, forced, violated, steps);
            }
            let active: Vec<_> = d
                .rules
                .iter()
                .enumerate()
                .filter_map(|(id, r)| {
                    if let DomainRuleKind::Source(s) = &r.kind {
                        (s.path == var.path && rule_active(r, env) == Some(true)).then_some((id, s))
                    } else {
                        None
                    }
                })
                .collect();
            if active.is_empty() {
                return Err(domain_error(format!(
                    "field {} has no value specification",
                    var.name
                )));
            }
            if active.len() != 1 {
                return Err(domain_error(format!(
                    "field {} has duplicate constructive value specifications",
                    var.name
                )));
            }
            let (id, source) = active[0];
            let negative = violated.contains(&id);
            let choices = if let Some(v) = get(forced, &var.path) {
                if !var.schema.accepts(v) || source.space.contains(v) == negative {
                    return Ok(None);
                }
                vec![v.clone()]
            } else {
                d.candidates(var, source, negative)
            };
            for value in choices {
                env.push((var.path.clone(), value));
                if let Some(found) = dfs(d, i + 1, env, forced, violated, steps)? {
                    return Ok(Some(found));
                }
                env.pop();
            }
            Ok(None)
        }
        dfs(self, 0, &mut vec![], &forced, violated, &mut 0).map_err(|e| {
            domain_error(format!(
                "{}; forced: {}; violated: {:?}",
                e.message,
                bounded_debug(&forced, 512),
                violated
            ))
        })
    }
    fn add_target(&mut self, category: usize, target: DomainTarget) -> Result<(), AnalysisError> {
        if self.targets[category].iter().any(|t| {
            t.forced == target.forced && t.violated == target.violated && t.focus == target.focus
        }) {
            return Ok(());
        }
        if self.solve(&target.forced, &target.violated)?.is_some() {
            self.targets[category].push(target)
        }
        Ok(())
    }
    fn relation_targets(
        &self,
        id: usize,
        relation: &DomainRelation,
        context: &Assignment,
    ) -> Vec<DomainTarget> {
        let left = self
            .possible_operand(&relation.left, context, context, &[])
            .unwrap_or_default();
        let right = self
            .possible_operand(&relation.right, context, context, &[])
            .unwrap_or_default();
        let pairs = relation.invalid_pairs(&left, &right);
        if pairs.is_empty() {
            return vec![];
        }
        let mut result = vec![];
        for (depth, index) in [0, pairs.len() / 2, pairs.len() - 1]
            .into_iter()
            .enumerate()
        {
            let (a, b) = &pairs[index];
            let mut forced = context.clone();
            let mut compatible = true;
            for (operand, value) in [(&relation.left, a), (&relation.right, b)] {
                if let DomainOperand::Field(path) = operand {
                    if get(&forced, path).is_some_and(|v| v != value) {
                        compatible = false;
                        break;
                    }
                    if get(&forced, path).is_none() {
                        forced.push((path.clone(), value.clone()));
                    }
                }
            }
            if compatible {
                result.push(DomainTarget {
                    forced,
                    violated: vec![id],
                    focus: vec![],
                    depth,
                });
            }
        }
        result
    }
    fn build_targets(&mut self) -> Result<(), AnalysisError> {
        let contexts = self.contexts.clone();
        for context in &contexts {
            for var in self.variables.clone() {
                for (id, rule) in self.rules.clone().iter().enumerate() {
                    if let DomainRuleKind::Source(source) = &rule.kind {
                        if source.path != var.path {
                            continue;
                        }
                        for (depth, value) in source.space.anchors().into_iter().enumerate() {
                            let mut forced = context.clone();
                            if get(&forced, &var.path).is_some_and(|v| v != &value) {
                                continue;
                            }
                            if get(&forced, &var.path).is_none() {
                                forced.push((var.path.clone(), normalize(&var.schema, value)))
                            }
                            self.add_target(
                                0,
                                DomainTarget {
                                    forced,
                                    violated: vec![],
                                    focus: vec![var.path.clone()],
                                    depth: if matches!(var.schema, DomainSchema::Enum(_)) {
                                        0
                                    } else {
                                        depth
                                    },
                                },
                            )?
                        }
                        for (depth, (value, _, _distance)) in
                            source.space.negative(&var.schema).into_iter().enumerate()
                        {
                            let mut forced = context.clone();
                            forced.retain(|(p, _)| p != &var.path);
                            forced.push((var.path.clone(), value));
                            self.add_target(
                                1,
                                DomainTarget {
                                    forced,
                                    violated: vec![id],
                                    focus: vec![var.path.clone()],
                                    depth,
                                },
                            )?
                        }
                        // Preserve breadth-first ordering across distinct negative constraints.
                    }
                }
            }
            for (id, r) in self.rules.clone().iter().enumerate() {
                if let DomainRuleKind::Relation(relation) = &r.kind {
                    for target in self.relation_targets(id, relation, context) {
                        self.add_target(1, target)?;
                    }
                    self.add_target(
                        1,
                        DomainTarget {
                            forced: context.clone(),
                            violated: vec![id],
                            focus: vec![],
                            depth: 0,
                        },
                    )?
                }
            }
        }
        let enum_vars: Vec<_> = self
            .variables
            .iter()
            .filter(|v| matches!(v.schema, DomainSchema::Enum(_)))
            .cloned()
            .collect();
        for (i, a) in enum_vars.iter().enumerate() {
            for b in &enum_vars[i + 1..] {
                let av: Vec<_> = self
                    .rules
                    .iter()
                    .filter_map(|r| {
                        if let DomainRuleKind::Source(s) = &r.kind {
                            (s.path == a.path).then(|| s.space.anchors())
                        } else {
                            None
                        }
                    })
                    .flatten()
                    .collect();
                let bv: Vec<_> = self
                    .rules
                    .iter()
                    .filter_map(|r| {
                        if let DomainRuleKind::Source(s) = &r.kind {
                            (s.path == b.path).then(|| s.space.anchors())
                        } else {
                            None
                        }
                    })
                    .flatten()
                    .collect();
                for x in &av {
                    for y in &bv {
                        for c in &contexts {
                            let mut forced = c.clone();
                            if get(&forced, &a.path).is_some_and(|v| v != x)
                                || get(&forced, &b.path).is_some_and(|v| v != y)
                            {
                                continue;
                            }
                            if get(&forced, &a.path).is_none() {
                                forced.push((a.path.clone(), x.clone()))
                            }
                            if get(&forced, &b.path).is_none() {
                                forced.push((b.path.clone(), y.clone()))
                            }
                            self.add_target(
                                0,
                                DomainTarget {
                                    forced,
                                    violated: vec![],
                                    focus: vec![a.path.clone(), b.path.clone()],
                                    depth: 1,
                                },
                            )?
                        }
                    }
                }
            }
        }
        let negative_ids: Vec<_> = self
            .rules
            .iter()
            .enumerate()
            .filter_map(|(id, rule)| match &rule.kind {
                DomainRuleKind::Relation(_) => Some(id),
                DomainRuleKind::Source(source) => self
                    .variables
                    .iter()
                    .find(|v| v.path == source.path)
                    .and_then(|v| (!source.space.negative(&v.schema).is_empty()).then_some(id)),
            })
            .collect();
        for (i, a) in negative_ids.iter().enumerate() {
            for b in &negative_ids[i + 1..] {
                for c in &contexts {
                    self.add_target(
                        2,
                        DomainTarget {
                            forced: c.clone(),
                            violated: vec![*a, *b],
                            focus: vec![],
                            depth: 0,
                        },
                    )?;
                    if self.targets[2]
                        .last()
                        .is_some_and(|t| t.violated == vec![*a, *b])
                    {
                        break;
                    }
                }
            }
        }
        if self.targets[0].is_empty() {
            self.add_target(
                0,
                DomainTarget {
                    forced: vec![],
                    violated: vec![],
                    focus: vec![],
                    depth: 0,
                },
            )?;
        }
        for targets in &mut self.targets {
            targets.sort_by_key(|t| t.depth);
        }
        Ok(())
    }
    pub fn classify(&self, input: &T) -> Result<CaseDomainCoordinate, AnalysisError> {
        self.coordinate(&input.to_domain_value())
    }

    fn materialize(&self, env: &Assignment) -> Result<T, AnalysisError> {
        fn build(
            schema: &DomainSchema,
            path: &InputPath,
            env: &Assignment,
        ) -> Result<DomainValue, AnalysisError> {
            if let Some(value) = get(env, path) {
                if !matches!(schema, DomainSchema::Enum(_))
                    || matches!(value,DomainValue::Enum(_,v) if !v.is_empty())
                {
                    return Ok(value.clone());
                }
            }
            let value = match schema {
                DomainSchema::Struct(fields) => {
                    let mut values = vec![];
                    for (i, (_, s)) in fields.iter().enumerate() {
                        let mut p = path.clone();
                        p.push(DomainPathStep::Field(i));
                        values.push(build(s, &p, env)?)
                    }
                    DomainValue::Struct(values)
                }
                DomainSchema::Enum(variants) => {
                    let Some(DomainValue::Enum(tag, _)) = get(env, path) else {
                        return Err(domain_error("missing enum selection"));
                    };
                    let mut values = vec![];
                    for (i, (_, s)) in variants[*tag].1.iter().enumerate() {
                        let mut p = path.clone();
                        p.extend([DomainPathStep::Variant(*tag), DomainPathStep::Field(i)]);
                        values.push(build(s, &p, env)?)
                    }
                    DomainValue::Enum(*tag, values)
                }
                DomainSchema::Sequence(s) => {
                    let mut length = path.clone();
                    length.push(DomainPathStep::Length);
                    let Some(DomainValue::Int(n)) = get(env, &length) else {
                        return Err(domain_error("missing collection length"));
                    };
                    let n = usize::try_from(*n)
                        .map_err(|_| domain_error("invalid collection length"))?;
                    if n > COLLECTION_LIMIT {
                        return Err(domain_error(
                            "collection materialization exceeds safety bound",
                        ));
                    }
                    let mut each = path.clone();
                    each.push(DomainPathStep::Each);
                    DomainValue::Sequence(if n == 0 {
                        vec![]
                    } else {
                        vec![build(s, &each, env)?; n]
                    })
                }
                _ => return Err(domain_error("missing scalar value")),
            };
            Ok(value)
        }
        T::from_domain_value(build(&self.schema, &vec![], env)?)
    }
    pub(crate) fn target_counts(&self) -> [usize; 3] {
        std::array::from_fn(|i| self.targets[i].len())
    }
    pub(crate) fn coverage_catalog(&self) -> &DomainCoverageCatalog {
        &self.catalog
    }
    fn make_catalog(&self) -> Result<DomainCoverageCatalog, AnalysisError> {
        let mut anchors = BTreeSet::new();
        let mut enums = BTreeSet::new();
        let mut pairs = BTreeSet::new();
        let mut negative = BTreeSet::new();
        for target in self.targets.iter().flatten() {
            let Some(env) = self.solve(&target.forced, &target.violated)? else {
                return Err(domain_error("validated coverage target lost its witness"));
            };
            if env.iter().any(|(path, v)| {
                path.last() == Some(&DomainPathStep::Length)
                    && matches!(v,DomainValue::Int(n) if *n>COLLECTION_LIMIT as i128)
            }) {
                return Err(domain_error(
                    "collection Domain coverage exceeds materialization safety bound",
                ));
            }
            let coordinate = self.coordinate_of(&env)?;
            if coordinate.violated != target.violated {
                return Err(domain_error(
                    "Domain witness violates unplanned constraints",
                ));
            }
            let mut enum_slots = vec![];
            for entry in coordinate.entries {
                if target.violated.contains(&entry.sector.constraint) {
                    negative.insert(entry.sector);
                } else if target.violated.is_empty() {
                    if matches!(entry.slot, CaseDomainCoordinateSlot::Anchor(_)) {
                        anchors.insert((entry.sector.clone(), entry.slot));
                    }
                    if self
                        .variables
                        .iter()
                        .any(|v| v.path == entry.path && matches!(v.schema, DomainSchema::Enum(_)))
                    {
                        enums.insert(entry.sector.clone());
                        enum_slots.push(entry.sector);
                    }
                }
            }
            for (i, a) in enum_slots.iter().enumerate() {
                for b in &enum_slots[i + 1..] {
                    pairs.insert((a.clone(), b.clone()));
                }
            }
        }
        Ok(DomainCoverageCatalog {
            positive_anchors: anchors,
            enum_values: enums,
            pairwise_combinations: pairs,
            negative_sectors: negative,
        })
    }
    pub(crate) fn next(
        &self,
        category: usize,
        cursor: &mut usize,
        seed: u64,
        occupied: &BTreeSet<Vec<(CaseDomainCoordinateSector, CaseDomainCoordinateSlot)>>,
    ) -> Result<Option<(T, CaseDomainCoordinate)>, AnalysisError> {
        let targets = &self.targets[category];
        let free_limit = if category == 0 {
            self.free_capacity().saturating_mul(2)
        } else {
            0
        };
        while *cursor < targets.len() + free_limit {
            let index = *cursor;
            *cursor += 1;
            let target = if let Some(t) = targets.get(index) {
                t.clone()
            } else {
                let Some(t) = self.free_target(index - targets.len(), seed)? else {
                    continue;
                };
                t
            };
            if let Some(env) = self.solve(&target.forced, &target.violated)? {
                let coordinate = self.coordinate_of(&env)?;
                if coordinate.violated != target.violated {
                    return Err(domain_error(
                        "materialized coordinate does not match requested constraint violations",
                    ));
                }
                if !occupied.contains(&coordinate_key(&coordinate))
                    && !target.is_covered(&coordinate, occupied)
                {
                    let input = self.materialize(&env)?;
                    let actual = self.classify(&input)?;
                    if coordinate_key(&actual) != coordinate_key(&coordinate)
                        || actual.violated != coordinate.violated
                    {
                        return Err(domain_error(
                            "materialized input differs from its planned coordinate",
                        ));
                    }
                    return Ok(Some((input, coordinate)));
                }
            }
        }
        Ok(None)
    }
    fn free_capacity(&self) -> usize {
        let ranges: usize = self
            .rules
            .iter()
            .map(|r| match &r.kind {
                DomainRuleKind::Source(DomainSource {
                    space: ValueSpace::Ranges(rs),
                    ..
                }) => rs.len() * RANGE_STRATA as usize,
                _ => 0,
            })
            .sum();
        let enums: Vec<_> = self
            .variables
            .iter()
            .filter(|v| matches!(v.schema, DomainSchema::Enum(_)))
            .collect();
        let combinations = if enums.len() < 3 {
            0
        } else {
            enums
                .iter()
                .fold(1usize, |n, v| {
                    let choices = self
                        .rules
                        .iter()
                        .filter_map(|r| match &r.kind {
                            DomainRuleKind::Source(s) if s.path == v.path => {
                                Some(s.space.anchors().len())
                            }
                            _ => None,
                        })
                        .sum::<usize>();
                    n.saturating_mul(choices)
                })
                .min(CONTEXT_LIMIT)
        };
        ranges.max(combinations)
    }
    fn free_target(&self, index: usize, seed: u64) -> Result<Option<DomainTarget>, AnalysisError> {
        let level = index / 2;
        if index % 2 == 1 {
            let vars: Vec<_> = self
                .variables
                .iter()
                .filter(|v| matches!(v.schema, DomainSchema::Enum(_)))
                .collect();
            if vars.len() < 3 {
                return Ok(None);
            }
            let mut remainder = level;
            let mut forced = vec![];
            let mut focus = vec![];
            for var in vars {
                let mut values = vec![];
                for r in &self.rules {
                    if let DomainRuleKind::Source(s) = &r.kind {
                        if s.path == var.path {
                            for v in s.space.anchors() {
                                push_value(&mut values, v)
                            }
                        }
                    }
                }
                if values.is_empty() {
                    return Ok(None);
                }
                let choice = remainder % values.len();
                remainder /= values.len();
                forced.push((var.path.clone(), values[choice].clone()));
                focus.push(var.path.clone());
            }
            if remainder != 0 {
                return Ok(None);
            }
            return Ok(Some(DomainTarget {
                forced,
                violated: vec![],
                focus,
                depth: index,
            }));
        }
        let ranges: Vec<_> = self
            .rules
            .iter()
            .enumerate()
            .flat_map(|(id, r)| match &r.kind {
                DomainRuleKind::Source(s) => match &s.space {
                    ValueSpace::Ranges(rs) => rs
                        .iter()
                        .enumerate()
                        .map(|(branch, bounds)| (id, s, branch, bounds))
                        .collect(),
                    _ => vec![],
                },
                _ => vec![],
            })
            .collect();
        if ranges.is_empty() {
            return Ok(None);
        }
        let (id, source, branch, (a, b)) = ranges[level % ranges.len()];
        let stratum_rank = level / ranges.len();
        let tercile_index = stratum_rank % 3;
        let stratum_in_tercile = stratum_rank / 3;
        if stratum_in_tercile >= TERCILE_STRATA as usize {
            return Ok(None);
        }
        let tercile = match tercile_index {
            0 => DomainTercile::Low,
            1 => DomainTercile::Mid,
            _ => DomainTercile::High,
        };
        let bin = tercile_index * TERCILE_STRATA as usize + stratum_in_tercile;
        let sector = CaseDomainCoordinateSector {
            constraint: id,
            region: DomainRegion::Range { branch, tercile },
        };
        let slot = CaseDomainCoordinateSlot::Stratum(bin as u64);
        let h = local_seed(seed, &(sector, slot, &source.path));
        let value = match (a, b) {
            (DomainValue::UInt(a), DomainValue::UInt(b)) => {
                let len = b - a + 1;
                let boundary = |n: u128| stratum_boundary(len, n);
                let start = boundary(bin as u128);
                let end = boundary(bin as u128 + 1);
                if start >= end {
                    return Ok(None);
                }
                let lo = a + start;
                let hi = a + (end - 1);
                DomainValue::UInt(lo + (h as u128) % (hi - lo + 1))
            }
            (DomainValue::Int(a), DomainValue::Int(b)) => {
                let len = b
                    .checked_sub(*a)
                    .and_then(|n| n.checked_add(1))
                    .ok_or_else(|| domain_error("range width exceeds constructive arithmetic"))?;
                let boundary = |n: i128| stratum_boundary(len as u128, n as u128) as i128;
                let start = boundary(bin as i128);
                let end = boundary(bin as i128 + 1);
                if start >= end {
                    return Ok(None);
                }
                let lo = a + start;
                let hi = a + (end - 1);
                DomainValue::Int(lo + (h as i128) % (hi - lo + 1))
            }
            _ => a.interpolate(
                b,
                bin as u64 * STRATIFIED_RESOLUTION + h % STRATIFIED_RESOLUTION,
                RANGE_STRATA * STRATIFIED_RESOLUTION,
            ),
        };
        let var = self
            .variables
            .iter()
            .find(|v| v.path == source.path)
            .unwrap();
        let value = normalize(&var.schema, value);
        for context in &self.contexts {
            let Some(witness) = self.solve(context, &[])? else {
                continue;
            };
            if rule_active(&self.rules[id], &witness) != Some(true) {
                continue;
            }
            if get(context, &var.path).is_some_and(|v| v != &value) {
                continue;
            }
            let mut forced = context.clone();
            if get(&forced, &var.path).is_none() {
                forced.push((var.path.clone(), value.clone()));
            }
            return Ok(Some(DomainTarget {
                forced,
                violated: vec![],
                focus: vec![var.path.clone()],
                depth: index,
            }));
        }
        Ok(None)
    }
}

pub(crate) fn coordinate_key(
    c: &CaseDomainCoordinate,
) -> Vec<(CaseDomainCoordinateSector, CaseDomainCoordinateSlot)> {
    c.entries
        .iter()
        .map(|e| (e.sector.clone(), e.slot.clone()))
        .collect()
}
pub(crate) fn local_seed(seed: u64, value: &impl std::hash::Hash) -> u64 {
    struct Stable(u64);
    impl std::hash::Hasher for Stable {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for b in bytes {
                self.0 ^= *b as u64;
                self.0 = self.0.wrapping_mul(0x100000001b3)
            }
        }
    }
    let mut h = Stable(seed ^ 0xcbf29ce484222325);
    std::hash::Hash::hash(value, &mut h);
    std::hash::Hasher::finish(&h)
}
