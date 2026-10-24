use super::*;
type RowContext = Vec<(InputPath, usize)>;
#[derive(Clone)]
struct DomainRow {
    context: RowContext,
    value: DomainValue,
}
trait InputProjection {
    fn rows(&self, path: &InputPath) -> Vec<DomainRow>;
}
impl InputProjection for DomainValue {
    fn rows(&self, path: &InputPath) -> Vec<DomainRow> {
        fn visit(
            value: &DomainValue,
            path: &[DomainPathStep],
            prefix: &mut InputPath,
            context: &mut RowContext,
            out: &mut Vec<DomainRow>,
        ) {
            let Some((step, rest)) = path.split_first() else {
                out.push(DomainRow {
                    context: context.clone(),
                    value: value.clone(),
                });
                return;
            };
            match (step, value) {
                (DomainPathStep::Field(i), DomainValue::Struct(v)) => {
                    if let Some(v) = v.get(*i) {
                        prefix.push(step.clone());
                        visit(v, rest, prefix, context, out);
                        prefix.pop();
                    }
                }
                (DomainPathStep::Variant(i), DomainValue::Enum(tag, v)) if i == tag => {
                    prefix.push(step.clone());
                    visit(&DomainValue::Struct(v.clone()), rest, prefix, context, out);
                    prefix.pop();
                }
                (DomainPathStep::Length, DomainValue::Sequence(v)) => out.push(DomainRow {
                    context: context.clone(),
                    value: DomainValue::Int(v.len() as i128),
                }),
                (DomainPathStep::Each, DomainValue::Sequence(v)) => {
                    prefix.push(step.clone());
                    for (i, v) in v.iter().enumerate() {
                        context.push((prefix.clone(), i));
                        visit(v, rest, prefix, context, out);
                        context.pop();
                    }
                    prefix.pop();
                }
                _ => {}
            }
        }
        let mut out = vec![];
        visit(self, path, &mut vec![], &mut vec![], &mut out);
        out
    }
}
fn compatible(a: &RowContext, b: &RowContext) -> bool {
    a.iter()
        .all(|(p, i)| b.iter().all(|(q, j)| p != q || i == j))
}
fn operand_value(
    operand: &DomainOperand,
    context: &RowContext,
    input: &impl InputProjection,
) -> Result<Option<DomainValue>, AnalysisError> {
    match operand {
        DomainOperand::Value(v) => Ok(Some(v.clone())),
        DomainOperand::Field(path) => {
            let mut rows = input
                .rows(path)
                .into_iter()
                .filter(|r| compatible(&r.context, context));
            let Some(row) = rows.next() else {
                return Ok(None);
            };
            if rows.next().is_some() {
                return Err(domain_error(
                    "ambiguous collection relation or conditional operands",
                ));
            }
            Ok(Some(row.value))
        }
    }
}
fn evaluate(
    relation: &DomainRelation,
    context: &RowContext,
    input: &impl InputProjection,
) -> Result<Option<bool>, AnalysisError> {
    let Some(a) = operand_value(&relation.left, context, input)? else {
        return Ok(None);
    };
    let Some(b) = operand_value(&relation.right, context, input)? else {
        return Ok(None);
    };
    DomainRelation {
        left: DomainOperand::Value(a),
        right: DomainOperand::Value(b),
        kind: relation.kind,
    }
    .evaluate(&[])
    .map(Some)
    .ok_or_else(|| domain_error("incomparable relation operands"))
}
fn gates_match(
    rule: &DomainRule,
    context: &RowContext,
    input: &impl InputProjection,
) -> Result<bool, AnalysisError> {
    for gate in &rule.gates {
        if evaluate(gate, context, input)? != Some(true) {
            return Ok(false);
        }
    }
    Ok(true)
}
impl ValueSpace {
    pub(super) fn slot(
        &self,
        value: &DomainValue,
        schema: &DomainSchema,
    ) -> CaseDomainCoordinateSlot {
        if let (Self::Enum(_), DomainValue::Enum(tag, _)) = (self, value) {
            return if self.contains(value) {
                CaseDomainCoordinateSlot::EnumVariant(*tag)
            } else {
                CaseDomainCoordinateSlot::ExcludedVariant(*tag)
            };
        }
        let mut anchors = vec![];
        for v in self.anchors() {
            push_value(&mut anchors, normalize(schema, v))
        }
        if let Some(i) = anchors
            .iter()
            .position(|v| v == value || v.cmp_scalar(value) == Some(Ordering::Equal))
        {
            return CaseDomainCoordinateSlot::Anchor(i);
        }
        if let Some((_, _, distance)) = self
            .negative(schema)
            .into_iter()
            .find(|(v, _, _)| v == value)
        {
            return CaseDomainCoordinateSlot::Negative(distance);
        }
        if let ValueSpace::Ranges(rs) = self {
            if let DomainRegion::Range { branch, tercile } = self.region(value) {
                let (a, b) = &rs[branch];
                return CaseDomainCoordinateSlot::Stratum(range_stratum_bin(a, b, tercile, value));
            }
        }
        CaseDomainCoordinateSlot::Real(value.key())
    }
}
impl<T: DomainInput> CompiledDomain<T> {
    pub(super) fn coordinate(
        &self,
        input: &DomainValue,
    ) -> Result<CaseDomainCoordinate, AnalysisError> {
        self.classify_projection(input)
    }
    fn classify_projection(
        &self,
        input: &impl InputProjection,
    ) -> Result<CaseDomainCoordinate, AnalysisError> {
        let mut entries = vec![];
        let mut violated = BTreeSet::new();
        for var in &self.variables {
            for row in input.rows(&var.path) {
                let mut active = vec![];
                for (id, rule) in self.rules.iter().enumerate() {
                    if let DomainRuleKind::Source(source) = &rule.kind {
                        if source.path == var.path && gates_match(rule, &row.context, input)? {
                            active.push((id, source))
                        }
                    }
                }
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
                if !source.space.contains(&row.value) {
                    violated.insert(id);
                }
                let sector = CaseDomainCoordinateSector {
                    constraint: id,
                    region: source.space.region(&row.value),
                };
                let slot = source.space.slot(&row.value, &var.schema);
                if entries.iter().any(|e: &DomainCoordinateEntry| {
                    e.path == var.path && e.sector == sector && e.slot == slot
                }) {
                    continue;
                }
                let display = if let (DomainSchema::Enum(vs), DomainValue::Enum(tag, _)) =
                    (&var.schema, &row.value)
                {
                    vs.get(*tag)
                        .map(|(n, _)| n.to_string())
                        .unwrap_or_else(|| "invalid variant".into())
                } else {
                    bounded_debug(&row.value, 96)
                };
                entries.push(DomainCoordinateEntry {
                    path: var.path.clone(),
                    sector,
                    slot,
                    name: var.name.clone(),
                    value: display,
                });
            }
        }
        for (id, rule) in self.rules.iter().enumerate() {
            if let DomainRuleKind::Relation(relation) = &rule.kind {
                let mut contexts = vec![];
                for p in relation.paths() {
                    for row in input.rows(&p) {
                        if !contexts.contains(&row.context) {
                            contexts.push(row.context)
                        }
                    }
                }
                if contexts.is_empty() {
                    contexts.push(vec![])
                }
                let all_contexts = contexts.clone();
                contexts.retain(|c| {
                    !all_contexts
                        .iter()
                        .any(|other| other.len() > c.len() && c.iter().all(|v| other.contains(v)))
                });
                for context in contexts {
                    if !gates_match(rule, &context, input)? {
                        continue;
                    }
                    if evaluate(relation, &context, input)? == Some(false) {
                        violated.insert(id);
                        if !entries.iter().any(|e| e.sector.constraint == id) {
                            entries.push(DomainCoordinateEntry {
                                path: vec![],
                                sector: CaseDomainCoordinateSector {
                                    constraint: id,
                                    region: DomainRegion::Relation,
                                },
                                slot: CaseDomainCoordinateSlot::Negative(
                                    self.relation_distance(relation, &context, input)?,
                                ),
                                name: self.relation_name(relation),
                                value: "violated".into(),
                            });
                        }
                    }
                }
            }
        }
        if violated.len() > 1 {
            for entry in &mut entries {
                if violated.contains(&entry.sector.constraint) {
                    entry.slot = CaseDomainCoordinateSlot::Combination;
                }
            }
        }
        Ok(CaseDomainCoordinate {
            entries,
            violated: violated.into_iter().collect(),
        })
    }
    fn relation_distance(
        &self,
        relation: &DomainRelation,
        context: &RowContext,
        input: &impl InputProjection,
    ) -> Result<DomainNegativeDistance, AnalysisError> {
        let left = self
            .possible_operand(&relation.left, &vec![], &[], &[])
            .unwrap_or_default();
        let right = self
            .possible_operand(&relation.right, &vec![], &[], &[])
            .unwrap_or_default();
        let a = operand_value(&relation.left, context, input)?
            .ok_or_else(|| domain_error("missing relation operand"))?;
        let b = operand_value(&relation.right, context, input)?
            .ok_or_else(|| domain_error("missing relation operand"))?;
        Ok(relation.negative_distance(&a, &b, &left, &right))
    }
    fn relation_name(&self, r: &DomainRelation) -> String {
        let operand = |o: &DomainOperand| match o {
            DomainOperand::Field(p) => self
                .variables
                .iter()
                .find(|v| &v.path == p)
                .map(|v| v.name.clone())
                .unwrap_or_else(|| "unavailable".into()),
            DomainOperand::Value(v) => bounded_debug(v, 64),
        };
        let op = match r.kind {
            RelationKind::LessThan => "<",
            RelationKind::LessEqual => "<=",
            RelationKind::GreaterThan => ">",
            RelationKind::GreaterEqual => ">=",
            RelationKind::Equal => "==",
            RelationKind::NotEqual => "!=",
        };
        format!("{} {op} {}", operand(&r.left), operand(&r.right))
    }
}
