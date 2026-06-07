//! Runtime abstraction over tokio (native) and wasm-bindgen-futures / gloo-timers (wasm32).

use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use super::error::Error;

#[derive(Debug)]
pub struct JoinError;

impl fmt::Display for JoinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JoinError")
    }
}

impl std::error::Error for JoinError {}

pub struct JoinHandle<T: Send> {
    rx: tokio::sync::oneshot::Receiver<T>,
}

impl<T: Send> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.rx).poll(cx) {
            Poll::Ready(Ok(v)) => Poll::Ready(Ok(v)),
            Poll::Ready(Err(_)) => Poll::Ready(Err(JoinError)),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
macro_rules! spawn_impl {
    ($fut:expr) => {{
        tokio::spawn($fut);
    }};
}

#[cfg(target_arch = "wasm32")]
macro_rules! spawn_impl {
    ($fut:expr) => {
        wasm_bindgen_futures::spawn_local($fut)
    };
}

pub fn spawn<F, T>(future: F) -> JoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    spawn_impl!(async move {
        let _ = tx.send(future.await);
    });
    JoinHandle { rx }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn timeout<F: Future>(duration: Duration, future: F) -> Result<F::Output, Error> {
    tokio::time::timeout(duration, future)
        .await
        .map_err(|_| Error::Timeout)
}

// wasm32-unknown-unknown is single-threaded, so Send is trivially safe
#[cfg(target_arch = "wasm32")]
struct SendWrapper<F>(F);

#[cfg(target_arch = "wasm32")]
unsafe impl<F> Send for SendWrapper<F> {}

#[cfg(target_arch = "wasm32")]
unsafe impl<F> Sync for SendWrapper<F> {}

#[cfg(target_arch = "wasm32")]
impl<F: Future> Future for SendWrapper<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe { self.map_unchecked_mut(|w| &mut w.0) }.poll(cx)
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn timeout<F: Future>(duration: Duration, future: F) -> Result<F::Output, Error> {
    let timer = SendWrapper(gloo_timers::future::TimeoutFuture::new(
        duration.as_millis() as u32,
    ));
    tokio::pin!(future);
    tokio::pin!(timer);
    tokio::select! {
        v = &mut future => Ok(v),
        _ = &mut timer => Err(Error::Timeout),
    }
}
