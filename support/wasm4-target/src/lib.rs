mod wasm4;
pub mod w4alloc;

use skylite_core::SkyliteTarget;
pub use wasm4::*;

pub struct Wasm4Target {
    disk_used: u32
}

impl Wasm4Target {
    pub fn new() -> Wasm4Target {
        Wasm4Target {
            disk_used: 0
        }
    }
}

impl SkyliteTarget for Wasm4Target {
    fn draw_sub(&mut self, data: &[u8], x: i16, y: i16, src_x: i16, src_y: i16, src_w: u16, src_h: u16, flip_h: bool, flip_v: bool, rotate: bool) {
        let atlas_width = u16::from_le_bytes([data[data.len() - 2], data[data.len() - 1]]) as u32;
        let flags = (if flip_h { BLIT_FLIP_X } else { 0 })
            + (if flip_v { BLIT_FLIP_Y } else { 0 })
            + (if rotate { BLIT_ROTATE } else { 0 })
            + BLIT_2BPP;
        blit_sub(data, x as i32, y as i32, src_w as u32, src_h as u32, src_x as u32, src_y as u32, atlas_width, flags);
    }

    fn get_screen_size(&self) -> (u16, u16) {
        (SCREEN_SIZE as u16, SCREEN_SIZE as u16)
    }

    fn write_storage(&mut self, offset: usize, data: &[u8]) {
        let buffer_len = usize::max(self.disk_used as usize, offset + data.len());
        let mut buffer = Vec::from_iter(std::iter::repeat(0).take(buffer_len));
        unsafe {
            diskr(buffer.as_mut_ptr(), self.disk_used);
        }

        for i in 0..data.len() {
            buffer[i + offset] = data[i]
        }

        let real_len = unsafe {
            diskw(buffer.as_ptr(), buffer.len() as u32)
        };
        self.disk_used = real_len;
    }

    fn read_storage(&self, offset: usize, len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len);
        unsafe {
            let real_len = diskr(out.as_mut_ptr(), (offset + len) as u32);
            out.set_len(real_len as usize);
        }
        out
    }
}
