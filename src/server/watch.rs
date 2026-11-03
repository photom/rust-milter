//! Read access to the application configuration.

use std::sync::Arc;

use tokio::sync::watch;

/// Read access to the current application configuration and its
/// generation. Clones follow the same configuration.
pub struct ConfigWatch<A> {
    receiver: watch::Receiver<(u64, Arc<A>)>,
}

/// The sending side, held by the server.
pub(crate) struct Publisher<A> {
    _sender: watch::Sender<(u64, Arc<A>)>,
}

impl<A> ConfigWatch<A> {
    /// A watch on `app` as generation 0, and its publisher.
    pub(crate) fn new(app: A) -> (ConfigWatch<A>, Publisher<A>) {
        let (sender, receiver) = watch::channel((0, Arc::new(app)));
        (ConfigWatch { receiver }, Publisher { _sender: sender })
    }

    /// The current configuration. Never blocks.
    pub fn current(&self) -> Arc<A> {
        Arc::clone(&self.receiver.borrow().1)
    }

    /// The generation of the current configuration.
    pub fn generation(&self) -> u64 {
        self.receiver.borrow().0
    }

    /// Completes when a new generation is published.
    pub async fn changed(&mut self) {
        if self.receiver.changed().await.is_err() {
            // The server is gone, so no generation will follow.
            std::future::pending::<()>().await;
        }
    }
}

impl<A> Clone for ConfigWatch<A> {
    fn clone(&self) -> Self {
        ConfigWatch {
            receiver: self.receiver.clone(),
        }
    }
}
