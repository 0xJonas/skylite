use crate::Decoder;

fn emit_code(start: u64, width: u64) -> (u8, u64, u64) {
    let code = (start >> 24) as u8;
    let start_masked = start & 0x00ff_ffff;

    // If start and start + width to not share the same leading byte,
    // either start or width has to be adjusted so that the leading
    // byte is the same for both ends of the range.
    let discrepancy = (0xff00_0000 & start) + 0x0100_0000 - start;
    if discrepancy < width {
        // Adjust range from the top
        (code, start_masked << 8, discrepancy << 8)
    } else {
        // Keep range as-is
        (code, start_masked << 8, width << 8)
    }
}

fn split_nibbles(data: &[u8]) -> Vec<u8> {
    data.iter()
        .flat_map(|b| [b >> 4, b & 0xf])
        .collect()
}

fn calculate_probabilities(nibbles: &[u8]) -> [u8; 16] {
    let len = nibbles.len();
    let mut counts = [0; 16];
    for n in nibbles {
        counts[*n as usize] += 1;
    }

    let mut out = [0_u8; 16];
    for i in 1..16 {
        // Use 0xf0 here instead of 0xff, to ensure that corrections
        // added by .max(1) do not overflow.
        out[i] = out[i - 1] + (counts[i - 1] * 0xf0 / len).max(1) as u8;
    }

    out
}

/// Encode `data` using range coding.
pub fn encode_rc<'a>(data: &[u8]) -> Vec<u8> {
    assert!(data.len() > 0);

    let nibbles = split_nibbles(data);

    let probabilities_raw = calculate_probabilities(&nibbles);
    let mut out: Vec<u8> = Vec::new();

    for p in &probabilities_raw[1..16] {
        out.push(*p);
    }

    let probabilities: Vec<u64> = (0..17)
        .map(|i| if i < 16 {
            (probabilities_raw[i] as u64) << 24
        } else {
            0xffff_ffff
        })
        .collect();

    let mut start: u64 = 0;
    let mut width: u64 = 0x1_0000_0000;

    for nibble in nibbles {
        // println!("start = {:x}, width = {:x}, nibble = {:x}", start, width, nibble);
        start += width * probabilities[nibble as usize] / 0x1_0000_0000;
        width = width * (probabilities[(nibble + 1) as usize] - probabilities[nibble as usize]) / 0x1_0000_0000;

        while (start >> 24) == (start + width >> 24) || width <= 0xffff {
            // print!("start = {:x}, width = {:x} ... emitting", start, width);
            let code: u8;
            (code, start, width) = emit_code(start, width);
            out.push(code);
            // println!(" => {:x}", code);
        }
    }

    while width < 0x1_0000_0000 {
        let code: u8;
        (code, start, width) = emit_code(start, width);
        out.push(code);
    }

    out
}

/// Decoder state for range coding.
pub struct RCDecoder<'a> {
    source: Box<dyn Decoder + 'a>,
    probabilities: [u64; 17],
    start: u64,
    width: u64,
    x: u64
}

impl<'a> RCDecoder<'a> {

    pub fn new<'b>(mut source: Box<dyn Decoder + 'b>) -> RCDecoder<'b> {
        let mut probabilities = [0_u64; 17];
        for i in 1..16 {
            probabilities[i] = (source.decode_u8() as u64) << 24;
        }
        probabilities[16] = 0xffff_ffff;

        let x = ((source.decode_u8() as u64) << 24)
                + ((source.decode_u8() as u64) << 16)
                + ((source.decode_u8() as u64) << 8)
                + (source.decode_u8() as u64);

        RCDecoder {
            source,
            probabilities,
            start: 0,
            width: 0x1_0000_0000,
            x
        }
    }

    fn adjust_range(&mut self) {
        let start_masked = self.start & 0x00ff_ffff;
        let discrepancy = (0xff00_0000 & self.start) + 0x0100_0000 - self.start;

        if discrepancy < self.width {
            // Adjust range from the top
            self.start = start_masked << 8;
            self.width = discrepancy << 8;
        } else {
            // Keep range as-is
            self.start = start_masked << 8;
            self.width <<= 8;
        }

        self.x = (self.x & 0x00ff_ffff) << 8;
        self.x |= self.source.decode_u8() as u64;
    }

    fn decode_nibble(&mut self) -> u8 {
        // print!("start = {:x}, width = {:x}, x = {:x}", self.start, self.width, self.x);
        let mut out = 0;
        for nibble in 0..16 {
            let threshold = self.start + self.width * self.probabilities[nibble + 1] / 0x1_0000_0000;
            if self.x < threshold {
                // print!(" threshold = {:x}", threshold);
                out = nibble;
                break;
            }
        }
        // println!(" => {:x}", out);

        self.start += self.width * self.probabilities[out] / 0x1_0000_0000;
        self.width = self.width * (self.probabilities[out + 1] - self.probabilities[out]) / 0x1_0000_0000;

        while (self.start >> 24) == (self.start + self.width >> 24) || self.width <= 0xffff {
            // println!("start = {:x}, width = {:x}, x = {:x} ... adjusting", self.start, self.width, self.x);
            self.adjust_range();
        }

        out as u8
    }
}

impl<'a> Decoder for RCDecoder<'a> {

    fn decode_u8(&mut self) -> u8 {
        (self.decode_nibble() << 4) + self.decode_nibble()
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

    use crate::{encode_rc, range_coding::RCDecoder, Decoder, RawSliceDecoder};

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

        let encoded = encode_rc(&data);
        let expectation = &[
            142, 215, 216, 217,
            218, 242, 243, 244,
            245, 246, 247, 248,
            249, 250, 251,
            61, 25, 19, 186,
            173, 250, 34, 33,
            164, 213, 42, 91,
            58, 228, 120, 65,
            42, 149, 1, 187,
            222, 22, 224, 173,
            126, 210, 62, 180,
            86, 186, 31, 228,
            210, 210, 202, 160,
            96, 53, 230, 188,
            201, 96, 108, 169,
            190, 75, 108, 100
        ];

        assert_eq!(&encoded[..], expectation);

        let mut decoder = RCDecoder::new(Box::new(RawSliceDecoder::new(&encoded)));
        let decoded: Vec<u8> = repeat_with(|| decoder.decode_u8()).take(data.len()).collect();
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

            // println!("{:?}", expanded_data);
            let encoded = encode_rc(&expanded_data);

            let mut decoder = RCDecoder::new(Box::new(RawSliceDecoder::new(&encoded)));
            let decoded: Vec<u8> = repeat_with(|| decoder.decode_u8()).take(expanded_data.len()).collect();
            TestResult::from_bool(decoded == expanded_data)
        }
    }
}
