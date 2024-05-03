use crate::Decoder;

fn last_fibonacci_numbers_below(value: usize) -> (usize, usize) {
    let mut f_prev_prev = 1;
    let mut f_prev = 1;
    let mut f = f_prev + f_prev_prev;
    while value >= f {
        f_prev_prev = f_prev;
        f_prev = f;
        f = f_prev + f_prev_prev;
    }
    (f_prev, f_prev_prev)
}

pub fn encode_fibonacci(value: usize) -> Vec<bool> {
    // 0 cannot be represented as a Fibonacci code, so the
    // value is simply incremented here and decremented in the decoder.
    let mut to_encode = value + 1;
    let (mut f, mut f_prev) = last_fibonacci_numbers_below(to_encode);
    let mut out = vec![true]; // Start with a 'seperator' value of true.

    while f_prev > 0 {
        if f <= to_encode {
            to_encode -= f;
            out.push(true);
        } else {
            out.push(false);
        }

        let f_prev_prev = f - f_prev;
        f = f_prev;
        f_prev = f_prev_prev;
    }

    out.reverse();
    out
}

pub fn decode_fibonacci(decoder: &mut dyn Decoder) -> usize {
    let mut f_prev = 0;
    let mut f = 1;
    let mut prev_bit = false;
    let mut out: usize = 0;

    loop {
        (f, f_prev) = (f + f_prev, f);
        let bit = decoder.decode_bit();
        if bit {
            if prev_bit {
                // The value has been incremented during encoding,
                // so it has to be decremented here. This is required
                // to encode a 0.
                return out - 1;
            }
            out += f;
        }
        prev_bit = bit;
    }
}

#[cfg(test)]
mod tests {
    use crate::Decoder;

    use super::{encode_fibonacci, decode_fibonacci};

    struct BitVecDecoder<'a> {
        bits: &'a [bool],
        index: usize
    }

    impl<'a> BitVecDecoder<'a> {
        fn new<'b>(bits: &'b [bool]) -> BitVecDecoder {
            BitVecDecoder {
                bits,
                index: 0
            }
        }
    }

    impl<'a> Decoder for BitVecDecoder<'a> {
        fn decode_bit(&mut self) -> bool {
            let out = self.bits[self.index];
            self.index += 1;
            out
        }
    }

    #[test]
    fn test_fibonacci_simple() {
        let res = encode_fibonacci(12);
        assert_eq!(res, vec![false, false, false, false, false, true, true]);

        let res = encode_fibonacci(16);
        assert_eq!(res, vec![true, false, true, false, false, true, true]);

        let decoded = decode_fibonacci(&mut BitVecDecoder::new(&res));
        assert_eq!(decoded, 16);
    }

    #[test]
    fn test_encode_zero() {
        let res = encode_fibonacci(0);
        assert_eq!(res, vec![true, true]);

        let decoded = decode_fibonacci(&mut BitVecDecoder::new(&res));
        assert_eq!(decoded, 0);
    }
}
