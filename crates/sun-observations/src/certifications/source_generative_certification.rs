use super::*;
use crate::certifications::test_support::{assert_pairwise_certified, KeyScenario, Mismatches};
use std::{cell::Cell, fmt};

struct Provenance;
impl SourceProvenance for Provenance {
    fn describe(&self, index: usize, writer: &mut dyn fmt::Write) -> fmt::Result {
        write!(writer, "row:{index}")
    }
}
fn execute(s: &KeyScenario) -> Result<Vec<String>, ()> {
    let (keys, start, end) = s.materialize();
    let rows: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| (*k, (i as f64 - 3.) * 0.25))
        .collect();
    let source = collection("producer", &rows).with_provenance(&Provenance);
    let mut m = Mismatches::default();
    let unique = source.unique(|r| r.0);
    let contiguous = source.contiguous(|r| r.0, start, end);
    m.equal("unique source", &unique.source, &"producer".to_string());
    m.equal(
        "unique owner",
        &unique.observed.unwrap(),
        &check_unique(&rows, |r| r.0),
    );
    m.equal(
        "contiguous source",
        &contiguous.source,
        &"producer".to_string(),
    );
    m.equal(
        "contiguous owner",
        &contiguous.observed.unwrap(),
        &check_contiguous(&rows, |r| r.0, start, end),
    );
    let calls = Cell::new(0);
    let numeric = source.numeric(|r| {
        calls.set(calls.get() + 1);
        r.1
    });
    let direct = NumericSeries::new(rows.iter().map(|r| r.1));
    let ordinary = numeric.summary();
    let robust = numeric.robust_summary();
    m.equal("numeric source", &ordinary.source, &"producer".to_string());
    m.equal("robust source", &robust.source, &"producer".to_string());
    m.equal(
        "summary owner",
        &ordinary.observed.map_err(|e| e.message),
        &direct.summary().map_err(|e| e.to_string()),
    );
    m.equal(
        "robust owner",
        &robust.observed.map_err(|e| e.message),
        &direct.robust_summary().map_err(|e| e.to_string()),
    );
    m.equal("projection reuse", &calls.get(), &rows.len());
    for entry in source.iter() {
        m.equal("entry", &entry.value.0, &keys[entry.index]);
        let mut description = String::new();
        entry
            .provenance
            .unwrap()
            .describe(entry.index, &mut description)
            .unwrap();
        m.equal("provenance", &description, &format!("row:{}", entry.index));
    }
    let other = source.clone();
    let compared = source.compare_with(&other).on(|r| r.0, |r| r.0);
    m.equal(
        "left source",
        &compared.comparison().left_source(),
        &Some("producer"),
    );
    m.equal(
        "right source",
        &compared.comparison().right_source(),
        &Some("producer"),
    );
    for (_, left, right) in compared.matched_pairs() {
        m.equal("matched indices", &left.index, &right.index);
        m.require(
            "comparison provenance",
            left.provenance.is_some() && right.provenance.is_some(),
        );
    }
    let prepared = compared.numeric(|r| r.1, |r| r.1 * 2. + 1.);
    if compared.comparison().is_complete() {
        let pairs: Vec<_> = compared
            .matched_pairs()
            .map(|(_, a, b)| (a.value.1, b.value.1 * 2. + 1.))
            .collect();
        m.equal(
            "relation owner",
            &prepared.relation().observed.map_err(|e| e.message),
            &numeric_relation(pairs.iter().copied()).map_err(|e| e.to_string()),
        );
    } else {
        m.require(
            "incomplete correspondence fails closed",
            prepared.relation().observed.is_err(),
        );
    }
    Ok(m.finish())
}
#[test]
fn generated_prepared_sources_are_faithful_named_provenance_preserving_owner_views() {
    let mut c = certification("source canonical owner parity").executor(execute);
    c.domain(KeyScenario::domain());
    c.seed(463).cases_budget(128);
    let case = c.case();
    c.check(fact!(case.result().clone()).expect(Vec::<String>::new()));
    assert_pairwise_certified(&c.report());
}
