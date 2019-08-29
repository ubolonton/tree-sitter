use std::os::raw::{c_char, c_void};

use crate::ffi;
use crate::Point;

pub trait Input<C>: Sized {
    fn read(&mut self, byte: usize, point: Point) -> &[C];
}

pub(crate) fn raw_input<T: Input<u8>>(input: &mut T) -> ffi::TSInput {
    unsafe extern "C" fn read<T: Input<u8>>(
        payload: *mut c_void,
        byte_offset: u32,
        position: ffi::TSPoint,
        bytes_read: *mut u32,
    ) -> *const c_char {
        let input = (payload as *mut T).as_mut().unwrap();
        let slice = input.read(byte_offset as usize, position.into());
        *bytes_read = slice.len() as u32;
        slice.as_ptr() as *const c_char
    }

    ffi::TSInput {
        payload: input as *mut T as *mut c_void,
        read: Some(read::<T>),
        encoding: ffi::TSInputEncoding_TSInputEncodingUTF8,
    }
}

pub(crate) fn raw_utf16_input<T: Input<u16>>(input: &mut T) -> ffi::TSInput {
    unsafe extern "C" fn read<T: Input<u16>>(
        payload: *mut c_void,
        byte_offset: u32,
        position: ffi::TSPoint,
        bytes_read: *mut u32,
    ) -> *const c_char {
        let input = (payload as *mut T).as_mut().unwrap();
        let slice = input.read(
            (byte_offset / 2) as usize,
            Point {
                row: position.row as usize,
                column: position.column as usize / 2,
            },
        );
        *bytes_read = slice.len() as u32 * 2;
        slice.as_ptr() as *const c_char
    }

    ffi::TSInput {
        payload: input as *mut T as *mut c_void,
        read: Some(read::<T>),
        encoding: ffi::TSInputEncoding_TSInputEncodingUTF16,
    }
}

/// Input that borrows text fragments from the original source.
pub struct Borrowing<'a, C: 'static, T: FnMut(usize, Point) -> &'a [C]> {
    input: T,
}

impl<'a, C: 'static, T: FnMut(usize, Point) -> &'a [C]> Borrowing<'a, C, T> {
    pub fn new(input: T) -> Self {
        Self { input }
    }
}

impl<'a, C: 'static, T: FnMut(usize, Point) -> &'a [C]> Input<C> for Borrowing<'a, C, T> {
    fn read(&mut self, byte: usize, point: Point) -> &[C] {
        (&mut self.input)(byte, point)
    }
}

/// Input that clones text fragments from the original source.
pub struct Cloning<T, S> {
    buffer: Option<S>,
    input: T,
}

impl<T, S> Cloning<T, S> {
    pub fn new(input: T) -> Self {
        Self { input, buffer: None }
    }
}

impl<C: 'static, T: FnMut(usize, Point) -> S, S: AsRef<[C]>> Input<C> for Cloning<T, S> {
    fn read(&mut self, byte: usize, point: Point) -> &[C] {
        let buffer = (&mut self.input)(byte, point);
        self.buffer.replace(buffer);
        self.buffer.as_ref().unwrap().as_ref()
    }
}

