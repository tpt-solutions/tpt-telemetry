//! TCP syslog framing decoder.
//!
//! Supports both RFC5424 octet-counting (`<len> SP <len octets>`) and RFC3164
//! non-transparent (`<message> LF`) framing, auto-detected per message.

/// TCP syslog framing mode. `Auto` tries octet-counting when the buffer begins
/// with `<digits> SP`, otherwise falls back to LF-delimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpFraming {
    Auto,
    OctetCounting,
    NonTransparent,
}

#[derive(Debug)]
enum Mode {
    Octet,
    Lf,
}

/// Incremental TCP frame decoder. Feed raw stream chunks via [`TcpDecoder::push`]
/// and collect completed frames.
pub struct TcpDecoder {
    mode: TcpFraming,
    buf: Vec<u8>,
}

impl TcpDecoder {
    pub fn new(mode: TcpFraming) -> Self {
        TcpDecoder {
            mode,
            buf: Vec::new(),
        }
    }

    /// Append `chunk` and return any complete frames decoded from the buffer.
    pub fn push(&mut self, chunk: &[u8], out: &mut Vec<Vec<u8>>) {
        self.buf.extend_from_slice(chunk);
        self.drain(out);
    }

    /// Force-decode any trailing bytes (a connection closing without a final
    /// delimiter yields one last frame, if non-empty).
    pub fn flush(&mut self, out: &mut Vec<Vec<u8>>) {
        if !self.buf.is_empty() {
            out.push(std::mem::take(&mut self.buf));
        }
    }

    fn drain(&mut self, out: &mut Vec<Vec<u8>>) {
        loop {
            let mode = match self.mode {
                TcpFraming::OctetCounting => Mode::Octet,
                TcpFraming::NonTransparent => Mode::Lf,
                TcpFraming::Auto => {
                    if let Some(m) = self.detect() {
                        m
                    } else {
                        // Need more data to decide.
                        break;
                    }
                }
            };
            match mode {
                Mode::Octet => {
                    // Require `<digits> SP` prefix.
                    let Some(space) = self.buf.iter().position(|&b| b == b' ') else {
                        break;
                    };
                    let Ok(len) = std::str::from_utf8(&self.buf[..space])
                        .map_err(|_| ())
                        .and_then(|s| s.parse::<usize>().map_err(|_| ()))
                    else {
                        // Not a valid length prefix; treat as LF framing.
                        if self.mode == TcpFraming::Auto {
                            self.mode = TcpFraming::NonTransparent;
                            continue;
                        }
                        break;
                    };
                    let start = space + 1;
                    let end = start + len;
                    if self.buf.len() < end {
                        break;
                    }
                    let frame = self.buf[start..end].to_vec();
                    self.buf.drain(..end);
                    out.push(frame);
                }
                Mode::Lf => {
                    let Some(lf) = self.buf.iter().position(|&b| b == b'\n') else {
                        break;
                    };
                    let mut frame = self.buf[..lf].to_vec();
                    // Strip a trailing CR if present (RFC3164 style).
                    if frame.last() == Some(&b'\r') {
                        frame.pop();
                    }
                    self.buf.drain(..lf + 1);
                    out.push(frame);
                }
            }
        }
    }

    fn detect(&self) -> Option<Mode> {
        // Octet-counting begins with ASCII digits followed by a space.
        let space = self.buf.iter().position(|&b| b == b' ')?;
        if space == 0 {
            return Some(Mode::Lf);
        }
        let is_digits = self.buf[..space].iter().all(|b| b.is_ascii_digit());
        if is_digits {
            Some(Mode::Octet)
        } else {
            Some(Mode::Lf)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_all(mode: TcpFraming, stream: &[&[u8]]) -> Vec<Vec<u8>> {
        let mut d = TcpDecoder::new(mode);
        let mut out = Vec::new();
        for c in stream {
            d.push(c, &mut out);
        }
        d.flush(&mut out);
        out
    }

    #[test]
    fn non_transparent_lf() {
        let frames = decode_all(TcpFraming::NonTransparent, &[b"msg one\nmsg two\n", b"msg three\n"]);
        let got: Vec<&[u8]> = frames.iter().map(|v| v.as_slice()).collect();
        assert_eq!(got, vec![&b"msg one"[..], &b"msg two"[..], &b"msg three"[..]]);
    }

    #[test]
    fn octet_counting() {
        // "11 hello world" + "5 short"
        let stream = b"11 hello world5 short";
        let frames = decode_all(TcpFraming::OctetCounting, &[stream]);
        let got: Vec<&[u8]> = frames.iter().map(|v| v.as_slice()).collect();
        assert_eq!(got, vec![&b"hello world"[..], &b"short"[..]]);
    }

    #[test]
    fn auto_detects_octet() {
        let frames = decode_all(TcpFraming::Auto, &[b"3 abc"]);
        assert_eq!(frames, vec![&b"abc"[..]]);
    }

    #[test]
    fn auto_detects_lf() {
        let frames = decode_all(TcpFraming::Auto, &[b"no digits here\n"]);
        assert_eq!(frames, vec![&b"no digits here"[..]]);
    }
}
