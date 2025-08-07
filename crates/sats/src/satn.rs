use crate::time_duration::TimeDuration;
use crate::timestamp::Timestamp;
use crate::{i256, u256, AlgebraicValue, ProductValue, Serialize, SumValue, ValueWithType};
use crate::{ser, ProductType, ProductTypeElement};
use core::fmt;
use core::fmt::Write as _;
use derive_more::{Display, From, Into};
use std::borrow::Cow;
use std::marker::PhantomData;

/// An extension trait for [`Serialize`] providing formatting methods.
pub trait Satn: ser::Serialize {
    /// Formats the value using the SATN data format into the formatter `f`.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Writer::with(f, |f| self.serialize(SatnFormatter { f }))?;
        Ok(())
    }

    /// Formats the value using the postgres SATN(PsqlFormatter { f }, /* PsqlType */) formatter `f`.
    fn fmt_psql(&self, f: &mut fmt::Formatter, ty: &PsqlType<'_>) -> fmt::Result {
        Writer::with(f, |f| {
            self.serialize(TypedSerializer {
                ty,
                f: &mut SqlFormatter {
                    fmt: SatnFormatter { f },
                    ty,
                },
            })
        })?;
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
pub struct PsqlWrapper<'a, T: ?Sized> {
    pub ty: PsqlType<'a>,
    pub value: T,
}

impl<T: ?Sized> PsqlWrapper<'_, T> {
    /// Converts `&T` to `&PsqlWrapper<T>`.
    pub fn from_ref(t: &T) -> &Self {
        // SAFETY: `repr(transparent)` turns the ABI of `T`
        // into the same as `Self` so we can also cast `&T` to `&Self`.
        unsafe { &*(t as *const T as *const Self) }
    }
}

impl<T: Satn + ?Sized> fmt::Display for PsqlWrapper<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt_psql(f, &self.ty)
    }
}

impl<T: Satn + ?Sized> fmt::Debug for PsqlWrapper<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt_psql(f, &self.ty)
    }
}

/// Wraps a writer for formatting lists separated by `SEP` into it.
struct EntryWrapper<'a, 'f, const SEP: char> {
    /// The writer we're formatting into.
    fmt: Writer<'a, 'f>,
    /// Whether there were any fields.
    /// Initially `false` and then `true` after calling [`.entry(..)`](EntryWrapper::entry).
    has_fields: bool,
}

impl<'a, 'f, const SEP: char> EntryWrapper<'a, 'f, SEP> {
    /// Constructs the entry wrapper using the writer `fmt`.
    fn new(fmt: Writer<'a, 'f>) -> Self {
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
enum Writer<'a, 'f> {
    /// Uses the standard library's formatter i.e. plain formatting.
    Normal(&'a mut fmt::Formatter<'f>),
    /// Uses indented formatting.
    Pretty(IndentedWriter<'a, 'f>),
}

impl<'f> Writer<'_, 'f> {
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
    fn as_mut(&mut self) -> Writer<'_, 'f> {
        match self {
            Writer::Normal(f) => Writer::Normal(f),
            Writer::Pretty(f) => Writer::Pretty(f.as_mut()),
        }
    }
}

/// A formatter that adds decoration atop of the standard library's formatter.
struct IndentedWriter<'a, 'f> {
    f: &'a mut fmt::Formatter<'f>,
    state: &'a mut IndentState,
}

/// The indentation state.
struct IndentState {
    /// Number of tab indentations to make.
    indent: u32,
    /// Whether we were last on a newline.
    on_newline: bool,
}

impl<'f> IndentedWriter<'_, 'f> {
    /// Returns a sub-writer without moving `self`.
    fn as_mut(&mut self) -> IndentedWriter<'_, 'f> {
        IndentedWriter {
            f: self.f,
            state: self.state,
        }
    }
}

impl fmt::Write for IndentedWriter<'_, '_> {
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

impl fmt::Write for Writer<'_, '_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        match self {
            Writer::Normal(f) => f.write_str(s),
            Writer::Pretty(f) => f.write_str(s),
        }
    }
}

/// Provides the SATN data format implementing [`Serializer`](ser::Serializer).
struct SatnFormatter<'a, 'f> {
    /// The sink / writer / output / formatter.
    f: Writer<'a, 'f>,
}

