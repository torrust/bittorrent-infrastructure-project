use std::sync::Arc;

use futures::channel::mpsc;
use futures::lock::Mutex;
use futures::{FutureExt, SinkExt as _};
use metainfo::Metainfo;
use util::bt::InfoHash;

use crate::disk::fs::FileSystem;
use crate::disk::tasks::context::DiskManagerContext;
use crate::disk::tasks::helpers::piece_accessor::PieceAccessor;
use crate::disk::tasks::helpers::piece_checker::{PieceChecker, PieceCheckerState, PieceState};
use crate::disk::{IDiskMessage, ODiskMessage};
use crate::error::{BlockError, BlockResult, TorrentError, TorrentResult};
use crate::memory::block::{Block, BlockMut};

pub mod context;
mod helpers;

pub async fn execute<F>(msg: IDiskMessage, context: DiskManagerContext<F>)
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    let mut sender = context.out.clone();

    let out_msg = match msg {
        IDiskMessage::AddTorrent(metainfo) => {
            let info_hash = metainfo.info().info_hash();

            match execute_add_torrent(metainfo, context, sender.clone()).await {
                Ok(()) => ODiskMessage::TorrentAdded(info_hash),
                Err(err) => ODiskMessage::TorrentError(info_hash, err),
            }
        }
        IDiskMessage::RemoveTorrent(hash) => match execute_remove_torrent(hash, &context) {
            Ok(()) => ODiskMessage::TorrentRemoved(hash),
            Err(err) => ODiskMessage::TorrentError(hash, err),
        },
        IDiskMessage::SyncTorrent(hash) => match execute_sync_torrent(hash, context).await {
            Ok(()) => ODiskMessage::TorrentSynced(hash),
            Err(err) => ODiskMessage::TorrentError(hash, err),
        },
        IDiskMessage::LoadBlock(mut block) => match execute_load_block(&mut block, context).await {
            Ok(()) => ODiskMessage::BlockLoaded(block),
            Err(err) => ODiskMessage::LoadBlockError(block, err),
        },
        IDiskMessage::ProcessBlock(block) => match execute_process_block(&block, context, sender.clone()).await {
            Ok(()) => ODiskMessage::BlockProcessed(block),
            Err(err) => ODiskMessage::ProcessBlockError(block, err),
        },
    };

    tracing::trace!("sending output disk message:  {out_msg:?}");

    sender
        .send(out_msg)
        .await
        .expect("bip_disk: Failed To Send Out Message In execute_on_pool");

    tracing::debug!("finished sending output message... ");
}

async fn execute_add_torrent<F>(
    file: Metainfo,
    context: DiskManagerContext<F>,
    sender: mpsc::Sender<ODiskMessage>,
) -> TorrentResult<()>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    let info_hash = file.info().info_hash();
    let init_state = PieceChecker::init_state(context.filesystem().clone(), file.info().clone()).await?;

    // In case we are resuming a download, we need to send the diff for the newly added torrent
    send_piece_diff(&init_state, info_hash, sender, true).await;

    match context.insert_torrent(file, &init_state) {
        Ok(_) => Ok(()),
        Err((hash, _)) => Err(TorrentError::ExistingInfoHash { hash }),
    }
}

fn execute_remove_torrent<F>(hash: InfoHash, context: &DiskManagerContext<F>) -> TorrentResult<()>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    if context.remove_torrent(hash) {
        Ok(())
    } else {
        Err(TorrentError::InfoHashNotFound { hash })
    }
}

async fn execute_sync_torrent<F>(hash: InfoHash, context: DiskManagerContext<F>) -> TorrentResult<()>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    let filesystem = context.filesystem().clone();

    let sync_result = context
        .update_torrent(hash, |_, state| {
            let opt_parent_dir = state.file.info().directory();

            for file in state.file.info().files() {
                let path = helpers::build_path(opt_parent_dir, file);

                match filesystem.sync_file(path) {
                    Ok(()) => continue,
                    Err(e) => return std::future::ready(Err(e)).boxed(),
                }
            }

            std::future::ready(Ok(())).boxed()
        })
        .await;

    match sync_result {
        Some(result) => Ok(result?),
        None => Err(TorrentError::InfoHashNotFound { hash }),
    }
}

async fn execute_load_block<F>(block: &mut BlockMut, context: DiskManagerContext<F>) -> BlockResult<()>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    let metadata = block.metadata();
    let info_hash = metadata.info_hash();
    let context = context.clone();

    let access_result = context
        .update_torrent(info_hash, |fs, state| {
            async move {
                let piece_accessor = PieceAccessor::new(fs, state);

                // Read The Piece In From The Filesystem;
                piece_accessor.read_piece(&mut *block, &metadata)
            }
            .boxed()
        })
        .await;

    match access_result {
        Some(result) => Ok(result?),
        None => Err(BlockError::InfoHashNotFound { hash: info_hash }),
    }
}

async fn execute_process_block<F>(
    block: &Block,
    context: DiskManagerContext<F>,
    sender: mpsc::Sender<ODiskMessage>,
) -> BlockResult<()>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    let metadata = block.metadata();
    let info_hash = metadata.info_hash();

    let block_result = context
        .update_torrent(info_hash, |fs, state| {
            tracing::trace!("Updating Blocks for Torrent: {info_hash}");

            async move {
                let piece_accessor = PieceAccessor::new(fs.clone(), state.clone());

                // Write Out Piece Out To The Filesystem And Recalculate The Diff
                let block_result = match piece_accessor.write_piece(block, &metadata) {
                    Ok(()) => {
                        state.checker.lock().await.add_pending_block(metadata);

                        PieceChecker::with_state(fs, state.clone()).calculate_diff().await
                    }
                    Err(e) => Err(e),
                };

                send_piece_diff(&state.checker, state.file.info().info_hash(), sender.clone(), false).await;

                block_result
            }
            .boxed()
        })
        .await;

    tracing::debug!("Finished Updating Torrent: {info_hash}, {block_result:?}");

    match block_result {
        Some(result) => Ok(result?),
        None => Err(BlockError::InfoHashNotFound { hash: info_hash }),
    }
}

async fn send_piece_diff(
    checker_state: &Arc<Mutex<PieceCheckerState>>,
    hash: InfoHash,
    sender: mpsc::Sender<ODiskMessage>,
    ignore_bad: bool,
) {
    checker_state
        .lock()
        .await
        .run_with_diff(|piece_state| {
            let mut sender = sender.clone();

            async move {
                let opt_out_msg = match (piece_state, ignore_bad) {
                    (&PieceState::Good(index), _) => Some(ODiskMessage::FoundGoodPiece(hash, index)),
                    (&PieceState::Bad(index), false) => Some(ODiskMessage::FoundBadPiece(hash, index)),
                    (&PieceState::Bad(_), true) => None,
                };

                if let Some(out_msg) = opt_out_msg {
                    sender
                        .send(out_msg)
                        .await
                        .expect("bip_disk: Failed To Send Piece State Message");
                }
            }
            .boxed()
        })
        .await;
}
