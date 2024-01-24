use derive_more::{From, Into};
use std::fmt::{self, Write as _};

use crate::{
    algebraic_value::ser::ValueSerializer,
    ser::{self, Serialize},
};

/// An extension trait for [`Serialize`](ser::Serialize) providing formatting methods.
pub trait Satn: ser::Serialize {
    /// Formats the value using the SATN data format into the formatter `f`.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Writer::with(f, |f| self.serialize(SatnFormatter { f }))?;
        Ok(())
    }

    /// Formats the value using the postgres SATN data format into the formatter `f`.
    fn fmt_psql(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Writer::with(f, |f| self.serialize(PsqlFormatter(SatnFormatter { f })))?;
        Ok(())
    }

    /// Formats the value using the SATN data format into the returned `String`.
    fn to_satn(&self) -> String {
        Wrapper::from_ref(self).to_string()
    }

    /// Pretty prints the value using the SATN data format into the returned `String`.
    fn to_satn_pretty(&self) -> String {
        format!("{:#}", Wrapper::from_ref(self))
    }
}

impl<T: ser::Serialize + ?Sized> Satn for T {}

/// A wrapper around a `T: Satn`
/// providing `Display` and `Debug` implementations
/// that uses the SATN formatting for `T`.
#[repr(transparent)]
pub struct Wrapper<T: ?Sized>(pub T);

impl<T: ?Sized> Wrapper<T> {
    /// Converts `&T` to `&Wrapper<T>`.
    pub fn from_ref(t: &T) -> &Self {
        // SAFETY: `repr(transparent)` turns the ABI of `T`
        // into the same as `Self` so we can also cast `&T` to `&Self`.
        unsafe { &*(t as *const T as *const Self) }
    }
}

impl<T: Satn + ?Sized> fmt::Display for Wrapper<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Satn + ?Sized> fmt::Debug for Wrapper<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A wrapper around a `T: Satn`
/// providing `Display` and `Debug` implementations
/// that uses postgres SATN formatting for `T`.
#[repr(transparent)]
pub struct PsqlWrapper<T: ?Sized>(pub T);

impl<T: ?Sized> PsqlWrapper<T> {
    /// Converts `&T` to `&PsqlWrapper<T>`.
    pub fn from_ref(t: &T) -> &Self {
        // SAFETY: `repr(transparent)` turns the ABI of `T`
        // into the same as `Self` so we can also cast `&T` to `&Self`.
        unsafe { &*(t as *const T as *const Self) }
    }
}

impl<T: Satn + ?Sized> fmt::Display for PsqlWrapper<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt_psql(f)
    }
}

impl<T: Satn + ?Sized> fmt::Debug for PsqlWrapper<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt_psql(f)
    }
}

/// Wraps a writer for formatting lists separated by `SEP` into it.
struct EntryWrapper<'a, 'b, const SEP: char> {
    /// The writer we're formatting into.
    fmt: Writer<'a, 'b>,
    /// Whether there were any fields.
    /// Initially `false` and then `true` after calling [`.entry(..)`](EntryWrapper::entry).
    has_fields: bool,
}

impl<'a, 'b, const SEP: char> EntryWrapper<'a, 'b, SEP> {
    /// Constructs the entry wrapper using the writer `fmt`.
    fn new(fmt: Writer<'a, 'b>) -> Self {
        Self { fmt, has_fields: false }
    }

    /// Formats another entry in the larger structure.
    ///
    /// The formatting for the element / entry itself is provided by the function `entry`.
    fn entry(&mut self, entry: impl FnOnce(Writer) -> fmt::Result) -> fmt::Result {
        let res = (|| match &mut self.fmt {
            Writer::Pretty(f) => {
                if !self.has_fields {
                    f.write_char('\n')?;
                }
                f.state.indent += 1;
                entry(Writer::Pretty(f.as_mut()))?;
                f.write_char(SEP)?;
                f.write_char('\n')?;
                f.state.indent -= 1;
                Ok(())
            }
            Writer::Normal(f) => {
                if self.has_fields {
                    f.write_char(SEP)?;
                    f.write_char(' ')?;
                }
                entry(Writer::Normal(f))
            }
        })();
        self.has_fields = true;
        res
    }
}

/// An implementation of [`fmt::Write`] supporting indented and non-idented formatting.
enum Writer<'a, 'b> {
    /// Uses the standard library's formatter i.e. plain formatting.
    Normal(&'a mut fmt::Formatter<'b>),
    /// Uses indented formatting.
    Pretty(IndentedWriter<'a, 'b>),
}

