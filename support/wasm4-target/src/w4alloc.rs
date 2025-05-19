use std::{alloc::{GlobalAlloc, Layout}, mem::size_of, ptr::null_mut, cell::RefCell};

struct Chunk {
    next_with_status: u16,
    prev: u16
}

/*
 * Use dedicated function to convert between addresses and pointers.
 * Because the address space in WASM4 is limited to 64K, the addresses used
 * by Chunks are stored in u16 members. To still be able to test the allocator,
 * these functions transparently map the allocators address space to a more
 * reasonable one.
 */

#[cfg(not(test))]
const fn address_to_pointer(address: u16) -> *mut Chunk {
    address as *mut Chunk
}

#[cfg(not(test))]
#[inline]
fn pointer_to_address(pointer: *const Chunk) -> u16 {
    pointer as u16
}

#[cfg(test)]
fn address_to_pointer(address: u16) -> *mut Chunk {
    use std::ptr::addr_of;

    unsafe {
        (addr_of!(test::TEST_MEMORY) as *const u8).offset(address as isize) as *mut Chunk
    }
}

#[cfg(test)]
fn pointer_to_address(pointer: *const Chunk) -> u16 {
    use std::ptr::addr_of;

    unsafe {
        (pointer as *const u8).offset_from(addr_of!(test::TEST_MEMORY) as *const u8) as u16
    }
}

impl Chunk {

    fn is_used(&self) -> bool {
        self.next_with_status & 0x3 != 0
    }

    fn set_used(&mut self, used: bool) {
        if used {
            self.next_with_status |= 1;
        } else {
            self.next_with_status &= !1;
        }
    }

    fn size(&self) -> usize {
        ((self.next_with_status & !0x3) - pointer_to_address(self as *const Chunk)) as usize - size_of::<Chunk>()
    }

    fn body(&self) -> *mut u8 {
        let addr = (self as *const Chunk) as usize + size_of::<Chunk>();
        addr as *mut u8
    }

    unsafe fn split(&mut self, offset: u16) -> *mut Self {
        debug_assert!(offset as usize >= size_of::<Chunk>(), "Offset must be greater than size_of::<Chunk>()");
        debug_assert_eq!(offset & 0x3, 0, "Offset must have an alignment of at least 4 bytes.");

        let address_self = pointer_to_address(self as *const Self);
        debug_assert!(self.next_with_status < address_self || address_self + offset < self.next_with_status, "Offset is not between the given chunks.");

        let address_new = address_self + offset;
        let new_ptr = address_to_pointer(address_new);
        new_ptr.write(Self { next_with_status: self.next_with_status, prev: address_self });

        self.next_with_status = address_new;

        return new_ptr;
    }

    unsafe fn merge_with_prev(&self) {
        debug_assert!(!self.is_used(), "Only free chunks can be merged.");

        let prev_chunk = &mut *address_to_pointer(self.prev);
        debug_assert!(!prev_chunk.is_used(), "Only free chunks can be merged.");
        prev_chunk.next_with_status = self.next_with_status;

        let next_chunk = &mut *address_to_pointer(self.next_with_status);
        next_chunk.prev = self.prev;
    }

    fn calc_alignment_offset(&self, align: usize) -> usize {
        let address = pointer_to_address(self as *const Self) as usize;

        // Round the given pointer up to the next multiple of `align`, making
        // use of the fact that `layout.align()` is always a power of 2.
        let aligned_addr = (address + size_of::<Self>() + (align - 1)) & !(align - 1);
        return aligned_addr - address;
    }

    fn can_hold_layout(&self, size: usize, align: usize) -> bool {
        debug_assert_eq!(size & 0x3, 0);
        debug_assert_eq!(align & 0x3, 0);

        if self.is_used() {
            return false;
        }

        let address = pointer_to_address(self as *const Self) as usize;

        let aligned_addr = address + self.calc_alignment_offset(align);
        return self.next_with_status as usize >= aligned_addr + size;
    }
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    // These are supplied by the linker.
    static __heap_base: u8;
    static __heap_end: u8;
}

pub struct W4Alloc {
    start: RefCell<*mut Chunk>
}

impl W4Alloc {
    pub const fn new() -> W4Alloc {
        W4Alloc { start: RefCell::new(null_mut()) }
    }

    unsafe fn init_heap(&self, heap_base: usize, heap_end: usize) {
        let start_chunk_addr = ((heap_base + 3) & !0x3) as u16;
        let start_chunk = address_to_pointer(start_chunk_addr);
        let terminator_chunk_addr = ((heap_end as usize - size_of::<Chunk>()) & !0x3) as u16;
        let terminator_chunk = address_to_pointer(terminator_chunk_addr);

        start_chunk.write(Chunk { next_with_status: terminator_chunk_addr, prev: terminator_chunk_addr });
        // Use a permanently allocated chunk of size 0 as the end of the heap.
        // This is required, because chunks do not store size information, so the
        // size of a chunk is calculated as the offset to the next chunk. However,
        // this cannot work at the end of the heap, because the chunks would wrap
        // back to the start of the heap.
        terminator_chunk.write(Chunk { next_with_status: start_chunk_addr | 1, prev: start_chunk_addr });
        self.start.replace(start_chunk);
    }

    #[cfg(target_arch = "wasm32")]
    pub unsafe fn init(&self) {
        self.init_heap(&raw const __heap_base as usize, &raw const __heap_end as usize);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub unsafe fn init(&self) {
        panic!("W4Alloc must be manually initialized on targets other than wasm32.");
    }

    #[cfg(test)]
    pub unsafe fn init_test(&self, heap_base: usize, heap_end: usize) {
        self.init_heap(heap_base, heap_end);
    }
}

unsafe impl GlobalAlloc for W4Alloc {

    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if self.start.borrow().is_null() {
            self.init()
        }

