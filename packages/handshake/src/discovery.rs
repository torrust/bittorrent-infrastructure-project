use util::bt::PeerId;

/// Trait for advertisement information that other peers can discover.
#[allow(clippy::module_name_repetitions)]
pub trait DiscoveryInfo {
    /// Retrieve our public port that we advertise to others.
    fn port(&self) -> u16;

    /// Retrieve our `PeerId` that we advertise to others.
    fn peer_id(&self) -> PeerId;
}

impl<'a, T> DiscoveryInfo for &'a T
where
    T: DiscoveryInfo,
{
    fn port(&self) -> u16 {
        (*self).port()
    }

    fn peer_id(&self) -> PeerId {
        (*self).peer_id()
    }
}
