use crate::Decoder;

const MAX_LENGTH: usize = 128;
const MAX_RECALL_DIST: usize = 256;

struct RingBuffer {
    content: [u8; MAX_RECALL_DIST],
    input_idx: usize
}

impl RingBuffer {
    pub fn new() -> RingBuffer {
        RingBuffer {
            content: [0; MAX_RECALL_DIST],
            input_idx: 0
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

struct LZ77Encoder {
    pending_symbols: usize,
    buffer: RingBuffer,
    recall_distances: Vec<usize>,
    recall_length: usize,
    out: Vec<u8>
}

impl LZ77Encoder {

    pub fn new() -> LZ77Encoder {
        LZ77Encoder {
            pending_symbols: 0,
            buffer: RingBuffer::new(),
            recall_distances: Vec::new(),
            recall_length: 0,
            out: Vec::new()
        }
    }

    fn emit_direct_data_code(&mut self, len: usize) {
        if len == 0 {
            return;
        }

        self.out.push((len - 1) as u8);

        for i in 0 .. len {
            self.out.push(self.buffer.read(self.pending_symbols - i - 1));
        }
        self.pending_symbols -= len;
    }

    fn emit_recall_code(&mut self, distance: usize, len: usize) {
        if len == 0 {
            return
        }

        self.out.push((len - 1) as u8 | 0x80);
        self.out.push(distance as u8);
        self.pending_symbols -= len;
    }

    pub fn push_symbol(&mut self, symbol: u8) {
        if !self.recall_distances.is_empty() {
            let current_max_recall_distance = *self.recall_distances.iter().max().unwrap();
            self.recall_distances.retain(|dist| (self.buffer.read(*dist) == symbol));
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
            self.recall_distances = (0 .. MAX_RECALL_DIST).filter(|dist| self.buffer.read(*dist) == symbol).collect();
            if !self.recall_distances.is_empty() {
                self.recall_length = 1;
            }
        }

        self.buffer.push(symbol);
        self.pending_symbols += 1;

        // Check if an output needs to be forced, to prevent the ring buffer from overwriting unprocessed data
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

    pub fn finish(mut self) -> Vec<u8> {
        if self.pending_symbols > 0 {
            self.emit_direct_data_code(self.pending_symbols - self.recall_length);
        }

        if self.recall_length > 0 {
            let current_max_recall_distance = *self.recall_distances.iter().max().unwrap();
            self.emit_recall_code(current_max_recall_distance, self.recall_length);
        }

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
    Recall(usize, usize)
}

pub struct LZ77Decoder<'a> {
    source: Box<dyn Decoder + 'a>,
    buffer: RingBuffer,
    opcode: LZ77Opcode,
    progress: usize
}

impl<'a> LZ77Decoder<'a> {
    pub fn new<'b>(source: Box<dyn Decoder + 'b>) -> LZ77Decoder<'b> {
        LZ77Decoder {
            source,
            buffer: RingBuffer::new(),
            opcode: LZ77Opcode::DirectData(0),
            progress: 0
        }
    }
}

impl<'a> Decoder for LZ77Decoder<'a> {

    fn decode_u8(&mut self) -> u8 {
        let len = match self.opcode {
            LZ77Opcode::DirectData(len) => len,
            LZ77Opcode::Recall(_, len) => len
        };

        if self.progress >= len {
            let opcode = self.source.decode_u8();
            let code_type = opcode >> 7 != 0;
            let len = (opcode & 0x7f) as usize + 1;
            if code_type {
                let distance = self.source.decode_u8() as usize;
                self.opcode = LZ77Opcode::Recall(distance, len);
            } else {
                self.opcode = LZ77Opcode::DirectData(len);
            }
            self.progress = 0;
        }

        let out = match self.opcode {
            LZ77Opcode::DirectData(_) => self.source.decode_u8(),
            LZ77Opcode::Recall(distance, _) => self.buffer.read(distance)
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
    use std::{cmp::Ordering, iter::repeat_with};

    use super::quickcheck::{
        quickcheck, TestResult
    };

    use crate::{encode_lz77, lz77::LZ77Decoder, Decoder, RawSliceDecoder};

    #[test]
    fn test_compression() {
        let data: Vec<u8> = (0..1024)
            .map(|i| match i % 10 {
                1 => 0x11,
                2 => 0x11,
                3 => 0x11,
                5 => 0x55,
                _ => 0
            })
            .collect();

        let encoded = encode_lz77(&data);

        let expectation = &[5, 0, 17, 17, 17, 0, 85, 255, 9, 255, 129, 255, 249, 255, 249, 255, 249, 255, 249, 255, 249, 249, 249];
        assert_eq!(&encoded[..], expectation);

        let mut decoder = LZ77Decoder::new(Box::new(RawSliceDecoder::new(&encoded)));
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

            let encoded = encode_lz77(&expanded_data);

            let mut decoder = LZ77Decoder::new(Box::new(RawSliceDecoder::new(&encoded)));
            let decoded: Vec<u8> = repeat_with(|| decoder.decode_u8()).take(expanded_data.len()).collect();
            return TestResult::from_bool(decoded.cmp(&expanded_data) == Ordering::Equal);
        }
    }
}
