use std::io::{self, Read};

pub(crate) const MAX_BODY: u64 = 1_048_576;

#[derive(Debug)]
pub(crate) enum ReadError {
    Io,
    TooLarge,
}

pub(crate) fn read_bounded(reader: impl Read, max: u64) -> Result<Vec<u8>, ReadError> {
    let mut bytes = Vec::new();
    reader
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|_: io::Error| ReadError::Io)?;
    if bytes.len() as u64 > max {
        Err(ReadError::TooLarge)
    } else {
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_chunked_readers_before_parsing() {
        assert_eq!(read_bounded(&b"1234"[..], 4).unwrap(), b"1234");
        assert!(matches!(
            read_bounded(&b"12345"[..], 4),
            Err(ReadError::TooLarge)
        ));
    }
}
