// pub use fibonacci_code::{decode_fibonacci, encode_fibonacci};

#[cfg(feature = "range_coding")]
mod range_coding;
#[cfg(feature = "range_coding")]
use range_coding::*;

#[cfg(feature = "lz77")]
mod lz77;
#[cfg(feature = "lz77")]
use lz77::*;

#[cfg(feature = "lz78")]
mod lz78;
#[cfg(feature = "lz78")]
use lz78::*;

#[cfg(feature = "bit_prediction")]
mod bit_prediction;
// mod fibonacci_code;

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

    /// Decode the next byte from the data stream.
    ///
    /// This method does not indicate when the meaningful data
    /// has ended, so the length of the original data must be
    /// known to the caller.
    fn decode_u8(&mut self) -> u8;
}

struct RawSliceDecoder<'a> {
    data: &'a [u8],
    index: u16,
}

impl<'a> RawSliceDecoder<'a> {
    fn new<'b>(data: &'b [u8]) -> RawSliceDecoder<'b> {
        RawSliceDecoder {
            data,
            index: 0
        }
    }
}

impl<'a> Decoder for RawSliceDecoder<'a> {
    fn decode_u8(&mut self) -> u8 {
        if (self.index as usize) < self.data.len() {
            let out = self.data[self.index as usize];
            self.index += 1;
            out
        } else {
            0
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum CompressionMethods {
    Raw = 0,
    #[cfg(feature = "lz77")] LZ77 = 1,
    #[cfg(feature = "lz78")] LZ78 = 2,
    #[cfg(feature = "range_coding")] RC = 3
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
    let mut out = data.to_owned();
    let mut reports = Vec::with_capacity(methods.len());
    out.insert(0, 0);
    for method in methods {
        let mut new = match method {
            CompressionMethods::Raw => out.clone(),
            #[cfg(feature = "lz77")] CompressionMethods::LZ77 => encode_lz77(&out),
            #[cfg(feature = "lz78")] CompressionMethods::LZ78 => encode_lz78(&out),
            #[cfg(feature = "range_coding")] CompressionMethods::RC => encode_rc(&out)
        };
        if new.len() + 1 < out.len() {
            let mut tag = vec![method.to_owned() as u8];
            tag.append(&mut new);
            out = tag;
            reports.push(CompressionReport { method: *method, compressed_size: out.len(), skipped: false });
        } else {
            reports.push(CompressionReport { method: *method, compressed_size: out.len(), skipped: true });
        }
    }
    (out, reports)
}

/// Creates a `Decoder` for the compressed data.
///
/// Note that no checks are made to ensure that the data is in a valid format.
/// If the data was not created by `compress`, or if it is corrupted
/// in any way, this function will likely panic. Furthermore, the returned
/// `Decoder` does not know the original length of the data. Reading past the
/// end of the original data will likely also panic.
#[no_mangle]
pub fn make_decoder<'a>(data: &'a [u8]) -> Box<dyn Decoder + 'a> {
    let mut decoder: Box<dyn Decoder + 'a> = Box::new(RawSliceDecoder::new(data));
    loop {
        let method = decoder.decode_u8();
        match method {
            #[cfg(feature = "lz77")] 1 => decoder = Box::new(LZ77Decoder::new(decoder)),
            #[cfg(feature = "lz78")] 2 => decoder = Box::new(LZ78Decoder::new(decoder)),
            #[cfg(feature = "range_coding")] 3 => decoder = Box::new(RCDecoder::new(decoder)),
            _ => return decoder,
        }
    }
}

#[cfg(test)]
extern crate quickcheck;

#[cfg(test)]
mod tests {

    use std::{cmp::Ordering, iter::repeat_with};

    use crate::{compress, make_decoder, CompressionMethods};

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
            let decoded: Vec<u8> = repeat_with(|| decoder.decode_u8()).take(expanded_data.len()).collect();
            TestResult::from_bool(decoded.cmp(&expanded_data) == Ordering::Equal)
        }
    }
}
