use crate::bit_prediction::BitPredictor;
use crate::decode_symbol;
use crate::symbol_to_bits;
use crate::Decoder;
use crate::bit_prediction;

fn emit_code(start: u64, width: u64) -> (bool, u64, u64) {
    let code = (start & 0x8000_0000) != 0;
    let start_masked = start & 0x7fff_ffff;

    // If start and start + width to not share the same leading nibble,
    // either start or width has to be adjusted so that the leading
    // nibble is the same for both ends of the range.
    let discrepancy = 0x8000_0000 - start_masked;
    if discrepancy < width / 2 {
        // Adjust range from the bottom
        (true, 0, (width - discrepancy) << 1)
    } else if discrepancy < width {
        // Adjust range from the top
        (false, start_masked << 1, discrepancy << 1)
    } else {
        // Keep range as-is
        (code, start_masked << 1, width << 1)
    }
}

/// Encode `data` using range coding.
pub fn encode_rc<'a>(data: &[bool]) -> Vec<bool> {
    assert!(data.len() > 0);

    // Start by applying bit prediction to reduce the number of 1-bits.
    let (predicted, taps) = bit_prediction::encode(data);

    let zeros_count: u64 = predicted.iter().filter(|b| !**b).count() as u64;
    let probability_0: u64 = if zeros_count > 0 {
        // The probability must not get rounded to 0 when there are actually 0s to
        // encode, otherwise the encoder will get stuck in an infinite loop when
        // encountering a 0.
        u64::max((zeros_count * 0xffff_ffff / predicted.len() as u64) & 0xffff_0000, 1)
    } else {
        // If there really are no 0s to encode, a probability of 0 is ok
        // because the branch which would cause problems will never be
        // taken.
        0
    };
    let mut out: Vec<bool> = Vec::new();

    out.append(&mut symbol_to_bits(((probability_0 >> 16) & 0xffff) as u16));
    out.append(&mut symbol_to_bits(taps));

    let mut start: u64 = 0;
    let mut width: u64 = 0x1_0000_0000;

    for bit in predicted {
        if bit {
            start += width * probability_0 / 0x1_0000_0000;
            width = width * (0x1_0000_0000 - probability_0) / 0x1_0000_0000;
        } else {
            width = width * probability_0 / 0x1_0000_0000;
        }

        while (start >> 31) == (start + width >> 31) || width <= 0xffff {
            let bit: bool;
            (bit, start, width) = emit_code(start, width);
            out.push(bit);
        }
    }

    while width < 0x1_0000_0000 {
        let bit: bool;
        (bit, start, width) = emit_code(start, width);
        out.push(bit);
    }

    out
}

/// Decoder state for range coding.
pub struct RCDecoder<'a> {
    source: Box<dyn Decoder + 'a>,
    probability_0: u64,
    predictor: bit_prediction::BitPredictor,
    start: u64,
    width: u64,
    x: u64
}

impl<'a> RCDecoder<'a> {

    pub fn new<'b>(mut source: Box<dyn Decoder + 'b>) -> RCDecoder<'b> {
        let probability = decode_symbol::<u16>(source.as_mut()) as u64;
        let taps = decode_symbol::<u16>(source.as_mut());
        let x = decode_symbol::<u32>(source.as_mut()) as u64;

        RCDecoder {
            source,
            probability_0: probability << 16,
            predictor: BitPredictor::new(taps),
            start: 0,
            width: 0x1_0000_0000,
            x
        }
    }

    fn adjust_range(&mut self) {
        let start_masked = self.start & 0x7fff_ffff;
        let discrepancy = 0x8000_0000 - start_masked;

        if discrepancy < self.width / 2 {
            // Adjust range from the bottom
            self.start = 0;
            self.width = (self.width - discrepancy) << 1;
        } else if discrepancy < self.width {
            // Adjust range from the top
            self.start = start_masked << 1;
            self.width = discrepancy << 1;
        } else {
            // Keep range as-is
            self.start = start_masked << 1;
            self.width <<= 1;
        }

        self.x = (self.x & 0x7fff_ffff) << 1;
        self.x |= self.source.decode_bit() as u64;
    }
}

impl<'a> Decoder for RCDecoder<'a> {

    fn decode_bit(&mut self) -> bool {
        let threshold = self.x - self.start;
        let decoded_bit = self.probability_0 * self.width / 0x1_0000_0000 <= threshold;
        if decoded_bit {
            self.start += self.width * self.probability_0 / 0x1_0000_0000;
            self.width = self.width * (0x1_0000_0000 - self.probability_0) / 0x1_0000_0000;
        } else {
            self.width = self.width * self.probability_0 / 0x1_0000_0000;
        }

        while (self.start >> 31) == (self.start + self.width >> 31) || self.width <= 0xffff {
            self.adjust_range();
        }

        let predicted = self.predictor.predict();
        let bit = predicted != decoded_bit;
        self.predictor.push_bit(bit);

        bit
    }
}

#[cfg(test)]
extern crate quickcheck;

#[cfg(test)]
mod tests {
    use std::iter::repeat_with;

    use super::quickcheck::{
        quickcheck, TestResult
    };

    use crate::{bits_to_data, data_to_bits, decode_symbol, encode_rc, range_coding::RCDecoder, RawSliceDecoder};

    #[test]
    fn test_compression() {
        let data: Vec<u8> = (0..128)
            .map(|i| match i % 10 {
                1 => 0x11,
                2 => 0x11,
                3 => 0x11,
                5 => 0x55,
                _ => 0
            })
            .collect();

        let encoded = bits_to_data(&encode_rc(&data_to_bits(&data)));
        let expectation = &[
            236, 127, 0, 8,
            100, 50, 84, 208,
            187, 206, 247, 3,
            221, 191, 37, 156,
            207, 192, 209, 198,
            49, 167, 196, 196,
            83, 121, 124, 242,
            13, 26, 114, 207,
            135, 94, 138, 176,
            229, 25, 193, 95,
            156, 206, 53, 119,
            152, 109, 2, 59,
            222, 234, 212, 250,
            197, 220
        ];

        assert_eq!(&encoded[..], expectation);

        let mut decoder = RCDecoder::new(Box::new(RawSliceDecoder::new(&encoded)));
        let decoded: Vec<u8> = repeat_with(|| decode_symbol::<u8>(&mut decoder)).take(data.len()).collect();
        assert_eq!(decoded[..], data);
    }

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

            let encoded = bits_to_data(&encode_rc(&data_to_bits(&expanded_data)));

            let mut decoder = RCDecoder::new(Box::new(RawSliceDecoder::new(&encoded)));
            let decoded: Vec<u8> = repeat_with(|| decode_symbol::<u8>(&mut decoder)).take(expanded_data.len()).collect();
            TestResult::from_bool(decoded == expanded_data)
        }
    }
}
