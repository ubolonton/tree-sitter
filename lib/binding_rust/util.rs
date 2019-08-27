use std::os::raw::c_void;

extern "C" {
    #[link_name = "rust_tree_sitter_free"]
    pub fn free_ptr(ptr: *mut c_void);
}

pub struct CBufferIter<T> {
    ptr: *mut T,
    count: usize,
    i: usize,
}

impl<T> CBufferIter<T> {
    pub unsafe fn new(ptr: *mut T, count: usize) -> Self {
        Self { ptr, count, i: 0 }
    }
}

impl<T: Copy> Iterator for CBufferIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let i = self.i;
        self.i += 1;
        if i >= self.count {
            None
        } else {
            Some(unsafe { *self.ptr.offset(i as isize) })
        }
    }
}

impl<T> Drop for CBufferIter<T> {
    fn drop(&mut self) {
        unsafe { free_ptr(self.ptr as *mut c_void); }
    }
}
