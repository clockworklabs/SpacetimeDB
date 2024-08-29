//! Types, traits, and macros for working with non-empty streams of errorrs.
//!
//! The `ErrorStream<_>` type provides a collection that stores a non-empty, unordered stream of errors.
//! This is valuable for collecting as many errors as possible before returning them to the user,
//! which allows the user to work through the errors in the order of their choosing.
//! This is particularly useful for CLI tools.
//!
//! Example usage:
//! ```
//! use spacetimedb_data_structures::error_stream::{
//!     ErrorStream,
//!     CombineErrors,
//!     CollectAllErrors
//! };
//! use std::collections::HashSet;
//!
//! enum MyError { /* ... */ };
//! type MyErrors = ErrorStream<MyError>;
//!
//! type Name =
//!     /* ... */
//! #   String
//!     ;
//!
//! type Age =
//!     /* ... */
//! #   i32
//!     ;
//!
//! fn validate_name(name: String) -> Result<Name, MyErrors> {
//!     // ...
//! #   Ok(name)
//! }
//!
//! fn validate_age(age: i32) -> Result<Age, MyErrors> {
//!     // ...
//! #   Ok(age)
//! }
//!
//! fn validate_person(
//!     name: String,
//!     age: i32,
//!     friends: Vec<String>
//! ) -> Result<(Name, Age, HashSet<Name>), MyErrors> {
//!     // First, we perform some validation on various pieces
//!     // of input data, WITHOUT using `?`.
//!     let name: Result<Name, MyErrors> = validate_name(name);
//!     let age: Result<Age, MyErrors> = validate_age(age);
//!
//!     // If we have multiple pieces of data, we can use
//!     // `collect_all_errors` to build an arbitrary collection from them.
//!     // If there are any errors, all of these errors
//!     // will be returned in a single ErrorStream.
//!     let friends: Result<HashSet<Name>, MyErrors> = friends
//!         .into_iter()
//!         .map(validate_name)
//!         .collect_all_errors();
//!
//!     // Now, we can combine the results into a single result.
//!     // If there are any errors, they will be returned in a
//!     // single ErrorStream.
//!     let (name, age, friends): (Name, Age, HashSet<Name>) =
//!         (name, age, friends).combine_errors()?;
//!     
//!     Ok((name, age, friends))
//! }
//! ```
//!
//! ## Best practices
//!
//! ### Use `ErrorStream` everywhere
//! It is best to use `ErrorStream` everywhere in a multiple-error module, even
//! for methods that return only a single error. `CombineAllErrors` and `CollectAllErrors`
//! can only be implemented for types built from `Result<_, ErrorStream<_>>` due to trait conflicts.
//! `ErrorStream` uses a `smallvec::SmallVec` internally, so it is efficient for single errors.
//!
//! You can convert an `E` to an `ErrorStream<E>` using `.into()`.
//!
//! ### Not losing any errors
//! When using this module, it is best to avoid using `?` until as late as possible,
//! and to completely avoid using the `Result<Collection<_>, _>::collect` method.
//! Both of these may result in errors being discarded.
//!
//! Prefer using `Result::and_then` for chaining operations that may fail,
//! and `CollectAllErrors::collect_all_errors` for collecting errors from iterators.

use crate::map::HashSet;
use std::{fmt, hash::Hash};

/// A non-empty stream of errors.
///
/// Logically, this type is unordered, and it is not guaranteed that the errors will be returned in the order they were added.
/// Attach identifying information to your errors if you want to sort them.
///
/// This struct is intended to be used with:
/// - The [CombineErrors] trait, which allows you to combine a tuples of results.
/// - The [CollectAllErrors] trait, which allows you to collect errors from an iterator of results.
///
/// To create an `ErrorStream` from a single error, you can use `from` or `into`:
/// ```
/// use spacetimedb_data_structures::error_stream::ErrorStream;
///
/// enum MyError {
///     A(u32),
///     B
/// }
///
/// let error: ErrorStream<MyError> = MyError::A(1).into();
/// // or
/// let error = ErrorStream::from(MyError::A(1));
/// ```
///
/// This does not allocate (unless your error allocates, of course).
#[derive(thiserror::Error, Debug, Clone, Default, PartialEq, Eq)]
pub struct ErrorStream<E>(smallvec::SmallVec<[E; 1]>);

