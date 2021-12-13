// Copyright 2021 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use std::{
    hash::{Hash, Hasher},
    io,
    str::FromStr,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RequestError {
    #[error("apt command failed: {}", _0)]
    Command(io::Error),
    #[error("uri not found in output: {}", _0)]
    UriNotFound(String),
    #[error("invalid URI value: {}", _0)]
    UriInvalid(String),
    #[error("name not found in output: {}", _0)]
    NameNotFound(String),
    #[error("size not found in output: {}", _0)]
    SizeNotFound(String),
    #[error("size in output could not be parsed as an integer: {}", _0)]
    SizeParse(String),
    #[error("md5sum not found in output: {}", _0)]
    Md5NotFound(String),
    #[error("md5 prefix (MD5Sum:) not found in md5sum: {}", _0)]
    Md5Prefix(String),
}

#[derive(Debug, Clone, Eq)]
pub struct Request {
    pub uri: String,
    pub name: String,
    pub size: u64,
    pub md5sum: String,
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
        let mut md5sum = words
            .next()
            .ok_or_else(|| RequestError::Md5NotFound(line.into()))?;

        if md5sum.starts_with("MD5Sum:") {
            md5sum = &md5sum[7..];
        } else {
            return Err(RequestError::Md5Prefix(md5sum.into()));
        }

        Ok(Request {
            uri: uri.into(),
            name: name.into(),
            size,
            md5sum: md5sum.into(),
        })
    }
}
