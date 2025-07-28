use futures::future::{FutureExt, JoinAll, LocalBoxFuture, join_all};
use slab::Slab;
use std::future::Future;

pub type AsyncEventHandler<'a, TEventArgs> =
    Box<dyn Fn(&'a TEventArgs) -> LocalBoxFuture<'a, ()> + 'a>;

pub struct AsyncEvent<'a, TEventArgs> {
    handlers: Slab<AsyncEventHandler<'a, TEventArgs>>,
}

impl<'a, TEventArgs> Default for AsyncEvent<'a, TEventArgs> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, TEventArgs> AsyncEvent<'a, TEventArgs> {
    pub fn new() -> Self {
        Self {
            handlers: Slab::new(),
        }
    }

    pub fn add<F, Fut>(&mut self, handler: F) -> usize
    where
        F: Fn(&'a TEventArgs) -> Fut + 'a,
        Fut: Future<Output = ()> + 'a,
    {
        self.handlers
            .insert(Box::new(move |arg| handler(arg).boxed_local()))
    }

    pub fn remove(&mut self, handle: usize) -> bool {
        self.handlers.try_remove(handle).is_some()
    }

    pub fn clear(&mut self) {
        self.handlers.clear();
    }

    pub async fn invoke_async(&self, arg: &'a TEventArgs) {
        for (_, handler) in self.handlers.iter() {
            handler(arg).await;
        }
    }

    pub fn invoke_parallel_async(
        &self,
        arg: &'a TEventArgs,
    ) -> JoinAll<impl Future<Output = ()> + 'a> {
        join_all(self.handlers.iter().map(|(_, handler)| handler(arg)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[tokio::test]
    async fn test_invoke_async() {
        let counter = Rc::new(RefCell::new(0));
        let mut event = AsyncEvent::new();

        event.invoke_async(&()).await;

        event.add(|_| {
            let counter = Rc::clone(&counter);
            async move {
                *counter.borrow_mut() += 1;
            }
        });

        event.add(|_| {
            let counter = Rc::clone(&counter);
            async move {
                *counter.borrow_mut() += 1;
            }
        });

        event.invoke_async(&()).await;
        assert_eq!(*counter.borrow(), 2);
    }

    #[tokio::test]
    async fn test_invoke_parallel_async() {
        let log = Rc::new(RefCell::new(vec![]));
        let mut event = AsyncEvent::new();

        event.invoke_parallel_async(&()).await;

        for i in 0..3 {
            let log = Rc::clone(&log);
            event.add(move |_| {
                let log = Rc::clone(&log);
                async move {
                    log.borrow_mut().push(i);
                }
            });
        }

        event.invoke_parallel_async(&()).await;

        let result = log.borrow().clone();
        assert_eq!(result.len(), 3);
        for i in 0..3 {
            assert!(result.contains(&i));
        }
    }

    #[tokio::test]
    async fn test_remove_handler() {
        let counter = Rc::new(RefCell::new(0));
        let mut event = AsyncEvent::new();

        let handle = event.add(|_| {
            let counter = Rc::clone(&counter);
            async move {
                *counter.borrow_mut() += 1;
            }
        });

        assert!(event.remove(handle));
        event.invoke_async(&()).await;

        assert_eq!(*counter.borrow(), 0);
    }

    #[tokio::test]
    async fn test_clear_handlers() {
        let counter = Rc::new(RefCell::new(0));
        let mut event = AsyncEvent::new();

        for _ in 0..5 {
            event.add(|_| {
                let counter = Rc::clone(&counter);
                async move {
                    *counter.borrow_mut() += 1;
                }
            });
        }

        event.clear();
        event.invoke_async(&()).await;

        assert_eq!(*counter.borrow(), 0);
    }

    #[tokio::test]
    async fn test_add_handler_twice() {
        let counter = Rc::new(RefCell::new(0));
        let mut event = AsyncEvent::new();

        let handler = |_| {
            let counter: Rc<RefCell<i32>> = Rc::clone(&counter);
            async move {
                *counter.borrow_mut() += 1;
            }
        };

        event.add(handler);
        event.add(handler);

        event.invoke_async(&()).await;
        assert_eq!(*counter.borrow(), 2);
    }

    #[tokio::test]
    async fn test_remove_handler_twice() {
        let counter = Rc::new(RefCell::new(0));
        let mut event = AsyncEvent::new();

        let handle = event.add(|_| {
            let counter: Rc<RefCell<i32>> = Rc::clone(&counter);
            async move {
                *counter.borrow_mut() += 1;
            }
        });

        assert!(event.remove(handle));
        assert!(!event.remove(handle));

        event.invoke_async(&()).await;
        assert_eq!(*counter.borrow(), 0);
    }
}
