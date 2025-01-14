use std::collections::BTreeMap;
use std::ops::Deref;

use bencode::inner::BCowConvert;
use bencode::{ben_bytes, ben_map, BConvert, BDictAccess, BMutAccess, BRefAccess, BencodeMut};
use util::bt::{InfoHash, NodeId};

use crate::error::DhtError;
use crate::message;
use crate::message::compact_info::{CompactNodeInfo, CompactValueInfo};
use crate::message::request::{self, RequestValidate};
use crate::message::response::{self, ResponseValidate};

#[allow(clippy::module_name_repetitions)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct GetPeersRequest<'a> {
    trans_id: &'a [u8],
    node_id: NodeId,
    info_hash: InfoHash,
}

impl<'a> GetPeersRequest<'a> {
    #[must_use]
    pub fn new(trans_id: &'a [u8], node_id: NodeId, info_hash: InfoHash) -> GetPeersRequest<'a> {
        GetPeersRequest {
            trans_id,
            node_id,
            info_hash,
        }
    }

    /// Create a `GetPeersRequest` from parts
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to lookup, convert, and validate node.
    pub fn from_parts<B>(rqst_root: &dyn BDictAccess<B::BKey, B>, trans_id: &'a [u8]) -> Result<GetPeersRequest<'a>, DhtError>
    where
        B: BRefAccess,
    {
        let validate = RequestValidate::new(trans_id);

        let node_id_bytes = validate.lookup_and_convert_bytes(rqst_root, message::NODE_ID_KEY)?;
        let node_id = validate.validate_node_id(node_id_bytes)?;

        let info_hash_bytes = validate.lookup_and_convert_bytes(rqst_root, message::INFO_HASH_KEY)?;
        let info_hash = validate.validate_info_hash(info_hash_bytes)?;

        Ok(GetPeersRequest::new(trans_id, node_id, info_hash))
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
    pub fn info_hash(&self) -> InfoHash {
        self.info_hash
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        (ben_map! {
            //message::CLIENT_TYPE_KEY => ben_bytes!(dht::CLIENT_IDENTIFICATION),
            message::TRANSACTION_ID_KEY => ben_bytes!(self.trans_id),
            message::MESSAGE_TYPE_KEY => ben_bytes!(message::REQUEST_TYPE_KEY),
            message::REQUEST_TYPE_KEY => ben_bytes!(request::GET_PEERS_TYPE_KEY),
            request::REQUEST_ARGS_KEY => ben_map!{
                message::NODE_ID_KEY => ben_bytes!(self.node_id.as_ref()),
                message::INFO_HASH_KEY => ben_bytes!(self.info_hash.as_ref())
            }
        })
        .encode()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum CompactInfoType<'a, B>
where
    B: BRefAccess<BType = B> + Clone,
    B::BType: PartialEq + Eq + core::hash::Hash + std::fmt::Debug,
{
    Nodes(CompactNodeInfo<'a>),
    Values(CompactValueInfo<'a, B::BType>),
    Both(CompactNodeInfo<'a>, CompactValueInfo<'a, B::BType>),
}

#[allow(clippy::module_name_repetitions)]
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct GetPeersResponse<'a, B>
where
    B: BRefAccess<BType = B> + Clone,
    B::BType: PartialEq + Eq + core::hash::Hash + std::fmt::Debug,
{
    trans_id: &'a [u8],
    node_id: NodeId,
    // It looks like bootstrap nodes don't provide a nodes key, probably
    // because they are only used for bootstrapping and not to announce to.
    token: Option<&'a [u8]>,
    info_type: CompactInfoType<'a, B::BType>,
}

impl<'a, B> GetPeersResponse<'a, B>
where
    B: BRefAccess<BType = B> + Clone,
    B::BType: PartialEq + Eq + core::hash::Hash + std::fmt::Debug,
{
    #[must_use]
    pub fn new(
        trans_id: &'a [u8],
        node_id: NodeId,
        token: Option<&'a [u8]>,
        info_type: CompactInfoType<'a, B::BType>,
    ) -> GetPeersResponse<'a, B::BType> {
        GetPeersResponse {
            trans_id,
            node_id,
            token,
            info_type,
        }
    }

    /// Create a `GetPeersResponse` from parts.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to lookup, convert, and validate the nodes.
    pub fn from_parts(
        rsp_root: &'a dyn BDictAccess<B::BKey, B::BType>,
        trans_id: &'a [u8],
    ) -> Result<GetPeersResponse<'a, B::BType>, DhtError> {
        let validate = ResponseValidate::new(trans_id);

        let node_id_bytes = validate.lookup_and_convert_bytes(rsp_root, message::NODE_ID_KEY)?;
        let node_id = validate.validate_node_id(node_id_bytes)?;

        let token = validate.lookup_and_convert_bytes(rsp_root, message::TOKEN_KEY).ok();

        let maybe_nodes = validate.lookup_and_convert_bytes(rsp_root, message::NODES_KEY);
        let maybe_values = validate.lookup_and_convert_list(rsp_root, message::VALUES_KEY);

        // TODO: Check if nodes in the wild actually send a 2d array of bytes as values or if they
        // stick with the more compact single byte array like that used for nodes.
        let info_type = match (maybe_nodes, maybe_values) {
            (Ok(nodes), Ok(values)) => {
                let nodes_info = validate.validate_nodes(nodes)?;
                let values_info: CompactValueInfo<'_, B> = validate.validate_values(values)?;
                CompactInfoType::<B>::Both(nodes_info, values_info)
            }
            (Ok(nodes), Err(_)) => {
                let nodes_info = validate.validate_nodes(nodes)?;
                CompactInfoType::Nodes(nodes_info)
            }
            (Err(_), Ok(values)) => {
                let values_info = validate.validate_values(values)?;
                CompactInfoType::Values(values_info)
            }
            (Err(_), Err(_)) => {
                return Err(DhtError::InvalidResponse {
                    details: "Failed To Find nodes Or values In Node Response".to_owned(),
                })
            }
        };

        Ok(GetPeersResponse::<B>::new(trans_id, node_id, token, info_type))
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
    pub fn token(&self) -> Option<&'a [u8]> {
        self.token
    }

    #[must_use]
    pub fn info_type(self) -> CompactInfoType<'a, B> {
        self.info_type
    }
}

impl<'a> GetPeersResponse<'a, BencodeMut<'a>> {
    /// Returns the encode of this [`GetPeersResponse<BencodeMut<'a>>`].
    ///
    /// # Panics
    ///
    /// Panics if unable to get the bencoded list.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut response_args = BTreeMap::new();

        response_args.insert(message::NODE_ID_KEY.as_bytes(), ben_bytes!(self.node_id.as_ref()));
        if let Some(token) = self.token {
            response_args.insert(message::TOKEN_KEY.as_bytes(), ben_bytes!(token));
        };

        match &self.info_type {
            CompactInfoType::Nodes(nodes) => {
                response_args.insert(message::NODES_KEY.as_bytes(), ben_bytes!(nodes.nodes()));
            }
            CompactInfoType::Values(values) => {
                response_args.insert(message::VALUES_KEY.as_bytes(), {
                    let mut b_mut = BencodeMut::new_list();
                    let b_list = b_mut.list_mut().unwrap();
                    for b in values.values() {
                        b_list.push(b.deref().clone());
                    }
                    b_mut
                });
            }
            CompactInfoType::Both(nodes, values) => {
                response_args.insert(message::NODES_KEY.as_bytes(), ben_bytes!(nodes.nodes()));
                response_args.insert(message::VALUES_KEY.as_bytes(), {
                    let mut b_mut = BencodeMut::new_list();
                    let b_list = b_mut.list_mut().unwrap();
                    for b in values.values() {
                        b_list.push(b.deref().clone());
                    }
                    b_mut
                });
            }
        };

        let mut bencode_map = BencodeMut::new_dict();

        {
            let map = bencode_map.dict_mut().unwrap();
            for (key, value) in response_args {
                map.insert(BCowConvert::convert(key), value);
            }

            let bencode_response = bencode_map;

            (ben_map! {
                //message::CLIENT_TYPE_KEY => ben_bytes!(dht::CLIENT_IDENTIFICATION),
                message::TRANSACTION_ID_KEY => ben_bytes!(self.trans_id),
                message::MESSAGE_TYPE_KEY => ben_bytes!(message::RESPONSE_TYPE_KEY),
                message::REQUEST_TYPE_KEY => ben_bytes!(request::GET_PEERS_TYPE_KEY),
                response::RESPONSE_ARGS_KEY => bencode_response

            })
            .encode()
        }
    }
}
