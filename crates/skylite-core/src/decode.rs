// This module is the counterpart to `generate/encode.rs` in skylite-proc.

#![allow(non_snake_case)]
use skylite_compress::Decoder;

use crate::nodes::Node;

pub trait Deserialize {
    fn deserialize(decoder: &mut dyn Decoder) -> Self;
}

macro_rules! deserialize_for_primitive {
    ($typename:ident, $bytes:expr) => {
        impl Deserialize for $typename {
            fn deserialize(decoder: &mut dyn Decoder) -> $typename {
                let mut data = [0; $bytes];
                for i in 0..$bytes {
                    data[i] = decoder.decode_u8();
                }
                $typename::from_ne_bytes(data)
            }
        }
    };
}

deserialize_for_primitive!(u8, 1);
deserialize_for_primitive!(u16, 2);
deserialize_for_primitive!(u32, 4);
deserialize_for_primitive!(u64, 8);
deserialize_for_primitive!(i8, 1);
deserialize_for_primitive!(i16, 2);
deserialize_for_primitive!(i32, 4);
deserialize_for_primitive!(i64, 8);
deserialize_for_primitive!(f32, 4);
deserialize_for_primitive!(f64, 8);

pub fn read_varint(decoder: &mut dyn Decoder) -> usize {
    let mut out = 0;
    loop {
        let byte = decoder.decode_u8();
        out = (out << 7) + (byte & 0x7f) as usize;
        if byte < 0x80 {
            break;
        }
    }
    out
}

impl<T: Deserialize> Deserialize for Vec<T> {
    fn deserialize(decoder: &mut dyn Decoder) -> Vec<T> {
        let len = read_varint(decoder);
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            out.push(<T as Deserialize>::deserialize(decoder));
        }
        out
    }
}

macro_rules! deserialize_for_tuple {
    ($($t:ident),+) => {
        impl<$($t),+> Deserialize for ($($t),+,)
        where $($t: Deserialize),+
        {
            fn deserialize(decoder: &mut dyn Decoder) -> ($($t),+,) {
                $(
                    let $t = <$t as Deserialize>::deserialize(decoder);
                )+
                ($($t),+,)
            }
        }
    };
}

deserialize_for_tuple!(T1);
deserialize_for_tuple!(T1, T2);
deserialize_for_tuple!(T1, T2, T3);
deserialize_for_tuple!(T1, T2, T3, T4);
deserialize_for_tuple!(T1, T2, T3, T4, T5);
deserialize_for_tuple!(T1, T2, T3, T4, T5, T6);
deserialize_for_tuple!(T1, T2, T3, T4, T5, T6, T7);
deserialize_for_tuple!(T1, T2, T3, T4, T5, T6, T7, T8);

impl Deserialize for String {
    fn deserialize(decoder: &mut dyn Decoder) -> Self {
        let len = read_varint(decoder);
        let bytes = (0..len)
            .map(|_| u8::deserialize(decoder))
            .collect::<Vec<u8>>();
        unsafe {
            // SAFETY: If the decoder is not desynced, the data
            // should originate from string.as_bytes(), so UTF-8
            // conformance is guaranteed.
            // If the decoder is desynced, we likely already hit
            // undefined behavior with other data.
            String::from_utf8_unchecked(bytes)
        }
    }
}

impl Deserialize for bool {
    fn deserialize(decoder: &mut dyn Decoder) -> Self {
        decoder.decode_u8() != 0
    }
}

impl<N: Node> Deserialize for N {
    fn deserialize(decoder: &mut dyn Decoder) -> Self {
        N::_private_decode(decoder)
    }
}

#[cfg(test)]
mod tests {
    use skylite_compress::make_decoder;

    use super::Deserialize;

    #[cfg(target_endian = "little")]
    #[test]
    fn test_deserialize() {
        // Should be the same as the result in the test from encode.rs
        let input = vec![
            3, 0, 1, 6, 18, 64, 232, 123, 221, 72, 188, 42, 225, 101, 167, 1, 99, 5, 16, 136, 132,
            81, 84, 227, 47, 35, 136, 157, 39, 32, 154, 217, 143, 168, 154, 148, 186, 68, 100, 167,
            101, 255, 159, 177, 3, 73, 26, 67, 16, 194, 36, 23, 87, 1, 248, 40, 2, 223, 20, 30,
            222, 82, 209,
        ];
        let mut decoder = make_decoder(&input);

        assert_eq!(u8::deserialize(decoder.as_mut()), 0x12_u8);
        assert_eq!(u16::deserialize(decoder.as_mut()), 0x1234_u16);
        assert_eq!(u32::deserialize(decoder.as_mut()), 0x12345678_u32);

        assert_eq!(i8::deserialize(decoder.as_mut()), -0x12_i8);
        assert_eq!(i16::deserialize(decoder.as_mut()), -0x1234_i16);
        assert_eq!(i32::deserialize(decoder.as_mut()), -0x12345678_i32);

        assert_eq!(f32::deserialize(decoder.as_mut()), 0.5_f32);
        assert_eq!(f64::deserialize(decoder.as_mut()), 0.5_f64);

        assert_eq!(bool::deserialize(decoder.as_mut()), true);
        assert_eq!(bool::deserialize(decoder.as_mut()), false);

        assert_eq!(String::deserialize(decoder.as_mut()), "A Test! 🎵");
        assert_eq!(<(bool, i32)>::deserialize(decoder.as_mut()), (true, 5));

        assert_eq!(
            Vec::<(i32, i32)>::deserialize(decoder.as_mut()),
            vec![(5, 10), (15, 20), (25, 30)]
        );
    }
}
