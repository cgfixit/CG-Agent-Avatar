use std::io::{self, BufRead, BufReader, Read, Take};
use std::time::Instant;

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

/// Why an event stream gave no further event.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EventError {
    /// The stream ended, broke, or ran past its deadline.
    Interrupted,
    TooLarge,
    /// A line was not UTF-8.
    Malformed,
}

/// A server-sent event stream, read as it arrives. Both streamed chat
/// backends read through this, so the bound lives in one place.
///
/// Fail closed: the reader stops at `max` bytes in total, so no single line
/// and no sum of lines can exceed the cap every other response has, and a
/// stream that ends, breaks, or is still going at `deadline` is an error.
pub(crate) struct EventStream<R: Read> {
    reader: BufReader<Take<R>>,
    max: u64,
    read: u64,
    deadline: Instant,
    line: Vec<u8>,
    data: String,
}

impl<R: Read> EventStream<R> {
    pub(crate) fn new(reader: R, max: u64, deadline: Instant) -> Self {
        Self {
            reader: BufReader::new(reader.take(max + 1)),
            max,
            read: 0,
            deadline,
            line: Vec::new(),
            data: String::new(),
        }
    }

    /// The next event's `data`, its lines joined with `\n`. Comments
    /// (keep-alives), the other SSE fields, and events with no data carry
    /// nothing either backend uses and are skipped.
    pub(crate) fn next_data(&mut self) -> Result<&str, EventError> {
        self.data.clear();
        loop {
            self.line.clear();
            let n = self
                .reader
                .read_until(b'\n', &mut self.line)
                .map_err(|_| EventError::Interrupted)?;
            self.read += n as u64;
            if self.read > self.max {
                return Err(EventError::TooLarge);
            }
            if n == 0 || Instant::now() >= self.deadline {
                return Err(EventError::Interrupted);
            }
            let field = std::str::from_utf8(&self.line)
                .map_err(|_| EventError::Malformed)?
                .trim_end_matches(['\n', '\r']);
            if let Some(value) = field.strip_prefix("data:") {
                if !self.data.is_empty() {
                    self.data.push('\n');
                }
                self.data.push_str(value.strip_prefix(' ').unwrap_or(value));
            } else if field.is_empty() && !self.data.is_empty() {
                // A blank line ends an event.
                return Ok(&self.data);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn bounds_chunked_readers_before_parsing() {
        assert_eq!(read_bounded(&b"1234"[..], 4).unwrap(), b"1234");
        assert!(matches!(
            read_bounded(&b"12345"[..], 4),
            Err(ReadError::TooLarge)
        ));
    }

    fn soon() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    #[test]
    fn events_are_split_on_blank_lines_and_everything_else_is_skipped() {
        let body = b": keep-alive\n\nevent: note\ndata: one\n\ndata:two\r\ndata: three\r\n\r\nid: 7\n\ndata:\n\ndata: four\n\n";
        let mut events = EventStream::new(&body[..], MAX_BODY, soon());
        assert_eq!(events.next_data(), Ok("one"));
        assert_eq!(events.next_data(), Ok("two\nthree"));
        assert_eq!(events.next_data(), Ok("four"));
        assert_eq!(events.next_data(), Err(EventError::Interrupted));
    }

    #[test]
    fn an_event_cut_short_is_never_returned() {
        for body in [&b"data: half"[..], b"data: whole\n", b""] {
            let mut events = EventStream::new(body, MAX_BODY, soon());
            assert_eq!(events.next_data(), Err(EventError::Interrupted));
        }
        let mut events = EventStream::new(&b"data: \xff\n\n"[..], MAX_BODY, soon());
        assert_eq!(events.next_data(), Err(EventError::Malformed));
    }

    #[test]
    fn the_cap_and_the_deadline_cover_the_whole_stream() {
        // Two events of 9 bytes each: the first fits a 12-byte cap, and the
        // second is refused even though it is small on its own.
        let body = b"data: a\n\ndata: b\n\n";
        let mut events = EventStream::new(&body[..], 12, soon());
        assert_eq!(events.next_data(), Ok("a"));
        assert_eq!(events.next_data(), Err(EventError::TooLarge));
        let late = Instant::now() - Duration::from_secs(1);
        let mut events = EventStream::new(&body[..], MAX_BODY, late);
        assert_eq!(events.next_data(), Err(EventError::Interrupted));
    }
}
