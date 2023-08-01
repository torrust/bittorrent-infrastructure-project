use std::borrow::Cow;
use std::fmt::Debug;
use std::hash::Hash;
use std::net::{Ipv4Addr, SocketAddrV4};

use bencode::{BListAccess, BRefAccess};
use util::bt::{self, NodeId};
use util::error::{LengthError, LengthErrorKind, LengthResult};
use util::sha::ShaHash;

// TODO: Update this module to accept data sources as both a slice of bytes and probably
// a wrapper around a closest nodes iterator. Eventually when the interfaces are updated
// to a writer interface instead of a reader interface, we wont expose nodes as a series
// of bytes but instead offer to write the nodes into a provided buffer.

const BYTES_PER_COMPACT_IP: usize = 6;
const BYTES_PER_COMPACT_NODE_INFO: usize = 26;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct CompactNodeInfo<'a> {
    nodes: &'a [u8],
}

impl<'a> CompactNodeInfo<'a> {
    pub fn new(nodes: &'a [u8]) -> LengthResult<CompactNodeInfo<'a>> {
        if nodes.len() % BYTES_PER_COMPACT_NODE_INFO != 0 {
            Err(LengthError::new(
                LengthErrorKind::LengthMultipleExpected,
                BYTES_PER_COMPACT_NODE_INFO,
            ))
        } else {
            Ok(CompactNodeInfo { nodes })
        }
    }

    #[must_use]
    pub fn nodes(&self) -> &'a [u8] {
        self.nodes
    }
}

