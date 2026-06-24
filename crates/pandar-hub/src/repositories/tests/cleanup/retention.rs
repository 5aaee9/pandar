use crate::db::Database;

pub(super) async fn insert_ticket(
    database: &Database,
    id: &str,
    created_at: &str,
    used_at: Option<&str>,
    revoked_at: Option<&str>,
    expires_at: &str,
) {
    let tenant_id = ensure_cleanup_tenant(database).await;
    match database {
        Database::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO plugin_login_tickets (id, tenant_id, user_id, ticket_hash, redirect_url, created_at, expires_at, used_at, revoked_at)
                 VALUES (?1, ?2, NULL, ?3, 'http://localhost', ?4, ?5, ?6, ?7)",
            )
            .bind(id)
            .bind(&tenant_id)
            .bind(format!("hash-{id}"))
            .bind(created_at)
            .bind(expires_at)
            .bind(used_at)
            .bind(revoked_at)
            .execute(pool)
            .await
            .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO plugin_login_tickets (id, tenant_id, user_id, ticket_hash, redirect_url, created_at, expires_at, used_at, revoked_at)
                 VALUES ($1, $2, NULL, $3, 'http://localhost', $4, $5, $6, $7)",
            )
            .bind(id)
            .bind(&tenant_id)
            .bind(format!("hash-{id}"))
            .bind(created_at)
            .bind(expires_at)
            .bind(used_at)
            .bind(revoked_at)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

pub(super) async fn insert_tenant_token(
    database: &Database,
    id: &str,
    created_at: &str,
    revoked_at: Option<&str>,
    expires_at: Option<&str>,
) {
    let tenant_id = ensure_cleanup_tenant(database).await;
    match database {
        Database::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO tenant_tokens (id, tenant_id, name, token_hash, scopes_json, created_by_user_id, created_at, last_used_at, expires_at, revoked_at)
                 VALUES (?1, ?2, ?3, ?4, '[]', NULL, ?5, NULL, ?6, ?7)",
            )
            .bind(id)
            .bind(&tenant_id)
            .bind(id)
            .bind(format!("hash-{id}"))
            .bind(created_at)
            .bind(expires_at)
            .bind(revoked_at)
            .execute(pool)
            .await
            .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO tenant_tokens (id, tenant_id, name, token_hash, scopes_json, created_by_user_id, created_at, last_used_at, expires_at, revoked_at)
                 VALUES ($1, $2, $3, $4, '[]', NULL, $5, NULL, $6, $7)",
            )
            .bind(id)
            .bind(&tenant_id)
            .bind(id)
            .bind(format!("hash-{id}"))
            .bind(created_at)
            .bind(expires_at)
            .bind(revoked_at)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

async fn ensure_cleanup_tenant(database: &Database) -> String {
    let tenant_id = "cleanup-token-tenant";
    match database {
        Database::Sqlite(pool) => {
            sqlx::query(
                "INSERT OR IGNORE INTO tenants (id, slug, display_name, created_at) VALUES (?1, 'cleanup-token', 'Cleanup Token', '2025-01-01T00:00:00Z')",
            )
            .bind(tenant_id)
            .execute(pool)
            .await
            .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO tenants (id, slug, display_name, created_at) VALUES ($1, 'cleanup-token', 'Cleanup Token', '2025-01-01T00:00:00Z') ON CONFLICT (id) DO NOTHING",
            )
            .bind(tenant_id)
            .execute(pool)
            .await
            .unwrap();
        }
    }
    tenant_id.to_owned()
}
