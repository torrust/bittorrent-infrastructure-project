#[macro_use]
extern crate bip_bencode;
extern crate bip_handshake;
extern crate bip_util;
extern crate byteorder;
extern crate bytes;
extern crate crossbeam;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_timer;
#[macro_use]
extern crate nom;

#[macro_use]
mod macros;

mod codec;
mod manager;
mod message;
mod protocol;

pub use codec::PeerProtocolCodec;
pub use manager::builder::PeerManagerBuilder;
pub use manager::peer_info::PeerInfo;
pub use manager::{
    IPeerManagerMessage, ManagedMessage, MessageId, OPeerManagerMessage, PeerManager, PeerManagerSink, PeerManagerStream,
};
pub use protocol::{NestedPeerProtocol, PeerProtocol};

/// Serializable and deserializable protocol messages.
pub mod messages {
    /// Builder types for protocol messages.
    pub mod builders {
        pub use message::ExtendedMessageBuilder;
    }

    pub use message::{
        BitFieldIter, BitFieldMessage, BitsExtensionMessage, CancelMessage, ExtendedMessage, ExtendedType, HaveMessage,
        NullProtocolMessage, PeerExtensionProtocolMessage, PeerWireProtocolMessage, PieceMessage, PortMessage, RequestMessage,
        UtMetadataDataMessage, UtMetadataMessage, UtMetadataRejectMessage, UtMetadataRequestMessage,
    };
}

/// `PeerManager` error types.
pub mod error {
    pub use manager::error::{PeerManagerError, PeerManagerErrorKind, PeerManagerResult, PeerManagerResultExt};
}

/// Implementations of `PeerProtocol`.
pub mod protocols {
    pub use protocol::extension::PeerExtensionProtocol;
    pub use protocol::null::NullProtocol;
    pub use protocol::unit::UnitProtocol;
    pub use protocol::wire::PeerWireProtocol;
}
