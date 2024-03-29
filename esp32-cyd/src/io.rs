use rustzx_core::{
    error::IoError,
    host::{ SeekableAsset, LoadableAsset, SeekFrom }
};

#[derive(Debug)]
pub enum FileAssetError {
    ReadError,
    WriteError,
}

pub struct FileAsset {
    data: &'static [u8],
    position: usize,
}

impl FileAsset {
    pub fn new(data: &'static [u8]) -> Self {
        FileAsset {data, position: 0 }
    }

    fn convert_error(error : FileAssetError) -> IoError {
        match error {
            FileAssetError::ReadError => IoError::HostAssetImplFailed,
            FileAssetError::WriteError => IoError::SeekBeforeStart,
        }
    }
}

impl SeekableAsset for FileAsset {
    fn seek(&mut self, _pos : SeekFrom) -> Result<usize, IoError> {
        todo!();
    }
}

impl LoadableAsset for FileAsset {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
        let available = self.data.len();
        let to_read = available.min(buf.len());

        if to_read == 0 {
            return Err(FileAsset::convert_error(FileAssetError::ReadError));
        }

        buf[..to_read] = copy_from_slice(&self.data[self.position..self.position+to_read]);
        self.position += to_read;

        Ok(to_read)
    }
}