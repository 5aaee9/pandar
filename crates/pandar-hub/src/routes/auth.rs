use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use pandar_core::TenantId;

use crate::{
    AppState,
    identity::VerifiedExternalIdentity,
    repositories::{
        AuditActor, AuthenticatedPrincipal, AuthenticatedUser, ExternalIdentityProfile,
        TenantTokenScope, UserRole,
    },
    routes::ApiError,
};

pub(super) async fn authorize_tenant(
    state: &AppState,
    headers: &HeaderMap,
    tenant_id: TenantId,
    required_role: UserRole,
) -> Result<AuthenticatedPrincipal, ApiError> {
    let principal = authorize_principal(state, headers, tenant_id).await?;
    match &principal {
        AuthenticatedPrincipal::User(authenticated) => {
            if !authenticated.user.role.allows(required_role) {
                return Err(ApiError::new(StatusCode::FORBIDDEN, "role_forbidden"));
            }
        }
        AuthenticatedPrincipal::TenantToken(authenticated) => {
            if !(authenticated.token.has_scope(TenantTokenScope::All)
                || required_role == UserRole::Viewer && authenticated.token.scopes.is_empty())
            {
                return Err(ApiError::new(StatusCode::FORBIDDEN, "role_forbidden"));
            }
        }
    }
    Ok(principal)
}

pub(super) async fn authorize_tenant_admin_principal(
    state: &AppState,
    headers: &HeaderMap,
    tenant_id: TenantId,
) -> Result<AuthenticatedPrincipal, ApiError> {
    authorize_principal_for_role(state, headers, tenant_id, UserRole::TenantAdmin).await
}

pub(super) async fn authorize_tenant_admin_user(
    state: &AppState,
    headers: &HeaderMap,
    tenant_id: TenantId,
) -> Result<AuthenticatedUser, ApiError> {
    let principal = authorize_principal(state, headers, tenant_id).await?;
    let AuthenticatedPrincipal::User(authenticated) = principal else {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "role_forbidden"));
    };
    if !authenticated.user.role.allows(UserRole::TenantAdmin) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "role_forbidden"));
    }
    Ok(authenticated)
}

pub(super) async fn authorize_tenant_principal(
    state: &AppState,
    headers: &HeaderMap,
    tenant_id: TenantId,
    required_role: UserRole,
) -> Result<AuthenticatedPrincipal, ApiError> {
    authorize_principal_for_role(state, headers, tenant_id, required_role).await
}

pub(super) async fn authorize_agent_registration(
    state: &AppState,
    headers: &HeaderMap,
    tenant_id: TenantId,
) -> Result<AuthenticatedPrincipal, ApiError> {
    let principal = authorize_principal(state, headers, tenant_id).await?;
    match &principal {
        AuthenticatedPrincipal::User(authenticated) => {
            if !authenticated.user.role.allows(UserRole::TenantAdmin) {
                return Err(ApiError::new(StatusCode::FORBIDDEN, "role_forbidden"));
            }
        }
        AuthenticatedPrincipal::TenantToken(authenticated) => {
            if !authenticated
                .token
                .has_scope(TenantTokenScope::AgentRegister)
            {
                return Err(ApiError::new(StatusCode::FORBIDDEN, "role_forbidden"));
            }
        }
    }
    Ok(principal)
}

pub(super) async fn authorize_plugin_login_ticket_creation(
    state: &AppState,
    headers: &HeaderMap,
    tenant_id: TenantId,
) -> Result<AuthenticatedPrincipal, ApiError> {
    let principal = authorize_principal(state, headers, tenant_id).await?;
    match &principal {
        AuthenticatedPrincipal::User(authenticated) => {
            if !authenticated.user.role.allows(UserRole::Viewer) {
                return Err(ApiError::new(StatusCode::FORBIDDEN, "role_forbidden"));
            }
        }
        AuthenticatedPrincipal::TenantToken(authenticated) => {
            if !authenticated.token.has_scope(TenantTokenScope::All) {
                return Err(ApiError::new(StatusCode::FORBIDDEN, "role_forbidden"));
            }
        }
    }
    Ok(principal)
}

pub(super) async fn authorize_plugin_studio(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<crate::repositories::AuthenticatedTenantToken, ApiError> {
    let Some(header) = headers.get(AUTHORIZATION) else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "missing_auth_token",
        ));
    };
    let header = header
        .to_str()
        .map_err(|_| ApiError::new(StatusCode::UNAUTHORIZED, "invalid_auth_token"))?;
    let Some(token) = header.strip_prefix("Bearer ") else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_auth_token",
        ));
    };
    let authenticated = state
        .auth()
        .authenticate_tenant_token(token)
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "invalid_auth_token"))?;
    if authenticated.token.scopes != [TenantTokenScope::PluginStudio] {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "role_forbidden"));
    }
    Ok(authenticated)
}

