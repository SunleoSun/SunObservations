use crate::evidence::test_support::{record, shuffle};
use crate::evidence::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sun_observations::certifications::*;
use sun_observations::Observation;

#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "group::sample")]
struct Sample {
    row: u64,
    fold: u64,
    value: f64,
}
fn view(folds: &[u64]) -> TypedView<Sample> {
    ObservationDataset::from_shared(
        folds
            .iter()
            .enumerate()
            .map(|(row, fold)| {
                record(
                    Sample::NAMESPACE,
                    json!({"row":row,"fold":fold,"value":row as f64}),
                )
            })
            .collect(),
    )
    .to_typed()
    .unwrap()
}
#[test]
fn empty_single_and_repeated_groups_retain_every_member_in_key_then_source_order() {
    let mut c = certification("group membership and Observation provenance");
    for folds in [vec![], vec![7], vec![7; 20], vec![9, 3, 9, 1, 3, 9]] {
        let view = view(&folds);
        let source = view.source("samples");
        let groups = source.group_by(|v| v.fold);
        let mut expected_keys = folds.clone();
        expected_keys.sort_unstable();
        expected_keys.dedup();
        c.check(
            "key order",
            fact("groups", groups.iter().map(|g| *g.key).collect::<Vec<_>>()).expect(expected_keys),
        );
        c.check(
            "every member retained",
            fact("groups", groups.iter().map(|g| g.len()).sum::<usize>()).expect(folds.len()),
        );
        for group in groups.iter() {
            let expected: Vec<_> = folds
                .iter()
                .enumerate()
                .filter(|(_, k)| *k == group.key)
                .map(|(i, _)| i)
                .collect();
            c.check(
                "original member order",
                fact(
                    "group rows",
                    group.values().map(|v| v.row as usize).collect::<Vec<_>>(),
                )
                .expect(expected.clone()),
            );
            for (entry, index) in group.iter().zip(expected) {
                let mut coordinates = String::new();
                entry
                    .provenance
                    .unwrap()
                    .describe(entry.index, &mut coordinates)
                    .unwrap();
                c.check(
                    "source-local identity",
                    fact("entry", entry.index).expect(index),
                )
                .check(
                    "borrowed canonical value",
                    fact("entry", std::ptr::eq(entry.value, &view.values()[index])).expect(true),
                )
                .check(
                    "original dataset coordinates",
                    fact(
                        "entry",
                        coordinates.contains(&format!("dataset_index={index}")),
                    )
                    .expect(true),
                );
            }
        }
    }
    c.assert_certify();
}
#[test]
fn unordered_many_groups_and_summaries_equal_independent_member_analysis() {
    let mut folds: Vec<_> = (0..1000).map(|i| i % 79).collect();
    shuffle(&mut folds, 42);
    let view = view(&folds);
    let source = view.source("samples");
    let groups = source.group_by(|v| v.fold);
    let projection = groups.numeric(|v| v.value);
    let report = projection.summaries_with_robust();
    let mut c = certification("per-group numerical oracle");
    c.check("all groups", fact("groups", report.groups.len()).expect(79));
    for (key, group) in report.groups.iter().enumerate() {
        let expected: Vec<_> = view
            .values()
            .iter()
            .filter(|v| v.fold == group.key)
            .map(|v| v.value)
            .collect();
        let series = NumericSeries::new(expected.iter().copied());
        c.check("key", fact("group", group.key).expect(key as u64))
            .check(
                "count",
                fact("group", group.member_count).expect(expected.len()),
            )
            .check(
                "summary",
                fact("group", &group.summary).expect(&series.summary()),
            )
            .check(
                "robust",
                fact("group", &group.robust).expect(&Some(series.robust_summary())),
            );
    }
    let json = serde_json::to_value(&report).unwrap();
    let restored = serde_json::from_value::<GroupedSummaryReport<u64>>(json.clone()).unwrap();
    c.check(
        "serialization kind",
        fact("report", &json["analysis_kind"]).expect(&json!("grouped_summary")),
    )
    .check("roundtrip", fact("report", restored).expect(report))
    .assert_certify();
}
#[test]
fn grouped_numeric_failures_retain_group_identity_without_hiding_healthy_groups() {
    let view = view(&[0, 1, 0, 1]);
    let groups = view.source("samples").group_by(|v| v.fold);
    let report = groups
        .numeric(|v| if v.fold == 1 { f64::NAN } else { v.value })
        .summaries();
    let mut c = certification("bad subgroup isolation");
    c.check(
        "healthy group available",
        fact("group 0", report.groups[0].summary.is_ok()).expect(true),
    )
    .check(
        "invalid group explicit",
        fact(
            "group 1",
            matches!(
                report.groups[1].summary,
                Err(NumericError::NonFinite { .. })
            ),
        )
        .expect(true),
    )
    .check(
        "identity",
        fact("bad group", report.groups[1].key).expect(1),
    )
    .check(
        "member count",
        fact("bad group", report.groups[1].member_count).expect(2),
    )
    .check(
        "robust opt-in",
        fact("groups", report.groups.iter().all(|g| g.robust.is_none())).expect(true),
    );
    let result = fact("groups", report).expect(GroupExpectation::new(1, 0., 10.).unwrap());
    c.check(
        "unavailable required statistic is error",
        fact("acceptance", result.class).expect(CheckClass::EvaluationError),
    )
    .assert_certify();
}
#[test]
fn nonclone_composite_group_keys_need_no_hashing_or_value_cloning() {
    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    struct Key(u64, u64);
    let view = view(&[3, 2, 3, 2]);
    let groups = view.source("samples").group_by(|v| Key(v.fold, v.row % 2));
    certification("typed nonclone grouping identity")
        .check(
            "membership",
            fact(
                "groups",
                groups
                    .iter()
                    .map(|g| (g.key.0, g.len()))
                    .collect::<Vec<_>>(),
            )
            .expect(vec![(2, 2), (3, 2)]),
        )
        .assert_certify();
}
