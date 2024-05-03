#[cfg(feature = "rc")]
mod range_coding;
use std::{mem::size_of, ops::{BitAnd, BitOrAssign, ShlAssign, Shr}};

use fibonacci_code::{decode_fibonacci, encode_fibonacci};
#[cfg(feature = "rc")]
use range_coding::*;

#[cfg(feature = "lz77")]
mod lempel_ziv;
#[cfg(feature = "lz77")]
use lempel_ziv::*;

mod bit_prediction;
mod fibonacci_code;

pub(crate) fn symbol_to_bits<T>(symbol: T) -> Vec<bool>
where
    T: BitAnd<T, Output = T> + Shr<usize, Output = T> + From<u8> + PartialEq + Copy,
{
    let zero = Into::<T>::into(0);
    let one = Into::<T>::into(1);
    (0..(size_of::<T>() * 8))
        .rev()
        .map(|b| ((symbol >> b) & one) != zero)
        .collect()
}

pub(crate) fn data_to_bits(data: &[u8]) -> Vec<bool> {
    data.iter()
        .flat_map(|v| (0..8)
            .rev()
            .map(move |bit| (v & (1 << bit)) != 0))
        .collect()
}

pub(crate) fn bits_to_data(bits: &[bool]) -> Vec<u8> {
    let mut bit_index = 0;
    let mut out = Vec::new();
    for b in bits {
        if bit_index == 0 {
            out.push(0);
        }
        *out.last_mut().unwrap() |= (*b as u8) << (7 - bit_index);
        bit_index += 1;
        if bit_index >= 8 {
            bit_index = 0;
        }
    }
    out
}

/// A `Decoder` decodes a compressed data stream.
pub trait Decoder {

    /// Decode the next bit from the data stream.
    ///
    /// This method does not indicate when the meaningful data
    /// has ended, so the length of the original data must be
    /// known to the caller.
    fn decode_bit(&mut self) -> bool;
}

pub(crate) fn decode_symbol<T>(decoder: &mut dyn Decoder) -> T
where T: ShlAssign<u8> + BitOrAssign<T> + Default + From<bool>
{
    let mut out = T::default();
    for _ in 0..(size_of::<T>() * 8) {
        out <<= 1;
        out |= decoder.decode_bit().into();
    }
    out
}

struct RawSliceDecoder<'a> {
    data: &'a [u8],
    index: u16,
    bit_index: u8
}

impl<'a> RawSliceDecoder<'a> {
    fn new<'b>(data: &'b [u8]) -> RawSliceDecoder<'b> {
        RawSliceDecoder {
            data,
            index: 0,
            bit_index: 0
        }
    }
}

impl<'a> Decoder for RawSliceDecoder<'a> {
    fn decode_bit(&mut self) -> bool {
        if (self.index as usize) < self.data.len() {
            let out = self.data[self.index as usize] & (1 << (7 - self.bit_index));
            self.bit_index += 1;
            if self.bit_index >= 8 {
                self.bit_index = 0;
                self.index += 1;
            }
            out != 0
        } else {
            false
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum CompressionMethods {
    Raw = 0,
    #[cfg(feature = "lz77")] LZ77 = 1,
    #[cfg(feature = "rc")] RC = 2
}

/// Information on the invocation of a compression method.
pub struct CompressionReport {
    /// The compression method used.
    pub method: CompressionMethods,
    /// The size of the data after compression. If this method was skipped,
    /// this will hold the size of the uncompressed data.
    pub compressed_size: usize,
    /// Whether this method was skipped within `compress`
    pub skipped: bool
}

/// Compresses the data using the list of `CompressionMethods`.
/// If the use of a compression did not decrease the size of the data,
/// it is skipped.
///
/// The function returns both the compressed data and a list of `CompressionReport`s,
/// with one entry for each compression method.
pub fn compress(data: &[u8], methods: &[CompressionMethods]) -> (Vec<u8>, Vec<CompressionReport>) {
    let mut out = data_to_bits(data);
    let mut reports = Vec::with_capacity(methods.len());
    out.insert(0, true);
    out.insert(0, true);
    for method in methods {
        let mut new = match method {
            CompressionMethods::Raw => out.clone(),
            #[cfg(feature = "lz77")] CompressionMethods::LZ77 => encode_lz77(&out),
            #[cfg(feature = "rc")] CompressionMethods::RC => encode_rc(&out)
        };
        if new.len() + 1 < out.len() {
            let mut tag = encode_fibonacci(method.to_owned() as usize);
            tag.append(&mut new);
            out = tag;
            reports.push(CompressionReport { method: *method, compressed_size: out.len(), skipped: false });
        } else {
            reports.push(CompressionReport { method: *method, compressed_size: out.len(), skipped: true });
        }
    }
    (bits_to_data(&out), reports)
}

/// Creates a `Decoder` for the compressed data.
///
/// Note that no checks are made to ensure that the data is in a valid format.
/// If the data was not created by `compress`, or if it is corrupted
/// in any way, this function will likely panic. Furthermore, the returned
/// `Decoder` does not know the original length of the data. Reading past the
/// end of the original data will likely also panic.
pub fn make_decoder<'a>(data: &'a [u8]) -> Box<dyn Decoder + 'a> {
    let mut decoder: Box<dyn Decoder + 'a> = Box::new(RawSliceDecoder::new(data));
    loop {
        let method = decode_fibonacci(decoder.as_mut());
        match method {
            #[cfg(feature = "lz77")] 1 => decoder = Box::new(LZ77Decoder::new(decoder)),
            #[cfg(feature = "rc")] 2 => decoder = Box::new(RCDecoder::new(decoder)),
            _ => return decoder,
        }
    }
}

#[cfg(test)]
extern crate quickcheck;

#[cfg(test)]
mod tests {

    use std::{cmp::Ordering, iter::repeat_with};

    use crate::{compress, decode_symbol, make_decoder, CompressionMethods};

    use super::quickcheck::{
        quickcheck, TestResult
    };

    quickcheck! {
        fn encoded_data_can_be_decoded(data: Vec<u8>) -> TestResult {
            let expanded_data: Vec<u8> = data.chunks_exact(2)
                .flat_map(|d| {
                    std::iter::repeat(d[1]).take(d[0] as usize)
                })
                .collect();
            if expanded_data.len() <= 0 {
                return TestResult::discard();
            }

            let (encoded, _) = compress(&expanded_data, &[CompressionMethods::LZ77, CompressionMethods::RC]);

            let mut decoder = make_decoder(&encoded);
            let decoded: Vec<u8> = repeat_with(|| decode_symbol::<u8>(decoder.as_mut())).take(expanded_data.len()).collect();
            TestResult::from_bool(decoded.cmp(&expanded_data) == Ordering::Equal)
        }
    }
}
