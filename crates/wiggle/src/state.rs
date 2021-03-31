use std::cell::Cell;
use std::future::Future;
use std::marker;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::thread::LocalKey;

pub struct HostState<T> {
    key: &'static LocalKey<Cell<usize>>,
    _marker: marker::PhantomData<fn() -> T>,
}

pub struct HostFuture<'a, T, F> {
    ptr: &'a T,
    key: &'static LocalKey<Cell<usize>>,
    future: F,
}

impl<'a, T, F> HostFuture<'a, T, F> {
    /// Unsafety: only construct from generated code!
    pub unsafe fn new(ptr: &'a T, key: &'static LocalKey<Cell<usize>>, future: F) -> Self {
        Self { ptr, key, future }
    }
}

impl<T, F> Future for HostFuture<'_, T, F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        self.key.with(|c| {
            // Replace the previous value with our pointer, and then when we
            // finish with this `poll` we'll reset it back.
            struct Reset<'a, T: Copy>(&'a Cell<T>, T);
            impl<T: Copy> Drop for Reset<'_, T> {
                fn drop(&mut self) {
                    self.0.set(self.1);
                }
            }
            let prev = c.replace(self.ptr as *const _ as usize);
            let _reset = Reset(c, prev);

            // this is just pin projection unsafety, should be fine since
            // it's the only way we access `self.future`
            unsafe { self.map_unchecked_mut(|f| &mut f.future).poll(cx) }
        })
    }
}

impl<T> HostState<T> {
    // Not generally safe due to safety of `with`, not intended to be
    // called by users. Only called from generated code.
    pub unsafe fn new(key: &'static LocalKey<Cell<usize>>) -> HostState<T> {
        HostState {
            key,
            _marker: marker::PhantomData,
        }
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        // Safety here depends on `&HostState` only being accessible within
        // a context that it's configured, which due to the unsafety of
        // `new` should be ok.
        //
        // Note that the `assert` is needed for safety if this value crosses
        // to another thread by accident.
        self.key.with(|p| {
            assert!(p.get() != 0);
            unsafe { f(&*(p.get() as *const T)) }
        })
    }
}