impl<'a> IntoIterator for CompactNodeInfo<'a> {
    type Item = (NodeId, SocketAddrV4);
    type IntoIter = CompactNodeInfoIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        CompactNodeInfoIter {
            nodes: self.nodes,
            pos: 0,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct CompactNodeInfoIter<'a> {
    nodes: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for CompactNodeInfoIter<'a> {
    type Item = (NodeId, SocketAddrV4);

    fn next(&mut self) -> Option<(NodeId, SocketAddrV4)> {
        if self.pos == self.nodes.len() {
            None
        } else {
            let compact_info_offset = self.pos + BYTES_PER_COMPACT_NODE_INFO;
            let compact_info = &self.nodes[self.pos..compact_info_offset];

            self.pos += BYTES_PER_COMPACT_NODE_INFO;

            Some(parts_from_compact_info(compact_info))
        }
    }
}

// ----------------------------------------------------------------------------//

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CompactValueInfo<'a, B>
where
    B: BRefAccess<BType = B> + Clone,
{
    values: Cow<'a, Vec<Cow<'a, B::BType>>>,
}

impl<'a, B> CompactValueInfo<'a, B>
where
    B: BRefAccess<BType = B> + Clone,
{
    /// Creates a new `CompactValueInfo` container for the given values.
    ///
    /// It is VERY important that the values have been checked to contain only
    /// bencoded bytes and not other types as that will result in a panic.
    pub fn new(values: &'a dyn BListAccess<B::BType>) -> LengthResult<CompactValueInfo<'a, B>> {
        for (index, node) in values.into_iter().enumerate() {
            // TODO: Do not unwrap here please
            let compact_value = node.bytes().unwrap();

            if compact_value.len() != BYTES_PER_COMPACT_IP {
                return Err(LengthError::with_index(
                    LengthErrorKind::LengthExpected,
                    BYTES_PER_COMPACT_IP,
                    index,
                ));
            }
        }

        Ok(CompactValueInfo {
            values: Cow::Owned(values.into_iter().map(|b| Cow::Borrowed(b)).collect()),
        })
    }

    #[must_use]
    pub fn values(&self) -> Vec<Cow<'a, B::BType>> {
        self.values.to_vec()
    }
}

impl<'a, B> IntoIterator for CompactValueInfo<'a, B>
where
    B: BRefAccess<BType = B> + Clone,
    B::BType: PartialEq + Eq + core::hash::Hash + Debug,
{
    type Item = SocketAddrV4;
    type IntoIter = CompactValueInfoIter<'a, B>;

    fn into_iter(self) -> Self::IntoIter {
        CompactValueInfoIter {
            values: self.values,
            pos: 0,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CompactValueInfoIter<'a, B>
where
    B: BRefAccess<BType = B> + Clone,
{
    values: Cow<'a, Vec<Cow<'a, B::BType>>>,
    pos: usize,
}

impl<'a, B> Iterator for CompactValueInfoIter<'a, B>
where
    B: BRefAccess<BType = B> + Clone,
{
    type Item = SocketAddrV4;

    fn next(&mut self) -> Option<SocketAddrV4> {
        if self.pos == self.values.len() {
            None
        } else {
            let compact_info = &self.values[self.pos];

            self.pos += 1;

            Some(socket_v4_from_bytes_be(compact_info.bytes().unwrap()).unwrap())
        }
    }
}

// ----------------------------------------------------------------------------//

/// Panics if the size of `compact_info` is less than `BYTES_PER_COMPACT_NODE_INFO`.
fn parts_from_compact_info(compact_info: &[u8]) -> (NodeId, SocketAddrV4) {
    // Use unwrap here because we know these can never fail, but they aren't statically guaranteed
    let node_id = ShaHash::from_hash(&compact_info[0..bt::NODE_ID_LEN]).unwrap();

    let compact_ip_offset = bt::NODE_ID_LEN + BYTES_PER_COMPACT_IP;
    let socket = socket_v4_from_bytes_be(&compact_info[bt::NODE_ID_LEN..compact_ip_offset]).unwrap();

    (node_id, socket)
}

fn socket_v4_from_bytes_be(bytes: &[u8]) -> LengthResult<SocketAddrV4> {
    if bytes.len() != BYTES_PER_COMPACT_IP {
        Err(LengthError::new(LengthErrorKind::LengthExpected, BYTES_PER_COMPACT_IP))
    } else {
        let (oc_one, oc_two, oc_three, oc_four) = (bytes[0], bytes[1], bytes[2], bytes[3]);

        let mut port = 0u16;
        port |= u16::from(bytes[4]);
        port <<= 8;
        port |= u16::from(bytes[5]);

        let ip = Ipv4Addr::new(oc_one, oc_two, oc_three, oc_four);

        Ok(SocketAddrV4::new(ip, port))
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddrV4};

    use bencode::{ben_bytes, ben_list, BRefAccess, BencodeMut, BencodeRef};
    use util::bt::NodeId;
    use util::sha::ShaHash;

    use crate::message::compact_info::{CompactNodeInfo, CompactValueInfo};

    #[test]
    fn positive_compact_nodes_empty() {
        let bytes = [0u8; 0];
        let compact_node = CompactNodeInfo::new(&bytes[..]).unwrap();

        assert_eq!(compact_node.into_iter().count(), 0);
    }

    #[test]
    fn positive_compact_nodes_one() {
        let bytes = [
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 192, 168, 0, 1, 170, 169,
        ];
        let compact_node = CompactNodeInfo::new(&bytes[..]).unwrap();

        let collected_info: Vec<(NodeId, SocketAddrV4)> = compact_node.into_iter().collect();
        assert_eq!(collected_info.len(), 1);

        assert_eq!(collected_info[0].0, ShaHash::from_hash(&bytes[0..20]).unwrap());
        assert_eq!(collected_info[0].1, SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, 1), 43689));
    }

    #[test]
    fn positive_compact_nodes_many() {
        let bytes = [
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 192, 168, 0, 1, 0, 240, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
            1, 1, 1, 1, 1, 1, 1, 1, 1, 192, 168, 0, 2, 0, 240,
        ];
        let compact_node = CompactNodeInfo::new(&bytes[..]).unwrap();

        let collected_info: Vec<(NodeId, SocketAddrV4)> = compact_node.into_iter().collect();
        assert_eq!(collected_info.len(), 2);

        assert_eq!(collected_info[0].0, ShaHash::from_hash(&bytes[0..20]).unwrap());
        assert_eq!(collected_info[0].1, SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, 1), 240));

        assert_eq!(collected_info[1].0, ShaHash::from_hash(&bytes[0..20]).unwrap());
        assert_eq!(collected_info[1].1, SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, 2), 240));
    }

    #[test]
    fn positive_compact_values_empty() {
        let bencode_values = Vec::new();
        let compact_value: CompactValueInfo<'_, BencodeRef<'_>> = CompactValueInfo::new(&bencode_values).unwrap();

        let collected_info: Vec<SocketAddrV4> = compact_value.into_iter().collect();

        assert!(collected_info.is_empty());
    }

    #[test]
    fn positive_compact_values_one() {
        let bytes = [127, 0, 0, 1, (6881 >> 8) as u8, (6881 & 0x00FF) as u8];
        let bencode_values = ben_list!(ben_bytes!(&bytes[..]));
        let compact_value: CompactValueInfo<'_, BencodeMut<'_>> = CompactValueInfo::new(bencode_values.list().unwrap()).unwrap();

        let collected_info: Vec<SocketAddrV4> = compact_value.into_iter().collect();
        assert_eq!(collected_info.len(), 1);

        assert_eq!(collected_info[0], SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 6881));
    }

    #[test]
    fn positive_compact_values_many() {
        let bytes_one = [127, 0, 0, 1, (6881 >> 8) as u8, (6881 & 0x00FF) as u8];
        let bytes_two = [10, 0, 0, 1, (6889 >> 8) as u8, (6889 & 0x00FF) as u8];
        let bencode_values = ben_list!(ben_bytes!(&bytes_one[..]), ben_bytes!(&bytes_two[..]));
        let compact_value: CompactValueInfo<'_, BencodeMut<'_>> = CompactValueInfo::new(bencode_values.list().unwrap()).unwrap();

        let collected_info: Vec<SocketAddrV4> = compact_value.into_iter().collect();
        assert_eq!(collected_info.len(), 2);

        assert_eq!(collected_info[0], SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 6881));
        assert_eq!(collected_info[1], SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 6889));
    }
}
