// This module is the counterpart to `decode.rs` in skylite-core.

#![allow(non_snake_case)]

use skylite_compress::{compress, CompressionMethods};

use crate::parse::values::TypedValue;

pub trait Serialize {
    fn serialize(&self, buffer: &mut CompressionBuffer);
}

pub struct CompressionBuffer {
    buffer: Vec<u8>
}

impl CompressionBuffer {

    pub fn new() -> CompressionBuffer {
        CompressionBuffer {
            buffer: Vec::new()
        }
    }

    fn write_byte(&mut self, byte: u8) {
        self.buffer.push(byte);
    }

    pub fn write_varint(&mut self, val: usize) {
        if val == 0 {
            self.write_byte(0);
            return;
        }

        let mut writes = val.ilog2() / 7;
        while writes > 1 {
            self.write_byte(((val >> (writes * 7)) & 0x7f | 0x80) as u8);
            writes -= 1;
        }
        self.write_byte((val & 0x7f) as u8);
    }

    pub fn encode(self) -> Vec<u8> {
        let (out, _reports) = compress(&self.buffer, &[CompressionMethods::LZ77, CompressionMethods::RC]);
        // for r in reports {
        //     println!("{}", r);
        // }
        // TODO: print reports to stdout
        out
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }
}

macro_rules! serialize_for_primitive {
    ($typename:ident) => {
        impl Serialize for $typename {
            fn serialize(&self, buffer: &mut CompressionBuffer) {
                let bytes = self.to_be_bytes();
                bytes.iter().for_each(|b| buffer.write_byte(*b));
            }
        }
    };
}

serialize_for_primitive!(u8);
serialize_for_primitive!(u16);
serialize_for_primitive!(u32);
serialize_for_primitive!(u64);
serialize_for_primitive!(i8);
serialize_for_primitive!(i16);
serialize_for_primitive!(i32);
serialize_for_primitive!(i64);
serialize_for_primitive!(f32);
serialize_for_primitive!(f64);

impl Serialize for bool {
    fn serialize(&self, buffer: &mut CompressionBuffer) {
        buffer.write_byte(*self as u8);
    }
}

impl<T: Serialize> Serialize for &[T] {

    fn serialize(&self, buffer: &mut CompressionBuffer) {
        buffer.write_varint(self.len());
        for item in *self {
            item.serialize(buffer);
        }
    }
}

impl Serialize for &str {
    fn serialize(&self, buffer: &mut CompressionBuffer) {
        self.as_bytes().serialize(buffer);
    }
}

macro_rules! serialize_for_tuple {
    ($($t:ident),+) => {
        impl<$($t: Serialize),+> Serialize for ($($t),+,) {
            fn serialize(&self, buffer: &mut CompressionBuffer) {
                let ($($t),+,) = self;
                $(
                    $t.serialize(buffer);
                )+
            }
        }
    };
}

serialize_for_tuple!(T1);
serialize_for_tuple!(T1, T2);
serialize_for_tuple!(T1, T2, T3);
serialize_for_tuple!(T1, T2, T3, T4);
serialize_for_tuple!(T1, T2, T3, T4, T5);
serialize_for_tuple!(T1, T2, T3, T4, T5, T6);
serialize_for_tuple!(T1, T2, T3, T4, T5, T6, T7);
serialize_for_tuple!(T1, T2, T3, T4, T5, T6, T7, Z8);

impl Serialize for TypedValue {
    fn serialize(&self, buffer: &mut CompressionBuffer) {
        match self {
            TypedValue::U8(v) => v.serialize(buffer),
            TypedValue::U16(v) => v.serialize(buffer),
            TypedValue::U32(v) => v.serialize(buffer),
            TypedValue::U64(v) => v.serialize(buffer),
            TypedValue::I8(v) => v.serialize(buffer),
            TypedValue::I16(v) => v.serialize(buffer),
            TypedValue::I32(v) => v.serialize(buffer),
            TypedValue::I64(v) => v.serialize(buffer),
            TypedValue::F32(v) => v.serialize(buffer),
            TypedValue::F64(v) => v.serialize(buffer),
            TypedValue::Bool(v) => v.serialize(buffer),
            TypedValue::String(v) => v.as_str().serialize(buffer),
            TypedValue::Tuple(v) => v.iter().for_each(|i| i.serialize(buffer)),
            TypedValue::Vec(v) => (&v[..]).serialize(buffer),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::generate::encode::Serialize;

    use super::CompressionBuffer;

    #[test]
    fn test_serialize() {
        let mut buffer = CompressionBuffer::new();

        0x12_u8.serialize(&mut buffer);
        0x1234_u16.serialize(&mut buffer);
        0x12345678_u32.serialize(&mut buffer);

        (-0x12_i8).serialize(&mut buffer);
        (-0x1234_i16).serialize(&mut buffer);
        (-0x12345678_i32).serialize(&mut buffer);

        0.5_f32.serialize(&mut buffer);
        0.5_f64.serialize(&mut buffer);

        true.serialize(&mut buffer);
        false.serialize(&mut buffer);

        "A Test! 🎵".serialize(&mut buffer);
        (true, 5).serialize(&mut buffer);

        let data = [(5, 10), (15, 20), (25, 30)];
        (&data[..]).serialize(&mut buffer);

        let encoded = buffer.encode();
        let expected = vec![
            3,
            0, 1, 6, 18,
            64, 232, 140, 25,
            133, 254, 148, 114,
            121, 80, 150, 38,
            203, 10, 145, 49,
            75, 159, 24, 235,
            88, 128, 173, 107,
            26, 106, 176, 79,
            150, 183, 6, 57,
            242, 188, 94, 113,
            15, 244, 245, 231,
            182, 250, 51, 110,
            98, 154, 5, 119,
            126, 131, 176, 116,
            178, 13, 45, 142,
            113, 4, 128
        ];
        assert_eq!(encoded, expected);
    }
}