impl<'a, 'b> Writer<'a, 'b> {
    /// Provided with a formatter `f`, runs `func` provided with a `Writer`.
    fn with<R>(f: &mut fmt::Formatter<'_>, func: impl FnOnce(Writer<'_, '_>) -> R) -> R {
        let mut state;
        // We use `alternate`, i.e., the `#` flag to let the user trigger pretty printing.
        let f = if f.alternate() {
            state = IndentState {
                indent: 0,
                on_newline: true,
            };
            Writer::Pretty(IndentedWriter { f, state: &mut state })
        } else {
            Writer::Normal(f)
        };
        func(f)
    }

    /// Returns a sub-writer without moving `self`.
    fn as_mut(&mut self) -> Writer<'_, 'b> {
        match self {
            Writer::Normal(f) => Writer::Normal(f),
            Writer::Pretty(f) => Writer::Pretty(f.as_mut()),
        }
    }
}

/// A formatter that adds decoration atop of the standard library's formatter.
struct IndentedWriter<'a, 'b> {
    f: &'a mut fmt::Formatter<'b>,
    state: &'a mut IndentState,
}

/// The indentation state.
struct IndentState {
    /// Number of tab indentations to make.
    indent: u32,
    /// Whether we were last on a newline.
    on_newline: bool,
}

impl<'a, 'b> IndentedWriter<'a, 'b> {
    /// Returns a sub-writer without moving `self`.
    fn as_mut(&mut self) -> IndentedWriter<'_, 'b> {
        IndentedWriter {
            f: self.f,
            state: self.state,
        }
    }
}

impl<'a, 'b> fmt::Write for IndentedWriter<'a, 'b> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for s in s.split_inclusive('\n') {
            if self.state.on_newline {
                // Indent 4 characters times the indentation level.
                for _ in 0..self.state.indent {
                    self.f.write_str("    ")?;
                }
            }

            self.state.on_newline = s.ends_with('\n');
            self.f.write_str(s)?;
        }
        Ok(())
    }
}

impl<'a, 'b> fmt::Write for Writer<'a, 'b> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        match self {
            Writer::Normal(f) => f.write_str(s),
            Writer::Pretty(f) => f.write_str(s),
        }
    }
}

/// Provides the SATN data format implementing [`Serializer`](ser::Serializer).
struct SatnFormatter<'a, 'b> {
    /// The sink / writer / output / formatter.
    f: Writer<'a, 'b>,
}

/// An error occured during serialization to the SATS data format.
#[derive(From, Into)]
struct SatnError(fmt::Error);

impl ser::Error for SatnError {
    fn custom<T: fmt::Display>(_msg: T) -> Self {
        Self(fmt::Error)
    }
}

impl<'a, 'b> SatnFormatter<'a, 'b> {
    /// Writes `args` formatted to `self`.
    #[inline(always)]
    fn write_fmt(&mut self, args: fmt::Arguments) -> Result<(), SatnError> {
        self.f.write_fmt(args)?;
        Ok(())
    }
}

impl<'a, 'b> ser::Serializer for SatnFormatter<'a, 'b> {
    type Ok = ();
    type Error = SatnError;
    type SerializeArray = ArrayFormatter<'a, 'b>;
    type SerializeMap = MapFormatter<'a, 'b>;
    type SerializeSeqProduct = SeqFormatter<'a, 'b>;
    type SerializeNamedProduct = NamedFormatter<'a, 'b>;

