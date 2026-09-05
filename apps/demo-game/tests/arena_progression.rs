mod support;

#[test]
fn complete_stock_run_matches_csharp_through_eleven_natural_death_and_retry() {
    use kitu_osc_ir::OscArg;
    let mut events = Vec::new();
    assert_eq!(
        support::replay_reference_observing("stock-eleven-death-retry", usize::MAX, |message| {
            if message.address.starts_with("/game/arena/") {
                let OscArg::Str(json) = &message.args[0] else {
                    panic!("event JSON")
                };
                events.push((
                    message.address.clone(),
                    serde_json::from_str::<serde_json::Value>(json).unwrap(),
                ));
            }
        }),
        (550, 53)
    );
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../kitu-integration-runner/scenarios/arena/reference/stock-eleven-death-retry/expected.ndjson");
    let checkpoints: Vec<serde_json::Value> = std::fs::read_to_string(directory)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let rewards: Vec<_> = events
        .iter()
        .filter(|(address, _)| address == "/game/arena/reward")
        .collect();
    assert_eq!(rewards.len(), 2);
    for (reward, floor) in rewards.iter().zip([5, 10]) {
        let expected = checkpoints
            .iter()
            .find(|state| state["floor"] == floor && state["phase"] == 4)
            .unwrap();
        assert_eq!(reward.1["floor"], floor);
        assert_eq!(reward.1["tick"], expected["tick"]);
        support::compare(
            &expected["inventory"],
            &reward.1["inventory"],
            "reward inventory",
        );
    }
    let results: Vec<_> = events
        .iter()
        .filter(|(address, _)| address == "/game/arena/result")
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1["tick"], 5526);
    let end: Vec<_> = events
        .iter()
        .filter(|(_, value)| value["tick"] == 5526)
        .map(|(address, _)| address.as_str())
        .collect();
    assert!(end.ends_with(&[
        "/game/arena/death",
        "/game/arena/phase",
        "/game/arena/result"
    ]));
    let retry = events
        .iter()
        .filter(|(address, value)| address == "/game/arena/phase" && value["previous"] == 5)
        .collect::<Vec<_>>();
    assert_eq!(retry.len(), 1);
    assert_eq!(
        (retry[0].1["tick"].as_i64(), retry[0].1["phase"].as_i64()),
        (Some(5527), Some(1))
    );
}

#[test]
fn preparation_remains_unchanged_after_enabling_endless_progression() {
    assert_eq!(support::replay_reference("preparation", 28), (12, 12));
}
