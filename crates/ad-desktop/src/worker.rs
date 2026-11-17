//! One background job at a time; the latest ticket wins. Shared by import
//! and edit preparation (ADR-0031). Workers never publish: the UI thread
//! polls a completion and decides whether its ticket is still current.
use ad_document::{AssetError, AssetResult, Cancellation, ErrorCode};
use std::{
    sync::{Arc, Condvar, Mutex},
    thread,
};

pub struct Request<Q> {
    pub ticket: u64,
    pub input: Q,
    pub cancel: Cancellation,
}
pub struct Completion<R> {
    pub ticket: u64,
    pub result: AssetResult<R>,
}
struct State<Q, R> {
    pending: Option<Request<Q>>,
    result: Option<Completion<R>>,
    ticket: u64,
    stop: bool,
    active: bool,
}
pub struct Worker<Q, R> {
    shared: Arc<(Mutex<State<Q, R>>, Condvar)>,
    cancel: Cancellation,
    ticket: u64,
}

impl<Q: Send + 'static, R: Send + 'static> Worker<Q, R> {
    pub fn new(job: fn(&Request<Q>) -> AssetResult<R>, panic_message: &'static str) -> Self {
        let shared = Arc::new((
            Mutex::new(State {
                pending: None,
                result: None,
                ticket: 0,
                stop: false,
                active: false,
            }),
            Condvar::new(),
        ));
        let worker = shared.clone();
        thread::spawn(move || {
            loop {
                let request = {
                    let (lock, ready) = &*worker;
                    let mut state = lock.lock().expect("Worker lock");
                    while state.pending.is_none() && !state.stop {
                        state = ready.wait(state).expect("Worker wake lock");
                    }
                    if state.stop {
                        return;
                    }
                    state.active = true;
                    state.pending.take().expect("Pending request checked")
                };
                // A third-party parser failure must not strand every future
                // request; this boundary also keeps the worker usable after
                // an unwind.
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(&request)))
                        .unwrap_or_else(|_| {
                            Err(AssetError::new(ErrorCode::InvalidFormat, panic_message))
                        });
                let ticket = request.ticket;
                // Release the input (e.g. a shared Session) before the result
                // becomes visible, so the UI thread can obtain exclusive access.
                drop(request);
                let mut state = worker.0.lock().expect("Worker completion lock");
                state.active = false;
                if state.ticket == ticket && !state.stop {
                    state.result = Some(Completion { ticket, result });
                }
            }
        });
        Self {
            shared,
            cancel: Cancellation::default(),
            ticket: 0,
        }
    }
    pub fn request(&mut self, input: Q) -> u64 {
        self.cancel.cancel();
        self.cancel = Cancellation::default();
        self.ticket += 1;
        let mut state = self.shared.0.lock().expect("Worker request lock");
        state.ticket = self.ticket;
        state.result = None;
        state.pending = Some(Request {
            ticket: self.ticket,
            input,
            cancel: self.cancel.clone(),
        });
        self.shared.1.notify_one();
        self.ticket
    }
    pub fn cancel(&mut self) {
        self.cancel.cancel();
        self.ticket += 1;
        let mut state = self.shared.0.lock().expect("Worker cancel lock");
        state.ticket = self.ticket;
        state.pending = None;
        state.result = None;
    }
    pub fn ticket(&self) -> u64 {
        self.ticket
    }
    pub fn poll(&mut self) -> Option<Completion<R>> {
        self.shared
            .0
            .lock()
            .expect("Worker poll lock")
            .result
            .take()
    }
    #[cfg(test)]
    pub fn is_idle(&self) -> bool {
        let state = self.shared.0.lock().unwrap();
        !state.active && state.pending.is_none()
    }
}
impl<Q, R> Drop for Worker<Q, R> {
    fn drop(&mut self) {
        self.cancel.cancel();
        let mut state = self.shared.0.lock().expect("Worker shutdown lock");
        state.stop = true;
        state.pending = None;
        state.result = None;
        self.shared.1.notify_one();
        // No UI-thread join on slow filesystem reads; the worker owns only
        // read data and cooperatively exits. It never writes user files.
    }
}
