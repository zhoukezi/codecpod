use std::path::Path;

use crate::{AudioBuffer, AudioInfo, Error, LoadOptions};

pub fn info(_path: &Path) -> Result<AudioInfo, Error> {
    unimplemented!("docs.rs stub: FFmpeg backend not built")
}

pub fn info_bytes(_data: &[u8]) -> Result<AudioInfo, Error> {
    unimplemented!("docs.rs stub: FFmpeg backend not built")
}

pub fn load(_path: &Path, _opts: &LoadOptions) -> Result<AudioBuffer, Error> {
    unimplemented!("docs.rs stub: FFmpeg backend not built")
}

pub fn load_bytes(_data: &[u8], _opts: &LoadOptions) -> Result<AudioBuffer, Error> {
    unimplemented!("docs.rs stub: FFmpeg backend not built")
}
