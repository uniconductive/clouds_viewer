pub type MessagesSender<T> = tokio::sync::mpsc::UnboundedSender<T>;
pub type MessagesReceiver<T> = tokio::sync::mpsc::UnboundedReceiver<T>;

struct FutureHolder<F>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    inner: Option<F>,
}

trait FutureStatePrinter {
    fn print_state(&mut self);
}

impl<F> FutureStatePrinter for FutureHolder<F>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn print_state(&mut self) {
        use futures::FutureExt; // for now_or_never()
        if let Some(f) = self.inner.take() {
            if let Some(_) = f.now_or_never() {
            } else {
                log::error!("...........................hyper future still working...............................");
            }
        }
    }
}

#[derive(Clone)]
pub struct RuntimeHolder {
    pub inner: std::sync::Arc<std::sync::RwLock<Option<tokio::runtime::Runtime>>>,
    hyper_handles:
        std::sync::Arc<std::sync::RwLock<Vec<Box<dyn FutureStatePrinter + Send + Sync>>>>,
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum AsyncRuntimeError {
    #[error("tokio runtime not exists anymore")]
    RuntimeNotExistsAnymore,
}

impl RuntimeHolder {
    pub fn spawn<T>(
        &self,
        future: T,
    ) -> Result<tokio::task::JoinHandle<T::Output>, AsyncRuntimeError>
    where
        T: std::future::Future + Send + 'static,
        T::Output: Send + 'static,
    {
        let rtg = self.inner.read().unwrap();
        if let Some(rt) = rtg.as_ref() {
            Ok(rt.spawn(future))
        } else {
            Err(AsyncRuntimeError::RuntimeNotExistsAnymore)
        }
    }

    fn spawn_for_hyper<T>(&self, future: T)
    where
        T: std::future::Future + Send + 'static,
        T::Output: Send + 'static,
    {
        let rtg = self.inner.read().unwrap();
        if let Some(rt) = rtg.as_ref() {
            let h = rt.spawn(future);
            {
                let list = &mut *self.hyper_handles.write().unwrap();
                let t = Box::new(FutureHolder { inner: Some(h) })
                    as Box<dyn FutureStatePrinter + Send + Sync>;
                list.push(t);
            }
        } else {
            log::error!("hyper::rt::Executor.execute after shutdown of async runtime");
        }
    }

    pub fn block_on<T: std::future::Future>(
        &self,
        future: T,
    ) -> Result<T::Output, AsyncRuntimeError> {
        let rtg = self.inner.read().unwrap();
        if let Some(rt) = rtg.as_ref() {
            Ok(rt.block_on(future))
        } else {
            Err(AsyncRuntimeError::RuntimeNotExistsAnymore)
        }
    }

    pub fn shutdown_timeout(
        &self,
        duration: std::time::Duration,
    ) -> Result<(), AsyncRuntimeError> {
        let rto = {
            let mut rtg = self.inner.write().unwrap();
            rtg.take()
        };
        if let Some(rt) = rto {
            rt.shutdown_timeout(duration);
            Ok(())
        } else {
            Err(AsyncRuntimeError::RuntimeNotExistsAnymore)
        }
    }

    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(std::sync::RwLock::new(Some(
                tokio::runtime::Runtime::new().unwrap(),
            ))),
            hyper_handles: std::sync::Arc::new(std::sync::RwLock::new(vec![])),
        }
    }

    pub fn print_hyper_futures_states(&mut self) {
        let list = &mut *self.hyper_handles.write().unwrap();
        while let Some(mut item) = list.pop() {
            item.print_state();
        }
    }
}

impl<T> hyper::rt::Executor<T> for RuntimeHolder
where
    T: std::future::Future + Send + 'static,
    T::Output: Send + 'static,
{
    fn execute(&self, future: T) {
        log::debug!("hyper::rt::Executor.execute");
        self.spawn_for_hyper(future);
    }
}

