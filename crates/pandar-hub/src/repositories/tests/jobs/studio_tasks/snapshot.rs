use super::*;

#[tokio::test]
async fn file_sqlite_studio_task_count_and_page_share_one_snapshot() {
    let state = crate::AppState::file_sqlite_for_tests().await.unwrap();
    let tenant = state
        .tenants()
        .create("studio-query-snapshot", "Studio Query Snapshot")
        .await
        .unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture(
        state.database(),
        tenant.id,
        agent.id,
    )
    .await
    .unwrap();
    let first = state
        .jobs()
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "studio-query-snapshot-first",
        ))
        .await
        .unwrap();

    let mut pause = crate::repositories::studio_task_test_pause::install();
    let list_state = state.clone();
    let list_printer_id = printer_id.clone();
    let list = tokio::spawn(async move {
        list_state
            .jobs()
            .list_studio_tasks(
                tenant.id,
                StudioTaskQuery {
                    printer_id: Some(list_printer_id),
                    status: None,
                    offset: 0,
                    limit: 20,
                },
            )
            .await
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        pause.wait_until_counted(),
    )
    .await
    .expect("Studio task query must reach its post-count pause");
    state
        .jobs()
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "studio-query-snapshot-second",
        ))
        .await
        .unwrap();
    pause.release();

    let page = list.await.unwrap().unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.jobs.len(), 1);
    assert_eq!(page.jobs[0].job.id, first.job.id);
}
