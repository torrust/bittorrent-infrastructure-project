use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Once, Weak};
use std::time::Duration;

use bytes::BytesMut;
use disk::{BlockMetadata, BlockMut, FileSystem, IDiskMessage};
use futures::future::BoxFuture;
use futures::stream::Stream;
use futures::{future, Sink, SinkExt as _, StreamExt as _};
use metainfo::{Accessor, IntoAccessor, PieceAccess};
use tokio::time::timeout;
use tracing::level_filters::LevelFilter;
use util::bt::InfoHash;

#[allow(dead_code)]
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);

#[allow(dead_code)]
pub static INIT: Once = Once::new();

#[allow(dead_code)]
pub fn tracing_stderr_init(filter: LevelFilter) {
    let builder = tracing_subscriber::fmt()
        .with_max_level(filter)
        .with_ansi(true)
        .with_writer(std::io::stderr);

    builder.pretty().with_file(true).init();

    tracing::info!("Logging initialized");
}

/// Generate buffer of size random bytes.
pub fn random_buffer(size: usize) -> Vec<u8> {
    let mut buffer = vec![0u8; size];
    rand::Rng::fill(&mut rand::thread_rng(), buffer.as_mut_slice());
    buffer
}

/// Initiate a runtime loop with the given timeout, state, and closure.
///
/// Returns R or panics if an error occurred in the loop (including a timeout).
#[allow(dead_code)]
pub async fn runtime_loop_with_timeout<'a, 'b, I, S, F, R>(timeout_time: Duration, initial_state: (I, S), mut call: F) -> R
where
    F: FnMut(I, S, S::Item) -> future::Either<BoxFuture<'a, R>, BoxFuture<'b, (I, S)>>,
    S: Stream + Unpin,
    R: 'static,
    I: std::fmt::Debug + Clone,
{
    let mut state = initial_state;
    loop {
        let (init, mut stream) = state;
        if let Some(msg) = {
            timeout(timeout_time, stream.next())
                .await
                .unwrap_or_else(|_| panic!("timeout while waiting for next message: {timeout_time:?}, {init:?}"))
        } {
            match call(init.clone(), stream, msg) {
                future::Either::Left(fut) => {
                    return timeout(timeout_time, fut)
                        .await
                        .unwrap_or_else(|_| panic!("timeout waiting for final processing: {timeout_time:?}, {init:?}"));
                }
                future::Either::Right(fut) => {
                    state = timeout(timeout_time, fut)
                        .await
                        .unwrap_or_else(|_| panic!("timeout waiting for next loop state: {timeout_time:?}, {init:?}"));
                }
            }
        } else {
            panic!("End Of Stream Reached");
        }
    }
}

/// Send block with the given metadata and entire data given.
#[allow(dead_code)]
pub async fn send_block<S, M>(
    sink: &mut S,
    data: &[u8],
    hash: InfoHash,
    piece_index: u64,
    block_offset: u64,
    block_len: usize,
    modify: M,
) where
    S: Sink<IDiskMessage> + Unpin,
    M: Fn(&mut [u8]),
    <S as Sink<IDiskMessage>>::Error: std::fmt::Display,
{
    tracing::trace!(
        "sending block for torrent: {hash}, index: {piece_index}, block_offset: {block_offset}, block_length: {block_len}"
    );

    let mut bytes = BytesMut::new();
    bytes.extend_from_slice(data);

    let mut block = BlockMut::new(BlockMetadata::new(hash, piece_index, block_offset, block_len), bytes);

    modify(&mut block[..]);

    sink.send(IDiskMessage::ProcessBlock(block.into()))
        .await
        .unwrap_or_else(|e| panic!("Failed To Send Process Block Message: {e}"));
}

//----------------------------------------------------------------------------//

/// Allow us to mock out multi file torrents.
pub struct MultiFileDirectAccessor {
    dir: PathBuf,
    files: Vec<(Vec<u8>, PathBuf)>,
}

impl MultiFileDirectAccessor {
    pub fn new(dir: PathBuf, files: Vec<(Vec<u8>, PathBuf)>) -> MultiFileDirectAccessor {
        MultiFileDirectAccessor { dir, files }
    }
}

// TODO: Ugh, once specialization lands, we can see about having a default impl for IntoAccessor
impl IntoAccessor for MultiFileDirectAccessor {
    type Accessor = MultiFileDirectAccessor;

