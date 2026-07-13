use std::{
    borrow::Borrow,
    hash::{Hash, Hasher},
    ops::Deref,
};

use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

#[derive(Clone, Eq, PartialEq)]
pub(super) struct FirmwareSecret(Zeroizing<String>);

impl FirmwareSecret {
    pub(super) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for FirmwareSecret {
    fn from(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
}

impl Deref for FirmwareSecret {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl AsRef<str> for FirmwareSecret {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for FirmwareSecret {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Hash for FirmwareSecret {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl Zeroize for FirmwareSecret {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl ZeroizeOnDrop for FirmwareSecret {}
