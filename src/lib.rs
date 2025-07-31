use futures::future::{FutureExt, JoinAll, LocalBoxFuture, join_all};
use slab::Slab;
use std::future::Future;

/// Type alias for asynchronous event handlers.
///
/// This represents a boxed function that takes a reference to event arguments
/// and returns a boxed local future that resolves to ().
pub type AsyncEventHandler<'a, TEventArgs> = Box<dyn Fn(TEventArgs) -> LocalBoxFuture<'a, ()> + 'a>;

/// An asynchronous event that can have multiple handlers attached to it.
///
/// This is similar to C#'s `event` keyword but designed for async/await patterns.
/// Handlers are stored in a slab storage for efficient access by index.
///
/// # Examples
///
/// ```
/// use async_event_rs::AsyncEvent;
///
/// #[derive(Debug, Copy, Clone, Eq, PartialEq)]
/// struct EventArgs<'a> {
///     id: u32,
///     message: &'a str,
/// }
///
/// # futures::executor::block_on(async {
/// let mut event = AsyncEvent::<EventArgs>::new();
/// event.add(|args| async move {
///     println!("Event triggered with args: {:?}", args);
///     assert_eq!(args, EventArgs {id: 0, message: ""});
/// });
///
/// let arg = EventArgs {id: 0, message: ""};
/// event.invoke_async(arg).await;
/// # });
/// ```
pub struct AsyncEvent<'a, TEventArgs> {
    handlers: Slab<AsyncEventHandler<'a, TEventArgs>>,
}

impl<'a, TEventArgs> Default for AsyncEvent<'a, TEventArgs> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, TEventArgs> AsyncEvent<'a, TEventArgs> {
    /// Creates a new, empty AsyncEvent
    ///
    /// # Examples
    ///
    /// ```
    /// use async_event_rs::AsyncEvent;
    ///
    /// let mut event: AsyncEvent<()> = AsyncEvent::new();
    /// ```
    pub fn new() -> Self {
        Self {
            handlers: Slab::new(),
        }
    }

    /// Adds an event handler to the event.
    ///
    /// The handler should be a closure that accepts a reference to the event arguments
    /// and returns a future. The future will be executed when the event is triggered.
    ///
    /// Returns a handle that can be used to remove the handler later.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_event_rs::AsyncEvent;
    ///
    /// # futures::executor::block_on(async {
    /// let mut event = AsyncEvent::<()>::new();
    /// let handle = event.add(|args| async move {
    ///     println!("Event triggered");
    /// });
    /// # });
    /// ```
    pub fn add<F, Fut>(&mut self, handler: F) -> usize
    where
        F: Fn(TEventArgs) -> Fut + 'a,
        Fut: Future<Output = ()> + 'a,
    {
        self.handlers
            .insert(Box::new(move |arg| handler(arg).boxed_local()))
    }

    /// Removes an event handler using its handle.
    ///
    /// Returns `true` if the handler was found and removed, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_event_rs::AsyncEvent;
    ///
    /// # futures::executor::block_on(async {
    /// let mut event = AsyncEvent::<()>::new();
    /// let handle = event.add(|args| async move {
    ///     println!("Event triggered");
    /// });
    ///
    /// assert!(event.remove(handle));
    /// assert!(!event.remove(handle)); // Already removed
    /// # });
    /// ```
    pub fn remove(&mut self, handle: usize) -> bool {
        self.handlers.try_remove(handle).is_some()
    }

    /// Removes all event handlers.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_event_rs::AsyncEvent;
    ///
    /// # futures::executor::block_on(async {
    /// let mut event = AsyncEvent::<()>::new();
    /// event.add(|args| async move { println!("Handler 1"); });
    /// event.add(|args| async move { println!("Handler 2"); });
    ///
    /// event.clear(); // Remove all handlers
    /// # });
    /// ```
    pub fn clear(&mut self) {
        self.handlers.clear();
    }

    /// Invokes all event handlers sequentially (one after another).
    ///
    /// Each handler is awaited before the next one is executed.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_event_rs::AsyncEvent;
    ///
    /// # futures::executor::block_on(async {
    /// let mut event = AsyncEvent::new();
    /// event.add(|args| async move { println!("Handler 1"); });
    /// event.add(|args| async move { println!("Handler 2"); });
    ///
    /// event.invoke_async(()).await; // Execute all handlers in order
    /// # });
    /// ```
    pub async fn invoke_async(&self, arg: TEventArgs)
    where
        TEventArgs: Clone,
    {
        for (_, handler) in self.handlers.iter() {
            handler(arg.clone()).await;
        }
    }

    /// Invokes all event handlers in parallel.
    ///
    /// All handlers are spawned concurrently and executed simultaneously.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_event_rs::AsyncEvent;
    ///
    /// # futures::executor::block_on(async {
    /// let mut event = AsyncEvent::new();
    /// event.add(|args| async move { println!("Handler 1"); });
    /// event.add(|args| async move { println!("Handler 2"); });
    ///
    /// event.invoke_parallel_async(()).await; // Execute all handlers in parallel
    /// # });
    /// ```
    pub fn invoke_parallel_async(&self, arg: TEventArgs) -> JoinAll<impl Future<Output = ()> + 'a>
    where
        TEventArgs: Clone,
    {
        join_all(
            self.handlers
                .iter()
                .map(|(_, handler)| handler(arg.clone())),
        )
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

        event.invoke_async(()).await;

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

        event.invoke_async(()).await;
        assert_eq!(*counter.borrow(), 2);
    }

    #[tokio::test]
    async fn test_invoke_parallel_async() {
        let log = Rc::new(RefCell::new(vec![]));
        let mut event = AsyncEvent::new();

        event.invoke_parallel_async(()).await;

        for i in 0..3 {
            let log = Rc::clone(&log);
            event.add(move |_| {
                let log = Rc::clone(&log);
                async move {
                    log.borrow_mut().push(i);
                }
            });
        }

        event.invoke_parallel_async(()).await;

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
        event.invoke_async(()).await;

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
        event.invoke_async(()).await;

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

        event.invoke_async(()).await;
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

        event.invoke_async(()).await;
        assert_eq!(*counter.borrow(), 0);
    }
}
