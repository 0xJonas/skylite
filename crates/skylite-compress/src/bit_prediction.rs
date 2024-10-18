pub struct BitPredictor {
    taps: u16,
    state: u16
}

impl BitPredictor {
    pub fn new(taps: u16) -> BitPredictor {
        BitPredictor {
            taps, state: 0
        }
    }

    pub fn predict(&self) -> bool {
        ((self.state & self.taps).count_ones() & 0x1) != 0
    }

    pub fn push_bit(&mut self, bit: bool) {
        self.state <<= 1;
        self.state += bit as u16;
    }
}

fn test_encode(data: &[bool], taps: u16) -> Vec<bool> {
    let mut predictor = BitPredictor::new(taps);
    let mut out: Vec<bool> = Vec::new();

    for bit in data {
        let prediction = predictor.predict();
        predictor.push_bit(*bit);
        out.push(prediction != *bit);
    }

    out
}

pub fn encode(data: &[bool]) -> (Vec<bool>, u16) {
    let mut taps = 0;

    // Since the goal of the bit prediction is to reduce the number of 1-bits, the initial
    // number of mispredictions to beat is the number of 1-bits in the input data
    let mut prev_best_result = data.iter().filter(|b| **b).count();

    loop {
        let mut best_result = prev_best_result;
        let mut best_result_bit = 0;
        for i in 0..16 {
            let res = test_encode(data, taps | (1 << i));
            let mispredictions = res.iter().filter(|b| **b).count();

            if mispredictions < best_result {
                best_result = mispredictions;
                best_result_bit = i;
            }
        }
        if best_result >= prev_best_result {
            break;
        }
        prev_best_result = best_result;
        taps |= 1 << best_result_bit;
    }

    (test_encode(data, taps), taps)
}

#[cfg(test)]
mod tests {
    use crate::{bits_to_data, data_to_bits};

    use super::encode;

    #[test]
    fn test_bit_prediction_simple() {
        let data: &[u8] = &[
            0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55
        ];

        let (encoded, taps) = encode(&data_to_bits(&data));

        assert_eq!(bits_to_data(&encoded), vec![0x40, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0]);
        assert_eq!(taps, 0x2);
    }

    #[test]
    fn test_bit_prediction_iota() {
        let data: Vec<u8> = (0..=255).collect();

        let (encoded, taps) = encode(&data_to_bits(&data));
        assert_eq!(bits_to_data(&encoded), vec![
            0, 1, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            30, 30, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            62, 62, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            30, 30, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            126, 126, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            30, 30, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            62, 62, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            30, 30, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            254, 254, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            30, 30, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            62, 62, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            30, 30, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            126, 126, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            30, 30, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            62, 62, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2,
            30, 30, 2, 2, 6, 6, 2, 2,
            14, 14, 2, 2, 6, 6, 2, 2
        ]);
        assert_eq!(taps, 0x8000);
    }
}
