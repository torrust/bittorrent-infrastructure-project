//! Module for discovery error types.

use error_chain::error_chain;
use handshake::InfoHash;
use peer::PeerInfo;

error_chain! {
    types {
        DiscoveryError, DiscoveryErrorKind, DiscoveryResultExt;
    }

    errors {
        InvalidMessage {
            info:    PeerInfo,
            message: String
        } {
            description("Peer Sent An Invalid Message")
            display("Peer {:?} Sent An Invalid Message: {:?}", info, message)
        }
        InvalidMetainfoExists {
            hash: InfoHash
        } {
            description("Metainfo Has Already Been Added")
            display("Metainfo With Hash {:?} Has Already Been Added", hash)
        }
        InvalidMetainfoNotExists {
            hash: InfoHash
        } {
            description("Metainfo Was Not Already Added")
            display("Metainfo With Hash {:?} Was Not Already Added", hash)
        }
    }
}