impl SatnFormatter<'_, '_> {
    fn ser_variant<T: ser::Serialize + ?Sized>(
        &mut self,
        _tag: u8,
        name: Option<&str>,
        value: &T,
    ) -> Result<(), SatnError> {
        write!(self, "(")?;
        EntryWrapper::<','>::new(self.f.as_mut()).entry(|mut f| {
            if let Some(name) = name {
                write!(f, "{name}")?;
            }
            write!(f, " = ")?;
            value.serialize(SatnFormatter { f })?;
            Ok(())
        })?;
        write!(self, ")")?;

        Ok(())
    }
}
/// An error occurred during serialization to the SATS data format.
#[derive(From, Into)]
pub struct SatnError(fmt::Error);

impl ser::Error for SatnError {
    fn custom<T: fmt::Display>(_msg: T) -> Self {
        Self(fmt::Error)
    }
}

impl SatnFormatter<'_, '_> {
    /// Writes `args` formatted to `self`.
    #[inline(always)]
    fn write_fmt(&mut self, args: fmt::Arguments) -> Result<(), SatnError> {
        self.f.write_fmt(args)?;
        Ok(())
    }
}

impl<'a, 'f> ser::Serializer for SatnFormatter<'a, 'f> {
    type Ok = ();
    type Error = SatnError;
    type SerializeArray = ArrayFormatter<'a, 'f>;
    type SerializeSeqProduct = SeqFormatter<'a, 'f>;
    type SerializeNamedProduct = NamedFormatter<'a, 'f>;

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
    fn serialize_u256(mut self, v: u256) -> Result<Self::Ok, Self::Error> {
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
    fn serialize_i256(mut self, v: i256) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }
    fn serialize_f32(mut self, v: f32) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }
    fn serialize_f64(mut self, v: f64) -> Result<Self::Ok, Self::Error> {
        write!(self, "{v}")
    }

    fn serialize_str(mut self, v: &str) -> Result<Self::Ok, Self::Error> {
        write!(self, "\"{v}\"")
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
        tag: u8,
        name: Option<&str>,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.ser_variant(tag, name, value)
    }
}

/// Defines the SATN formatting for arrays.
struct ArrayFormatter<'a, 'f> {
    /// The formatter for each element separating elements by a `,`.
    f: EntryWrapper<'a, 'f, ','>,
}