        let align = layout.align().max(4);
        let size = layout.size().max(4) + 3 & !0x3;

        let mut current_chunk = &mut **self.start.borrow();

        loop {
            if current_chunk.can_hold_layout(size, align) {
                // Split off alignment
                let alignment_offset = current_chunk.calc_alignment_offset(align);
                if alignment_offset > size_of::<Chunk>() {
                    current_chunk = &mut *current_chunk.split((alignment_offset - size_of::<Chunk>()) as u16);
                }

                // Split off excess size
                if current_chunk.size() > size {
                    let _ = current_chunk.split((size + size_of::<Chunk>()) as u16);
                }

                current_chunk.set_used(true);
                return current_chunk.body();
            } else {
                current_chunk = &mut *address_to_pointer(current_chunk.next_with_status & !0x3);
            }
            if current_chunk as *mut Chunk == *self.start.borrow() {
                return null_mut();
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        let chunk_ptr = ptr.offset(-(size_of::<Chunk>() as isize)) as *mut Chunk;
        let chunk = &mut *chunk_ptr;
        debug_assert!(chunk.is_used(), "Attempted to free an unused chunk.");
        chunk.next_with_status &= !0x3;

        let prev_chunk = &mut *address_to_pointer(chunk.prev);
        if !prev_chunk.is_used() {
            chunk.merge_with_prev();
        }
        let next_chunk = &mut *address_to_pointer(chunk.next_with_status);
        if !next_chunk.is_used() {
            next_chunk.merge_with_prev();
        }
    }
}

unsafe impl Sync for W4Alloc {
    // Lies, but this is required for use as a global allocator.
    // There won't be any threads where this allocator is used.
}

#[cfg(test)]
mod test {
    use std::{ptr::addr_of, alloc::{GlobalAlloc, Layout}};

    use super::{W4Alloc, Chunk};

    const TEST_MEM_SIZE: usize = 1 << 16;
    // Make an array of u32, to guarantee 32bit alignment (otherwise the test executable just aborts).
    pub static mut TEST_MEMORY: [u32; TEST_MEM_SIZE >> 2] = [0; TEST_MEM_SIZE >> 2];

    unsafe fn chunk_at(offset: usize) -> *const Chunk {
        addr_of!(TEST_MEMORY).cast::<u8>().add(offset) as *const Chunk
    }

    #[test]
    fn initialization() {
        unsafe {
            let alloc = W4Alloc::new();
            alloc.init_test(0x2000, 0x4000);

            let start_chunk = chunk_at(0x2000);
            assert_eq!((*start_chunk).next_with_status, 0x3ffc);
            assert_eq!((*start_chunk).prev, 0x3ffc);

            let terminator_chunk = chunk_at(0x3ffc);
            assert_eq!((*terminator_chunk).next_with_status, 0x2001);
            assert_eq!((*terminator_chunk).prev, 0x2000);
        }
    }

    #[test]
    fn alloc() {
        unsafe {
            let alloc = W4Alloc::new();
            alloc.init_test(0x4000, 0x6000);

            let ptr = alloc.alloc(Layout::from_size_align(0x100, 1).unwrap());
            assert!(!ptr.is_null());
            let chunk = chunk_at(0x4000);
            assert_eq!((*chunk).next_with_status, 0x4105);
            assert_eq!((*chunk).prev, 0x5ffc);

            let chunk = chunk_at(0x4104);
            assert_eq!((*chunk).next_with_status, 0x5ffc);
            assert_eq!((*chunk).prev, 0x4000);

            let ptr = alloc.alloc(Layout::from_size_align(0x200, 16).unwrap());
            assert!(!ptr.is_null());
            let chunk = chunk_at(0x4104);
            assert_eq!((*chunk).next_with_status, 0x410c);
            assert_eq!((*chunk).prev, 0x4000);

            let chunk = chunk_at(0x410c);
            assert_eq!((*chunk).next_with_status, 0x4311);
            assert_eq!((*chunk).prev, 0x4104);

            let ptr = alloc.alloc(Layout::from_size_align(0x2000, 1).unwrap());
            assert!(ptr.is_null());
        }
    }

    #[test]
    fn dealloc() {
        unsafe {
            let alloc = W4Alloc::new();
            alloc.init_test(0x6000, 0x8000);

            let layout = Layout::from_size_align(0x100, 1).unwrap();
            let ptr1 = alloc.alloc(layout);
            assert!(!ptr1.is_null());

            let ptr2 = alloc.alloc(layout);
            assert!(!ptr2.is_null());

            let ptr3 = alloc.alloc(layout);
            assert!(!ptr3.is_null());

            alloc.dealloc(ptr1, layout);
            let chunk = chunk_at(0x6000);
            assert!(!(*chunk).is_used());
            assert_eq!((*chunk).next_with_status, 0x6104);
            assert_eq!((*chunk).prev, 0x7ffc);

            alloc.dealloc(ptr2, layout);
            let chunk = chunk_at(0x6000);
            assert!(!(*chunk).is_used());
            assert_eq!((*chunk).next_with_status, 0x6208);
            assert_eq!((*chunk).prev, 0x7ffc);

            let chunk = chunk_at(0x6208);
            assert!((*chunk).is_used());
        }
    }
}
