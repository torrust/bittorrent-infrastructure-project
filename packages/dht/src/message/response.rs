use bencode::{BConvert, BDictAccess, BListAccess, BRefAccess, BencodeConvertError, BencodeRef};
use util::bt::NodeId;

use crate::error::{DhtError, DhtErrorKind, DhtResult};
use crate::message::announce_peer::AnnouncePeerResponse;
use crate::message::compact_info::{CompactNodeInfo, CompactValueInfo};
use crate::message::find_node::FindNodeResponse;
use crate::message::get_peers::GetPeersResponse;
use crate::message::ping::PingResponse;

pub const RESPONSE_ARGS_KEY: &str = "r";

// ----------------------------------------------------------------------------//

pub struct ResponseValidate<'a> {
    trans_id: &'a [u8],
}

impl<'a> ResponseValidate<'a> {
    pub fn new(trans_id: &'a [u8]) -> ResponseValidate<'a> {
        ResponseValidate { trans_id }
    }

    pub fn validate_node_id(&self, node_id: &[u8]) -> DhtResult<NodeId> {
        NodeId::from_hash(node_id).map_err(|_| {
            DhtError::from_kind(DhtErrorKind::InvalidResponse {
                details: format!(
                    "TID {:?} Found Node ID With Invalid Length {:?}",
                    self.trans_id,
                    node_id.len()
                ),
            })
        })
    }

    /// Validate the given nodes string which should be IPv4 compact
    pub fn validate_nodes<'b>(&self, nodes: &'b [u8]) -> DhtResult<CompactNodeInfo<'b>> {
        CompactNodeInfo::new(nodes).map_err(|_| {
            DhtError::from_kind(DhtErrorKind::InvalidResponse {
                details: format!(
                    "TID {:?} Found Nodes Structure With {} Number Of Bytes Instead \
                                  Of Correct Multiple",
                    self.trans_id,
                    nodes.len()
                ),
            })
        })
    }

    pub fn validate_values<'b, B, C>(self, values: Box<std::vec::Vec<Box<B>>>) -> DhtResult<CompactValueInfo<'b, B, C>>
    where
        B: BRefAccess<BKey = &'b [u8], BType = C>,
    {
        for bencode in values.iter() {
            match bencode.bytes() {
                Some(_) => (),
                None => {
                    return Err(DhtError::from_kind(DhtErrorKind::InvalidResponse {
                        details: format!("TID {:?} Found Values Structure As Non Bytes Type", self.trans_id),
                    }))
                }
            }
        }

        let compact_error = CompactValueInfo::new(values);

        compact_error.map_err(|_| {
            DhtError::from_kind(DhtErrorKind::InvalidResponse {
                details: format!("TID {:?} Found Values Structrue With Wrong Number Of Bytes", self.trans_id),
            })
        })
    }
}

impl<'a> BConvert for ResponseValidate<'a> {
    type Error = DhtError;

    fn handle_error(&self, error: BencodeConvertError) -> DhtError {
        error.into()
    }
}

// ----------------------------------------------------------------------------//

#[allow(unused)]
pub enum ExpectedResponse {
    Ping,
    FindNode,
    GetPeers,
    AnnouncePeer,
    GetData,
    PutData,
    None,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ResponseType<'a> {
    Ping(PingResponse<'a>),
    FindNode(FindNodeResponse<'a>),
    GetPeers(GetPeersResponse<'a>),
    AnnouncePeer(AnnouncePeerResponse<'a>), /* GetData(GetDataResponse<'a>),
                                             * PutData(PutDataResponse<'a>) */
}

impl<'a> ResponseType<'a> {
    pub fn from_parts<B>(
        root: &'a dyn BDictAccess<B::BKey, B::BType>,
        trans_id: &'a [u8],
        rsp_type: ExpectedResponse,
    ) -> DhtResult<ResponseType<'a>>
    where
        B: for<'a_> BRefAccess<BKey = &'a [u8], BType = BencodeRef<'a>>,
    {
        let validate = ResponseValidate::new(trans_id);
        let rqst_root = (validate.lookup_and_convert_dict(root, RESPONSE_ARGS_KEY))?;

        match rsp_type {
            ExpectedResponse::Ping => {
                let ping_rsp = (PingResponse::from_parts::<BencodeRef>(rqst_root, trans_id))?;
                Ok(ResponseType::Ping(ping_rsp))
            }
            ExpectedResponse::FindNode => {
                let find_node_rsp = (FindNodeResponse::from_parts::<BencodeRef>(rqst_root, trans_id))?;
                Ok(ResponseType::FindNode(find_node_rsp))
            }
            ExpectedResponse::GetPeers => {
                let get_peers_rsp = (GetPeersResponse::from_parts::<BencodeRef>(rqst_root, trans_id))?;
                Ok(ResponseType::GetPeers(get_peers_rsp))
            }
            ExpectedResponse::AnnouncePeer => {
                let announce_peer_rsp = (AnnouncePeerResponse::from_parts::<BencodeRef>(rqst_root, trans_id))?;
                Ok(ResponseType::AnnouncePeer(announce_peer_rsp))
            }
            ExpectedResponse::GetData => {
                unimplemented!();
            }
            ExpectedResponse::PutData => {
                unimplemented!();
            }
            ExpectedResponse::None => Err(DhtError::from_kind(DhtErrorKind::UnsolicitedResponse)),
        }
    }
}
