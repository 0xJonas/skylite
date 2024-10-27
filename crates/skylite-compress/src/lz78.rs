use crate::Decoder;

const NO_IDX: u16 = 0xffff;

const MAX_NODES: usize = 1024;

struct TrieNode {
    content: u8,
    prev_idx: u16,
    next_list_idx: u16
}

struct Trie {
    nodes: Vec<TrieNode>,
    next_lists: Vec<Vec<u16>>
}

impl Trie {
    fn new() -> Trie {
        Trie {
            nodes: vec![TrieNode { prev_idx: NO_IDX, content: 0, next_list_idx: NO_IDX }],
            next_lists: Vec::new()
        }
    }

    fn get_phrase(&self, idx: u16) -> Vec<u8> {
        let mut node = &self.nodes[idx as usize];
        let mut out = Vec::new();
        loop {
            // The final node is the root node, which does not contain meaningful content.
            if node.prev_idx != NO_IDX {
                out.push(node.content);
                node = &self.nodes[node.prev_idx as usize];
            } else {
                break;
            }
        }
        out.reverse();
        out
    }

    fn add_node(&mut self, node: TrieNode) -> u16 {
        let new_idx = self.nodes.len() as u16;
        let prev_node = &mut self.nodes[node.prev_idx as usize];
        if prev_node.next_list_idx == NO_IDX {
            prev_node.next_list_idx = self.next_lists.len() as u16;
            self.next_lists.push(vec![new_idx]);
        } else {
            self.next_lists[prev_node.next_list_idx as usize].push(new_idx);
        }

        self.nodes.push(node);
        new_idx
    }
}

fn write_varint(mut val: usize, out: &mut Vec<u8>) {
    let pos = out.len();

    // Insert instead of push since the result should be in big-endian order
    out.insert(pos, (val & 0x7f) as u8);
    while val > 127 {
        val >>= 7;
        out.insert(pos, (val & 0x7f | 0x80) as u8);
    }

}

pub fn encode_lz78(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut trie = Trie::new();
    let mut current_idx = 0;

    for b in data {
        let current_node = &trie.nodes[current_idx];
        if current_node.next_list_idx != NO_IDX {
            let next_list = &trie.next_lists[current_node.next_list_idx as usize];
            match next_list.iter().find(|&next_idx| trie.nodes[*next_idx as usize].content == *b) {
                Some(idx) => {
                    current_idx = *idx as usize;
                    continue;
                },
                None => ()
            }
        }

        if trie.nodes.len() < MAX_NODES {
            trie.add_node(TrieNode { prev_idx: current_idx as u16, content: *b, next_list_idx: NO_IDX });
        }

        write_varint(current_idx, &mut out);
        out.push(*b);
        current_idx = 0;
    }

    write_varint(current_idx, &mut out);
    // Write a dummy 0 here because the decoder does not know when the data has ended
    // and will always read one byte after the node index.
    out.push(0);

    out
}

pub struct LZ78Decoder<'source> {
    source: Box<dyn Decoder + 'source>,
    trie: Trie,
    current_phrase: Vec<u8>,
    progress: u16
}

impl<'source> LZ78Decoder<'source> {
    pub fn new<'s>(source: Box<dyn Decoder + 's>) -> LZ78Decoder<'s> {
        LZ78Decoder {
            source,
            trie: Trie::new(),
            current_phrase: Vec::new(),
            progress: 0
        }
    }

    fn decode_next_phrase(&mut self, idx: u16) {
        self.current_phrase = self.trie.get_phrase(idx);
        let next_byte = self.source.decode_u8();

        if self.trie.nodes.len() < MAX_NODES {
            self.trie.add_node(TrieNode {
                prev_idx: idx,
                content: next_byte,
                next_list_idx: NO_IDX
            });
        }

        self.current_phrase.push(next_byte);
        self.progress = 0;
    }
}

fn read_varint<'source>(source: &'source mut dyn Decoder) -> usize {
    let mut b = source.decode_u8();
    let mut out = 0;
    while b >= 0x80 {
        out += (b & 0x7f) as usize;
        out <<= 7;
        b = source.decode_u8()
    }
    out + b as usize
}

impl<'source> Decoder for LZ78Decoder<'source> {
    fn decode_u8(&mut self) -> u8 {
        if self.progress as usize >= self.current_phrase.len() {
            let idx = read_varint(self.source.as_mut());
            self.decode_next_phrase(idx as u16);
        }
        let out = self.current_phrase[self.progress as usize];
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

    use crate::{lz78::LZ78Decoder, Decoder, RawSliceDecoder};

    use super::encode_lz78;

    use super::quickcheck::{
        quickcheck, TestResult
    };

    #[test]
    fn test_compression() {
        let data: Vec<u8> = (0..256)
            .map(|i| match i % 10 {
                1 => 0x11,
                2 => 0x11,
                3 => 0x11,
                5 => 0x55,
                _ => 0
            })
            .collect();

        let expectation = vec![
            0, 0, 0, 17,
            2, 17, 1, 85,
            1, 0, 5, 0, 3,
            17, 4, 0, 6,
            0, 7, 0, 0,
            85, 9, 0, 10,
            85, 12, 17, 3,
            0, 11, 0, 9,
            17, 15, 85, 14,
            17, 2, 0, 16,
            0, 6, 17, 18,
            0, 17, 17, 20,
            85, 19, 17, 8,
            0, 22, 17, 25,
            0, 24, 17, 27,
            0, 5, 17, 23,
            0, 28, 17, 31,
            0, 1, 17, 33,
            0, 32, 17, 29,
            0, 34, 0, 21,
            0, 38, 17, 35,
            0, 13, 0, 30,
            0, 41, 0, 36,
            17, 39, 0, 42,
            0, 46, 0, 44,
            0, 40, 85, 26,
            0, 50, 17, 18,
            0
        ];
        let encoded = encode_lz78(&data);
        assert_eq!(encoded, expectation);

        let mut decoder = LZ78Decoder::new(Box::new(RawSliceDecoder::new(&encoded)));
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

            let encoded = encode_lz78(&expanded_data);

            let mut decoder = LZ78Decoder::new(Box::new(RawSliceDecoder::new(&encoded)));
            let decoded: Vec<u8> = repeat_with(|| decoder.decode_u8()).take(expanded_data.len()).collect();
            return TestResult::from_bool(decoded.cmp(&expanded_data) == Ordering::Equal);
        }
    }
}
