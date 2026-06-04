use std::path::Path;

use crate::{AudioBuffer, Error, SaveOptions};

pub(crate) fn save(_path: &Path, _buf: &AudioBuffer, _opts: &SaveOptions) -> Result<(), Error> {
    unimplemented!("docs.rs stub: FFmpeg backend not built")
}

pub(crate) fn save_bytes(_buf: &AudioBuffer, _opts: &SaveOptions) -> Result<Vec<u8>, Error> {
    unimplemented!("docs.rs stub: FFmpeg backend not built")
}
