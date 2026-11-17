// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The zip package. Parts we don't rewrite are copied as raw compressed bytes,
//! so pictures, settings, thumbnails, `manifest.rdf` and any unknown parts are
//! preserved exactly, in their original order.

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::Error;

pub struct Package {
    bytes: Vec<u8>,
    names: Vec<String>,
}

impl Package {
    pub fn open(bytes: Vec<u8>) -> Result<Self, Error> {
        let mut archive = ZipArchive::new(Cursor::new(&bytes[..]))?;
        let mut names = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            names.push(archive.by_index_raw(i)?.name()?.into_owned());
        }
        drop(archive);
        Ok(Package { bytes, names })
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn read(&self, name: &str) -> Result<Option<Vec<u8>>, Error> {
        let mut archive = ZipArchive::new(Cursor::new(&self.bytes[..]))?;
        let mut f = match archive.by_name(name) {
            Ok(f) => f,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        f.read_to_end(&mut out)?;
        Ok(Some(out))
    }

    pub fn read_string(&self, name: &str) -> Result<Option<String>, Error> {
        match self.read(name)? {
            Some(b) => Ok(Some(
                String::from_utf8(b).map_err(|_| Error::Xml(format!("{name} is not UTF-8")))?,
            )),
            None => Ok(None),
        }
    }

    /// Write the package back, replacing the named parts and copying every other
    /// entry raw. `mimetype` stays first and stored, as ODF requires.
    pub fn write(&self, replace: &HashMap<&str, Vec<u8>>) -> Result<Vec<u8>, Error> {
        let mut archive = ZipArchive::new(Cursor::new(&self.bytes[..]))?;
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        for (i, name) in self.names.iter().enumerate() {
            match replace.get(name.as_str()) {
                Some(data) => {
                    let method = archive.by_index_raw(i)?.compression();
                    let method = if name == "mimetype" {
                        CompressionMethod::Stored
                    } else {
                        method
                    };
                    w.start_file(
                        name.as_str(),
                        SimpleFileOptions::default().compression_method(method),
                    )?;
                    w.write_all(data)?;
                }
                None => {
                    let f = archive.by_index_raw(i)?;
                    w.raw_copy_file(f)?;
                }
            }
        }
        Ok(w.finish()?.into_inner())
    }
}
