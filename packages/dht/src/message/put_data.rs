use bencode::{Bencode, BencodeConvert, Dictionary};

use crate::error::{DhtError, DhtErrorKind, DhtResult};

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct PutDataRequest<'a> {
    trans_id: &'a [u8],
}

impl<'a> PutDataRequest<'a> {
    pub fn new(rqst_root: &BDictAccess<'a, BRefAccess<'a>>, trans_id: &'a [u8]) -> DhtResult<PutDataRequest<'a>> {
        unimplemented!();
    }

    pub fn transaction_id(&self) -> &'a [u8] {
        self.trans_id
    }
}
