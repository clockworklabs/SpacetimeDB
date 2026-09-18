/// An error `E`, along with auxiliary data `T`.
///
/// `T` is usually a buffer of type [AlignedBytes], whose ownership is
/// transferred back to the caller when an error occurs.
///
/// As this type signifies an error condition, the contents of `T` are
/// unspecified.
///
/// [AlignedBytes]: crate::io::buf::AlignedBytes
#[derive(Debug)]
pub struct ErrorWith<E, T> {
    pub error: E,
    pub with: T,
}

impl<E, T> ErrorWith<E, T> {
    /// Map a type-changing function over `self.error`.
    pub fn map_err<F>(self, f: impl FnOnce(E) -> F) -> ErrorWith<F, T> {
        ErrorWith {
            error: f(self.error),
            with: self.with,
        }
    }

    /// Map a type-changing function over `self.with`.
    pub fn map_with<U>(self, f: impl FnOnce(T) -> U) -> ErrorWith<E, U> {
        ErrorWith {
            error: self.error,
            with: f(self.with),
        }
    }

    /// Extract `self.error`, discarding `self.with`.
    pub fn into_err(self) -> E {
        self.error
    }

    /// Convert from `&ErrorWith<E, T>` to `ErrorWith<&E, &T>`.
    pub fn as_ref(&self) -> ErrorWith<&E, &T> {
        let Self { ref error, ref with } = *self;
        ErrorWith { error, with }
    }
}
