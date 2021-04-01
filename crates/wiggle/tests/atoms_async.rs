use proptest::prelude::*;
use std::cell::Cell;
use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use wiggle::state::{HostFuture, HostState};
use wiggle::GuestMemory;
use wiggle_test::{impl_errno, HostMemory, MemArea, WasiCtx};

wiggle::from_witx!({
    witx: ["$CARGO_MANIFEST_DIR/tests/atoms.witx"],
    async: {
        atoms::{int_float_args, double_int_return_float}
    }
});

impl_errno!(types::Errno);

#[wiggle::async_trait]
impl atoms::Atoms for WasiCtx {
    async fn int_float_args(
        host_state: &HostState<Self>,
        an_int: u32,
        an_float: f32,
    ) -> Result<(), types::Errno> {
        host_state.with(|s| s.log(format!("int_float_args: {} {}", an_int, an_float)));
        Ok(())
    }
    async fn double_int_return_float(
        host_state: &HostState<Self>,
        an_int: u32,
    ) -> Result<types::AliasToFloat, types::Errno> {
        host_state.with(|s| s.log(format!("double_int_return_float: {}", an_int)));
        Ok((an_int as f32) * 2.0)
    }
}

// There's nothing meaningful to test here - this just demonstrates the test machinery

#[derive(Debug)]
struct IntFloatExercise {
    pub an_int: u32,
    pub an_float: f32,
}

impl IntFloatExercise {
    pub fn test(&self) {
        let ctx = WasiCtx::new();
        let host_memory = HostMemory::new();

        thread_local!(static PTR: Cell<usize> = Cell::new(0));
        let host_state: HostState<WasiCtx> = unsafe { HostState::new(&PTR) };

        let fut =
            atoms::int_float_args(&host_state, &host_memory, self.an_int as i32, self.an_float);
        let e = run(unsafe { HostFuture::new(&ctx, &PTR, fut) });

        assert_eq!(e, Ok(types::Errno::Ok as i32), "int_float_args error");
        assert_eq!(
            ctx.log.borrow().deref(),
            &[format!("int_float_args: {} {}", self.an_int, self.an_float)]
        );
    }

    pub fn strat() -> BoxedStrategy<Self> {
        (prop::num::u32::ANY, prop::num::f32::ANY)
            .prop_map(|(an_int, an_float)| IntFloatExercise { an_int, an_float })
            .boxed()
    }
}

proptest! {
    #[test]
    fn int_float_exercise(e in IntFloatExercise::strat()) {
        e.test()
    }
}
#[derive(Debug)]
struct DoubleIntExercise {
    pub input: u32,
    pub return_loc: MemArea,
}

impl DoubleIntExercise {
    pub fn test(&self) {
        let ctx = WasiCtx::new();
        let host_memory = HostMemory::new();

        thread_local!(static PTR: Cell<usize> = Cell::new(0));
        let host_state: HostState<WasiCtx> = unsafe { HostState::new(&PTR) };

        let fut = atoms::double_int_return_float(
            &host_state,
            &host_memory,
            self.input as i32,
            self.return_loc.ptr as i32,
        );

        let e = run(unsafe { HostFuture::new(&ctx, &PTR, fut) });

        let return_val = host_memory
            .ptr::<types::AliasToFloat>(self.return_loc.ptr)
            .read()
            .expect("failed to read return");
        assert_eq!(e, Ok(types::Errno::Ok as i32), "errno");
        assert_eq!(return_val, (self.input as f32) * 2.0, "return val");

        assert_eq!(
            ctx.log.borrow().deref(),
            &[format!("double_int_return_float: {}", self.input)]
        );
    }

    pub fn strat() -> BoxedStrategy<Self> {
        (prop::num::u32::ANY, HostMemory::mem_area_strat(4))
            .prop_map(|(input, return_loc)| DoubleIntExercise { input, return_loc })
            .boxed()
    }
}

proptest! {
    #[test]
    fn double_int_return_float(e in DoubleIntExercise::strat()) {
        e.test()
    }
}

fn run<F: Future>(future: F) -> F::Output {
    let mut f = Pin::from(Box::new(future));
    let waker = dummy_waker();
    let mut cx = Context::from_waker(&waker);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(val) => break val,
            Poll::Pending => {}
        }
    }
}

fn dummy_waker() -> Waker {
    return unsafe { Waker::from_raw(clone(5 as *const _)) };

    unsafe fn clone(ptr: *const ()) -> RawWaker {
        assert_eq!(ptr as usize, 5);
        const VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
        RawWaker::new(ptr, &VTABLE)
    }

    unsafe fn wake(ptr: *const ()) {
        assert_eq!(ptr as usize, 5);
    }

    unsafe fn wake_by_ref(ptr: *const ()) {
        assert_eq!(ptr as usize, 5);
    }

    unsafe fn drop(ptr: *const ()) {
        assert_eq!(ptr as usize, 5);
    }
}
