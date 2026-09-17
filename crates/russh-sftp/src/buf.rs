use bytes::Buf;

use crate::error::Error;

pub trait TryBuf: Buf {
    fn try_get_bytes(&mut self) -> Result<Vec<u8>, Error>;
    fn try_get_string(&mut self) -> Result<String, Error>;
}

impl<T: Buf> TryBuf for T {
    fn try_get_bytes(&mut self) -> Result<Vec<u8>, Error> {
        let len = self.try_get_u32()? as usize;
        if self.remaining() < len {
            return Err(Error::BadMessage("no remaining for vec".to_owned()));
        }

        Ok(self.copy_to_bytes(len).to_vec())
    }

    fn try_get_string(&mut self) -> Result<String, Error> {
        let bytes = self.try_get_bytes()?;
        Ok(String::from_utf8(bytes)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned()))
    }
}