    fn into_accessor(self) -> std::io::Result<MultiFileDirectAccessor> {
        Ok(self)
    }
}

impl Accessor for MultiFileDirectAccessor {
    fn access_directory(&self) -> Option<&Path> {
        // Do not just return the option here, unwrap it and put it in
        // another Option (since we know this is a multi file torrent)
        Some(self.dir.as_ref())
    }

    fn access_metadata<C>(&self, mut callback: C) -> std::io::Result<()>
    where
        C: FnMut(u64, &Path),
    {
        for (buffer, path) in &self.files {
            callback(buffer.len() as u64, path);
        }

        Ok(())
    }

    fn access_pieces<C>(&self, mut callback: C) -> std::io::Result<()>
    where
        C: for<'a> FnMut(PieceAccess<'a>) -> std::io::Result<()>,
    {
        for (buffer, _) in &self.files {
            callback(PieceAccess::Compute(&mut &buffer[..]))?;
        }

        Ok(())
    }
}

//----------------------------------------------------------------------------//

/// Allow us to mock out the file system.
#[derive(Debug)]
pub struct InMemoryFileSystem {
    #[allow(dead_code)]
    me: Weak<Self>,
    files: Mutex<HashMap<PathBuf, Vec<u8>>>,
}

impl InMemoryFileSystem {
    pub fn new() -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            files: Mutex::default(),
        })
    }

    #[allow(dead_code)]
    pub fn me(&self) -> Arc<Self> {
        self.me.upgrade().unwrap()
    }

    pub fn run_with_lock<C, R>(&self, call: C) -> R
    where
        C: FnOnce(&mut HashMap<PathBuf, Vec<u8>>) -> R,
    {
        let mut lock_files = self.files.lock().unwrap();

        call(&mut lock_files)
    }
}

pub struct InMemoryFile {
    path: PathBuf,
}

impl FileSystem for InMemoryFileSystem {
    type File = InMemoryFile;

    fn open_file<P>(&self, path: P) -> std::io::Result<Self::File>
    where
        P: AsRef<Path> + Send + 'static,
    {
        let file_path = path.as_ref().to_path_buf();

        self.run_with_lock(|files| {
            if !files.contains_key(&file_path) {
                files.insert(file_path.clone(), Vec::new());
            }
        });

        Ok(InMemoryFile { path: file_path })
    }

    fn sync_file<P>(&self, _path: P) -> std::io::Result<()>
    where
        P: AsRef<Path> + Send + 'static,
    {
        Ok(())
    }

    fn file_size(&self, file: &Self::File) -> std::io::Result<u64> {
        self.run_with_lock(|files| {
            files
                .get(&file.path)
                .map(|file| file.len() as u64)
                .ok_or(std::io::Error::new(std::io::ErrorKind::NotFound, "File Not Found"))
        })
    }

    fn read_file(&self, file: &mut Self::File, offset: u64, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.run_with_lock(|files| {
            files
                .get(&file.path)
                .map(|file_buffer| {
                    let cast_offset: usize = offset.try_into().unwrap();
                    let bytes_to_copy = std::cmp::min(file_buffer.len() - cast_offset, buffer.len());
                    let bytes = &file_buffer[cast_offset..(bytes_to_copy + cast_offset)];

                    buffer.clone_from_slice(bytes);

                    bytes_to_copy
                })
                .ok_or(std::io::Error::new(std::io::ErrorKind::NotFound, "File Not Found"))
        })
    }

    fn write_file(&self, file: &mut Self::File, offset: u64, buffer: &[u8]) -> std::io::Result<usize> {
        self.run_with_lock(|files| {
            files
                .get_mut(&file.path)
                .map(|file_buffer| {
                    let cast_offset: usize = offset.try_into().unwrap();

                    let last_byte_pos = cast_offset + buffer.len();
                    if last_byte_pos > file_buffer.len() {
                        file_buffer.resize(last_byte_pos, 0);
                    }

                    let bytes_to_copy = std::cmp::min(file_buffer.len() - cast_offset, buffer.len());

                    if bytes_to_copy != 0 {
                        file_buffer[cast_offset..(cast_offset + bytes_to_copy)].clone_from_slice(buffer);
                    }

                    // TODO: If the file is full, this will return zero, we should also simulate std::io::ErrorKind::WriteZero
                    bytes_to_copy
                })
                .ok_or(std::io::Error::new(std::io::ErrorKind::NotFound, "File Not Found"))
        })
    }
}
