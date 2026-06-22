use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::CoreError;

macro_rules! uuid_id {
    ($name:ident, $error:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn parse(value: &str) -> Result<Self, CoreError> {
                Uuid::parse_str(value)
                    .map(Self)
                    .map_err(|_| CoreError::$error)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

uuid_id!(TenantId, InvalidTenantId);
uuid_id!(AgentId, InvalidAgentId);
uuid_id!(CommandId, InvalidCommandId);
uuid_id!(JobId, InvalidJobId);
