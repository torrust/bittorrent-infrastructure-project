use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use futures::channel::mpsc;
use futures::future::BoxFuture;
use futures::lock::Mutex;
use futures::sink::SinkExt;
use metainfo::Metainfo;
use util::bt::InfoHash;

use crate::disk::tasks::helpers::piece_checker::PieceCheckerState;
use crate::disk::ODiskMessage;
use crate::FileSystem;

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
    pub fn new(file: Metainfo, state: Arc<Mutex<PieceCheckerState>>) -> MetainfoState {
        MetainfoState { file, checker: state }
    }
}

impl<F> DiskManagerContext<F>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    pub fn new(out: mpsc::Sender<ODiskMessage>, fs: Arc<F>) -> DiskManagerContext<F> {
        DiskManagerContext {
            torrents: Arc::new(RwLock::new(HashMap::new())),
            out,
            fs,
        }
    }

    #[allow(dead_code)]
    pub async fn send_message(&mut self, message: ODiskMessage) -> Result<(), futures::channel::mpsc::SendError> {
        self.out.send(message).await
    }

    pub fn filesystem(&self) -> &Arc<F> {
        &self.fs
    }

    pub fn insert_torrent(
        &self,
        file: Metainfo,
        state: &Arc<Mutex<PieceCheckerState>>,
    ) -> Result<InfoHash, (InfoHash, Box<MetainfoState>)> {
        let mut write_torrents = self
            .torrents
            .write()
            .expect("bip_disk: DiskManagerContext::insert_torrents Failed To Write Torrent");

        let hash = file.info().info_hash();

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

        Some(with_state(self.fs.clone(), state.clone()).await)
    }

    pub fn remove_torrent(&self, hash: InfoHash) -> bool {
        let mut write_torrents = self
            .torrents
            .write()
            .expect("bip_disk: DiskManagerContext::remove_torrent Failed To Write Torrent");

        write_torrents.remove(&hash).is_some()
    }
}