impl<E> ErrorStream<E> {
    /// Build an error stream from a non-empty collection.
    /// If the collection is empty, panic.
    pub fn expect_nonempty<I: IntoIterator<Item = E>>(errors: I) -> Self {
        let mut errors = errors.into_iter();
        let first = errors.next().expect("expected at least one error");
        let mut stream = Self::from(first);
        stream.extend(errors);
        stream
    }

    /// Add some extra errors to a result.
    ///
    /// If there are no errors, the result is not modified.
    /// If there are errors, and the result is `Err`, the errors are added to the stream.
    /// If there are errors, and the result is `Ok`, the `Ok` value is discarded, and the errors are returned in a stream.
    pub fn add_extra_errors<T>(
        result: Result<T, ErrorStream<E>>,
        extra_errors: impl IntoIterator<Item = E>,
    ) -> Result<T, ErrorStream<E>> {
        match result {
            Ok(value) => {
                let errors: SmallVec<[E; 1]> = extra_errors.into_iter().collect();
                if errors.is_empty() {
                    Ok(value)
                } else {
                    Err(ErrorStream(errors))
                }
            }
            Err(mut errors) => {
                errors.extend(extra_errors);
                Err(errors)
            }
        }
    }

    /// Returns an iterator over the errors in the stream.
    pub fn iter(&self) -> impl Iterator<Item = &E> {
        self.0.iter()
    }

    /// Returns a mutable iterator over the errors in the stream.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut E> {
        self.0.iter_mut()
    }

    /// Returns an iterator over the errors in the stream, consuming the stream.
    pub fn drain(&mut self) -> impl Iterator<Item = E> + '_ {
        self.0.drain(..)
    }

    /// Push an error onto the stream.
    pub fn push(&mut self, error: E) {
        self.0.push(error);
    }

    /// Extend the stream with another stream.
    pub fn extend(&mut self, other: impl IntoIterator<Item = E>) {
        self.0.extend(other);
    }

    /// Unpack an error into `self`, returning `None` if there was an error.
    /// This is not exposed because `CombineErrors` is more convenient.
    #[inline(never)] // don't optimize this too much
    #[cold]
    fn unpack<T, ES: Into<ErrorStream<E>>>(&mut self, result: Result<T, ES>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.0.extend(error.into().0);
                None
            }
        }
    }
}
impl<E: fmt::Display> fmt::Display for ErrorStream<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Errors occurred:")?;
        for error in self.iter() {
            writeln!(f, "{}\n", error)?;
        }
        Ok(())
    }
}

impl<E: Ord + Eq> ErrorStream<E> {
    /// Sort and deduplicate the errors in the error stream.
    pub fn sort_deduplicate(mut self) -> Self {
        self.0.sort_unstable();
        self.0.dedup();
        self
    }
}
impl<E: Eq + Hash> ErrorStream<E> {
    /// Hash and deduplicate the errors in the error stream.
    /// The resulting error stream has an arbitrary order.
    pub fn hash_deduplicate(mut self) -> Self {
        let set = self.0.drain(..).collect::<HashSet<_>>();
        self.0.extend(set);
        self
    }
}

impl<E> IntoIterator for ErrorStream<E> {
    type Item = <smallvec::SmallVec<[E; 1]> as IntoIterator>::Item;
    type IntoIter = <smallvec::SmallVec<[E; 1]> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<E> From<E> for ErrorStream<E> {
    fn from(error: E) -> Self {
        Self(smallvec::smallvec![error])
    }
}

/// A trait for converting a tuple of `Result<_, ErrorStream<_>>`s
/// into a `Result` of a tuple or combined `ErrorStream`.
pub trait CombineErrors {
    /// The type of the output if all results are `Ok`.
    type Ok;
    /// The type of the output if any result is `Err`.
    type Error;