pub(super) async fn verify_external_identity(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<VerifiedExternalIdentity, ApiError> {
    let Some(header) = headers.get(AUTHORIZATION) else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "missing_auth_token",
        ));
    };
    let header = header
        .to_str()
        .map_err(|_| ApiError::new(StatusCode::UNAUTHORIZED, "invalid_auth_token"))?;
    let Some(token) = header.strip_prefix("Bearer ") else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_auth_token",
        ));
    };
    let Some(verifier) = state.external_auth() else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_auth_token",
        ));
    };
    verifier.verify(token).await.map_err(|err| {
        let error = anyhow::Error::from(err);
        tracing::debug!(
            error = %format!("{error:#}"),
            "external bearer token verification failed"
        );
        ApiError::new(StatusCode::UNAUTHORIZED, "invalid_auth_token")
    })
}

pub(super) fn external_profile(
    verified: &VerifiedExternalIdentity,
) -> Result<ExternalIdentityProfile, ApiError> {
    let Some(email) = verified.verified_email() else {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "external_email_unverified",
        ));
    };
    Ok(ExternalIdentityProfile {
        provider: verified.provider.clone(),
        subject: verified.subject.clone(),
        email: email.to_owned(),
        display_name: verified.display_name(),
    })
}

async fn authorize_principal_for_role(
    state: &AppState,
    headers: &HeaderMap,
    tenant_id: TenantId,
    required_role: UserRole,
) -> Result<AuthenticatedPrincipal, ApiError> {
    let principal = authorize_principal(state, headers, tenant_id).await?;
    match &principal {
        AuthenticatedPrincipal::User(authenticated) => {
            if !authenticated.user.role.allows(required_role) {
                return Err(ApiError::new(StatusCode::FORBIDDEN, "role_forbidden"));
            }
        }
        AuthenticatedPrincipal::TenantToken(authenticated) => {
            if !authenticated.token.has_scope(TenantTokenScope::All) {
                return Err(ApiError::new(StatusCode::FORBIDDEN, "role_forbidden"));
            }
        }
    }
    Ok(principal)
}

pub(super) async fn authorize_principal(
    state: &AppState,
    headers: &HeaderMap,
    tenant_id: TenantId,
) -> Result<AuthenticatedPrincipal, ApiError> {
    let Some(header) = headers.get(AUTHORIZATION) else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "missing_auth_token",
        ));
    };
    let header = header
        .to_str()
        .map_err(|_| ApiError::new(StatusCode::UNAUTHORIZED, "invalid_auth_token"))?;
    let Some(token) = header.strip_prefix("Bearer ") else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_auth_token",
        ));
    };

    let authenticated =
        if let Some(authenticated) = state.auth().authenticate_tenant_token(token).await? {
            AuthenticatedPrincipal::TenantToken(authenticated)
        } else if let Some(verifier) = state.external_auth() {
            let verified = verifier.verify(token).await.map_err(|err| {
                let error = anyhow::Error::from(err);
                tracing::debug!(
                    error = %format!("{error:#}"),
                    "external bearer token verification failed"
                );
                ApiError::new(StatusCode::UNAUTHORIZED, "invalid_auth_token")
            })?;
            state
                .auth()
                .authenticate_external_identity(tenant_id, &verified.provider, &verified.subject)
                .await?
                .ok_or_else(|| ApiError::new(StatusCode::FORBIDDEN, "tenant_forbidden"))?
                .into()
        } else {
            return Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "invalid_auth_token",
            ));
        };
    if principal_tenant_id(&authenticated) != tenant_id {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "tenant_forbidden"));
    }

    Ok(authenticated)
}

pub(super) fn audit_actor(principal: &AuthenticatedPrincipal) -> AuditActor {
    match principal {
        AuthenticatedPrincipal::User(authenticated) => {
            AuditActor::user(authenticated.user.id.clone())
        }
        AuthenticatedPrincipal::TenantToken(authenticated) => AuditActor::tenant_token(
            authenticated.token.created_by_user_id.clone(),
            authenticated.token.id.clone(),
            authenticated
                .token
                .scopes
                .iter()
                .map(|scope| scope.as_str())
                .collect(),
        ),
    }
}

pub(super) fn plugin_audit_actor(
    authenticated: &crate::repositories::AuthenticatedTenantToken,
) -> AuditActor {
    AuditActor::plugin_token(
        authenticated.token.created_by_user_id.clone(),
        authenticated.token.id.clone(),
        authenticated
            .token
            .scopes
            .iter()
            .map(|scope| scope.as_str())
            .collect(),
    )
}

fn principal_tenant_id(principal: &AuthenticatedPrincipal) -> TenantId {
    match principal {
        AuthenticatedPrincipal::User(authenticated) => authenticated.user.tenant_id,
        AuthenticatedPrincipal::TenantToken(authenticated) => authenticated.token.tenant_id,
    }
}

impl From<AuthenticatedUser> for AuthenticatedPrincipal {
    fn from(value: AuthenticatedUser) -> Self {
        Self::User(value)
    }
}