impl ser::SerializeArray for ArrayFormatter<'_, '_> {
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

/// Provides the data format for unnamed products for SATN.
struct SeqFormatter<'a, 'f> {
    /// Delegates to the named format.
    inner: NamedFormatter<'a, 'f>,
}

impl ser::SerializeSeqProduct for SeqFormatter<'_, '_> {
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
struct NamedFormatter<'a, 'f> {
    /// The formatter for each element separating elements by a `,`.
    f: EntryWrapper<'a, 'f, ','>,
    /// The index of the element.
    idx: usize,
}

impl ser::SerializeNamedProduct for NamedFormatter<'_, '_> {
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
                write!(f, "{name}")?;
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

/// Which client is used to format the `SQL` output?
#[derive(PartialEq, Copy, Clone, Debug)]
pub enum PsqlClient {
    SpacetimeDB,
    Postgres,
}

/// How format of the `SQL` output?
#[derive(Debug, Copy, Clone, PartialEq, Display)]
pub enum PsqlPrintFmt {
    /// Print as `hex` format
    Hex,
    /// Print as [`Timestamp`] format
    Timestamp,
    /// Print as [`TimeDuration`] format
    Duration,
    /// Print as `Satn` format
    Satn,
}

impl PsqlPrintFmt {
    pub fn is_special(&self) -> bool {
        self != &PsqlPrintFmt::Satn
    }
    /// Returns if the type is a special type
    ///
    /// Is required to check both the enclosing type and the inner element type
    pub fn use_fmt(tuple: &ProductType, field: &ProductTypeElement, name: Option<&str>) -> PsqlPrintFmt {
        if tuple.is_identity()
            || tuple.is_connection_id()
            || field.algebraic_type.is_identity()
            || field.algebraic_type.is_connection_id()
            || name.map(ProductType::is_identity_tag).unwrap_or_default()
            || name.map(ProductType::is_connection_id_tag).unwrap_or_default()
        {
            return PsqlPrintFmt::Hex;
        };

        if tuple.is_timestamp()
            || field.algebraic_type.is_timestamp()
            || name.map(ProductType::is_timestamp_tag).unwrap_or_default()
        {
            return PsqlPrintFmt::Timestamp;
        };

        if tuple.is_time_duration()
            || field.algebraic_type.is_time_duration()
            || name.map(ProductType::is_time_duration_tag).unwrap_or_default()
        {
            return PsqlPrintFmt::Duration;
        };

        PsqlPrintFmt::Satn
    }
}

/// A wrapper that remember the `header` of the tuple/struct and the current field
#[derive(Debug, Clone)]
pub struct PsqlType<'a> {
    /// The client used to format the output
    pub client: PsqlClient,
    /// The header of the tuple/struct
    pub tuple: &'a ProductType,
    /// The current field
    pub field: &'a ProductTypeElement,
    /// The index of the field in the tuple/struct
    pub idx: usize,
}

impl PsqlType<'_> {
    /// Returns if the type is a special type
    ///
    /// Is required to check both the enclosing type and the inner element type
    pub fn use_fmt(&self) -> PsqlPrintFmt {
        PsqlPrintFmt::use_fmt(self.tuple, self.field, None)
    }
}

/// An implementation of [`Serializer`](ser::Serializer) for `SQL` output.
struct SqlFormatter<'a, 'f> {
    fmt: SatnFormatter<'a, 'f>,
    ty: &'a PsqlType<'a>,
}

/// A trait for writing values, after the special types has been determined.
///
/// This is used to write values that could have different representations depending on the output format,
/// as defined by [`PsqlClient`] and [`PsqlPrintFmt`].
pub trait TypedWriter {
    type Error: ser::Error;

    /// Writes a value using [`ser::Serializer`]
    fn write<W: fmt::Display>(&mut self, value: W) -> Result<(), Self::Error>;

    // Values that need special handling:

    fn write_bool(&mut self, value: bool) -> Result<(), Self::Error>;
    fn write_string(&mut self, value: &str) -> Result<(), Self::Error>;
    fn write_bytes(&mut self, value: &[u8]) -> Result<(), Self::Error>;
    fn write_hex(&mut self, value: &[u8]) -> Result<(), Self::Error>;
    fn write_timestamp(&mut self, value: Timestamp) -> Result<(), Self::Error>;
    fn write_duration(&mut self, value: TimeDuration) -> Result<(), Self::Error>;
    /// Writes a value as an alternative record format, e.g., for use `JSON` inside `SQL`.
    fn write_alt_record(
        &mut self,
        _ty: &PsqlType,
        _value: &ValueWithType<'_, ProductValue>,
    ) -> Result<bool, Self::Error> {
        Ok(false)
    }

    fn write_record(
        &mut self,
        fields: Vec<(Cow<str>, PsqlType, ValueWithType<AlgebraicValue>)>,
    ) -> Result<(), Self::Error>;

    fn write_variant(
        &mut self,
        tag: u8,
        ty: PsqlType,
        name: Option<&str>,
        value: ValueWithType<AlgebraicValue>,
    ) -> Result<(), Self::Error>;
}

/// A formatter for arrays that uses the `TypedWriter` trait to write elements.
pub struct TypedArrayFormatter<'a, 'f, F> {
    ty: &'a PsqlType<'a>,
    f: &'f mut F,
}

impl<F: TypedWriter> ser::SerializeArray for TypedArrayFormatter<'_, '_, F> {
    type Ok = ();
    type Error = F::Error;

    fn serialize_element<T: ser::Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        elem.serialize(TypedSerializer { ty: self.ty, f: self.f })?;
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

/// A formatter for sequences that uses the `TypedWriter` trait to write elements.
pub struct TypedSeqFormatter<'a, 'f, F> {
    ty: &'a PsqlType<'a>,
    f: &'f mut F,
}

impl<F: TypedWriter> ser::SerializeSeqProduct for TypedSeqFormatter<'_, '_, F> {
    type Ok = ();
    type Error = F::Error;

