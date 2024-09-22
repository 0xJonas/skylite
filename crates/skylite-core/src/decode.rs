// This module is the counterpart to `generate/encode.rs` in skylite-proc.

#![allow(non_snake_case)]
use skylite_compress::Decoder;
pub use skylite_compress::decode_fibonacci;

pub trait Deserialize {
    fn deserialize(decoder: &mut dyn Decoder) -> Self;
}

fn read_bytes<const BYTES: usize>(decoder: &mut dyn Decoder) -> [u8; BYTES] {
    let mut out = [0; BYTES];
    for byte in 0..BYTES {
        for _ in 0..8 {
            out[byte] <<= 1;
            out[byte] |= decoder.decode_bit() as u8;
        }
    }
    out
}

macro_rules! deserialize_for_primitive {
    ($typename:ident, $bytes:expr) => {
        impl Deserialize for $typename {
            fn deserialize(decoder: &mut dyn Decoder) -> $typename {
                $typename::from_be_bytes(read_bytes::<$bytes>(decoder))
            }
        }
    };
}

deserialize_for_primitive!(u8, 1);
deserialize_for_primitive!(u16, 2);
deserialize_for_primitive!(u32, 4);
deserialize_for_primitive!(i8, 1);
deserialize_for_primitive!(i16, 2);
deserialize_for_primitive!(i32, 4);
deserialize_for_primitive!(f32, 4);
deserialize_for_primitive!(f64, 8);

impl<T: Deserialize> Deserialize for Vec<T> {

    fn deserialize(decoder: &mut dyn Decoder) -> Vec<T> {
        let len = decode_fibonacci(decoder);
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
        let len = decode_fibonacci(decoder);
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
        decoder.decode_bit()
    }
}

#[cfg(test)]
mod tests {
    use skylite_compress::make_decoder;
    use super::Deserialize;


    #[test]
    fn test_deserialize() {
        // Should be the same as the result in the test from encode.rs
        let input = vec![
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

        assert_eq!(Vec::<(i32, i32)>::deserialize(decoder.as_mut()), vec![(5, 10), (15, 20), (25, 30)]);
    }
}
