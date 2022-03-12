use std::{
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering},
};

pub struct DoubleState<T: Synchronize> {
    readable: AtomicPtr<(AtomicU32, T)>,
    writable: *mut (AtomicU32, T),
    writing: AtomicBool,
}

unsafe impl<T: Send + Synchronize> Send for DoubleState<T> {}
unsafe impl<T: Sync + Synchronize> Sync for DoubleState<T> {}

impl<T: Synchronize + Clone> DoubleState<T> {
    pub fn new(init: T) -> Self {
        Self {
            readable: AtomicPtr::new(Box::into_raw(Box::new((AtomicU32::new(0), init.clone())))),
            writable: Box::into_raw(Box::new((AtomicU32::new(0), init.clone()))),
            writing: AtomicBool::new(false),
        }
    }

    pub fn borrow<'a>(&'a self) -> DoubleStateBorrow<'a, T> {
        let state = unsafe { &(*self.readable.load(Ordering::Relaxed)) };
        state.0.fetch_add(1, Ordering::Relaxed);
        DoubleStateBorrow { state }
    }

    pub fn borrow_mut<'a>(&'a self) -> DoubleStateBorrowMut<'a, T> {
        while let Ok(_) =
            self.writing
                .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        {
            std::hint::spin_loop();
        }

        DoubleStateBorrowMut {
            state: unsafe { &mut *(self as *const _ as *mut _) },
        }
    }
}

impl<T: Synchronize> Drop for DoubleState<T> {
    fn drop(&mut self) {
        unsafe {
            Box::from_raw(self.readable.get_mut());
            Box::from_raw(self.writable);
        }
    }
}

pub struct DoubleStateBorrow<'a, T: Synchronize> {
    state: &'a (AtomicU32, T),
}

impl<'a, T: Synchronize> Drop for DoubleStateBorrow<'a, T> {
    fn drop(&mut self) {
        self.state.0.fetch_sub(1, Ordering::Relaxed);
    }
}

impl<'a, T: Synchronize> Deref for DoubleStateBorrow<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.state.1
    }
}

pub struct DoubleStateBorrowMut<'a, T: Synchronize> {
    state: &'a mut DoubleState<T>,
}

impl<'a, T: Synchronize> Drop for DoubleStateBorrowMut<'a, T> {
    fn drop(&mut self) {
        let prev = self
            .state
            .readable
            .swap(self.state.writable, Ordering::Relaxed);
        self.state.writable = prev;
        unsafe {
            while (*self.state.writable).0.load(Ordering::Relaxed) != 0 {
                std::hint::spin_loop()
            }
            (*self.state.writable)
                .1
                .synchronize(&(**self.state.readable.get_mut()).1)
        }
        self.state.writing.store(false, Ordering::Relaxed);
    }
}

impl<'a, T: Synchronize> Deref for DoubleStateBorrowMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &(*self.state.writable).1 }
    }
}

impl<'a, T: Synchronize> DerefMut for DoubleStateBorrowMut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut (*self.state.writable).1 }
    }
}

pub trait Synchronize {
    fn synchronize(&mut self, other: &Self);
}
