use std::collections::BTreeMap;

use bencode::ext::BRefAccessExt;
use bencode::{BConvert, BDictAccess, BListAccess, BRefAccess, BencodeMut, BencodeRef};
use message::compact_info::{CompactNodeInfo, CompactValueInfo};
use message::request::{self, RequestValidate};
use message::response::{self, ResponseValidate};
use util::bt::{InfoHash, NodeId};

use crate::error::{self, DhtError, DhtErrorKind, DhtResult};
use crate::message;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct GetPeersRequest<'a> {
    trans_id: &'a [u8],
    node_id: NodeId,
    info_hash: InfoHash,
}

impl<'a> GetPeersRequest<'a> {
    pub fn new(trans_id: &'a [u8], node_id: NodeId, info_hash: InfoHash) -> GetPeersRequest<'a> {
        GetPeersRequest {
            trans_id: trans_id,
            node_id: node_id,
            info_hash: info_hash,
        }
    }

    pub fn from_parts<B>(rqst_root: &dyn BDictAccess<B::BKey, B::BType>, trans_id: &'a [u8]) -> DhtResult<GetPeersRequest<'a>>
    where
        B: BRefAccess,
    {
        let validate = RequestValidate::new(trans_id);

        let node_id_bytes = (validate.lookup_and_convert_bytes(rqst_root, message::NODE_ID_KEY))?;
        let node_id = (validate.validate_node_id(node_id_bytes))?;

        let info_hash_bytes = (validate.lookup_and_convert_bytes(rqst_root, message::INFO_HASH_KEY))?;
        let info_hash = (validate.validate_info_hash(info_hash_bytes))?;

        Ok(GetPeersRequest::new(trans_id, node_id, info_hash))
    }

    pub fn transaction_id(&self) -> &'a [u8] {
        &self.trans_id
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn info_hash(&self) -> InfoHash {
        self.info_hash
    }

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
pub enum CompactInfoType<'a, B, C: 'a>
where
    B: BRefAccess<BKey = &'a [u8], BType = C>,
{
    Nodes(CompactNodeInfo<'a>),
    Values(CompactValueInfo<'a, B, C>),
    Both(CompactNodeInfo<'a>, CompactValueInfo<'a, B, C>),
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct GetPeersResponse<'a> {
    trans_id: &'a [u8],
    node_id: NodeId,
    // It looks like bootstrap nodes don't provide a nodes key, probably
    // because they are only used for bootstraping and not to announce to.
    token: Option<&'a [u8]>,
    info_type: CompactInfoType<'a, BencodeRef<'a>, BencodeRef<'a>>,
}

impl<'a, 'b> GetPeersResponse<'a>
where
    'b: 'a,
{
    pub fn new(
        trans_id: &'a [u8],
        node_id: NodeId,
        token: Option<&'a [u8]>,
        info_type: CompactInfoType<'a, BencodeRef<'a>, BencodeRef<'a>>,
    ) -> GetPeersResponse<'a> {
        GetPeersResponse {
            trans_id: trans_id,
            node_id: node_id,
            token: token,
            info_type: info_type,
        }
    }

    pub fn from_parts<B>(rsp_root: &'a dyn BDictAccess<B::BKey, B::BType>, trans_id: &'a [u8]) -> DhtResult<GetPeersResponse<'a>>
    where
        B: for<'a_> BRefAccess<BKey = &'a [u8], BType = BencodeRef<'a>>,
    {
        let validate = ResponseValidate::new(trans_id);

        let node_id_bytes = (validate.lookup_and_convert_bytes(rsp_root, message::NODE_ID_KEY))?;
        let node_id = (validate.validate_node_id(node_id_bytes))?;

        let token = validate.lookup_and_convert_bytes(rsp_root, message::TOKEN_KEY).ok();

        let maybe_nodes = validate.lookup_and_convert_bytes(rsp_root, message::NODES_KEY);
        let maybe_values = validate.lookup_and_convert_list(rsp_root, message::VALUES_KEY);

        // TODO: Check if nodes in the wild actually send a 2d array of bytes as values or if they
        // stick with the more compact single byte array like that used for nodes.
        let info_type = match (maybe_nodes, maybe_values) {
            (Ok(nodes), Ok(values)) => {
                let nodes_info = (validate.validate_nodes(nodes))?;
                let vals = Box::new(values.into_iter().map(|f| Box::new(f.clone())).collect());
                let values_info = validate.validate_values(vals)?;
                CompactInfoType::Both(nodes_info, values_info)
            }
            (Ok(nodes), Err(_)) => {
                let nodes_info = (validate.validate_nodes(nodes))?;
                CompactInfoType::Nodes(nodes_info)
            }
            (Err(_), Ok(values)) => {
                let values_info =
                    (validate.validate_values(Box::new(values.into_iter().map(|f| Box::new(f.clone())).collect())))?;
                CompactInfoType::Values(values_info)
            }
            (Err(_), Err(_)) => {
                return Err(DhtError::from_kind(DhtErrorKind::InvalidResponse {
                    details: "Failed To Find nodes Or values In Node Response".to_owned(),
                }))
            }
        }
        .clone();

        Ok(GetPeersResponse::new(trans_id, node_id, token, info_type))
    }

    pub fn transaction_id(&self) -> &'a [u8] {
        self.trans_id
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn token(&self) -> Option<&'a [u8]> {
        self.token
    }

    pub fn info_type(&self) -> &CompactInfoType<'a, BencodeRef<'a>, BencodeRef<'a>> {
        &self.info_type
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut response_args = BTreeMap::new();

        response_args.insert(message::NODE_ID_KEY.as_bytes(), ben_bytes!(self.node_id.as_ref()));
        match self.token {
            Some(token) => {
                response_args.insert(message::TOKEN_KEY.as_bytes(), ben_bytes!(token));
            }
            None => (),
        };

        match &self.info_type {
            CompactInfoType::Nodes(nodes) => {
                response_args.insert(message::NODES_KEY.as_bytes(), ben_bytes!(nodes.nodes()));
            }
            CompactInfoType::Values(values) => {
                response_args.insert(
                    message::VALUES_KEY.as_bytes(),
                    BencodeMut::new_bytes(std::borrow::Cow::Borrowed(values.values().first().unwrap().bytes().unwrap())),
                );
            }
            CompactInfoType::Both(nodes, values) => {
                response_args.insert(message::NODES_KEY.as_bytes(), ben_bytes!(nodes.nodes()));
                response_args.insert(
                    message::VALUES_KEY.as_bytes(),
                    BencodeMut::new_bytes(std::borrow::Cow::Borrowed(values.values().first().unwrap().bytes().unwrap())),
                );
            }
        };

        (ben_map! {
            //message::CLIENT_TYPE_KEY => ben_bytes!(dht::CLIENT_IDENTIFICATION),
            message::TRANSACTION_ID_KEY => ben_bytes!(self.trans_id),
            message::MESSAGE_TYPE_KEY => ben_bytes!(message::RESPONSE_TYPE_KEY),
            message::REQUEST_TYPE_KEY => ben_bytes!(request::GET_PEERS_TYPE_KEY),
            response::RESPONSE_ARGS_KEY => response_args.first_key_value().unwrap().1.clone()
        })
        .encode()
    }
}
