// Copyright 2021-2022 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use std::{
    hash::{Hash, Hasher},
    io,
    str::FromStr,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RequestError {
    #[error("apt command failed")]
    Command(#[from] io::Error),
    #[error("uri not found in output: {0}")]
    UriNotFound(String),
    #[error("invalid URI value: {0}")]
    UriInvalid(String),
    #[error("name not found in output: {0}")]
    NameNotFound(String),
    #[error("size not found in output: {0}")]
    SizeNotFound(String),
    #[error("size in output could not be parsed as an integer: {0}")]
    SizeParse(String),
    #[error("checksum not found in output: {0}")]
    ChecksumNotFound(String),
    #[error("unknown checksum for print-uri output: {0}")]
    UnknownChecksum(String),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RequestChecksum {
    Md5(String),
    Sha1(String),
    Sha256(String),
    Sha512(String),
}

#[derive(Debug, Clone, Eq)]
pub struct Request {
    pub uri: String,
    pub name: String,
    pub size: u64,
    pub checksum: RequestChecksum,
}

impl PartialEq for Request {
    fn eq(&self, other: &Self) -> bool {
        self.uri == other.uri
    }
}

impl Hash for Request {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.uri.hash(state);
    }
}

impl FromStr for Request {
    type Err = RequestError;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let mut words = line.split_whitespace();

        let mut uri = words
            .next()
            .ok_or_else(|| RequestError::UriNotFound(line.into()))?;

        // We need to remove the single quotes that apt-get encloses the URI within.
        if uri.len() <= 3 {
            return Err(RequestError::UriInvalid(uri.into()));
        } else {
            uri = &uri[1..uri.len() - 1];
        }

        let name = words
            .next()
            .ok_or_else(|| RequestError::NameNotFound(line.into()))?;
        let size = words
            .next()
            .ok_or_else(|| RequestError::SizeNotFound(line.into()))?;
        let size = size
            .parse::<u64>()
            .map_err(|_| RequestError::SizeParse(size.into()))?;

        let checksum_string = words
            .next()
            .ok_or_else(|| RequestError::ChecksumNotFound(line.into()))?;

        let checksum = if let Some(value) = checksum_string.strip_prefix("MD5Sum:") {
            RequestChecksum::Md5(value.to_owned())
        } else if let Some(value) = checksum_string.strip_prefix("SHA1:") {
            RequestChecksum::Sha1(value.to_owned())
        } else if let Some(value) = checksum_string.strip_prefix("SHA256:") {
            RequestChecksum::Sha256(value.to_owned())
        } else if let Some(value) = checksum_string.strip_prefix("SHA512:") {
            RequestChecksum::Sha512(value.to_owned())
        } else {
            return Err(RequestError::UnknownChecksum(checksum_string.into()));
        };

        Ok(Request {
            uri: uri.into(),
            name: name.into(),
            size,
            checksum,
        })
    }
}
