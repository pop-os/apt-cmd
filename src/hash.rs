use async_fs::File;
use futures::prelude::*;
use hex::FromHex;
use md5::{digest::generic_array::GenericArray, Digest, Md5};
use std::{io, path::Path};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChecksumError {
    #[error("expected md5 checksum is not a valid md5 sum")]
    InvalidInput,

    #[error("unable to open the file to validate")]
    FileOpen(#[source] io::Error),

    #[error("file does not match expected size")]
    InvalidSize,

    #[error("error during read of file")]
    FileRead(#[source] io::Error),

    #[error("checksum mismatch")]
    Mismatch,
}

pub async fn compare_hash(
    path: &Path,
    expected_size: u64,
    expected_hash: &str,
) -> Result<(), ChecksumError> {
    let expected = <[u8; 16]>::from_hex(&*expected_hash)
        .map(GenericArray::from)
        .map_err(|_| ChecksumError::InvalidInput)?;

    let mut file = File::open(path).await.map_err(ChecksumError::FileOpen)?;

    let file_size = file.metadata().await.unwrap().len();
    if file_size != expected_size {
        return Err(ChecksumError::InvalidSize);
    }

    let mut buffer = vec![0u8; 8 * 1024];
    let mut hasher = Md5::new();

    loop {
        match file.read(&mut buffer).await {
            Ok(0) => break,
            Ok(bytes) => hasher.update(&buffer[..bytes]),
            Err(why) => return Err(ChecksumError::FileRead(why)),
        }
    }

    let hash = &*hasher.finalize();

    if &*expected == hash {
        Ok(())
    } else {
        Err(ChecksumError::Mismatch)
    }
}