    fn serialize_bool(mut self, v: bool) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }
    fn serialize_u8(mut self, v: u8) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }
    fn serialize_u16(mut self, v: u16) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }
    fn serialize_u32(mut self, v: u32) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }
    fn serialize_u64(mut self, v: u64) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }
    fn serialize_u128(mut self, v: u128) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }
    fn serialize_i8(mut self, v: i8) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }
    fn serialize_i16(mut self, v: i16) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }
    fn serialize_i32(mut self, v: i32) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }
    fn serialize_i64(mut self, v: i64) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }
    fn serialize_i128(mut self, v: i128) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }
    fn serialize_f32(mut self, v: f32) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }
    fn serialize_f64(mut self, v: f64) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }

    fn serialize_str(mut self, v: &str) -> Result<Self::Ok, Self::Error> {
        write!(self, "\"{}\"", v)
    }

    fn serialize_bytes(mut self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        write!(self, "0x{}", hex::encode(v))
    }

    fn serialize_array(mut self, _len: usize) -> Result<Self::SerializeArray, Self::Error> {
        write!(self, "[")?; // Closed via `.end()`.
        Ok(ArrayFormatter {
            f: EntryWrapper::new(self.f),
        })
    }

    fn serialize_map(mut self, len: usize) -> Result<Self::SerializeMap, Self::Error> {
        write!(self, "[")?; // Closed via `.end()`.
        if len == 0 {
            write!(self, ":")?;
        }
        Ok(MapFormatter {
            f: EntryWrapper::new(self.f),
        })
    }

    fn serialize_seq_product(self, len: usize) -> Result<Self::SerializeSeqProduct, Self::Error> {
        // Delegate to named products handling of element formatting.
        self.serialize_named_product(len).map(|inner| SeqFormatter { inner })
    }

    fn serialize_named_product(mut self, _len: usize) -> Result<Self::SerializeNamedProduct, Self::Error> {
        write!(self, "(")?; // Closed via `.end()`.
        Ok(NamedFormatter {
            f: EntryWrapper::new(self.f),
            idx: 0,
        })
    }

    fn serialize_variant<T: ser::Serialize + ?Sized>(
        mut self,
        _tag: u8,
        name: Option<&str>,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        write!(self, "(")?;
        EntryWrapper::<','>::new(self.f.as_mut()).entry(|mut f| {
            if let Some(name) = name {
                write!(f, "{}", name)?;
            }
            write!(f, " = ")?;
            value.serialize(SatnFormatter { f })?;
            Ok(())
        })?;
        write!(self, ")")
    }

    unsafe fn serialize_bsatn(self, ty: &crate::AlgebraicType, bsatn: &[u8]) -> Result<Self::Ok, Self::Error> {
        // TODO(Centril): Consider instead deserializing the `bsatn` through a
        // deserializer that serializes into `self` directly.

        // First convert the BSATN to an `AlgebraicValue`.
        // SAFETY: Forward caller requirements of this method to that we are calling.
        let res = unsafe { ValueSerializer.serialize_bsatn(ty, bsatn) };
        let value = res.unwrap_or_else(|x| match x {});

        // Then serialize that.
        value.serialize(self)
    }

    unsafe fn serialize_bsatn_in_chunks<'c, I: Clone + Iterator<Item = &'c [u8]>>(
        self,
        ty: &crate::AlgebraicType,
        total_bsatn_len: usize,
        bsatn: I,
    ) -> Result<Self::Ok, Self::Error> {
        // TODO(Centril): Unlike above, in this case we must at minimum concatenate `bsatn`
        // before we can do the piping mentioned above, but that's better than
        // serializing to `AlgebraicValue` first, so consider that.

        // First convert the BSATN to an `AlgebraicValue`.
        // SAFETY: Forward caller requirements of this method to that we are calling.
        let res = unsafe { ValueSerializer.serialize_bsatn_in_chunks(ty, total_bsatn_len, bsatn) };
        let value = res.unwrap_or_else(|x| match x {});

        // Then serialize that.
        value.serialize(self)
    }

    unsafe fn serialize_str_in_chunks<'c, I: Clone + Iterator<Item = &'c [u8]>>(
        self,
        total_len: usize,
        string: I,
    ) -> Result<Self::Ok, Self::Error> {
        // First convert the `string` to an `AlgebraicValue`.
        // SAFETY: Forward caller requirements of this method to that we are calling.
        let res = unsafe { ValueSerializer.serialize_str_in_chunks(total_len, string) };
        let value = res.unwrap_or_else(|x| match x {});

        // Then serialize that.
        // This incurs a very minor cost of branching on `AlgebraicValue::String`.
        value.serialize(self)
    }
}

/// Defines the SATN formatting for arrays.
struct ArrayFormatter<'a, 'b> {
    /// The formatter for each element separating elements by a `,`.
    f: EntryWrapper<'a, 'b, ','>,
}

impl<'a, 'b> ser::SerializeArray for ArrayFormatter<'a, 'b> {
    type Ok = ();
    type Error = SatnError;

    fn serialize_element<T: ser::Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        self.f.entry(|f| elem.serialize(SatnFormatter { f }).map_err(|e| e.0))?;
        Ok(())
    }

    fn end(mut self) -> Result<Self::Ok, Self::Error> {
        write!(self.f.fmt, "]")?;
        Ok(())
    }
}

/// Provides the data format for maps for SATN.
struct MapFormatter<'a, 'b> {
    /// The formatter for each element separating elements by a `,`.
    f: EntryWrapper<'a, 'b, ','>,
}

impl<'a, 'b> ser::SerializeMap for MapFormatter<'a, 'b> {
    type Ok = ();
    type Error = SatnError;

    fn serialize_entry<K: ser::Serialize + ?Sized, V: ser::Serialize + ?Sized>(
        &mut self,
        key: &K,
        value: &V,
    ) -> Result<(), Self::Error> {
        self.f.entry(|mut f| {
            key.serialize(SatnFormatter { f: f.as_mut() })?;
            f.write_str(": ")?;
            value.serialize(SatnFormatter { f })?;
            Ok(())
        })?;
        Ok(())
    }

    fn end(mut self) -> Result<Self::Ok, Self::Error> {
        write!(self.f.fmt, "]")?;
        Ok(())
    }
}

/// Provides the data format for unnamed products for SATN.
struct SeqFormatter<'a, 'b> {
    /// Delegates to the named format.
    inner: NamedFormatter<'a, 'b>,
}

