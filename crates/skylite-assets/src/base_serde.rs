use std::io::{Read, Write};

use crate::AssetError;

trait Serialize {
    fn serialize(&self, output: &mut impl Write) -> Result<(), AssetError>
    where
        Self: Sized;
}

macro_rules! serialize_for_primitive {
    ($t:ty) => {
        impl Serialize for $t {
            fn serialize(&self, output: &mut impl Write) -> Result<(), AssetError> {
                output.write(&self.to_le_bytes())?;
                Ok(())
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

impl Serialize for bool {
    fn serialize(&self, output: &mut impl Write) -> Result<(), AssetError> {
        if *self {
            1u8.serialize(output)?;
        } else {
            0u8.serialize(output)?;
        }
        Ok(())
    }
}

impl Serialize for String {
    fn serialize(&self, output: &mut impl Write) -> Result<(), AssetError> {
        let bytes = self.as_bytes();
        (bytes.len() as u32).serialize(output)?;
        output.write(&bytes)?;
        Ok(())
    }
}

impl<T: Serialize> Serialize for Vec<T> {
    fn serialize(&self, output: &mut impl Write) -> Result<(), AssetError> {
        (self.len() as u32).serialize(output)?;
        for elem in self {
            elem.serialize(output)?;
        }
        Ok(())
    }
}

macro_rules! serialize_for_tuple {
    ($($t:ident),+) => {
        impl<$($t),+> Serialize for ($($t),+,)
        where $($t: Serialize),+
        {
            #[allow(non_snake_case)]
            fn serialize(&self, output: &mut impl Write) -> Result<(), AssetError> {
                let ($($t,)+) = self;
                $(
                    $t.serialize(output)?;
                )+
                Ok(())
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
serialize_for_tuple!(T1, T2, T3, T4, T5, T6, T7, T8);

trait Deserialize {
    fn deserialize(input: &mut impl Read) -> Result<Self, AssetError>
    where
        Self: Sized;
}

macro_rules! deserialize_for_primitive {
    ($t:ty) => {
        impl Deserialize for $t {
            fn deserialize(input: &mut impl Read) -> Result<Self, AssetError> {
                let mut buf = [0u8; std::mem::size_of::<$t>()];
                input.read_exact(&mut buf)?;
                Ok(<$t>::from_le_bytes(buf))
            }
        }
    };
}

deserialize_for_primitive!(u8);
deserialize_for_primitive!(u16);
deserialize_for_primitive!(u32);
deserialize_for_primitive!(u64);
deserialize_for_primitive!(i8);
deserialize_for_primitive!(i16);
deserialize_for_primitive!(i32);
deserialize_for_primitive!(i64);

impl Deserialize for bool {
    fn deserialize(input: &mut impl Read) -> Result<Self, AssetError> {
        Ok(u8::deserialize(input)? != 0)
    }
}

impl Deserialize for String {
    fn deserialize(input: &mut impl Read) -> Result<Self, AssetError> {
        let len = u32::deserialize(input)? as usize;
        let mut buf = vec![0u8; len];
        input.read_exact(&mut buf)?;
        Ok(String::from_utf8(buf).map_err(|err| AssetError::OtherError(err.to_string()))?)
    }
}

impl<T: Deserialize> Deserialize for Vec<T> {
    fn deserialize(input: &mut impl Read) -> Result<Self, AssetError> {
        let len = u32::deserialize(input)? as usize;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::deserialize(input)?);
        }
        Ok(vec)
    }
}

macro_rules! deserialize_for_tuple {
    ($($t:ident),+) => {
        impl<$($t),+> Deserialize for ($($t),+,)
        where $($t: Deserialize),+
        {
            #[allow(non_snake_case)]
            fn deserialize(input: &mut impl Read) -> Result<($($t),+,), AssetError> {
                $(
                    let $t = <$t as Deserialize>::deserialize(input)?;
                )+
                Ok(($($t),+,))
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

// How is this four times as long as the Racket implementation?

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::base_serde::{Deserialize, Serialize};

    #[test]
    fn test_serde() {
        let mut data = Cursor::new(Vec::<u8>::new());

        5u8.serialize(&mut data).unwrap();
        10u16.serialize(&mut data).unwrap();
        15u32.serialize(&mut data).unwrap();
        20u64.serialize(&mut data).unwrap();
        (-5i8).serialize(&mut data).unwrap();
        (-10i16).serialize(&mut data).unwrap();
        (-15i32).serialize(&mut data).unwrap();
        (-20i64).serialize(&mut data).unwrap();
        false.serialize(&mut data).unwrap();
        "test".to_owned().serialize(&mut data).unwrap();
        vec![1i16, 2i16, 3i16, 4i16, 5i16]
            .serialize(&mut data)
            .unwrap();
        ("a".to_owned(), 5u8).serialize(&mut data).unwrap();

        assert_eq!(
            data.get_ref(),
            &[
                5, 10, 0, 15, 0, 0, 0, 20, 0, 0, 0, 0, 0, 0, 0, 251, 246, 255, 241, 255, 255, 255,
                236, 255, 255, 255, 255, 255, 255, 255, 0, 4, 0, 0, 0, 116, 101, 115, 116, 5, 0, 0,
                0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 1, 0, 0, 0, 97, 5
            ]
        );
        data.set_position(0);

        assert_eq!(u8::deserialize(&mut data).unwrap(), 5);
        assert_eq!(u16::deserialize(&mut data).unwrap(), 10);
        assert_eq!(u32::deserialize(&mut data).unwrap(), 15);
        assert_eq!(u64::deserialize(&mut data).unwrap(), 20);
        assert_eq!(i8::deserialize(&mut data).unwrap(), -5);
        assert_eq!(i16::deserialize(&mut data).unwrap(), -10);
        assert_eq!(i32::deserialize(&mut data).unwrap(), -15);
        assert_eq!(i64::deserialize(&mut data).unwrap(), -20);
        assert_eq!(bool::deserialize(&mut data).unwrap(), false);
        assert_eq!(String::deserialize(&mut data).unwrap(), "test");
        assert_eq!(
            Vec::<i16>::deserialize(&mut data).unwrap(),
            vec![1i16, 2i16, 3i16, 4i16, 5i16]
        );
        assert_eq!(
            <(String, u8)>::deserialize(&mut data).unwrap(),
            ("a".to_owned(), 5u8)
        );
    }
}