    /// Combine errors from multiple places into one.
    /// This can be thought of as a kind of parallel `?`.
    ///
    /// If your goal is to show the user as many errors as possible, you should
    /// call this as late as possible, on as wide a tuple as you can.
    ///
    /// Example usage:
    ///
    /// ```
    /// use spacetimedb_data_structures::error_stream::{ErrorStream, CombineErrors};
    ///
    /// struct MyError { cause: String };
    ///
    /// fn age() -> Result<i32, ErrorStream<MyError>> {
    ///     //...
    /// # Ok(1)
    /// }
    ///
    /// fn name() -> Result<String, ErrorStream<MyError>> {
    ///     // ...
    /// # Ok("hi".into())
    /// }
    ///
    /// fn likes_dogs() -> Result<bool, ErrorStream<MyError>> {
    ///     // ...
    /// # Ok(false)
    /// }
    ///
    /// fn description() -> Result<String, ErrorStream<MyError>> {
    ///     // A typical usage of the API:
    ///     // Collect multiple `Result`s in parallel, only using
    ///     // `.combine_errors()?` once no more progress can be made.
    ///     let (age, name, likes_dogs) =
    ///         (age(), name(), likes_dogs()).combine_errors()?;
    ///
    ///     Ok(format!(
    ///         "{} is {} years old and {}",
    ///         name,
    ///         age,
    ///         if likes_dogs { "likes dogs" } else { "does not like dogs" }
    ///     ))
    /// }
    /// ```
    fn combine_errors(self) -> Result<Self::Ok, ErrorStream<Self::Error>>;
}

macro_rules! tuple_combine_errors {
    ($($T:ident),*) => {
        impl<$($T,)* E> CombineErrors for ($(Result<$T, ErrorStream<E>>,)*) {
            type Ok = ($($T,)*);
            type Error = E;

            #[allow(non_snake_case)]
            fn combine_errors(self) -> Result<Self::Ok, ErrorStream<Self::Error>> {
                let mut errors = ErrorStream(Default::default());
                let ($($T,)* ) = self;
                $(
                    let $T = errors.unpack($T);
                )*
                if errors.0.is_empty() {
                    // correctness: none of these pushed an error to `errors`, so by the contract of `unpack`, they must all be `Some`.
                    Ok(($($T.unwrap(),)*))
                } else {
                    Err(errors)
                }
            }
        }
    };
}

tuple_combine_errors!(T1, T2);
tuple_combine_errors!(T1, T2, T3);
tuple_combine_errors!(T1, T2, T3, T4);
tuple_combine_errors!(T1, T2, T3, T4, T5);
tuple_combine_errors!(T1, T2, T3, T4, T5, T6);
tuple_combine_errors!(T1, T2, T3, T4, T5, T6, T7);
tuple_combine_errors!(T1, T2, T3, T4, T5, T6, T7, T8);
tuple_combine_errors!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
tuple_combine_errors!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
tuple_combine_errors!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
tuple_combine_errors!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

/// A trait for collecting errors from an iterator of results,
/// returning all errors if anything failed.
pub trait CollectAllErrors {
    /// The item type we are aggregating.
    type Item;

    /// The error type we are aggregating.
    type Error;

