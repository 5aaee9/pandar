use serde::{Deserialize, Deserializer};

#[derive(Debug, Default)]
pub(super) enum RequestField<T> {
    #[default]
    Missing,
    Present(Option<T>),
}

impl<'de, T> Deserialize<'de> for RequestField<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(deserializer).map(Self::Present)
    }
}

impl<T> RequestField<T> {
    pub(super) fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }

    pub(super) fn is_some(&self) -> bool {
        matches!(self, Self::Present(Some(_)))
    }

    pub(super) fn into_option(self) -> Option<T> {
        match self {
            Self::Missing => None,
            Self::Present(value) => value,
        }
    }

    pub(super) fn expect(self, message: &str) -> T {
        self.into_option().expect(message)
    }

    pub(super) fn unwrap_or(self, default: T) -> T {
        self.into_option().unwrap_or(default)
    }
}

impl<T: Default> RequestField<T> {
    pub(super) fn unwrap_or_default(self) -> T {
        self.into_option().unwrap_or_default()
    }
}
