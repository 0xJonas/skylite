use crate::Decoder;

const MAX_LENGTH: usize = 128;
const MAX_RECALL_DIST: usize = 256;

struct RingBuffer {
    content: [u8; MAX_RECALL_DIST],
    input_idx: usize,
}

impl RingBuffer {
    pub fn new() -> RingBuffer {
        RingBuffer {
            content: [0; MAX_RECALL_DIST],
            input_idx: 0,
        }
    }

    pub fn push(&mut self, value: u8) {
        self.content[self.input_idx] = value;
        self.input_idx += 1;
        if self.input_idx >= MAX_RECALL_DIST {
            self.input_idx = 0;
        }
    }

    pub fn read(&self, offset: usize) -> u8 {
        let idx = if offset + 1 <= self.input_idx {
            self.input_idx - (offset + 1)
        } else {
            MAX_RECALL_DIST - (offset + 1 - self.input_idx)
        };

        self.content[idx]
    }
}

fn map_output_bytes<C: FnMut(u8) -> u8, D: FnMut(u8) -> u8>(
    data: &mut [u8],
    mut control_code_fn: C,
    mut data_fn: D,
) {
    let mut idx = 0;

    while idx < data.len() {
        let opcode = data[idx];
        data[idx] = control_code_fn(opcode);
        idx += 1;
        if opcode & 1 != 0 {
            data[idx] = control_code_fn(data[idx]);
            idx += 1;
        } else {
            let len = (opcode as usize >> 1) + 1;
            for byte in data[idx..idx + len].iter_mut() {
                *byte = data_fn(*byte);
            }
            idx += len;
        }
    }
}

fn calc_max_correlation_offset(data_counts: &[u32; 256], control_code_counts: &[u32; 256]) -> u8 {
    let mut max_correlation = 0;
    let mut max_correlation_offset = 0;
    for offset in 0..256 {
        let correlation = data_counts
            .iter()
            .enumerate()
            .map(|(i, c)| *c * control_code_counts[(i + offset) & 0xff])
            .sum();
        if correlation > max_correlation {
            max_correlation = correlation;
            max_correlation_offset = offset;
        }
    }
    max_correlation_offset as u8
}

struct LZ77Encoder {
    pending_symbols: usize,
    buffer: RingBuffer,
    recall_distances: Vec<usize>,
    recall_length: usize,
    out: Vec<u8>,
}

impl LZ77Encoder {
    pub fn new() -> LZ77Encoder {
        LZ77Encoder {
            pending_symbols: 0,
            buffer: RingBuffer::new(),
            recall_distances: Vec::new(),
            recall_length: 0,
            out: Vec::new(),
        }
    }

    fn emit_direct_data_code(&mut self, len: usize) {
        if len == 0 {
            return;
        }

        self.out.push(((len - 1) as u8) << 1);

        for i in 0..len {
            self.out
                .push(self.buffer.read(self.pending_symbols - i - 1));
        }
        self.pending_symbols -= len;
    }

    fn emit_recall_code(&mut self, distance: usize, len: usize) {
        if len == 0 {
            return;
        }

        self.out.push((((len - 1) as u8) << 1) | 1);
        self.out.push(distance as u8);
        self.pending_symbols -= len;
    }

    pub fn push_symbol(&mut self, symbol: u8) {
        if !self.recall_distances.is_empty() {
            let current_max_recall_distance = *self.recall_distances.iter().max().unwrap();
            self.recall_distances
                .retain(|dist| (self.buffer.read(*dist) == symbol));
            if self.recall_distances.is_empty() {
                if self.recall_length > 2 {
                    self.emit_direct_data_code(self.pending_symbols - self.recall_length);
                    self.emit_recall_code(current_max_recall_distance, self.recall_length);
                }
                self.recall_length = 0;
            } else {
                self.recall_length += 1;
            }
        }

        if self.recall_distances.is_empty() {
            self.recall_distances = (0..MAX_RECALL_DIST)
                .filter(|dist| self.buffer.read(*dist) == symbol)
                .collect();
            if !self.recall_distances.is_empty() {
                self.recall_length = 1;
            }
        }

        self.buffer.push(symbol);
        self.pending_symbols += 1;

        // Check if an output needs to be forced, to prevent the ring buffer from
        // overwriting unprocessed data
        if self.pending_symbols >= MAX_LENGTH {
            self.emit_direct_data_code(self.pending_symbols - self.recall_length);
        }

        if self.recall_length >= MAX_LENGTH {
            let current_max_recall_distance = *self.recall_distances.iter().max().unwrap();
            self.emit_recall_code(current_max_recall_distance, self.recall_length);
            self.recall_distances.clear();
            self.recall_length = 0;
        }
    }

