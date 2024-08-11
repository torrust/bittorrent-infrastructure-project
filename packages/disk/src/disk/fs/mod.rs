use std::path::Path;
use std::sync::Arc;

pub mod cache;
pub mod native;

/// Trait for performing operations on some file system.
///
/// Relative paths will originate from an implementation defined directory.
pub trait FileSystem {
    /// Some file object.
    type File;

    /// Open a file, create it if it does not exist.
    ///
    /// Intermediate directories will be created if necessary.
    ///
    /// # Errors
    ///
    /// It would return an IO error if there is an problem.
    fn open_file<P>(&self, path: P) -> std::io::Result<Self::File>
    where
        P: AsRef<Path> + Send + 'static;

    /// Sync the file.
    ///
    /// # Errors
    ///
    /// It would return an IO error if there is an problem.
    fn sync_file<P>(&self, path: P) -> std::io::Result<()>
    where
        P: AsRef<Path> + Send + 'static;

    /// Get the size of the file in bytes.
    ///
    /// # Errors
    ///
    /// It would return an IO error if there is an problem.
    fn file_size(&self, file: &Self::File) -> std::io::Result<u64>;

    /// Read the contents of the file at the given offset.
    ///
    /// On success, return the number of bytes read.
    ///
    /// # Errors
    ///
    /// It would return an IO error if there is an problem.
    fn read_file(&self, file: &mut Self::File, offset: u64, buffer: &mut [u8]) -> std::io::Result<usize>;

    /// Write the contents of the file at the given offset.
    ///
    /// On success, return the number of bytes written. If offset is
    /// past the current size of the file, zeroes will be filled in.
    ///
    /// # Errors
    ///
    /// It would return an IO error if there is an problem.
    fn write_file(&self, file: &mut Self::File, offset: u64, buffer: &[u8]) -> std::io::Result<usize>;
}

impl<'a, F> FileSystem for &'a F
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    type File = F::File;

    fn open_file<P>(&self, path: P) -> std::io::Result<Self::File>
    where
        P: AsRef<Path> + Send + 'static,
    {
        FileSystem::open_file(*self, path)
    }

    fn sync_file<P>(&self, path: P) -> std::io::Result<()>
    where
        P: AsRef<Path> + Send + 'static,
    {
        FileSystem::sync_file(*self, path)
    }

    fn file_size(&self, file: &Self::File) -> std::io::Result<u64> {
        FileSystem::file_size(*self, file)
    }

    fn read_file(&self, file: &mut Self::File, offset: u64, buffer: &mut [u8]) -> std::io::Result<usize> {
        FileSystem::read_file(*self, file, offset, buffer)
    }

    fn write_file(&self, file: &mut Self::File, offset: u64, buffer: &[u8]) -> std::io::Result<usize> {
        FileSystem::write_file(*self, file, offset, buffer)
    }
}
