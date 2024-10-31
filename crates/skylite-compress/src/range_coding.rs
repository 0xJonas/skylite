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

fn calc_ring_buffer_init(data: &[u8]) -> [u8; 4] {
    let mut counts = [0; 256];
    for d in data {
        counts[*d as usize] += 1;
    }

    let mut counts_indexed: Vec<(usize, usize)> = counts.into_iter().enumerate().collect();
    counts_indexed.sort_by_key(|(_, c)| !*c);
    [
        counts_indexed[0].0 as u8,
        counts_indexed[1].0 as u8,
        counts_indexed[2].0 as u8,
        counts_indexed[3].0 as u8,
    ]
}

/// Encode `data` using range coding.
pub fn encode_rc<'a>(data: &[u8]) -> Vec<u8> {
    assert!(data.len() > 0);

    let mut out = Vec::new();

    // The ring buffer is used to manage the counts array.
    // It needs to be 255 bytes long, because otherwise it would be
    // possible for 256 of the same byte to be in the buffer, which would not fit
    // the counts array (max is 255).
    // The ring buffer is initialized by repeating the four most common bytes in
    // the first 255 bytes of the data.
    let ring_buffer_init = calc_ring_buffer_init(&data[0 .. 255.min(data.len())]);
    let mut ring_buffer: [u8; 255] = std::array::from_fn(|i| ring_buffer_init[i & 0x3]);
    let mut ring_buffer_idx = 0;

    // The number of occurances of each byte in the ring buffer at the current time.
    // These counts are used directly as the probabilities for the current byte to be encoded.
    // The sum of all counts will always be 255 (it should ideally be 256, but that is not
    // possible without increasing the size of the array type).
    let mut counts = [0_u8; 256];
    counts[ring_buffer_init[0] as usize] = 64;
    counts[ring_buffer_init[1] as usize] = 64;
    counts[ring_buffer_init[2] as usize] = 64;
    counts[ring_buffer_init[3] as usize] = 63;

    // The ring buffer initialization must be part of the output,
    // since the decoder has to initialize its ring buffer with
    // the same data as the encoder.
    for i in ring_buffer_init {
        out.push(i);
    }

    let mut start: u64 = 0;
    let mut width: u64 = 0x1_0000_0000;

    for byte in data {
        let count_acc: u64 = counts[0 .. (*byte as usize)]
            .iter()
            .map(|c| *c as u64 + 1)
            .sum::<u64>() << 23;
        // println!("start = {:x}, width = {:x}, byte = {:x}, p = {:x}, t = {:x}", start, width, byte, probability, total_scaled);
        start += width * count_acc / 0x1_0000_0000;
        width = width * ((counts[*byte as usize] as u64 + 1) << 23) / 0x1_0000_0000;

        while (start >> 24) == (start + width >> 24) || width <= 0xffff {
            // print!("start = {:x}, width = {:x} ... emitting", start, width);
            let code: u8;
            (code, start, width) = emit_code(start, width);
            out.push(code);
            // println!(" => {:x}", code);
        }

        // Update counts and ring buffer.
        counts[ring_buffer[ring_buffer_idx] as usize] -= 1;
        counts[*byte as usize] += 1;
        ring_buffer[ring_buffer_idx] = *byte;
        ring_buffer_idx = (ring_buffer_idx + 1) % 255;
    }

    // Finish up
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
    counts: [u8; 256],
    ring_buffer: [u8; 255],
    ring_buffer_idx: usize,
    start: u64,
    width: u64,
    x: u64
}

impl<'a> RCDecoder<'a> {

    pub fn new<'b>(mut source: Box<dyn Decoder + 'b>) -> RCDecoder<'b> {
        let ring_buffer_init = [
            source.decode_u8(),
            source.decode_u8(),
            source.decode_u8(),
            source.decode_u8()
        ];

        let mut counts = [0; 256];
        counts[ring_buffer_init[0] as usize] = 64;
        counts[ring_buffer_init[1] as usize] = 64;
        counts[ring_buffer_init[2] as usize] = 64;
        counts[ring_buffer_init[3] as usize] = 63;

        let x = ((source.decode_u8() as u64) << 24)
                + ((source.decode_u8() as u64) << 16)
                + ((source.decode_u8() as u64) << 8)
                + (source.decode_u8() as u64);

        RCDecoder {
            source,
            counts,
            ring_buffer: std::array::from_fn(|i| ring_buffer_init[i & 0x3]),
            ring_buffer_idx: 0,
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
}

impl<'a> Decoder for RCDecoder<'a> {

    fn decode_u8(&mut self) -> u8 {
        // print!("start = {:x}, width = {:x}, x = {:x}", self.start, self.width, self.x);
        let mut out = 0;
        let mut count_acc = 0;
        let mut count_inc = 0;
        for byte in 0..=255_u8 {
            count_inc = (self.counts[byte as usize] as u64 + 1) << 23;
            let threshold = self.start + self.width * (count_acc + count_inc) / 0x1_0000_0000;
            out = byte;
            if self.x < threshold {
                // print!(", threshold = {:x}, acc = {:x}, inc = {:x}", threshold, count_acc, count_inc);
                break;
            }
            count_acc += count_inc;
        }
        // println!(" => {:x}", out);

        self.start += self.width * count_acc / 0x1_0000_0000;
        self.width = self.width * count_inc / 0x1_0000_0000;

        while (self.start >> 24) == (self.start + self.width >> 24) || self.width <= 0xffff {
            // println!("start = {:x}, width = {:x}, x = {:x} ... adjusting", self.start, self.width, self.x);
            self.adjust_range();
            assert_ne!(self.width, 0);
        }

        // Update counts
        self.counts[self.ring_buffer[self.ring_buffer_idx] as usize] -= 1;
        self.counts[out as usize] += 1;
        self.ring_buffer[self.ring_buffer_idx] = out as u8;
        self.ring_buffer_idx = (self.ring_buffer_idx + 1) % 255;

        out as u8
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
            0, 17, 85, 1,
            10, 115, 248, 183,
            244, 208, 233, 246,
            143, 246, 104, 202,
            59, 38, 2, 131,
            66, 90, 223, 250,
            135, 18, 227, 13,
            12, 164, 160, 175,
            89, 143, 71, 255,
            118, 5, 21, 65,
            75, 88, 204, 114,
            117, 15, 160, 88,
            239, 207
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