    fn serialize_element<T: ser::Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        elem.serialize(TypedSerializer { ty: self.ty, f: self.f })?;
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

/// A formatter for named products that uses the `TypedWriter` trait to write elements.
pub struct TypedNamedProductFormatter<F> {
    f: PhantomData<F>,
}

impl<F: TypedWriter> ser::SerializeNamedProduct for TypedNamedProductFormatter<F> {
    type Ok = ();
    type Error = F::Error;

    fn serialize_element<T: ser::Serialize + ?Sized>(
        &mut self,
        _name: Option<&str>,
        _elem: &T,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

/// A serializer that uses the `TypedWriter` trait to serialize values
pub struct TypedSerializer<'a, 'f, F> {
    pub ty: &'a PsqlType<'a>,
    pub f: &'f mut F,
}

impl<'a, 'f, F: TypedWriter> ser::Serializer for TypedSerializer<'a, 'f, F> {
    type Ok = ();
    type Error = F::Error;
    type SerializeArray = TypedArrayFormatter<'a, 'f, F>;
    type SerializeSeqProduct = TypedSeqFormatter<'a, 'f, F>;
    type SerializeNamedProduct = TypedNamedProductFormatter<F>;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        self.f.write_bool(v)
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        self.f.write(v)
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        self.f.write(v)
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        self.f.write(v)
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        self.f.write(v)
    }

    fn serialize_u128(self, v: u128) -> Result<Self::Ok, Self::Error> {
        match self.ty.use_fmt() {
            PsqlPrintFmt::Hex => self.f.write_hex(&v.to_be_bytes()),
            _ => self.f.write(v),
        }
    }

    fn serialize_u256(self, v: u256) -> Result<Self::Ok, Self::Error> {
        match self.ty.use_fmt() {
            PsqlPrintFmt::Hex => self.f.write_hex(&v.to_be_bytes()),
            _ => self.f.write(v),
        }
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        self.f.write(v)
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        self.f.write(v)
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        self.f.write(v)
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        match self.ty.use_fmt() {
            PsqlPrintFmt::Duration => self.f.write_duration(TimeDuration::from_micros(v)),
            PsqlPrintFmt::Timestamp => self.f.write_timestamp(Timestamp::from_micros_since_unix_epoch(v)),
            _ => self.f.write(v),
        }
    }

    fn serialize_i128(self, v: i128) -> Result<Self::Ok, Self::Error> {
        self.f.write(v)
    }

    fn serialize_i256(self, v: i256) -> Result<Self::Ok, Self::Error> {
        self.f.write(v)
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        self.f.write(v)
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        self.f.write(v)
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        self.f.write_string(v)
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        if self.ty.use_fmt() == PsqlPrintFmt::Satn {
            self.f.write_hex(v)
        } else {
            self.f.write_bytes(v)
        }
    }

    fn serialize_array(self, _len: usize) -> Result<Self::SerializeArray, Self::Error> {
        Ok(TypedArrayFormatter { ty: self.ty, f: self.f })
    }

    fn serialize_seq_product(self, _len: usize) -> Result<Self::SerializeSeqProduct, Self::Error> {
        Ok(TypedSeqFormatter { ty: self.ty, f: self.f })
    }

    fn serialize_named_product(self, _len: usize) -> Result<Self::SerializeNamedProduct, Self::Error> {
        unreachable!("This should never be called, use `serialize_named_product_raw` instead.");
    }

    fn serialize_named_product_raw(self, value: &ValueWithType<'_, ProductValue>) -> Result<Self::Ok, Self::Error> {
        let val = &value.val.elements;
        assert_eq!(val.len(), value.ty().elements.len());
        // If the value is a special type, we can write it directly
        if self.ty.use_fmt().is_special() {
            // Is a nested product type?
            // We need to check for both the  enclosing(`self.ty`) type and the inner element type.
            let (tuple, field) = if let Some(product) = self.ty.field.algebraic_type.as_product() {
                (product, &product.elements[0])
            } else {
                (self.ty.tuple, self.ty.field)
            };
            return value.val.serialize(TypedSerializer {
                ty: &PsqlType {
                    client: self.ty.client,
                    tuple,
                    field,
                    idx: self.ty.idx,
                },
                f: self.f,
            });
        }
        // Allow to switch to an alternative record format, for example to write a `JSON` record.
        if self.f.write_alt_record(self.ty, value)? {
            return Ok(());
        }
        let mut record = Vec::with_capacity(val.len());

        for (idx, (val, field)) in val.iter().zip(&*value.ty().elements).enumerate() {
            let ty = PsqlType {
                client: self.ty.client,
                tuple: value.ty(),
                field,
                idx,
            };
            record.push((
                field
                    .name()
                    .map(Cow::from)
                    .unwrap_or_else(|| Cow::from(format!("col_{idx}"))),
                ty,
                value.with(&field.algebraic_type, val),
            ));
        }
        self.f.write_record(record)
    }

    fn serialize_variant_raw(self, sum: &ValueWithType<'_, SumValue>) -> Result<Self::Ok, Self::Error> {
        let sv = sum.value();
        let (tag, val) = (sv.tag, &*sv.value);
        let var_ty = &sum.ty().variants[tag as usize]; // Extract the variant type by tag.
        let product = ProductType::from([var_ty.algebraic_type.clone()]);
        let ty = PsqlType {
            client: self.ty.client,
            tuple: &product,
            field: &product.elements[0],
            idx: 0,
        };
        self.f
            .write_variant(tag, ty, var_ty.name(), sum.with(&var_ty.algebraic_type, val))
    }

    fn serialize_variant<T: Serialize + ?Sized>(
        self,
        _tag: u8,
        _name: Option<&str>,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        unreachable!("Use `serialize_variant_raw` instead.");
    }
}

impl TypedWriter for SqlFormatter<'_, '_> {
    type Error = SatnError;

