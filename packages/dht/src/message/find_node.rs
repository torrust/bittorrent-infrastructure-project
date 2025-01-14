use bencode::{ben_bytes, ben_map, BConvert, BDictAccess, BRefAccess};
use util::bt::NodeId;

use crate::error::DhtError;
use crate::message;
use crate::message::compact_info::CompactNodeInfo;
use crate::message::request::{self, RequestValidate};
use crate::message::response::ResponseValidate;

#[allow(clippy::module_name_repetitions)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FindNodeRequest<'a> {
    trans_id: &'a [u8],
    node_id: NodeId,
    target_id: NodeId,
}

impl<'a> FindNodeRequest<'a> {
    #[must_use]
    pub fn new(trans_id: &'a [u8], node_id: NodeId, target_id: NodeId) -> FindNodeRequest<'a> {
        FindNodeRequest {
            trans_id,
            node_id,
            target_id,
        }
    }

    /// Create a `FindNodeRequest` from parts.
    ///
    /// The `target_key` argument is provided for cases where, due to forward compatibility,
    /// the target key we are interested in could fall under the target key or another key.
    ///
    /// # Errors
    ///
    /// It will return an error if unable to lookup an validate the node parts.
    pub fn from_parts<B>(
        rqst_root: &dyn BDictAccess<B::BKey, B>,
        trans_id: &'a [u8],
        target_key: &str,
    ) -> Result<FindNodeRequest<'a>, DhtError>
    where
        B: BRefAccess,
    {
        let validate = RequestValidate::new(trans_id);

        let node_id_bytes = validate.lookup_and_convert_bytes(rqst_root, message::NODE_ID_KEY)?;
        let node_id = validate.validate_node_id(node_id_bytes)?;

        let target_id_bytes = validate.lookup_and_convert_bytes(rqst_root, target_key)?;
        let target_id = validate.validate_node_id(target_id_bytes)?;

        Ok(FindNodeRequest::new(trans_id, node_id, target_id))
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
    pub fn target_id(&self) -> NodeId {
        self.target_id
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        (ben_map! {
            //message::CLIENT_TYPE_KEY => ben_bytes!(dht::CLIENT_IDENTIFICATION),
            message::TRANSACTION_ID_KEY => ben_bytes!(self.trans_id),
            message::MESSAGE_TYPE_KEY => ben_bytes!(message::REQUEST_TYPE_KEY),
            message::REQUEST_TYPE_KEY => ben_bytes!(request::FIND_NODE_TYPE_KEY),
            request::REQUEST_ARGS_KEY => ben_map!{
                message::NODE_ID_KEY => ben_bytes!(self.node_id.as_ref()),
                message::TARGET_ID_KEY => ben_bytes!(self.target_id.as_ref())
            }
        })
        .encode()
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FindNodeResponse<'a> {
    trans_id: &'a [u8],
    node_id: NodeId,
    nodes: CompactNodeInfo<'a>,
}

impl<'a> FindNodeResponse<'a> {
    /// Create a new `FindNodeResponse`.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to validate the nodes.
    pub fn new(trans_id: &'a [u8], node_id: NodeId, nodes: &'a [u8]) -> Result<FindNodeResponse<'a>, DhtError> {
        let validate = ResponseValidate::new(trans_id);
        let compact_nodes = validate.validate_nodes(nodes)?;

        Ok(FindNodeResponse {
            trans_id,
            node_id,
            nodes: compact_nodes,
        })
    }

    /// Create a new `FindNodeResponse` from parts.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to lookup and and validate node.
    pub fn from_parts<B>(rsp_root: &'a dyn BDictAccess<B::BKey, B>, trans_id: &'a [u8]) -> Result<FindNodeResponse<'a>, DhtError>
    where
        B: BRefAccess,
    {
        let validate = ResponseValidate::new(trans_id);

        let node_id_bytes = validate.lookup_and_convert_bytes(rsp_root, message::NODE_ID_KEY)?;
        let node_id = validate.validate_node_id(node_id_bytes)?;

        let nodes = validate.lookup_and_convert_bytes(rsp_root, message::NODES_KEY)?;

        FindNodeResponse::new(trans_id, node_id, nodes)
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
    pub fn nodes(&self) -> CompactNodeInfo<'a> {
        self.nodes
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        (ben_map! {
            //message::CLIENT_TYPE_KEY => ben_bytes!(dht::CLIENT_IDENTIFICATION),
            message::TRANSACTION_ID_KEY => ben_bytes!(self.trans_id),
            message::MESSAGE_TYPE_KEY => ben_bytes!(message::RESPONSE_TYPE_KEY),
            message::RESPONSE_TYPE_KEY => ben_map!{
                message::NODE_ID_KEY => ben_bytes!(self.node_id.as_ref()),
                message::NODES_KEY => ben_bytes!(self.nodes.nodes())
            }
        })
        .encode()
    }
}
