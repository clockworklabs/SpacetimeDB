//! copied from headers::util::flat_csv, as it's not public API

// Copyright (c) 2014-2019 Sean McArthur
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

#![allow(clippy::all)]

use std::marker::PhantomData;

use bytes::BytesMut;
use http::HeaderValue;

// A single `HeaderValue` that can flatten multiple values with commas.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct FlatCsv<Sep = Comma> {
    pub(crate) value: HeaderValue,
    _marker: PhantomData<Sep>,
}

pub(crate) trait Separator {
    const BYTE: u8;
    const CHAR: char;
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Comma {}

impl Separator for Comma {
    const BYTE: u8 = b',';
    const CHAR: char = ',';
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SemiColon {}

impl Separator for SemiColon {
    const BYTE: u8 = b';';
    const CHAR: char = ';';
}

impl<Sep: Separator> FlatCsv<Sep> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = &str> {
        self.value.to_str().ok().into_iter().flat_map(|value_str| {
            let mut in_quotes = false;
            value_str
                .split(move |c| {
                    if in_quotes {
                        if c == '"' {
                            in_quotes = false;
                        }
                        false // dont split
                    } else {
                        if c == Sep::CHAR {
                            true // split
                        } else {
                            if c == '"' {
                                in_quotes = true;
                            }
                            false // dont split
                        }
                    }
                })
                .map(|item| item.trim())
        })
    }
}

impl<Sep> From<HeaderValue> for FlatCsv<Sep> {
    fn from(value: HeaderValue) -> Self {
        FlatCsv {
            value,
            _marker: PhantomData,
        }
    }
}

impl<'a, Sep: Separator> FromIterator<&'a HeaderValue> for FlatCsv<Sep> {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = &'a HeaderValue>,
    {
        let mut values = iter.into_iter();

        // Common case is there is only 1 value, optimize for that
        if let (1, Some(1)) = values.size_hint() {
            return values.next().expect("size_hint claimed 1 item").clone().into();
        }

        // Otherwise, there are multiple, so this should merge them into 1.
        let mut buf = values
            .next()
            .cloned()
            .map(|val| BytesMut::from(val.as_bytes()))
            .unwrap_or_else(|| BytesMut::new());

        for val in values {
            buf.extend_from_slice(&[Sep::BYTE, b' ']);
            buf.extend_from_slice(val.as_bytes());
        }

        let val = HeaderValue::from_maybe_shared(buf.freeze()).expect("comma separated HeaderValues are valid");

        val.into()
    }
}