    fn entropy_transform(&mut self) {
        let mut control_code_counts = [0; 256];
        let mut data_counts = [0; 256];
        map_output_bytes(
            &mut self.out,
            |c| {
                control_code_counts[c as usize] += 1;
                c
            },
            |d| {
                data_counts[d as usize] += 1;
                d
            },
        );

        let offset = calc_max_correlation_offset(&data_counts, &control_code_counts);

        map_output_bytes(&mut self.out, |c| c.wrapping_sub(offset), |d| d);

        self.out.insert(0, offset);
    }

    pub fn finish(mut self) -> Vec<u8> {
        if self.pending_symbols > 0 {
            self.emit_direct_data_code(self.pending_symbols - self.recall_length);
        }

        if self.recall_length > 0 {
            let current_max_recall_distance = *self.recall_distances.iter().max().unwrap();
            self.emit_recall_code(current_max_recall_distance, self.recall_length);
        }

        self.entropy_transform();
        self.out
    }
}

pub fn encode_lz77<'a>(data: &[u8]) -> Vec<u8> {
    let mut encoder = LZ77Encoder::new();
    for b in data {
        encoder.push_symbol(*b);
    }
    return encoder.finish();
}

enum LZ77Opcode {
    DirectData(usize),
    Recall(usize, usize),
}

pub struct LZ77Decoder<'a> {
    source: Box<dyn Decoder + 'a>,
    buffer: RingBuffer,
    control_code_offset: u8,
    opcode: LZ77Opcode,
    progress: usize,
}

impl<'a> LZ77Decoder<'a> {
    pub fn new<'b>(mut source: Box<dyn Decoder + 'b>) -> LZ77Decoder<'b> {
        let control_code_offset = source.decode_u8();
        LZ77Decoder {
            source,
            buffer: RingBuffer::new(),
            control_code_offset,
            opcode: LZ77Opcode::DirectData(0),
            progress: 0,
        }
    }
}

impl<'a> Decoder for LZ77Decoder<'a> {
    fn decode_u8(&mut self) -> u8 {
        let len = match self.opcode {
            LZ77Opcode::DirectData(len) => len,
            LZ77Opcode::Recall(_, len) => len,
        };

        if self.progress >= len {
            let opcode = self
                .source
                .decode_u8()
                .wrapping_add(self.control_code_offset);
            let code_type = opcode & 1 != 0;
            let len = (opcode as usize >> 1) + 1;
            if code_type {
                let distance = self
                    .source
                    .decode_u8()
                    .wrapping_add(self.control_code_offset) as usize;
                self.opcode = LZ77Opcode::Recall(distance, len);
            } else {
                self.opcode = LZ77Opcode::DirectData(len);
            }
            self.progress = 0;
        }

        let out = match self.opcode {
            LZ77Opcode::DirectData(_) => self.source.decode_u8(),
            LZ77Opcode::Recall(distance, _) => self.buffer.read(distance),
        };
        self.buffer.push(out);
        self.progress += 1;
        out
    }
}

#[cfg(test)]
extern crate quickcheck;

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::iter::repeat_with;

    use super::quickcheck::{quickcheck, TestResult};
    use crate::lz77::LZ77Decoder;
    use crate::{encode_lz77, Decoder, RawSliceDecoder};

    #[test]
    fn test_compression() {
        let data: Vec<u8> = (0..1024)
            .map(|i| match i % 10 {
                1 => 0x11,
                2 => 0x11,
                3 => 0x11,
                5 => 0x55,
                _ => 0,
            })
            .collect();

        let encoded = encode_lz77(&data);

        let expectation = &[
            238, 28, 0, 17, 17, 17, 0, 85, 17, 27, 17, 147, 17, 11, 17, 11, 17, 11, 17, 11, 17, 11,
            5, 11,
        ];
        assert_eq!(&encoded[..], expectation);

        let mut decoder = LZ77Decoder::new(Box::new(RawSliceDecoder::new(&encoded)));
        let decoded: Vec<u8> = repeat_with(|| decoder.decode_u8())
            .take(data.len())
            .collect();
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

            let encoded = encode_lz77(&expanded_data);

            let mut decoder = LZ77Decoder::new(Box::new(RawSliceDecoder::new(&encoded)));
            let decoded: Vec<u8> = repeat_with(|| decoder.decode_u8()).take(expanded_data.len()).collect();
            return TestResult::from_bool(decoded.cmp(&expanded_data) == Ordering::Equal);
        }
    }
}