impl<'a, 'b> ser::SerializeSeqProduct for SeqFormatter<'a, 'b> {
    type Ok = ();
    type Error = SatnError;

    fn serialize_element<T: ser::Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        ser::SerializeNamedProduct::serialize_element(&mut self.inner, None, elem)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        ser::SerializeNamedProduct::end(self.inner)
    }
}

/// Provides the data format for named products for SATN.
struct NamedFormatter<'a, 'b> {
    /// The formatter for each element separating elements by a `,`.
    f: EntryWrapper<'a, 'b, ','>,
    /// The index of the element.
    idx: usize,
}

impl<'a, 'b> ser::SerializeNamedProduct for NamedFormatter<'a, 'b> {
    type Ok = ();
    type Error = SatnError;

    fn serialize_element<T: ser::Serialize + ?Sized>(
        &mut self,
        name: Option<&str>,
        elem: &T,
    ) -> Result<(), Self::Error> {
        let res = self.f.entry(|mut f| {
            // Format the name or use the index if unnamed.
            if let Some(name) = name {
                write!(f, "{}", name)?;
            } else {
                write!(f, "{}", self.idx)?;
            }
            write!(f, " = ")?;
            elem.serialize(SatnFormatter { f })?;
            Ok(())
        });
        self.idx += 1;
        res?;
        Ok(())
    }

    fn end(mut self) -> Result<Self::Ok, Self::Error> {
        write!(self.f.fmt, ")")?;
        Ok(())
    }
}

/// An implementation of [`Serializer`](ser::Serializer)
/// that borrows from [`SatnFormatter`] except in `serialize_str`.
struct PsqlFormatter<'a, 'b>(SatnFormatter<'a, 'b>);

impl<'a, 'b> ser::Serializer for PsqlFormatter<'a, 'b> {
    type Ok = ();
    type Error = SatnError;
    type SerializeArray = ArrayFormatter<'a, 'b>;
    type SerializeMap = MapFormatter<'a, 'b>;
    type SerializeSeqProduct = SeqFormatter<'a, 'b>;
    type SerializeNamedProduct = NamedFormatter<'a, 'b>;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_bool(v)
    }
    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_u8(v)
    }
    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_u16(v)
    }
    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_u32(v)
    }
    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_u64(v)
    }
    fn serialize_u128(self, v: u128) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_u128(v)
    }
    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_i8(v)
    }
    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_i16(v)
    }
    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_i32(v)
    }
    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_i64(v)
    }
    fn serialize_i128(self, v: i128) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_i128(v)
    }
    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_f32(v)
    }
    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_f64(v)
    }

    fn serialize_str(mut self, v: &str) -> Result<Self::Ok, Self::Error> {
        self.0.f.write_str(v).map_err(SatnError)
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_bytes(v)
    }

    fn serialize_array(self, len: usize) -> Result<Self::SerializeArray, Self::Error> {
        self.0.serialize_array(len)
    }

    fn serialize_map(self, len: usize) -> Result<Self::SerializeMap, Self::Error> {
        self.0.serialize_map(len)
    }

    fn serialize_seq_product(self, len: usize) -> Result<Self::SerializeSeqProduct, Self::Error> {
        self.0.serialize_seq_product(len)
    }

    fn serialize_named_product(self, len: usize) -> Result<Self::SerializeNamedProduct, Self::Error> {
        self.0.serialize_named_product(len)
    }

    fn serialize_variant<T: ser::Serialize + ?Sized>(
        self,
        tag: u8,
        name: Option<&str>,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_variant(tag, name, value)
    }

    unsafe fn serialize_bsatn(self, ty: &crate::AlgebraicType, bsatn: &[u8]) -> Result<Self::Ok, Self::Error> {
        // SAFETY: Forward caller requirements of this method to that we are calling.
        unsafe { self.0.serialize_bsatn(ty, bsatn) }
    }

    unsafe fn serialize_bsatn_in_chunks<'c, I: Clone + Iterator<Item = &'c [u8]>>(
        self,
        ty: &crate::AlgebraicType,
        total_bsatn_len: usize,
        bsatn: I,
    ) -> Result<Self::Ok, Self::Error> {
        // SAFETY: Forward caller requirements of this method to that we are calling.
        unsafe { self.0.serialize_bsatn_in_chunks(ty, total_bsatn_len, bsatn) }
    }

    unsafe fn serialize_str_in_chunks<'c, I: Clone + Iterator<Item = &'c [u8]>>(
        self,
        total_len: usize,
        string: I,
    ) -> Result<Self::Ok, Self::Error> {
        // SAFETY: Forward caller requirements of this method to that we are calling.
        unsafe { self.0.serialize_str_in_chunks(total_len, string) }
    }
}
