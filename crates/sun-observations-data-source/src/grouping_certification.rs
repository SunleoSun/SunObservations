use crate::test_support::{record, shuffle};
use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sun_certifications::*;
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
    .typed()
    .unwrap()
}
#[test]
fn empty_single_and_repeated_groups_retain_every_member_in_key_then_source_order() {
    let mut c = certify("group membership and Observation provenance");
    for folds in [vec![], vec![7], vec![7; 20], vec![9, 3, 9, 1, 3, 9]] {
        let view = view(&folds);
        let source = view.source("samples");
        let groups = source.group_by(|v| v.fold);
        let mut expected_keys = folds.clone();
        expected_keys.sort_unstable();
        expected_keys.dedup();
        c.check(
            "key order",
            measured("groups", groups.iter().map(|g| *g.key).collect::<Vec<_>>())
                .compare(Equal(expected_keys)),
        );
        c.check(
            "every member retained",
            measured("groups", groups.iter().map(|g| g.len()).sum::<usize>())
                .compare(Equal(folds.len())),
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
                measured(
                    "group rows",
                    group.values().map(|v| v.row as usize).collect::<Vec<_>>(),
                )
                .compare(Equal(expected.clone())),
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
                    measured("entry", entry.index).compare(Equal(index)),
                )
                .check(
                    "borrowed canonical value",
                    measured("entry", std::ptr::eq(entry.value, &view.values()[index]))
                        .compare(Equal(true)),
                )
                .check(
                    "original dataset coordinates",
                    measured(
                        "entry",
                        coordinates.contains(&format!("dataset_index={index}")),
                    )
                    .compare(Equal(true)),
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
    let mut c = certify("per-group numerical oracle");
    c.check(
        "all groups",
        measured("groups", report.groups.len()).compare(Equal(79)),
    );
    for (key, group) in report.groups.iter().enumerate() {
        let expected: Vec<_> = view
            .values()
            .iter()
            .filter(|v| v.fold == group.key)
            .map(|v| v.value)
            .collect();
        let series = NumericSeries::new(expected.iter().copied());
        c.check(
            "key",
            measured("group", group.key).compare(Equal(key as u64)),
        )
        .check(
            "count",
            measured("group", group.member_count).compare(Equal(expected.len())),
        )
        .check(
            "summary",
            measured("group", &group.summary).compare(Equal(&series.summary())),
        )
        .check(
            "robust",
            measured("group", &group.robust).compare(Equal(&Some(series.robust_summary()))),
        );
    }
    let json = serde_json::to_value(&report).unwrap();
    let restored = serde_json::from_value::<GroupedSummaryReport<u64>>(json.clone()).unwrap();
    c.check(
        "serialization kind",
        measured("report", &json["analysis_kind"]).compare(Equal(&json!("grouped_summary"))),
    )
    .check(
        "roundtrip",
        measured("report", restored).compare(Equal(report)),
    )
    .assert_certify();
}
#[test]
fn grouped_numeric_failures_retain_group_identity_without_hiding_healthy_groups() {
    let view = view(&[0, 1, 0, 1]);
    let groups = view.source("samples").group_by(|v| v.fold);
    let report = groups
        .numeric(|v| if v.fold == 1 { f64::NAN } else { v.value })
        .summaries();
    let mut c = certify("bad subgroup isolation");
    c.check(
        "healthy group available",
        measured("group 0", report.groups[0].summary.is_ok()).compare(Equal(true)),
    )
    .check(
        "invalid group explicit",
        measured(
            "group 1",
            matches!(
                report.groups[1].summary,
                Err(NumericError::NonFinite { .. })
            ),
        )
        .compare(Equal(true)),
    )
    .check(
        "identity",
        measured("bad group", report.groups[1].key).compare(Equal(1)),
    )
    .check(
        "member count",
        measured("bad group", report.groups[1].member_count).compare(Equal(2)),
    )
    .check(
        "robust opt-in",
        measured("groups", report.groups.iter().all(|g| g.robust.is_none())).compare(Equal(true)),
    );
    let result = measured("groups", report).compare(GroupExpectation {
        minimum_count: 1,
        mean_min: 0.,
        mean_max: 10.,
    });
    c.check(
        "unavailable required statistic is error",
        measured("acceptance", result.class).compare(Equal(CheckClass::Error)),
    )
    .assert_certify();
}
#[test]
fn nonclone_composite_group_keys_need_no_hashing_or_value_cloning() {
    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    struct Key(u64, u64);
    let view = view(&[3, 2, 3, 2]);
    let groups = view.source("samples").group_by(|v| Key(v.fold, v.row % 2));
    certify("typed nonclone grouping identity")
        .check(
            "membership",
            measured(
                "groups",
                groups
                    .iter()
                    .map(|g| (g.key.0, g.len()))
                    .collect::<Vec<_>>(),
            )
            .compare(Equal(vec![(2, 2), (3, 2)])),
        )
        .assert_certify();
}
