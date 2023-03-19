#![cfg_attr(not(feature = "std"), no_std)]

use core::ffi::c_void;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct Allocator {
    _container: [u8; core::mem::size_of::<&mut dyn MallocFree>()],
}

pub type AllocatorHandle = *const Allocator;

pub type OnDropWithoutFree = fn();

#[derive(Debug)]
pub enum Action {
    Free,
    Malloc,
    Error,
}

#[derive(Debug)]
pub struct Status {
    pub action: Action,
    pub count: usize,
    pub usage: usize,
    pub maximum_usage: usize,
}

fn no_panic_on_drop_without_free() {}

pub trait MallocFree {
    fn malloc(self: &mut Self, size: usize) -> *mut c_void;
    fn free(self: &mut Self, _ptr: *mut c_void);
    fn get_allocator(self: &mut Self) -> Allocator;
}

pub struct Bump<const SIZE: usize, const ALIGNMENT: usize> {
    count: usize,
    head: usize,
    pub heap: [u8; SIZE],
    maximum_usage: usize,
    on_drop_without_free: OnDropWithoutFree,
    on_changed: Option<fn(Status)>,
}

impl<const SIZE: usize, const ALIGNMENT: usize> Bump<SIZE, ALIGNMENT> {
    pub fn new() -> Self {
        Self {
            count: 0,
            head: 0,
            heap: [0; SIZE],
            maximum_usage: 0,
            on_drop_without_free: no_panic_on_drop_without_free,
            on_changed: None,
        }
    }

    pub fn get_count(self: &Self) -> usize {
        self.count
    }

    pub fn get_maximum_usage(self: &Self) -> usize {
        self.maximum_usage
    }

    pub fn handle_drop_without_free(self: &mut Self, handler: OnDropWithoutFree) {
        self.on_drop_without_free = handler;
    }

    pub fn handle_on_changed(self: &mut Self, handler: fn(Status)) {
        self.on_changed = Some(handler);
    }

    fn changed(self: &Self, action: Action){
        if let Some(handler) = self.on_changed {
            handler(Status {
                action,
                count: self.count,
                usage: self.head,
                maximum_usage: self.maximum_usage
            })
        }
    }

}

impl<const SIZE: usize, const ALIGNMENT: usize> MallocFree for Bump<SIZE, ALIGNMENT> {
    fn malloc(self: &mut Self, size: usize) -> *mut c_void {
        let next_head = self.head + ((size + ALIGNMENT - 1) / ALIGNMENT) * ALIGNMENT;
        if next_head > SIZE {
            self.changed(Action::Error);
            return core::ptr::null_mut();
        }
        let result = &mut self.heap[self.head] as *mut u8;
        self.head = next_head;
        if self.maximum_usage < self.head {
            self.maximum_usage = self.head;
        }
        self.count = self.count + 1;
        self.changed(Action::Malloc);
        result as *mut c_void
    }

    fn free(self: &mut Self, _ptr: *mut c_void) {
        //if no items are used, reset the head
        if self.count > 0 {
            self.count = self.count - 1;
            if self.count == 0 {
                self.head = 0;
            }
        }
        self.changed(Action::Free);
    }

    fn get_allocator(self: &mut Self) -> Allocator {
        unsafe { core::mem::transmute(self as &mut dyn MallocFree) }
    }
}

impl Allocator {
    pub fn get_handle(self: Self) -> AllocatorHandle {
        return &self;
    }
}

impl<const SIZE: usize, const ALIGNMENT: usize> Drop for Bump<SIZE, ALIGNMENT> {
    fn drop(self: &mut Self) {
        if self.count > 0 {
            (self.on_drop_without_free)();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "std")]
    #[test]
    fn bump_malloc_free() {
        type BigBump = Bump<1024, 8>;
        fn get_location(bump: &BigBump, location: usize) -> *const c_void {
            &bump.heap[location] as *const u8 as *const c_void
        }

        let mut bump = BigBump::new();
        let bottom_of_heap = get_location(&bump, 0);

        let first = bump.malloc(20);
        assert_eq!(bottom_of_heap, first as *const c_void);
        let second = bump.malloc(20);
        let second_heap_loc = get_location(&bump, 24);
        assert_eq!(second_heap_loc, second as *const c_void);

        let no_space = bump.malloc(1024);
        assert_eq!(no_space, core::ptr::null_mut());
        bump.free(first);
        assert_ne!(bottom_of_heap, get_location(&bump, bump.head));
        bump.free(first);
        assert_eq!(bottom_of_heap, get_location(&bump, bump.head));
    }
}
