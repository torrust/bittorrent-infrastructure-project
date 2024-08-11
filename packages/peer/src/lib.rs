mod codec;
mod manager;
mod message;
mod protocol;

pub use codec::PeerProtocolCodec;

pub use crate::manager::builder::PeerManagerBuilder;
pub use crate::manager::messages::{ManagedMessage, PeerManagerInputMessage, PeerManagerOutputError, PeerManagerOutputMessage};
pub use crate::manager::peer_info::PeerInfo;
pub use crate::manager::sink::PeerManagerSink;
pub use crate::manager::stream::PeerManagerStream;
pub use crate::manager::PeerManager;
pub use crate::protocol::{NestedPeerProtocol, PeerProtocol};

/// Serializable and deserializable protocol messages.
pub mod messages {
    /// Builder types for protocol messages.
    pub mod builders {
        pub use crate::message::ExtendedMessageBuilder;
    }

    pub use crate::message::{
        BitFieldIter, BitFieldMessage, BitsExtensionMessage, CancelMessage, ExtendedMessage, ExtendedType, HaveMessage,
        NullProtocolMessage, PeerExtensionProtocolMessage, PeerExtensionProtocolMessageError, PeerWireProtocolMessage,
        PeerWireProtocolMessageError, PieceMessage, PortMessage, RequestMessage, UtMetadataDataMessage, UtMetadataMessage,
        UtMetadataRejectMessage, UtMetadataRequestMessage,
    };
}

/// `PeerManager` error types.
#[allow(clippy::module_name_repetitions)]
pub mod error {
    pub use crate::manager::error::{PeerManagerError, PeerManagerResult};
}

/// Implementations of `PeerProtocol`.
pub mod protocols {
    pub use crate::protocol::extension::PeerExtensionProtocol;
    pub use crate::protocol::null::NullProtocol;
    pub use crate::protocol::unit::UnitProtocol;
    pub use crate::protocol::wire::PeerWireProtocol;
}
