use std::borrow::Cow;
use std::io::{Read as _, Seek as _, Write as _};
use std::path::{Path, PathBuf};

use crate::disk::fs::FileSystem;

// TODO: This should be sanitizing paths passed into it so they don't escape the base directory!!!

/// File that exists on disk.
#[allow(clippy::module_name_repetitions)]
pub struct NativeFile {
    file: std::fs::File,
}

impl NativeFile {
    /// Create a new `NativeFile`.
    fn new(file: std::fs::File) -> NativeFile {
        NativeFile { file }
    }
}

/// File system that maps to the OS file system.
#[allow(clippy::module_name_repetitions)]
pub struct NativeFileSystem {
    current_dir: PathBuf,
}

impl NativeFileSystem {
    /// Initialize a new `NativeFileSystem` with the default directory set.
    pub fn with_directory<P>(default: P) -> NativeFileSystem
    where
        P: AsRef<Path>,
    {
        NativeFileSystem {
            current_dir: default.as_ref().to_path_buf(),
        }
    }
}

impl FileSystem for NativeFileSystem {
    type File = NativeFile;

    fn open_file<P>(&self, path: P) -> std::io::Result<Self::File>
    where
        P: AsRef<Path> + Send + 'static,
    {
        let combine_path = combine_user_path(&path, &self.current_dir);
        let file = create_new_file(combine_path)?;

        Ok(NativeFile::new(file))
    }

    fn sync_file<P>(&self, _path: P) -> std::io::Result<()>
    where
        P: AsRef<Path> + Send + 'static,
    {
        Ok(())
    }

    fn file_size(&self, file: &NativeFile) -> std::io::Result<u64> {
        file.file.metadata().map(|metadata| metadata.len())
    }

    fn read_file(&self, file: &mut NativeFile, offset: u64, buffer: &mut [u8]) -> std::io::Result<usize> {
        file.file.seek(std::io::SeekFrom::Start(offset))?;

        file.file.read(buffer)
    }

    fn write_file(&self, file: &mut NativeFile, offset: u64, buffer: &[u8]) -> std::io::Result<usize> {
        file.file.seek(std::io::SeekFrom::Start(offset))?;

        file.file.write(buffer)
    }
}

/// Create a new file with read and write options.
///
/// Intermediate directories will be created if they do not exist.
fn create_new_file<P>(path: P) -> std::io::Result<std::fs::File>
where
    P: AsRef<Path>,
{
    match path.as_ref().parent() {
        Some(parent_dir) => {
            std::fs::create_dir_all(parent_dir)?;

            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
        }
        None => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "File Path Has No Paren't",
        )),
    }
}

/// Create a path from the user path and current directory.
fn combine_user_path<'a, P>(user_path: &'a P, current_dir: &Path) -> Cow<'a, Path>
where
    P: AsRef<Path>,
{
    let ref_user_path = user_path.as_ref();

    if ref_user_path.is_absolute() {
        Cow::Borrowed(ref_user_path)
    } else {
        let mut combine_user_path = current_dir.to_path_buf();

        for user_path_piece in ref_user_path {
            combine_user_path.push(user_path_piece);
        }

        Cow::Owned(combine_user_path)
    }
}
