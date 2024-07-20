use std::{collections::hash_map::DefaultHasher, hash::Hasher};

use skylite_core::SkyliteTarget;

#[derive(Debug, PartialEq)]
pub enum Call {
    DrawSub {
        // Hash of the actual data. Storing the actual data here
        // would use up too much memory.
        data: u64,
        x: i16,
        y: i16,
        src_x: i16,
        src_y: i16,
        src_w: u16,
        src_h: u16,
        flip_h: bool,
        flip_v: bool,
        rotate: bool
    },
    SaveState {
        location: String,
        data: Vec<u8>
    }
}

fn apply_transform(pos: (i16, i16), w: u16, h: u16, flip_h: bool, flip_v: bool, rotate: bool) -> (i16, i16) {
    let pos = if flip_h {
        (w as i16 - pos.0 - 1, pos.1)
    } else {
        pos
    };

    let pos = if flip_v {
        (pos.0, h as i16 - pos.1 - 1)
    } else {
        pos
    };

    if rotate {
        (h as i16 - pos.1 - 1, pos.0)
    } else {
        pos
    }
}

pub struct MockTarget {
    pub call_history: Vec<Call>,
    pub screen_buffer: [u8; 128 * 128],
    pub state: Vec<u8>
}

impl SkyliteTarget for MockTarget {
    fn new() -> Self {
        MockTarget {
            call_history: Vec::new(),
            screen_buffer: [0; 128 * 128],
            state: Vec::new()
        }
    }

    fn draw_sub(&mut self, data: &[u8], x: i16, y: i16, src_x: i16, src_y: i16, src_w: u16, src_h: u16, flip_h: bool, flip_v: bool, rotate: bool) {
        let mut hasher = DefaultHasher::new();
        hasher.write(data);
        self.call_history.push(Call::DrawSub { data: hasher.finish(), x, y, src_x, src_y, src_w, src_h, flip_h, flip_v, rotate });

        let data_width = data[data.len() - 1] as i16;
        for offset_y in 0..src_h as i16 {
            for offset_x in 0..src_w as i16 {
                let src_index = (src_y + offset_y) * data_width + src_x + offset_x;
                let screen_offset = apply_transform((offset_x, offset_y), src_w, src_h, flip_h, flip_v, rotate);
                let screen_index = (y + screen_offset.1) * 128 + x + screen_offset.0;
                self.screen_buffer[screen_index as usize] = data[src_index as usize];
            }
        }
    }

    fn get_screen_size() -> (u16, u16) {
        (128, 128)
    }

    fn save_state(&mut self, location: &str, data: &[u8]) {
        self.state = data.to_owned();
        self.call_history.push(Call::SaveState { location: location.to_owned(), data: data.to_owned() });
    }

    fn load_state(&self, _location: &str) -> Vec<u8> {
        self.state.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::hash_map::DefaultHasher, hash::Hasher};

    use crate::{Call, SkyliteTarget};

    use super::MockTarget;


    #[test]
    fn test_draw_sub() {
        let graphics_data: &[u8] = &[
            0, 1, 2, 3, 4, 5, 6, 7,
            1, 2, 3, 4, 5, 6, 7, 8,
            2, 3, 4, 5, 6, 7, 8, 9,
            3, 4, 5, 6, 7, 8, 9, 10,
            4, 5, 6, 7, 8, 9, 10, 11,
            5, 6, 7, 8, 9, 10, 11, 12,
            6, 7, 8, 9, 10, 11, 12, 13,
            7, 8, 9, 10, 11, 12, 13, 14,
            8
        ];
        let graphics_data_hash = {
            let mut hasher = DefaultHasher::new();
            hasher.write(graphics_data);
            hasher.finish()
        };
        let mut target = MockTarget::new();
        target.draw_sub(graphics_data, 0, 0, 0, 0, 8, 8, false, false, false);
        target.draw_sub(graphics_data, 8, 0, 0, 0, 8, 8, true, false, false);
        target.draw_sub(graphics_data, 16, 0, 0, 0, 8, 8, false, true, false);
        target.draw_sub(graphics_data, 24, 0, 0, 0, 8, 8, true, true, false);
        target.draw_sub(graphics_data, 0, 8, 0, 0, 8, 8, false, false, true);
        target.draw_sub(graphics_data, 8, 8, 0, 0, 8, 8, true, false, true);
        target.draw_sub(graphics_data, 16, 8, 0, 0, 8, 8, false, true, true);
        target.draw_sub(graphics_data, 24, 8, 0, 0, 8, 8, true, true, true);

        assert_eq!(target.call_history.len(), 8);
        assert_eq!(target.call_history[0], Call::DrawSub { data: graphics_data_hash, x: 0, y: 0, src_x: 0, src_y: 0, src_w: 8, src_h: 8, flip_h: false, flip_v: false, rotate: false });
        assert_eq!(target.call_history[7], Call::DrawSub { data: graphics_data_hash, x: 24, y: 8, src_x: 0, src_y: 0, src_w: 8, src_h: 8, flip_h: true, flip_v: true, rotate: true });

        // Row 0
        assert_eq!(&target.screen_buffer[0..32], &[0, 1, 2, 3, 4, 5, 6, 7,  7, 6, 5, 4, 3, 2, 1, 0,  7, 8, 9, 10, 11, 12, 13, 14,  14, 13, 12, 11, 10, 9, 8, 7]);
        // Row 7
        assert_eq!(&target.screen_buffer[896..928], &[7, 8, 9, 10, 11, 12, 13, 14,  14, 13, 12, 11, 10, 9, 8, 7,  0, 1, 2, 3, 4, 5, 6, 7,  7, 6, 5, 4, 3, 2, 1, 0]);
        // Row 8
        assert_eq!(&target.screen_buffer[1024..1056], &[7, 6, 5, 4, 3, 2, 1, 0,  14, 13, 12, 11, 10, 9, 8, 7,  0, 1, 2, 3, 4, 5, 6, 7,  7, 8, 9, 10, 11, 12, 13, 14]);
        // Row 15
        assert_eq!(&target.screen_buffer[1920..1952], &[14, 13, 12, 11, 10, 9, 8, 7,  7, 6, 5, 4, 3, 2, 1, 0,  7, 8, 9, 10, 11, 12, 13, 14,  0, 1, 2, 3, 4, 5, 6, 7]);
    }
}