    /// Collect errors from an iterator of results into a single error stream.
    /// If all results are `Ok`, returns the collected values. Otherwise,
    /// combine all errors into a single error stream, and return it.
    ///
    /// You CANNOT use the standard library function `Result<T, ErrorStream<E>>::collect()` for this,
    /// as it will return the FIRST error it encounters, rather than collecting all errors!
    ///
    /// The collection can be anything that implements `FromIterator`.
    ///
    /// Example usage:
    /// ```
    /// use spacetimedb_data_structures::error_stream::{
    ///     ErrorStream,
    ///     CollectAllErrors
    /// };
    /// use std::collections::HashSet;
    ///
    /// enum MyError { /* ... */ }
    ///
    /// fn operation(
    ///     data: String,
    ///     checksum: u32
    /// ) -> Result<i32, ErrorStream<MyError>> {
    ///     /* ... */
    /// #   Ok(1)
    /// }
    ///
    /// fn many_operations(
    ///     data: Vec<(String, u32)>
    /// ) -> Result<HashSet<i32>, ErrorStream<MyError>> {
    ///     data
    ///         .into_iter()
    ///         .map(|(data, checksum)| operation(data, checksum))
    ///         .collect_all_errors::<HashSet<_>>()
    /// }
    /// ```
    fn collect_all_errors<C: FromIterator<Self::Item>>(self) -> Result<C, ErrorStream<Self::Error>>;
}

impl<T, E, I: Iterator<Item = Result<T, ErrorStream<E>>>> CollectAllErrors for I {
    type Item = T;
    type Error = E;

    fn collect_all_errors<Collection: FromIterator<Self::Item>>(self) -> Result<Collection, ErrorStream<Self::Error>> {
        // not in a valid state: contains no errors!
        let mut all_errors = ErrorStream(Default::default());

        let collection = self
            .filter_map(|result| match result {
                Ok(value) => Some(value),
                Err(errors) => {
                    all_errors.extend(errors);
                    None
                }
            })
            .collect::<Collection>();

        if all_errors.0.is_empty() {
            // invalid state is not returned.
            Ok(collection)
        } else {
            // not empty, so we're good to return it.
            Err(all_errors)
        }
    }
}

/// Helper macro to match against an error stream, expecting a specific error.
/// For use in tests.
/// Panics if a matching error is not found.
/// Multiple matches are allowed.
///
/// Parameters:
/// - `$result` must be a `Result<_, ErrorStream<E>>`.
/// - `$expected` is a pattern to match against the error.
/// - `$cond` is an optional expression that should evaluate to `true` if the error matches.
///     Variables from `$expected` are bound in `$cond` behind references.
///     Do not use any asserts in `$cond` as it may be called against multiple errors.
///
/// ```
/// use spacetimedb_data_structures::error_stream::{
///     ErrorStream,
///     CollectAllErrors,
///     expect_error_matching
/// };
///
/// #[derive(PartialEq, Eq, Clone, Copy, Debug)]
/// struct CaseRef(u32);
///
/// #[derive(Debug)]
/// enum MyError {
///     InsufficientSwag { amount: u32 },
///     TooMuchSwag { reason: String, precedent: CaseRef },
///     SomethingElse(String)
/// }
///
/// let result: Result<(), ErrorStream<MyError>> = vec![
///     Err(MyError::TooMuchSwag {
///         reason: "sunglasses indoors".into(),
///         precedent: CaseRef(37)
///     }.into()),
///     Err(MyError::TooMuchSwag {
///         reason: "fur coat".into(),
///         precedent: CaseRef(55)
///     }.into()),
///     Err(MyError::SomethingElse(
///         "non-service animals forbidden".into()
///     ).into())
/// ].into_iter().collect_all_errors();
///
/// // This will panic if the error stream does not contain
/// // an error matching `MyError::SomethingElse`.
/// expect_error_matching!(
///     result,
///     MyError::SomethingElse(_)
/// );
///
/// // This will panic if the error stream does not contain
/// // an error matching `MyError::TooMuchSwag`, plus some
/// // extra conditions.
/// expect_error_matching!(
///     result,
///     MyError::TooMuchSwag { reason, precedent } =>
///         precedent == &CaseRef(37) && reason.contains("sunglasses")
/// );
/// ```
#[macro_export]
macro_rules! expect_error_matching (
    ($result:expr, $expected:pat => $cond:expr) => {
        let result: &::std::result::Result<
            _,
            $crate::error_stream::ErrorStream<_>
        > = &$result;
        match result {
            Ok(_) => panic!("expected error, but got Ok"),
            Err(errors) => {
                let err = errors.iter().find(|error|
                    if let $expected = error {
                        $cond
                    } else {
                        false
                    }
                );
                if let None = err {
                    panic!("expected error matching `{}` satisfying `{}`,\n but got {:#?}", stringify!($expected), stringify!($cond), errors);
                }
            }
        }
    };
    ($result:expr, $expected:pat) => {
        let result: &::std::result::Result<
            _,
            $crate::error_stream::ErrorStream<_>
        > = &$result;
        match result {
            Ok(_) => panic!("expected error, but got Ok"),
            Err(errors) => {
                let err = errors.iter().find(|error| matches!(error, $expected));
                if let None = err {
                    panic!("expected error matching `{}`,\n but got {:#?}", stringify!($expected), errors);
                }
            }
        }
    };
);
// Make available in this module as well as crate root.
pub use expect_error_matching;
use smallvec::SmallVec;

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum MyError {
        A(u32),
        B,
    }

