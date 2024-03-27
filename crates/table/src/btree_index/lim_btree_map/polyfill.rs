use core::mem::MaybeUninit;
use core::ptr::{slice_from_raw_parts_mut, NonNull};
use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};

pub trait Allocator {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, ()> {
        unsafe {
            let len = layout.size();
            let data = if len == 0 {
                layout.align() as _
            } else {
                NonNull::new(alloc(layout)).ok_or(())?.as_ptr()
            };
            Ok(NonNull::new_unchecked(slice_from_raw_parts_mut(data, len)))
        }
    }

    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() != 0 {
            unsafe { dealloc(ptr.as_ptr(), layout) }
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Global;

impl Allocator for Global {}

#[macro_export]
macro_rules! A {
    (Box<$t:ty$(, $a:ty)?>) => { Box<$t> };
    (Vec<$t:ty$(, $a:ty)?>) => { Vec<$t> };
}

pub trait NewUninit<T, A: Allocator = Global> {
    type SelfWithAlloc<U: ?Sized, B: Allocator>;

    unsafe fn from_raw_in(ptr: *mut T, alloc: A) -> Self::SelfWithAlloc<T, Global>;

    fn new_uninit_in(alloc: A) -> Self::SelfWithAlloc<MaybeUninit<T>, A>;
    fn new_uninit() -> Self::SelfWithAlloc<MaybeUninit<T>, Global>;
}

pub trait AssumeInit<T, A: Allocator = Global>: NewUninit<MaybeUninit<T>, A> {
    unsafe fn assume_init(self) -> Self::SelfWithAlloc<T, A>;
}

impl<T, A: Allocator> NewUninit<T, A> for A!(Box<T, A>) {
    type SelfWithAlloc<U: ?Sized, B: Allocator> = A!(Box<U, B>);

    #[inline]
    unsafe fn from_raw_in(ptr: *mut T, _: A) -> A!(Box<T, Global>) {
        Self::from_raw(ptr)
    }

    #[inline]
    fn new_uninit_in(alloc: A) -> A!(Box<MaybeUninit<T>, A>) {
        unsafe {
            let layout = Layout::new::<MaybeUninit<T>>();
            match alloc.allocate(layout) {
                Ok(ptr) => <A!(Box<_, _>)>::from_raw_in(ptr.cast().as_ptr(), alloc),
                Err(_) => handle_alloc_error(layout),
            }
        }
    }

    #[inline]
    fn new_uninit() -> A!(Box<MaybeUninit<T>>) {
        <A!(Box<_>)>::new_uninit_in(Global)
    }
}

impl<T> AssumeInit<T, Global> for A!(Box<MaybeUninit<T>>) {
    #[inline]
    unsafe fn assume_init(self) -> A!(Box<T>) {
        let raw = Self::into_raw(self);
        <A!(Box<_>)>::from_raw(raw as *mut T)
    }
}

pub trait MaybeUninitSlice<T> {
    unsafe fn slice_assume_init_ref(slice: &[MaybeUninit<T>]) -> &[T];
}

impl<T> MaybeUninitSlice<T> for MaybeUninit<T> {
    unsafe fn slice_assume_init_ref(slice: &[MaybeUninit<T>]) -> &[T] {
        &*(slice as *const [MaybeUninit<T>] as *const [T])
    }
}
