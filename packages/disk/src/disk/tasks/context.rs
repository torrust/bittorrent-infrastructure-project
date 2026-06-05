use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::{Arc, RwLock};

use futures::channel::mpsc;
use futures::future::BoxFuture;
use futures::lock::Mutex;
use futures::sink::SinkExt;
use torrust_metainfo::Metainfo;
use torrust_util::bt::InfoHash;

use crate::FileSystem;
use crate::disk::ODiskMessage;
use crate::disk::tasks::helpers::piece_checker::PieceCheckerState;

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct DiskManagerContext<F>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    torrents: Arc<RwLock<HashMap<InfoHash, MetainfoState>>>,
    pub out: mpsc::Sender<ODiskMessage>,
    fs: Arc<F>,
}

impl<F> Clone for DiskManagerContext<F>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    fn clone(&self) -> Self {
        Self {
            torrents: self.torrents.clone(),
            out: self.out.clone(),
            fs: self.fs.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MetainfoState {
    pub file: Metainfo,
    pub checker: Arc<Mutex<PieceCheckerState>>,
}

impl MetainfoState {
    pub const fn new(file: Metainfo, state: Arc<Mutex<PieceCheckerState>>) -> Self {
        Self { file, checker: state }
    }
}

impl<F> DiskManagerContext<F>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    pub fn new(out: mpsc::Sender<ODiskMessage>, fs: Arc<F>) -> Self {
        Self {
            torrents: Arc::new(RwLock::new(HashMap::new())),
            out,
            fs,
        }
    }

    #[allow(dead_code)]
    pub async fn send_message(&mut self, message: ODiskMessage) -> Result<(), futures::channel::mpsc::SendError> {
        self.out.send(message).await
    }

    pub const fn filesystem(&self) -> &Arc<F> {
        &self.fs
    }

    #[allow(clippy::significant_drop_tightening)]
    pub fn insert_torrent(
        &self,
        file: Metainfo,
        state: &Arc<Mutex<PieceCheckerState>>,
    ) -> Result<InfoHash, (InfoHash, Box<MetainfoState>)> {
        let hash = file.info().info_hash();

        let mut write_torrents = self
            .torrents
            .write()
            .expect("bip_disk: DiskManagerContext::insert_torrents Failed To Write Torrent");

        let entry = write_torrents.entry(hash);

        match entry {
            Entry::Occupied(key) => Err((hash, key.get().clone().into())),
            Entry::Vacant(vac) => {
                vac.insert(MetainfoState::new(file, state.clone()));
                Ok(hash)
            }
        }
    }

    pub async fn update_torrent<'a, C, D>(self, hash: InfoHash, with_state: C) -> Option<D>
    where
        C: FnOnce(Arc<F>, MetainfoState) -> BoxFuture<'a, D>,
    {
        let state = {
            let read_torrents = self
                .torrents
                .read()
                .expect("bip_disk: DiskManagerContext::update_torrent Failed To Read Torrent");

            read_torrents.get(&hash)?.clone()
        };

        let result = with_state(self.fs.clone(), state.clone()).await;
        Some(result)
    }

    #[allow(clippy::let_and_return)] // Required for Rust 2024 drop order
    pub fn remove_torrent(&self, hash: InfoHash) -> bool {
        let mut write_torrents = self
            .torrents
            .write()
            .expect("bip_disk: DiskManagerContext::remove_torrent Failed To Write Torrent");

        let removed = write_torrents.remove(&hash).is_some();
        removed
    }
}
