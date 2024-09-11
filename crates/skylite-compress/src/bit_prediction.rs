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
    let mut best_result: Vec<bool> = data.to_vec();

    // Since the goal of the bit prediction is to reduce the number of 1-bits, the initial
    // number of mispredictions to beat is the number of 1-bits in the input data
    let mut best_result_mispredictions = data.iter().filter(|b| **b).count();

    for i in 0..16 {
        let res = test_encode(data, taps | (1 << i));
        let mispredictions = res.iter().filter(|b| **b).count();

        if mispredictions < best_result_mispredictions {
            best_result = res;
            best_result_mispredictions = mispredictions;
            taps |= 1 << i;
        }
    }

    (best_result, taps)
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
            0, 1, 169, 215, 249, 45, 5, 123,
            88, 216, 240, 142, 160, 116, 92, 34,
            27, 51, 27, 101, 75, 159, 183, 201,
            234, 106, 66, 60, 18, 198, 238, 144,
            156, 228, 204, 178, 156, 72, 96, 30,
            61, 189, 149, 235, 197, 17, 57, 71,
            126, 86, 126, 0, 46, 250, 210, 172,
            143, 15, 39, 89, 119, 163, 139, 245,
            147, 75, 99, 29, 51, 231, 207, 177,
            146, 18, 58, 68, 106, 190, 150, 232,
            209, 249, 209, 175, 129, 85, 125, 3,
            32, 160, 136, 246, 216, 12, 36, 90,
            86, 46, 6, 120, 86, 130, 170, 212,
            247, 119, 95, 33, 15, 219, 243, 141,
            180, 156, 180, 202, 228, 48, 24, 102,
            69, 197, 237, 147, 189, 105, 65, 63,
            140, 20, 60, 66, 108, 184, 144, 238,
            205, 77, 101, 27, 53, 225, 201, 183,
            142, 166, 142, 240, 222, 10, 34, 92,
            127, 255, 215, 169, 135, 83, 123, 5,
            9, 113, 89, 39, 9, 221, 245, 139,
            168, 40, 0, 126, 80, 132, 172, 210,
            235, 195, 235, 149, 187, 111, 71, 57,
            26, 154, 178, 204, 226, 54, 30, 96,
            6, 222, 246, 136, 166, 114, 90, 36,
            7, 135, 175, 209, 255, 43, 3, 125,
            68, 108, 68, 58, 20, 192, 232, 150,
            181, 53, 29, 99, 77, 153, 177, 207,
            195, 187, 147, 237, 195, 23, 63, 65,
            98, 226, 202, 180, 154, 78, 102, 24,
            33, 9, 33, 95, 113, 165, 141, 243,
            208, 80, 120, 6, 40, 252, 212, 170
        ]);
        assert_eq!(taps, 0x155);
    }
}
