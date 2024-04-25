use std::ops::Deref;

/// An owned string with a max length of `N`, like the venerable VARCHAR.
///
/// The length is in bytes, not characters.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct Varchar<const N: usize> {
    // TODO: Depending on `core` usage, we may want a SSO string type, a pointer
    // type, and / or a `Cow`-like type here.
    inner: String,
}

impl<const N: usize> Varchar<N> {
    /// Construct `Some(Self)` from a string slice,
    /// or `None` if the argument is longer than `N` bytes.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        (s.len() <= N).then(|| Self { inner: s.into() })
    }

    /// Construct [`Self`] from a string slice,
    /// or allocate a new string containing the first `N` bytes of the slice
    /// if it is longer than `N` bytes.
    ///
    /// In case of truncation, the resulting string may be shorter than `N` if
    /// `N` falls on a character boundary.
    pub fn from_str_truncate(s: &str) -> Self {
        Self::from_str(s).unwrap_or_else(|| {
            let mut s = s.to_owned();
            while s.len() > N {
                s.pop().unwrap();
            }
            Self { inner: s }
        })
    }

    /// Construct [`Self`] from a string,
    /// or `None` if the string is longer than `N`.
    pub fn from_string(s: String) -> Option<Self> {
        (s.len() <= N).then_some(Self { inner: s })
    }

    /// Move the given string into `Self` if its length does not exceed `N`,
    /// or truncate it to the appropriate length.
    ///
    /// In case of truncation, the resulting string may be shorter than `N` if
    /// `N` falls on a character boundary.
    pub fn from_string_truncate(s: String) -> Self {
        if s.len() <= N {
            Self { inner: s }
        } else {
            let mut s = s;
            while s.len() > N {
                s.pop().unwrap();
            }
            Self { inner: s }
        }
    }

    /// Discard the `Varchar` wrapper.
    pub fn into_inner(self) -> String {
        self.into()
    }

    /// Extract a string slice containing the entire `Varchar`.
    pub fn as_str(&self) -> &str {
        self
    }
}

impl<const N: usize> Deref for Varchar<N> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<const N: usize> From<Varchar<N>> for String {
    fn from(value: Varchar<N>) -> Self {
        value.inner
    }
}

#[cfg(feature = "serde")]
impl<const N: usize> serde::Serialize for Varchar<N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de, const N: usize> serde::Deserialize<'de> for Varchar<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let len = s.len();
        Self::from_string(s)
            .ok_or_else(|| serde::de::Error::custom(format!("input string too long: {} max-len={}", len, N)))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use proptest::prelude::*;

    impl<const N: usize> Arbitrary for Varchar<N> {
        type Strategy = BoxedStrategy<Varchar<N>>;
        type Parameters = ();

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            use proptest::char;
            use proptest::collection::vec;

            vec(char::ranges(char::DEFAULT_PREFERRED_RANGES.into()), 0..N)
                .prop_map(|chars| {
                    let inner = chars.into_iter().fold(String::with_capacity(N), |mut s, c| {
                        if s.len() + c.len_utf8() <= N {
                            s.push(c);
                        }
                        s
                    });
                    Varchar { inner }
                })
                .boxed()
        }
    }

    proptest! {
        #[test]
        fn prop_varchar_generator_does_not_break_invariant(varchar in any::<Varchar<255>>()) {
            assert!(varchar.len() <= 255);
        }

        #[test]
        fn prop_rejects_long(s in "\\w{33,}") {
            assert!(Varchar::<32>::from_string(s).is_none());
        }

        #[test]
        fn prop_accepts_short(s in "[[:ascii:]]{0,32}") {
            assert_eq!(s.as_str(), Varchar::<32>::from_str(&s).unwrap().as_str())
        }

        #[test]
        fn prop_truncate(s in "[[:ascii:]]{33,}") {
            let vc = Varchar::<32>::from_string_truncate(s);
            assert_eq!(32, vc.len());
        }

        #[test]
        fn prop_truncate_n_on_char_boundary(s in "[[:ascii:]]{31}") {
            let mut t = s.clone();
            t.push('ß');
            let vc = Varchar::<32>::from_string_truncate(t);
            assert_eq!(*vc, s);
        }
    }
}
