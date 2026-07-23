#[test]
fn model_task_trace_proves_callback_return_without_sensitive_values() {
    let evidence = super::evidence();
    let trace = &evidence.trace;
    let events = [
        "model-task request started",
        "model-task response accepted",
        "model-task callback started",
        "model-task callback returned",
    ];
    let positions = events.map(|event| {
        assert_eq!(
            trace.lines().filter(|line| *line == event).count(),
            1,
            "unexpected {event} trace count"
        );
        trace.find(event).expect("model-task trace event")
    });

    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    for sensitive in ["38191", "contract-token", "/api/v1/plugin/jobs/"] {
        assert!(!trace.contains(sensitive), "trace leaked {sensitive}");
    }
}
