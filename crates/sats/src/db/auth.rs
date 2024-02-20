use crate::{impl_deserialize, impl_serialize};

use crate::de::Error;

/// Describe the visibility of the table
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StAccess {
    /// Visible to all
    Public,
    /// Visible only to the owner
    Private,
}

impl StAccess {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
        }
    }

    /// Select the appropriated [Self] for the name.
    ///
    /// A name that start with '_' like '_sample' is [Self::Private]
    pub fn for_name(of: &str) -> Self {
        if of.starts_with('_') {
            Self::Private
        } else {
            Self::Public
        }
    }
}

impl<'a> TryFrom<&'a str> for StAccess {
    type Error = &'a str;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Ok(match value {
            "public" => Self::Public,
            "private" => Self::Private,
            x => return Err(x),
        })
    }
}

impl_serialize!([] StAccess, (self, ser) => ser.serialize_str(self.as_str()));
impl_deserialize!([] StAccess, de => {
    let value = de.deserialize_str_slice()?;
    StAccess::try_from(value).map_err(|x| {
        Error::custom(format!(
            "DecodeError for StAccess: `{x}`. Expected `public` | 'private'"
        ))
    })
});

/// Describe is the table is a `system table` or not.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StTableType {
    /// Created by the system
    ///
    /// System tables are `StAccess::Public` by default
    System,
    /// Created by the User
    User,
}

impl StTableType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
        }
    }
}

impl<'a> TryFrom<&'a str> for StTableType {
    type Error = &'a str;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Ok(match value {
            "system" => Self::System,
            "user" => Self::User,
            x => return Err(x),
        })
    }
}

impl_serialize!([] StTableType, (self, ser) => ser.serialize_str(self.as_str()));
impl_deserialize!([] StTableType, de => {
    let value = de.deserialize_str_slice()?;
    StTableType::try_from(value).map_err(|x| {
        Error::custom(format!(
            "DecodeError for StTableType: `{x}`. Expected 'system' | 'user'"
        ))
    })
});
