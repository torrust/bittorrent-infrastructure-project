// We don't really use PingRequests for our current algorithms, but that may change in the future!
#![allow(unused)]

use bencode::{ben_bytes, ben_map, BConvert, BDictAccess, BRefAccess};
use util::bt::NodeId;

use crate::error::DhtError;
use crate::message;
use crate::message::request::{self, RequestValidate};

#[allow(clippy::module_name_repetitions)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct PingRequest<'a> {
    trans_id: &'a [u8],
    node_id: NodeId,
}

impl<'a> PingRequest<'a> {
    #[must_use]
    pub fn new(trans_id: &'a [u8], node_id: NodeId) -> PingRequest<'a> {
        PingRequest { trans_id, node_id }
    }

    /// Create a `PingRequest` from parts.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to lookup, convert, and validate nodes.
    pub fn from_parts<B>(rqst_root: &dyn BDictAccess<B::BKey, B>, trans_id: &'a [u8]) -> Result<PingRequest<'a>, DhtError>
    where
        B: BRefAccess,
    {
        let validate = RequestValidate::new(trans_id);

        let node_id_bytes = validate.lookup_and_convert_bytes(rqst_root, message::NODE_ID_KEY)?;
        let node_id = validate.validate_node_id(node_id_bytes)?;

        Ok(PingRequest::new(trans_id, node_id))
    }

    #[must_use]
    pub fn transaction_id(&self) -> &'a [u8] {
        self.trans_id
    }

    #[must_use]
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        (ben_map! {
            //message::CLIENT_TYPE_KEY => ben_bytes!(dht::CLIENT_IDENTIFICATION),
            message::TRANSACTION_ID_KEY => ben_bytes!(self.trans_id),
            message::MESSAGE_TYPE_KEY => ben_bytes!(message::REQUEST_TYPE_KEY),
            message::REQUEST_TYPE_KEY => ben_bytes!(request::PING_TYPE_KEY),
            request::REQUEST_ARGS_KEY => ben_map!{
                message::NODE_ID_KEY => ben_bytes!(self.node_id.as_ref())
            }
        })
        .encode()
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct PingResponse<'a> {
    trans_id: &'a [u8],
    node_id: NodeId,
}

/// Reuse functionality of ping request since the structures are identical.
impl<'a> PingResponse<'a> {
    #[must_use]
    pub fn new(trans_id: &'a [u8], node_id: NodeId) -> PingResponse<'a> {
        PingResponse { trans_id, node_id }
    }

    /// Create a new `PingResponse` from parts.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to generate the ping request from the root.
    pub fn from_parts<B>(rsp_root: &dyn BDictAccess<B::BKey, B>, trans_id: &'a [u8]) -> Result<PingResponse<'a>, DhtError>
    where
        B: BRefAccess,
    {
        let request = PingRequest::from_parts(rsp_root, trans_id)?;

        Ok(PingResponse::new(request.trans_id, request.node_id))
    }

    #[must_use]
    pub fn transaction_id(&self) -> &'a [u8] {
        self.trans_id
    }

    #[must_use]
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        (ben_map! {
            //message::CLIENT_TYPE_KEY => ben_bytes!(dht::CLIENT_IDENTIFICATION),
            message::TRANSACTION_ID_KEY => ben_bytes!(self.trans_id),
            message::MESSAGE_TYPE_KEY => ben_bytes!(message::RESPONSE_TYPE_KEY),
            message::RESPONSE_TYPE_KEY => ben_map!{
                message::NODE_ID_KEY => ben_bytes!(self.node_id.as_ref())
            }
        })
        .encode()
    }
}
