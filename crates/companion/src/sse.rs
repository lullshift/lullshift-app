//! A small, strict server-sent-events parser (ADR-020, streaming).
//!
//! Follows the WHATWG "Server-sent events" parsing rules that matter here: lines end in CRLF, LF
//! or CR (also when the pair is split across two network chunks); a blank line dispatches the
//! event; lines starting with `:` are comments; `data` lines are joined with `\n`; one space
//! after the colon is dropped; a leading byte-order mark is ignored; an event that is not
//! terminated by a blank line when the stream ends is discarded.
//!
//! Hardening: a line or an event longer than `max_event_bytes` is an error (no unbounded
//! buffering), and invalid UTF-8 is an error rather than being replaced.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SseError {
    EventTooLarge,
    InvalidUtf8,
}

#[derive(Debug)]
pub struct SseParser {
    max_event_bytes: usize,
    line: Vec<u8>,
    data: String,
    event: Option<String>,
    has_data: bool,
    /// The previous byte was a CR, so a directly following LF belongs to it.
    skip_lf: bool,
    at_start: bool,
}

impl SseParser {
    pub fn new(max_event_bytes: usize) -> Self {
        Self {
            max_event_bytes,
            line: Vec::new(),
            data: String::new(),
            event: None,
            has_data: false,
            skip_lf: false,
            at_start: true,
        }
    }

    /// Feeds bytes and returns the events completed by them.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, SseError> {
        let mut out = Vec::new();
        for &b in bytes {
            if self.skip_lf {
                self.skip_lf = false;
                if b == b'\n' {
                    continue;
                }
            }
            match b {
                b'\r' => {
                    self.skip_lf = true;
                    self.end_line(&mut out)?;
                }
                b'\n' => self.end_line(&mut out)?,
                _ => {
                    self.line.push(b);
                    if self.line.len() > self.max_event_bytes {
                        return Err(SseError::EventTooLarge);
                    }
                }
            }
        }
        Ok(out)
    }

    fn end_line(&mut self, out: &mut Vec<SseEvent>) -> Result<(), SseError> {
        let raw = std::mem::take(&mut self.line);
        let mut line = std::str::from_utf8(&raw).map_err(|_| SseError::InvalidUtf8)?;
        if self.at_start {
            self.at_start = false;
            line = line.strip_prefix('\u{feff}').unwrap_or(line);
        }
        if line.is_empty() {
            if self.has_data {
                let mut data = std::mem::take(&mut self.data);
                if data.ends_with('\n') {
                    data.pop();
                }
                out.push(SseEvent {
                    event: self.event.take(),
                    data,
                });
            } else {
                self.event = None;
            }
            self.has_data = false;
            return Ok(());
        }
        if line.starts_with(':') {
            return Ok(());
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "data" => {
                if self.data.len() + value.len() + 1 > self.max_event_bytes {
                    return Err(SseError::EventTooLarge);
                }
                self.data.push_str(value);
                self.data.push('\n');
                self.has_data = true;
            }
            "event" => self.event = Some(value.chars().take(64).collect()),
            _ => {} // id, retry and unknown fields are not used.
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(chunks: &[&[u8]]) -> Result<Vec<SseEvent>, SseError> {
        let mut p = SseParser::new(1024);
        let mut out = Vec::new();
        for c in chunks {
            out.extend(p.feed(c)?);
        }
        Ok(out)
    }

    fn ev(event: Option<&str>, data: &str) -> SseEvent {
        SseEvent {
            event: event.map(str::to_owned),
            data: data.to_owned(),
        }
    }

    #[test]
    fn simple_events_with_all_line_endings() {
        let e = all(&[b"data: a\n\ndata: b\r\n\r\nevent: x\rdata: c\r\r"]).unwrap();
        assert_eq!(e, vec![ev(None, "a"), ev(None, "b"), ev(Some("x"), "c")]);
    }

    #[test]
    fn splits_anywhere_give_the_same_events() {
        let whole = b"event: m\r\ndata: one\r\ndata: two\r\n\r\n: c\r\ndata: three\n\n";
        let expect = all(&[whole]).unwrap();
        assert_eq!(expect, vec![ev(Some("m"), "one\ntwo"), ev(None, "three")]);
        for cut in 0..whole.len() {
            let (a, b) = whole.split_at(cut);
            assert_eq!(all(&[a, b]).unwrap(), expect, "cut at {cut}");
        }
        // Byte by byte, including a CRLF split between chunks.
        let singles: Vec<&[u8]> = whole.chunks(1).collect();
        assert_eq!(all(&singles).unwrap(), expect);
    }

    #[test]
    fn multibyte_characters_survive_chunk_boundaries() {
        let text = "data: h\u{e9}llo \u{1F600}\n\n".as_bytes();
        let singles: Vec<&[u8]> = text.chunks(1).collect();
        assert_eq!(
            all(&singles).unwrap(),
            vec![ev(None, "h\u{e9}llo \u{1F600}")]
        );
    }

    #[test]
    fn comments_unknown_fields_and_bom_are_ignored() {
        let e = all(&[b"\xEF\xBB\xBF: hi\nid: 7\nretry: 5\nfoo: bar\ndata:x\n\n"]).unwrap();
        assert_eq!(e, vec![ev(None, "x")]);
    }

    #[test]
    fn blank_data_and_missing_colon() {
        let e = all(&[b"data\n\ndata:\n\n"]).unwrap();
        assert_eq!(e, vec![ev(None, ""), ev(None, "")]);
        // A blank line with no data dispatches nothing and drops a stray event name.
        let e = all(&[b"event: lonely\n\ndata: z\n\n"]).unwrap();
        assert_eq!(e, vec![ev(None, "z")]);
    }

    #[test]
    fn an_unterminated_event_is_not_dispatched() {
        assert!(all(&[b"data: partial\n"]).unwrap().is_empty());
    }

    #[test]
    fn oversized_lines_and_events_are_errors() {
        let mut p = SseParser::new(16);
        assert_eq!(
            p.feed(&[b'a'; 17]).unwrap_err(),
            SseError::EventTooLarge,
            "one long line"
        );
        let mut p = SseParser::new(16);
        let mut r = Ok(Vec::new());
        for _ in 0..5 {
            r = p.feed(b"data: abcdef\n");
            if r.is_err() {
                break;
            }
        }
        assert_eq!(r.unwrap_err(), SseError::EventTooLarge, "many data lines");
    }

    #[test]
    fn invalid_utf8_is_an_error() {
        assert_eq!(
            all(&[b"data: \xff\n\n"]).unwrap_err(),
            SseError::InvalidUtf8
        );
    }
}