    type Result<T> = std::result::Result<T, ErrorStream<MyError>>;

    #[test]
    fn combine_errors() {
        type ResultTuple = (Result<i32>, Result<String>, Result<u8>);
        let tuple_1: ResultTuple = (Ok(1), Ok("hi".into()), Ok(3));
        assert_eq!(tuple_1.combine_errors(), Ok((1, "hi".into(), 3)));

        let tuple_2: ResultTuple = (Err(MyError::A(1).into()), Ok("hi".into()), Ok(3));
        assert_eq!(tuple_2.combine_errors(), Err(MyError::A(1).into()));

        let tuple_3: ResultTuple = (Err(MyError::A(1).into()), Err(MyError::A(2).into()), Ok(3));
        assert_eq!(
            tuple_3.combine_errors(),
            Err(ErrorStream(smallvec::smallvec![MyError::A(1), MyError::A(2)]))
        );

        let tuple_4: ResultTuple = (
            Err(MyError::A(1).into()),
            Err(MyError::A(2).into()),
            Err(MyError::A(3).into()),
        );
        assert_eq!(
            tuple_4.combine_errors(),
            Err(ErrorStream(smallvec::smallvec![
                MyError::A(1),
                MyError::A(2),
                MyError::A(3)
            ]))
        );
    }

    #[test]
    fn collect_all_errors() {
        let data: Vec<Result<i32>> = vec![Ok(1), Ok(2), Ok(3)];
        assert_eq!(data.into_iter().collect_all_errors::<Vec<_>>(), Ok(vec![1, 2, 3]));

        let data = vec![Ok(1), Err(MyError::A(0).into()), Ok(3)];
        assert_eq!(
            data.into_iter().collect_all_errors::<Vec<_>>(),
            Err(ErrorStream([MyError::A(0)].into()))
        );

        let data: Vec<Result<i32>> = vec![
            Err(MyError::A(1).into()),
            Err(MyError::A(2).into()),
            Err(MyError::A(3).into()),
        ];
        assert_eq!(
            data.into_iter().collect_all_errors::<Vec<_>>(),
            Err(ErrorStream(smallvec::smallvec![
                MyError::A(1),
                MyError::A(2),
                MyError::A(3)
            ]))
        );
    }

    #[test]
    #[should_panic]
    fn expect_error_matching_without_cond_panics() {
        let data: Result<()> = Err(ErrorStream(vec![MyError::B].into()));
        expect_error_matching!(data, MyError::A(_));
    }

    #[test]
    #[should_panic]
    fn expect_error_matching_with_cond_panics() {
        let data: Result<()> = Err(ErrorStream(vec![MyError::A(5), MyError::A(10)].into()));
        expect_error_matching!(data, MyError::A(n) => n == &12);
    }
}
