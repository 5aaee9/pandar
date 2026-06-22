use serde::{Deserialize, Serialize};

use crate::{CoreError, TenantId, created_at_now};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tenant {
    pub id: TenantId,
    pub slug: String,
    pub display_name: String,
    pub created_at: String,
}

impl Tenant {
    pub fn new(
        slug: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Result<Self, CoreError> {
        let slug = slug.into();
        if slug.trim().is_empty() {
            return Err(CoreError::EmptyTenantSlug);
        }

        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(CoreError::EmptyTenantDisplayName);
        }

        Self::from_parts(TenantId::new(), slug, display_name, created_at_now())
    }

    pub fn from_parts(
        id: TenantId,
        slug: impl Into<String>,
        display_name: impl Into<String>,
        created_at: impl Into<String>,
    ) -> Result<Self, CoreError> {
        let slug = slug.into();
        if slug.trim().is_empty() {
            return Err(CoreError::EmptyTenantSlug);
        }

        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(CoreError::EmptyTenantDisplayName);
        }

        Ok(Self {
            id,
            slug,
            display_name,
            created_at: created_at.into(),
        })
    }
}
