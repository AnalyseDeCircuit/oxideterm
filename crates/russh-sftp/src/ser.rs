use bytes::{BufMut, Bytes, BytesMut};
use serde::ser::{
    SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};

use crate::error::Error;

pub struct Serializer {
    output: BytesMut,
}

/// Converting type to bytes according to protocol
pub fn to_bytes<T>(value: &T) -> Result<Bytes, Error>
where
    T: serde::Serialize + ?Sized,
{
    let mut serializer = Serializer {
        output: BytesMut::new(),
    };
    value.serialize(&mut serializer)?;
    Ok(serializer.output.freeze())
}

/// Serialization of a [`Vec`] without length.
pub fn data_serialize<S>(data: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut seq = serializer.serialize_seq(None)?;
    for byte in data {
        seq.serialize_element(byte)?;
    }
    seq.end()
}

impl<'a> serde::Serializer for &'a mut Serializer {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = &'a mut Serializer;
    type SerializeTuple = &'a mut Serializer;
    type SerializeTupleStruct = &'a mut Serializer;
    type SerializeTupleVariant = &'a mut Serializer;
    type SerializeMap = &'a mut Serializer;
    type SerializeStruct = &'a mut Serializer;
    type SerializeStructVariant = &'a mut Serializer;

    fn serialize_bool(self, _v: bool) -> Result<Self::Ok, Self::Error> {
        Err(Error::BadMessage("bool not supported".to_owned()))
    }

    fn serialize_i8(self, _v: i8) -> Result<Self::Ok, Self::Error> {
        Err(Error::BadMessage("i8 not supported".to_owned()))
    }

    fn serialize_i16(self, _v: i16) -> Result<Self::Ok, Self::Error> {
        Err(Error::BadMessage("i16 not supported".to_owned()))
    }

    fn serialize_i32(self, _v: i32) -> Result<Self::Ok, Self::Error> {
        Err(Error::BadMessage("i32 not supported".to_owned()))
    }

    fn serialize_i64(self, _v: i64) -> Result<Self::Ok, Self::Error> {
        Err(Error::BadMessage("i64 not supported".to_owned()))
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        self.output.put_u8(v);
        Ok(())
    }

    fn serialize_u16(self, _v: u16) -> Result<Self::Ok, Self::Error> {
        Err(Error::BadMessage("u16 not supported".to_owned()))
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        self.output.put_u32(v);
        Ok(())
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        self.output.put_u64(v);
        Ok(())
    }

    fn serialize_f32(self, _v: f32) -> Result<Self::Ok, Self::Error> {
        Err(Error::BadMessage("f32 not supported".to_owned()))
    }

    fn serialize_f64(self, _v: f64) -> Result<Self::Ok, Self::Error> {
        Err(Error::BadMessage("f64 not supported".to_owned()))
    }

    fn serialize_char(self, _v: char) -> Result<Self::Ok, Self::Error> {
        Err(Error::BadMessage("char not supported".to_owned()))
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        let bytes = v.as_bytes();
        self.output.put_u32(bytes.len() as u32);
        self.output.put_slice(bytes);
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        self.output.put_u32(v.len() as u32);
        self.output.put_slice(v);
        Ok(())
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Err(Error::BadMessage("unit not supported".to_owned()))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        Err(Error::BadMessage("unit struct not supported".to_owned()))
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.serialize_u32(variant_index)
    }

    fn serialize_newtype_struct<T>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        value.serialize(self)
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        if let Some(len) = len {
            self.output.put_u32(len as u32);
        }

        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(self)
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(self)
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(Error::BadMessage("struct variant not supported".to_owned()))
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

impl SerializeSeq for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeMap for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        key.serialize(&mut **self)
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeTuple for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeStruct for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeStructVariant for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeTupleStruct for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeTupleVariant for &mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}
