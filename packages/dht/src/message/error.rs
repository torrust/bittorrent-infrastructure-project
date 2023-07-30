// TODO: Still trying to decide how we want to use this module.
#![allow(unused)]

use std::borrow::Cow;

use bencode::ext::BConvertExt;
use bencode::{ben_bytes, ben_int, ben_list, ben_map, BConvert, BDictAccess, BListAccess, BRefAccess, BencodeConvertError};

use crate::error::{DhtError, DhtErrorKind, DhtResult};
use crate::message;

const ERROR_ARGS_KEY: &str = "e";
const NUM_ERROR_ARGS: usize = 2;

const GENERIC_ERROR_CODE: u8 = 201;
const SERVER_ERROR_CODE: u8 = 202;
const PROTOCOL_ERROR_CODE: u8 = 203;
const METHOD_UNKNOWN_CODE: u8 = 204;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ErrorCode {
    GenericError,
    ServerError,
    ProtocolError,
    MethodUnknown,
}

impl ErrorCode {
    fn new(code: u8) -> DhtResult<ErrorCode> {
        match code {
            GENERIC_ERROR_CODE => Ok(ErrorCode::GenericError),
            SERVER_ERROR_CODE => Ok(ErrorCode::ServerError),
            PROTOCOL_ERROR_CODE => Ok(ErrorCode::ProtocolError),
            METHOD_UNKNOWN_CODE => Ok(ErrorCode::MethodUnknown),
            unknown => Err(DhtError::from_kind(DhtErrorKind::InvalidResponse {
                details: format!("Error Message Invalid Error Code {unknown:?}"),
            })),
        }
    }
}

impl From<ErrorCode> for u8 {
    fn from(val: ErrorCode) -> Self {
        match val {
            ErrorCode::GenericError => GENERIC_ERROR_CODE,
            ErrorCode::ServerError => SERVER_ERROR_CODE,
            ErrorCode::ProtocolError => PROTOCOL_ERROR_CODE,
            ErrorCode::MethodUnknown => METHOD_UNKNOWN_CODE,
        }
    }
}

// ----------------------------------------------------------------------------//

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
struct ErrorValidate;

impl ErrorValidate {
    fn extract_error_args<B>(&self, args: &dyn BListAccess<B::BType>) -> DhtResult<(u8, String)>
    where
        B: BRefAccess<BType = B>,
    {
        if args.len() != NUM_ERROR_ARGS {
            return Err(DhtError::from_kind(DhtErrorKind::InvalidResponse {
                details: format!("Error Message Invalid Number Of Error Args: {}", args.len()),
            }));
        }

        let code = self.convert_int(&args[0], format!("{ERROR_ARGS_KEY}[0]"))?;
        let message = String::from(self.convert_str(&args[1], &format!("{ERROR_ARGS_KEY}[1]"))?);

        let code2 = code;

        Ok((code as u8, message))
    }
}

impl BConvert for ErrorValidate {
    type Error = DhtError;

    fn handle_error(&self, error: BencodeConvertError) -> DhtError {
        error.into()
    }
}

impl BConvertExt for ErrorValidate {}

// ----------------------------------------------------------------------------//

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ErrorMessage<'a> {
    trans_id: Cow<'a, [u8]>,
    code: ErrorCode,
    message: Cow<'a, str>,
}

impl<'a> ErrorMessage<'a> {
    /// TODO: Figure out a way to make the error message non static while still providing a clean
    // interface in error.rs for the DhtErrorKind object. Most likely our error messages will not
    // need to be dynamically generated (up in the air at this point) so this is a performance loss.
    #[must_use]
    pub fn new(trans_id: Vec<u8>, code: ErrorCode, message: String) -> ErrorMessage<'static> {
        let trans_id_cow = Cow::Owned(trans_id);
        let message_cow = Cow::Owned(message);

        ErrorMessage {
            trans_id: trans_id_cow,
            code,
            message: message_cow,
        }
    }

    pub fn from_parts<B>(root: &dyn BDictAccess<B::BKey, B>, trans_id: &'a [u8]) -> DhtResult<ErrorMessage<'a>>
    where
        B: BRefAccess<BType = B>,
    {
        let validate = ErrorValidate;
        let error_args = validate.lookup_and_convert_list(root, ERROR_ARGS_KEY)?;

        let (code, message) = validate.extract_error_args::<B>(error_args)?;
        let error_code = ErrorCode::new(code)?;

        let trans_id_cow = Cow::Owned(trans_id.to_vec());
        let message_cow = Cow::Owned(message);

        Ok(ErrorMessage {
            trans_id: trans_id_cow,
            code: error_code,
            message: message_cow,
        })
    }

    #[must_use]
    pub fn transaction_id(&self) -> &[u8] {
        &self.trans_id
    }

    #[must_use]
    pub fn error_code(&self) -> ErrorCode {
        self.code
    }

    #[must_use]
    pub fn error_message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let error_code = i64::from(Into::<u8>::into(self.code));

        (ben_map! {
            //message::CLIENT_TYPE_KEY => ben_bytes!(dht::CLIENT_IDENTIFICATION),
            message::TRANSACTION_ID_KEY => ben_bytes!(self.trans_id.to_vec()),
            message::MESSAGE_TYPE_KEY => ben_bytes!(message::ERROR_TYPE_KEY),
            message::ERROR_TYPE_KEY => ben_list!(
                ben_int!(error_code),
                ben_bytes!(self.message.as_bytes())
            )
        })
        .encode()
    }
}
