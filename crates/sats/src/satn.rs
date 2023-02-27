use std::fmt::{self, Write as _};

use crate::ser;

pub trait Satn: ser::Serialize {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut state;
        let f = if f.alternate() {
            state = IndentState {
                indent: 0,
                on_newline: true,
            };
            Writer::Pretty(IndentedWriter { f, state: &mut state })
        } else {
            Writer::Normal(f)
        };
        self.serialize(SatnFormatter { f })?;
        Ok(())
    }
    fn to_satn(&self) -> String {
        Wrapper::from_ref(self).to_string()
    }
    fn to_satn_pretty(&self) -> String {
        format!("{:#}", Wrapper::from_ref(self))
    }
}

impl<T: ser::Serialize + ?Sized> Satn for T {}

#[repr(transparent)]
pub struct Wrapper<T: ?Sized>(pub T);

impl<T: ?Sized> Wrapper<T> {
    pub fn from_ref(t: &T) -> &Self {
        unsafe { &*(t as *const T as *const Wrapper<T>) }
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

struct EntryWrapper<'a, 'b, const SEP: char> {
    fmt: Writer<'a, 'b>,
    has_fields: bool,
}

impl<'a, 'b, const SEP: char> EntryWrapper<'a, 'b, SEP> {
    fn new(fmt: Writer<'a, 'b>) -> Self {
        Self { fmt, has_fields: false }
    }
    fn entry<F: FnOnce(Writer) -> fmt::Result>(&mut self, entry: F) -> fmt::Result {
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

enum Writer<'a, 'b> {
    Normal(&'a mut fmt::Formatter<'b>),
    Pretty(IndentedWriter<'a, 'b>),
}

impl<'a, 'b> Writer<'a, 'b> {
    fn as_mut(&mut self) -> Writer<'_, 'b> {
        match self {
            Writer::Normal(f) => Writer::Normal(f),
            Writer::Pretty(f) => Writer::Pretty(f.as_mut()),
        }
    }
}

struct IndentedWriter<'a, 'b> {
    f: &'a mut fmt::Formatter<'b>,
    state: &'a mut IndentState,
}

struct IndentState {
    indent: u32,
    on_newline: bool,
}

impl<'a, 'b> IndentedWriter<'a, 'b> {
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

struct SatnFormatter<'a, 'b> {
    f: Writer<'a, 'b>,
}

struct SatnError(fmt::Error);
impl From<SatnError> for fmt::Error {
    fn from(err: SatnError) -> Self {
        err.0
    }
}
impl From<fmt::Error> for SatnError {
    fn from(err: fmt::Error) -> Self {
        SatnError(err)
    }
}

impl ser::Error for SatnError {
    fn custom<T: fmt::Display>(_msg: T) -> Self {
        Self(fmt::Error)
    }
}

impl<'a, 'b> SatnFormatter<'a, 'b> {
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
        write!(self, "{:?}", v)
    }

    fn serialize_array(mut self, _len: usize) -> Result<Self::SerializeArray, Self::Error> {
        write!(self, "[")?;
        Ok(ArrayFormatter {
            f: EntryWrapper::new(self.f),
        })
    }

    fn serialize_map(mut self, len: usize) -> Result<Self::SerializeMap, Self::Error> {
        write!(self, "[")?;
        if len == 0 {
            write!(self, ":")?;
        }
        Ok(MapFormatter {
            f: EntryWrapper::new(self.f),
        })
    }

    fn serialize_seq_product(self, len: usize) -> Result<Self::SerializeSeqProduct, Self::Error> {
        self.serialize_named_product(len).map(|inner| SeqFormatter { inner })
    }

    fn serialize_named_product(mut self, _len: usize) -> Result<Self::SerializeNamedProduct, Self::Error> {
        write!(self, "(")?;
        Ok(NamedFormatter {
            f: EntryWrapper::new(self.f),
            i: 0,
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
}

struct ArrayFormatter<'a, 'b> {
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

struct MapFormatter<'a, 'b> {
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

struct SeqFormatter<'a, 'b> {
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

struct NamedFormatter<'a, 'b> {
    f: EntryWrapper<'a, 'b, ','>,
    i: usize,
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
            if let Some(name) = name {
                write!(f, "{}", name)?;
            } else {
                write!(f, "{}", self.i)?;
            }
            write!(f, " = ")?;
            elem.serialize(SatnFormatter { f })?;
            Ok(())
        });
        self.i += 1;
        res?;
        Ok(())
    }

    fn end(mut self) -> Result<Self::Ok, Self::Error> {
        write!(self.f.fmt, ")")?;
        Ok(())
    }
}
