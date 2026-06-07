use bytes::Buf;

use crate::error::Error;

pub trait TryBuf: Buf {
    fn try_get_bytes(&mut self) -> Result<Vec<u8>, Error>;
    fn try_get_string(&mut self) -> Result<String, Error>;
}

impl<T: Buf> TryBuf for T {
    fn try_get_bytes(&mut self) -> Result<Vec<u8>, Error> {
        let len = self
            .try_get_u32()
            .map_err(|e| Error::UnexpectedBehavior(e.to_string()))? as usize;
        if self.remaining() < len {
            return Err(Error::BadMessage("no remaining for vec".to_owned()));
        }

        Ok(self.copy_to_bytes(len).to_vec())
    }

    fn try_get_string(&mut self) -> Result<String, Error> {
        let bytes = self.try_get_bytes()?;
        //String::from_utf8(bytes).map_err(|_| Error::BadMessage("unable to parse str".to_owned()))
        Ok(String::from_utf8_lossy(&bytes).into())
    }
}