    fn write<W: fmt::Display>(&mut self, value: W) -> Result<(), Self::Error> {
        write!(self.fmt, "{value}")
    }

    fn write_bool(&mut self, value: bool) -> Result<(), Self::Error> {
        write!(self.fmt, "{value}")
    }

    fn write_string(&mut self, value: &str) -> Result<(), Self::Error> {
        write!(self.fmt, "\"{value}\"")
    }

    fn write_bytes(&mut self, value: &[u8]) -> Result<(), Self::Error> {
        write!(self.fmt, "0x{}", hex::encode(value))
    }

    fn write_hex(&mut self, value: &[u8]) -> Result<(), Self::Error> {
        write!(self.fmt, "0x{}", hex::encode(value))
    }

    fn write_timestamp(&mut self, value: Timestamp) -> Result<(), Self::Error> {
        write!(self.fmt, "{}", value.to_rfc3339().unwrap())
    }

    fn write_duration(&mut self, value: TimeDuration) -> Result<(), Self::Error> {
        match self.ty.client {
            PsqlClient::SpacetimeDB => write!(self.fmt, "{value}"),
            PsqlClient::Postgres => write!(self.fmt, "{}", value.to_iso8601()),
        }
    }

    fn write_record(
        &mut self,
        fields: Vec<(Cow<str>, PsqlType<'_>, ValueWithType<AlgebraicValue>)>,
    ) -> Result<(), Self::Error> {
        let (start, sep, end, quote) = match self.ty.client {
            PsqlClient::SpacetimeDB => ("(", " =", ")", ""),
            PsqlClient::Postgres => ("{", ":", "}", "\""),
        };
        write!(self.fmt, "{start}")?;
        for (idx, (name, ty, value)) in fields.into_iter().enumerate() {
            if idx > 0 {
                write!(self.fmt, ", ")?;
            }
            write!(self.fmt, "{quote}{name}{quote}{sep} ")?;

            // Serialize the value
            value.serialize(TypedSerializer { ty: &ty, f: self })?;
        }
        write!(self.fmt, "{end}")?;
        Ok(())
    }

    fn write_variant(
        &mut self,
        tag: u8,
        ty: PsqlType,
        name: Option<&str>,
        value: ValueWithType<AlgebraicValue>,
    ) -> Result<(), Self::Error> {
        self.write_record(vec![(
            name.map(Cow::from).unwrap_or_else(|| Cow::from(format!("col_{tag}"))),
            ty,
            value,
        )])
    }
}
