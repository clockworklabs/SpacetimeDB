//! Methods and traits for dealing with multiple errors simultaneously.

/// A non-empty stream of errors.
///
/// Logically, this type is unordered, and it is not guaranteed that the errors will be returned in the order they were added. Attach identifying information to your errors if you want to sort them.
///
/// This struct is intended to be used with:
/// - The [CombineErrors] trait, which allows you to combine a tuples of results.
/// - The [CollectAllErrors] trait, which allows you to collect errors from an iterator of results.
#[derive(thiserror::Error, Debug, Clone, Default, PartialEq, Eq)]
pub struct ErrorStream<E>(smallvec::SmallVec<[E; 1]>);

impl<E> ErrorStream<E> {
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

/// A trait for converting a tuple of `Result`s into a `Result` of a tuple or `ErrorStream`.
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
    /// fn description() -> Result<String, ErrorStream<MyError>> {
    ///     let (age, name) = (age(), name()).combine_errors()?;
    ///     Ok(format!("{} is {} years old", name, age))
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
    /// If all results are `Ok`, returns the collected values. Otherwise, returns the collected errors.
    ///
    /// You CANNOT use the standard library function `Result<T, ErrorStream<E>>::collect()` for this,
    /// as it will return the FIRST error it encounters, rather than collecting all errors.
    ///
    /// The collection can be anything that implements `FromIterator`.
    ///
    /// Example usage:
    /// ```
    /// use spacetimedb_data_structures::error_stream::{ErrorStream, CollectAllErrors};
    /// use std::collections::HashSet;
    ///
    /// struct MyError { cause: String };
    ///
    /// fn operation(data: i32, checksum: u32) -> Result<i32, MyError> {
    /// #   Ok(1)
    /// }
    ///
    /// fn many_operations(data: Vec<(i32, u32)>) -> Result<HashSet<i32>, ErrorStream<MyError>> {
    ///     data
    ///         .into_iter()
    ///         .map(|(data, checksum)| operation(data, checksum))
    ///         .collect_all_errors::<HashSet<_>>()
    /// }
    /// ```
    fn collect_all_errors<C: FromIterator<Self::Item>>(self) -> Result<C, ErrorStream<Self::Error>>;
}

impl<T, E, I: Iterator<Item = Result<T, E>>> CollectAllErrors for I {
    type Item = T;
    type Error = E;

    fn collect_all_errors<Collection: FromIterator<Self::Item>>(self) -> Result<Collection, ErrorStream<Self::Error>> {
        let mut errors = ErrorStream(Default::default());

        let collection = self
            .filter_map(|result| match result {
                Ok(value) => Some(value),
                Err(error) => {
                    errors.0.push(error);
                    None
                }
            })
            .collect::<Collection>();

        if errors.0.is_empty() {
            Ok(collection)
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct MyError(u32);

    type Result<T> = std::result::Result<T, ErrorStream<MyError>>;

    #[test]
    fn combine_errors() {
        type ResultTuple = (Result<i32>, Result<String>, Result<u8>);
        let tuple_1: ResultTuple = (Ok(1), Ok("hi".into()), Ok(3));
        assert_eq!(tuple_1.combine_errors(), Ok((1, "hi".into(), 3)));

        let tuple_2: ResultTuple = (Err(MyError(1).into()), Ok("hi".into()), Ok(3));
        assert_eq!(tuple_2.combine_errors(), Err(MyError(1).into()));

        let tuple_3: ResultTuple = (Err(MyError(1).into()), Err(MyError(2).into()), Ok(3));
        assert_eq!(
            tuple_3.combine_errors(),
            Err(ErrorStream(smallvec::smallvec![MyError(1), MyError(2)]))
        );

        let tuple_4: ResultTuple = (Err(MyError(1).into()), Err(MyError(2).into()), Err(MyError(3).into()));
        assert_eq!(
            tuple_4.combine_errors(),
            Err(ErrorStream(smallvec::smallvec![MyError(1), MyError(2), MyError(3)]))
        );
    }

    #[test]
    fn collect_all_errors() {
        let data: Vec<std::result::Result<i32, MyError>> = vec![Ok(1), Ok(2), Ok(3)];
        assert_eq!(data.into_iter().collect_all_errors::<Vec<_>>(), Ok(vec![1, 2, 3]));

        let data = vec![Ok(1), Err(MyError(0)), Ok(3)];
        assert_eq!(
            data.into_iter().collect_all_errors::<Vec<_>>(),
            Err(ErrorStream([MyError(0)].into()))
        );

        let data: Vec<std::result::Result<i32, MyError>> = vec![Err(MyError(1)), Err(MyError(2)), Err(MyError(3))];
        assert_eq!(
            data.into_iter().collect_all_errors::<Vec<_>>(),
            Err(ErrorStream(smallvec::smallvec![MyError(1), MyError(2), MyError(3)]))
        );
    }
}
