mod buffer;
mod dispatcher;
mod eloop;
mod provider;

const WAKER_TOKEN: Token = Token(0);
const UDP_SOCKET_TOKEN: Token = Token(2);

pub mod external;

pub use dispatcher::Dispatcher;
pub use eloop::{ELoop, ELoopBuilder, MessageSender, ShutdownHandle};
use mio::Token;
pub use provider::Provider;
