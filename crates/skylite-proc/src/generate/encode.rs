// This module is the counterpart to `decode.rs` in skylite-core.

#![allow(non_snake_case)]

use skylite_compress::{compress, encode_fibonacci, CompressionMethods};

pub trait Serialize {
    fn serialize(&self, buffer: &mut ByteBuffer);
}

pub struct ByteBuffer {
    buffer: Vec<u8>,
    current_bit: u8
}

impl ByteBuffer {

    pub fn new() -> ByteBuffer {
        ByteBuffer {
            buffer: Vec::new(),
            current_bit: 0
        }
    }

    fn write_bit(&mut self, val: bool) {
        if self.current_bit == 0 {
            self.buffer.push(0);
        }
        *self.buffer.last_mut().unwrap() |= (val as u8) << (7 - self.current_bit);
        self.current_bit = (self.current_bit + 1) & 0x7;
    }

    fn write_byte(&mut self, byte: u8) {
        if self.current_bit == 0 {
            self.buffer.push(byte);
        } else {
            let head = byte >> self.current_bit;
            let tail: u8 = (((byte as u16) << (8 - self.current_bit)) & 0xff).try_into().unwrap();
            *self.buffer.last_mut().unwrap() |= head;
            self.buffer.push(tail);
        }
    }

    pub fn write<T: Serialize>(&mut self, val: T) {
        val.serialize(self);
    }

    pub fn encode(self) -> Vec<u8> {
        let (out, _reports) = compress(&self.buffer, &[CompressionMethods::LZ77, CompressionMethods::RC]);
        // TODO: print reports to stdout
        out
    }
}

macro_rules! serialize_for_primitive {
    ($typename:ident) => {
        impl Serialize for $typename {
            fn serialize(&self, buffer: &mut ByteBuffer) {
                let bytes = self.to_be_bytes();
                bytes.iter().for_each(|b| buffer.write_byte(*b));
            }
        }
    };
}

serialize_for_primitive!(u8);
serialize_for_primitive!(u16);
serialize_for_primitive!(u32);
serialize_for_primitive!(i8);
serialize_for_primitive!(i16);
serialize_for_primitive!(i32);
serialize_for_primitive!(f32);
serialize_for_primitive!(f64);

impl Serialize for bool {
    fn serialize(&self, buffer: &mut ByteBuffer) {
        buffer.write_bit(*self);
    }
}

impl<T: Serialize> Serialize for &[T] {

    fn serialize(&self, buffer: &mut ByteBuffer) {
        let len_bits = encode_fibonacci(self.len());
        len_bits.into_iter().for_each(|bit| buffer.write_bit(bit));
        for item in *self {
            item.serialize(buffer);
        }
    }
}

impl Serialize for &str {
    fn serialize(&self, buffer: &mut ByteBuffer) {
        self.as_bytes().serialize(buffer);
    }
}

macro_rules! serialize_for_tuple {
    ($($t:ident),+) => {
        impl<$($t: Serialize),+> Serialize for ($($t),+,) {
            fn serialize(&self, buffer: &mut ByteBuffer) {
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

#[cfg(test)]
mod tests {
    use super::ByteBuffer;

    #[test]
    fn test_serialize() {
        let mut buffer = ByteBuffer::new();

        buffer.write(0x12_u8);
        buffer.write(0x1234_u16);
        buffer.write(0x12345678_u32);

        buffer.write(-0x12_i8);
        buffer.write(-0x1234_i16);
        buffer.write(-0x12345678_i32);

        buffer.write(0.5_f32);
        buffer.write(0.5_f64);

        buffer.write(true);
        buffer.write(false);

        buffer.write("A Test! 🎵");
        buffer.write((true, 5));

        let data = [(5, 10), (15, 20), (25, 30)];
        buffer.write(&data[..]);

        let encoded = buffer.encode();
        let expected = vec![
            98, 23, 137, 9,
            26, 9, 26, 43,
            60, 119, 118, 230,
            118, 229, 212, 196,
            31, 136, 157, 1,
            131, 255, 0, 202,
            44, 136, 192, 208,
            72, 21, 25, 92,
            221, 8, 72, 60,
            39, 227, 173, 74,
            209, 12, 29, 189,
            90, 14, 33, 165,
            147, 120, 72, 48,
            140, 153, 2, 23,
            32, 192
        ];
        assert_eq!(encoded, expected);
    }
}
