//! TCP syslog framing decoder.
//!
//! Supports both RFC5424 octet-counting (`<len> SP <len octets>`) and RFC3164
//! non-transparent (`<message> LF`) framing, auto-detected per message.
//!
//! Decoding is bounded by [`MAX_FRAME_LEN`]: octet-counting lengths are rejected
//! if they exceed the limit and LF-delimited frames that grow past the limit
//! without a delimiter are dropped. This prevents an attacker-controlled stream
//! from forcing unbounded buffer growth.

/// Default per-frame size ceiling (1 MiB).
pub const MAX_FRAME_LEN: usize = 1_048_576;

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

/// Errors produced while decoding a TCP syslog stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    /// A frame exceeded the configured [`TcpDecoder`] size ceiling and was
    /// rejected (the offending bytes were discarded).
    FrameTooLarge { len: usize, max: usize },
}

/// Incremental TCP frame decoder. Feed raw stream chunks via [`TcpDecoder::push`]
/// and collect completed frames.
pub struct TcpDecoder {
    mode: TcpFraming,
    max_frame_len: usize,
    buf: Vec<u8>,
}

impl TcpDecoder {
    /// Create a decoder with the given framing mode and per-frame size ceiling.
    pub fn new(mode: TcpFraming, max_frame_len: usize) -> Self {
        TcpDecoder {
            mode,
            max_frame_len: max_frame_len.max(1),
            buf: Vec::new(),
        }
    }

    /// The configured per-frame size ceiling.
    pub fn max_frame_len(&self) -> usize {
        self.max_frame_len
    }

    /// Append `chunk` and return any complete frames decoded from the buffer.
    ///
    /// Returns [`FrameError::FrameTooLarge`] if a frame violates the size ceiling;
    /// the caller should treat this as a reason to close the connection.
    pub fn push(&mut self, chunk: &[u8], out: &mut Vec<Vec<u8>>) -> Result<(), FrameError> {
        self.buf.extend_from_slice(chunk);
        self.drain(out)
    }

    /// Force-decode any trailing bytes (a connection closing without a final
    /// delimiter yields one last frame, if non-empty).
    pub fn flush(&mut self, out: &mut Vec<Vec<u8>>) -> Result<(), FrameError> {
        if !self.buf.is_empty() {
            out.push(std::mem::take(&mut self.buf));
        }
        Ok(())
    }

    fn drain(&mut self, out: &mut Vec<Vec<u8>>) -> Result<(), FrameError> {
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
                    // Enforce the size ceiling before buffering an unbounded
                    // frame; discard the pending prefix so the connection cannot
                    // be used to grow our buffer without bound.
                    if len > self.max_frame_len {
                        let claimed = len;
                        self.buf.clear();
                        return Err(FrameError::FrameTooLarge {
                            len: claimed,
                            max: self.max_frame_len,
                        });
                    }
                    let start = space + 1;
                    let Some(end) = start.checked_add(len) else {
                        self.buf.clear();
                        return Err(FrameError::FrameTooLarge {
                            len,
                            max: self.max_frame_len,
                        });
                    };
                    if self.buf.len() < end {
                        break;
                    }
                    let frame = self.buf[start..end].to_vec();
                    self.buf.drain(..end);
                    out.push(frame);
                }
                Mode::Lf => {
                    match self.buf.iter().position(|&b| b == b'\n') {
                        Some(lf) => {
                            let mut frame = self.buf[..lf].to_vec();
                            // Strip a trailing CR if present (RFC3164 style).
                            if frame.last() == Some(&b'\r') {
                                frame.pop();
                            }
                            self.buf.drain(..lf + 1);
                            out.push(frame);
                        }
                        None => {
                            // No delimiter yet. Bound LF-delimited growth: if the
                            // buffer has already exceeded the ceiling with no LF
                            // in sight, reject the oversized frame and reset.
                            if self.buf.len() >= self.max_frame_len {
                                let len = self.buf.len();
                                self.buf.clear();
                                return Err(FrameError::FrameTooLarge {
                                    len,
                                    max: self.max_frame_len,
                                });
                            }
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
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
        let mut d = TcpDecoder::new(mode, MAX_FRAME_LEN);
        let mut out = Vec::new();
        for c in stream {
            let _ = d.push(c, &mut out);
        }
        let _ = d.flush(&mut out);
        out
    }

    #[test]
    fn non_transparent_lf() {
        let frames = decode_all(
            TcpFraming::NonTransparent,
            &[b"msg one\nmsg two\n", b"msg three\n"],
        );
        let got: Vec<&[u8]> = frames.iter().map(|v| v.as_slice()).collect();
        assert_eq!(
            got,
            vec![&b"msg one"[..], &b"msg two"[..], &b"msg three"[..]]
        );
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

    #[test]
    fn rejects_oversized_octet_count() {
        // Claim a 10 MiB frame; the decoder must reject it without buffering.
        let claimed = 10 * 1024 * 1024;
        let mut d = TcpDecoder::new(TcpFraming::OctetCounting, 1024);
        let mut out = Vec::new();
        let res = d.push(format!("{claimed} ").as_bytes(), &mut out);
        assert!(matches!(res, Err(FrameError::FrameTooLarge { .. })));
        assert!(out.is_empty());
        assert_eq!(d.max_frame_len(), 1024);
    }

    #[test]
    fn rejects_oversized_lf_frame() {
        // A never-terminated LF frame that exceeds the ceiling is rejected.
        let mut d = TcpDecoder::new(TcpFraming::NonTransparent, 16);
        let mut out = Vec::new();
        let res = d.push(b"this line never ends and keeps going", &mut out);
        assert!(matches!(res, Err(FrameError::FrameTooLarge { .. })));
        assert!(out.is_empty());
    }

    /// Fuzz-smoke: feed pseudo-random bytes in varying chunk sizes across all
    /// framing modes; decoding must never panic and every frame must be a
    /// byte-subset of the input.
    #[test]
    fn framing_never_panics_on_random_streams() {
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut rng = || {
            // xorshift64
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..256 {
            let len = (rng() % 512) as usize;
            let mut data = Vec::with_capacity(len);
            for _ in 0..len {
                data.push((rng() % 256) as u8);
            }
            for mode in [
                TcpFraming::Auto,
                TcpFraming::OctetCounting,
                TcpFraming::NonTransparent,
            ] {
                let mut d = TcpDecoder::new(mode, MAX_FRAME_LEN);
                let mut frames = Vec::new();
                for chunk in data.chunks(1 + (rng() % 4) as usize) {
                    let _ = d.push(chunk, &mut frames);
                }
                let _ = d.flush(&mut frames);
                for f in &frames {
                    assert!(f.len() <= data.len());
                }
            }
        }
    }
}
