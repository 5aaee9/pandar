use super::*;

#[tokio::test]
async fn postgres_records_survive_reconnect_when_configured() {
    let url = match std::env::var("PANDAR_TEST_POSTGRES_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
            return;
        }
    };
    let config = DatabaseConfig::from_url(&url).unwrap();
    let database = Database::connect(&config).await.unwrap();
    database.migrate().await.unwrap();
    super::postgres::clear_postgres(&database).await;

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    agents.create(tenant.id, "agent").await.unwrap();
    drop(database);

    let database = Database::connect(&config).await.unwrap();
    database.migrate().await.unwrap();
    assert_eq!(
        TenantRepository::new(database.clone())
            .count()
            .await
            .unwrap(),
        1
    );
    assert_eq!(AgentRepository::new(database).count().await.unwrap(), 1);
}
