//! The config channel: each generation of the server configuration and the
//! application configuration, published together.

use std::sync::Arc;

use tokio::sync::watch;

use crate::ServerConfig;

/// One published generation.
struct Published<A> {
    generation: u64,
    server: Arc<ServerConfig>,
    app: Arc<A>,
}

/// Read access to the current application configuration and its
/// generation. Clones follow the same configuration.
pub struct ConfigWatch<A> {
    receiver: watch::Receiver<Published<A>>,
}

/// The sending side, held by the server.
pub(crate) struct Publisher<A> {
    sender: watch::Sender<Published<A>>,
}

impl<A> ConfigWatch<A> {
    /// A watch on the first generation, and its publisher.
    pub(crate) fn new(server: ServerConfig, app: A) -> (ConfigWatch<A>, Publisher<A>) {
        let (sender, receiver) = watch::channel(Published {
            generation: super::FIRST_GENERATION,
            server: Arc::new(server),
            app: Arc::new(app),
        });
        (ConfigWatch { receiver }, Publisher { sender })
    }

    /// The current configuration. Never blocks.
    pub fn current(&self) -> Arc<A> {
        Arc::clone(&self.receiver.borrow().app)
    }

    /// The generation of the current configuration.
    pub fn generation(&self) -> u64 {
        self.receiver.borrow().generation
    }

    /// Completes when a new generation is published.
    pub async fn changed(&mut self) {
        if self.receiver.changed().await.is_err() {
            // The server is gone, so no generation will follow.
            std::future::pending::<()>().await;
        }
    }

    /// The current server configuration and its generation.
    pub(crate) fn server(&self) -> (u64, Arc<ServerConfig>) {
        let published = self.receiver.borrow();
        (published.generation, Arc::clone(&published.server))
    }
}

impl<A> Clone for ConfigWatch<A> {
    fn clone(&self) -> Self {
        ConfigWatch {
            receiver: self.receiver.clone(),
        }
    }
}

impl<A> Publisher<A> {
    /// Publishes the next generation. Readers holding the previous one keep
    /// it.
    pub(crate) fn publish(&self, generation: u64, server: Arc<ServerConfig>, app: A) {
        self.sender.send_replace(Published {
            generation,
            server,
            app: Arc::new(app),
        });
    }
}
