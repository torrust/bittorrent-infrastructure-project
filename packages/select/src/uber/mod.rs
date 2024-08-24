use std::sync::{Arc, Mutex};

use futures::{Sink, Stream};
use peer::messages::builders::ExtendedMessageBuilder;
use sink::UberSink;
use stream::UberStream;

use crate::discovery::error::DiscoveryError;
use crate::discovery::{IDiscoveryMessage, ODiscoveryMessage};
use crate::extended::{ExtendedListener, ExtendedModule, IExtendedMessage, OExtendedMessage};
use crate::ControlMessage;

pub mod sink;
pub mod stream;

pub trait DiscoveryTrait:
    ExtendedListener
    + Sink<IDiscoveryMessage, Error = DiscoveryError>
    + Stream<Item = Result<ODiscoveryMessage, DiscoveryError>>
    + Send
    + Unpin
    + 'static
{
}
impl<T> DiscoveryTrait for T where
    T: ExtendedListener
        + Sink<IDiscoveryMessage, Error = DiscoveryError>
        + Stream<Item = Result<ODiscoveryMessage, DiscoveryError>>
        + Send
        + Unpin
        + 'static
{
}

/// Enumeration of uber messages that can be sent to the uber module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IUberMessage {
    /// Broadcast a control message out to all modules.
    Control(Box<ControlMessage>),
    /// Send an extended message to the extended module.
    Extended(Box<IExtendedMessage>),
    /// Send a discovery message to all discovery modules.
    Discovery(Box<IDiscoveryMessage>),
}

/// Enumeration of uber messages that can be received from the uber module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OUberMessage {
    /// Receive an extended message from the extended module.
    Extended(OExtendedMessage),
    /// Receive a discovery message from some discovery module.
    Discovery(ODiscoveryMessage),
}

type UberDiscovery = Arc<
    Mutex<Vec<Arc<dyn DiscoveryTrait<Error = DiscoveryError, Item = Result<ODiscoveryMessage, DiscoveryError>> + Send + Sync>>>,
>;

/// Builder for constructing an `UberModule`.
#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct UberModuleBuilder {
    pub discovery: UberDiscovery,
    ext_builder: Option<ExtendedMessageBuilder>,
}

impl UberModuleBuilder {
    /// Create a new `UberModuleBuilder`.
    #[must_use]
    pub fn new() -> UberModuleBuilder {
        UberModuleBuilder {
            discovery: Arc::default(),
            ext_builder: None,
        }
    }

    /// Specifies the given builder that all modules will add to when sending an extended message to a peer.
    ///
    /// This message will only be sent when the extension bit from the handshake it set. Note that if a builder
    /// is not given and a peer with the extension bit set connects, we will NOT send any extended message.
    #[must_use]
    pub fn with_extended_builder(mut self, builder: Option<ExtendedMessageBuilder>) -> UberModuleBuilder {
        self.ext_builder = builder;
        self
    }

    /// Add the given discovery module to the list of discovery modules.
    ///
    /// # Panics
    ///
    /// It would panic if unable to get a lock for the discovery.
    #[must_use]
    pub fn with_discovery_module<T>(self, module: T) -> UberModuleBuilder
    where
        T: ExtendedListener
            + Sink<IDiscoveryMessage, Error = DiscoveryError>
            + Stream<Item = Result<ODiscoveryMessage, DiscoveryError>>
            + Send
            + Sync
            + Unpin
            + 'static,
    {
        self.discovery.lock().unwrap().push(Arc::new(module));
        self
    }

    /// Build an `UberModule` based on the current builder.
    #[must_use]
    pub fn build(self) -> UberModule {
        UberModule::from_builder(self)
    }
}

//----------------------------------------------------------------------//
/// Module for multiplexing messages across zero or more other modules.
#[allow(clippy::module_name_repetitions)]
pub struct UberModule {
    sink: UberSink,
    stream: UberStream,
}

impl UberModule {
    /// Create an `UberModule` from the given `UberModuleBuilder`.
    pub fn from_builder(builder: UberModuleBuilder) -> UberModule {
        let discovery = builder.discovery;
        let extended = builder.ext_builder.map(ExtendedModule::new);

        UberModule {
            sink: UberSink {
                discovery: discovery.clone(),
                extended: extended.clone(),
            },
            stream: UberStream { discovery, extended },
        }
    }

    /// Splits the `UberModule` into its parts.
    #[must_use]
    pub fn into_parts(self) -> (UberSink, UberStream) {
        (self.sink, self.stream)
    }
}
