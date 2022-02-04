// Copyright 2021-2022 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use hex::FromHex;
use md5::{digest::generic_array::GenericArray, Digest, Md5};
use sha1::Sha1;
use std::{io, path::Path};
use thiserror::Error;

use crate::request::RequestChecksum;

#[derive(Debug, Error)]
pub enum ChecksumError {
    #[error("checksum invalid: {0}")]
    InvalidInput(String),

    #[error("unable to open the file to validate")]
    FileOpen(#[source] io::Error),

    #[error("file does not match expected size")]
    InvalidSize,

    #[error("error during read of file")]
    FileRead(#[source] io::Error),

    #[error("checksum mismatch")]
    Mismatch,
}

pub fn compare_hash(
    path: &Path,
    expected_size: u64,
    expected_hash: &RequestChecksum,
) -> Result<(), ChecksumError> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).map_err(ChecksumError::FileOpen)?;

    let file_size = file.metadata().unwrap().len();
    if file_size != expected_size {
        return Err(ChecksumError::InvalidSize);
    }

    match expected_hash {
        RequestChecksum::Sha1(sum) => {
            let expected = <[u8; 20]>::from_hex(&*sum)
                .map(GenericArray::from)
                .map_err(|_| ChecksumError::InvalidInput(format!("SHA1 {}", sum)))?;

            let mut buffer = vec![0u8; 8 * 1024];
            let mut hasher = Sha1::new();

            loop {
                match file.read(&mut buffer) {
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
        RequestChecksum::Md5(sum) => {
            let expected = <[u8; 16]>::from_hex(&*sum)
                .map(GenericArray::from)
                .map_err(|_| ChecksumError::InvalidInput(format!("MD5 {}", sum)))?;

            let mut buffer = vec![0u8; 8 * 1024];
            let mut hasher = Md5::new();

            loop {
                match file.read(&mut buffer) {
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
    }
}
