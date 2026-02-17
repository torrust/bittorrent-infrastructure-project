use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

/// Sender type used by the background dispatcher tasks.
pub type MessageSender<T> = UnboundedSender<T>;
/// Receiver type used by the background dispatcher tasks.
pub type MessageReceiver<T> = UnboundedReceiver<T>;
/// Handle for the spawned dispatcher runtime.
pub type ShutdownHandle = std::thread::JoinHandle<()>;

/// Create an unbounded channel for dispatcher messages.
///
/// Backpressure is enforced at the application level by [`crate::client::RequestLimiter`],
/// so the channel itself does not need a capacity bound.
pub fn channel<T>() -> (MessageSender<T>, MessageReceiver<T>) {
    tokio::sync::mpsc::unbounded_channel()
}
