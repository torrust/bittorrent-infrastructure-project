use std::path::{Path, PathBuf};

use torrust_metainfo::File;

pub mod piece_accessor;
pub mod piece_checker;

pub fn build_path(parent_directory: Option<&Path>, file: &File) -> PathBuf {
    parent_directory.map_or_else(|| file.path().to_owned(), |dir| PathBuf::from(dir).join(file.path()))
}
