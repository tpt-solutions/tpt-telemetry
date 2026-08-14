//! `tpt-telemetry-core` — runtime parsing and streaming line/frame reader.
//!
//! Wires the compiled schema ([`tpt_telemetry_compiler`]) into a single
//! [`Parser`] dispatch API and provides a chunked, allocation-reusing
//! [`StreamReader`] for multi-gigabyte inputs. The match hot path is
//! zero-copy (field values are borrowed from the input line) and the
//! steady-state loop can run with no heap allocation when reusing a
//! [`MatchCtx`]; see the allocation-tracking test gated behind the
//! `alloc-counter` feature.

use std::io::{self, Read};

pub use tpt_telemetry_compiler::{
    CompiledFormat, CompiledSchema, Field, MatchCtx, RawMatch, Record, TypedField, Value,
};
pub use tpt_telemetry_schema::{load_file, parse, Schema};

/// Convenience result alias.
pub type Result<T> = std::result::Result<T, tpt_telemetry_compiler::CompileError>;

/// Runtime parser: compiles a [`Schema`] once, then dispatches each line to the
/// first format that matches.
pub struct Parser {
    schema: CompiledSchema,
}

impl Parser {
    /// Compile a parsed schema into a runtime parser.
    pub fn new(schema: Schema) -> Result<Self> {
        let schema = CompiledSchema::compile(&schema)?;
        Ok(Parser { schema })
    }

    /// Wrap an already-compiled schema.
    pub fn from_compiled(schema: CompiledSchema) -> Self {
        Parser { schema }
    }

    /// Parse a single line into a fully-typed, coerced (and redacted) [`Record`].
    pub fn parse_line<'a>(&'a self, line: &'a str) -> Option<Record<'a>> {
        self.schema.parse_line(line)
    }

    /// Zero-allocation match: reuse `ctx` across calls so the steady-state loop
    /// performs no heap allocation. Returns borrowed `(field, value)` pairs with
    /// no coercion/redaction applied.
    pub fn match_line<'a>(&'a self, line: &'a str, ctx: &mut MatchCtx) -> Option<RawMatch<'a>> {
        self.schema.match_line(line, ctx)
    }

    /// Zero-allocation match test: does any format match this line? Reuses `ctx`
    /// so the steady-state loop performs no heap allocation.
    pub fn matches<'a>(&'a self, line: &'a str, ctx: &mut MatchCtx) -> bool {
        self.schema.matches(line, ctx)
    }

    /// The compiled schema backing this parser.
    pub fn schema(&self) -> &CompiledSchema {
        &self.schema
    }
}

/// Default per-line size ceiling for [`StreamReader`] (64 MiB). Lines longer
/// than this without a delimiter are treated as malformed input: the reader
/// records an error and stops, mirroring the TCP framer's `max_frame_len`
/// ceiling that bounds unbounded buffer growth on the network path.
pub const DEFAULT_MAX_LINE_LEN: usize = 64 * 1024 * 1024;

/// A chunked, allocation-reusing streaming reader over any [`Read`] source.
///
/// Lines are framed (newline-delimited, with optional trailing `\r`) by scanning
/// a single reused buffer; the buffer is grown only when a line exceeds its
/// current capacity, so once warmed up the framing loop performs no allocation.
/// Growth is bounded by [`StreamReader::max_line_len`]: a line that exceeds the
/// ceiling without a delimiter is rejected rather than buffering without bound.
pub struct StreamReader<R> {
    reader: R,
    buf: Vec<u8>,
    start: usize,
    end: usize,
    done: bool,
    last_error: Option<io::Error>,
    max_line_len: usize,
}

impl<R: Read> StreamReader<R> {
    /// Create a reader with a 64 KiB initial buffer.
    pub fn new(reader: R) -> Self {
        Self::with_capacity(reader, 1 << 16)
    }

    /// Create a reader with a caller-chosen initial buffer capacity.
    pub fn with_capacity(reader: R, cap: usize) -> Self {
        StreamReader {
            reader,
            buf: vec![0u8; cap],
            start: 0,
            end: 0,
            done: false,
            last_error: None,
            max_line_len: DEFAULT_MAX_LINE_LEN,
        }
    }

    /// Override the per-line size ceiling. A line (the un-framed region of the
    /// buffer without a trailing newline) longer than `max_line_len` bytes is
    /// rejected: the reader records an [`io::ErrorKind::InvalidData`] error,
    /// stops, and returns `None`; callers can distinguish this from a clean EOF
    /// via [`StreamReader::last_error`].
    pub fn with_max_line_len(mut self, max_line_len: usize) -> Self {
        self.max_line_len = max_line_len.max(1);
        self
    }

    /// The configured per-line size ceiling.
    pub fn max_line_len(&self) -> usize {
        self.max_line_len
    }

    /// Return the next line as a borrowed slice into the internal buffer, or
    /// `None` at end of input.
    pub fn next_line(&mut self) -> Option<&[u8]> {
        loop {
            if let Some(i) = self.buf[self.start..self.end]
                .iter()
                .position(|&b| b == b'\n')
            {
                let line_end = self.start + i;
                let mut e = line_end;
                if e > self.start && self.buf[e - 1] == b'\r' {
                    e -= 1;
                }
                let line = &self.buf[self.start..e];
                self.start = line_end + 1;
                return Some(line);
            }

            if self.done {
                if self.start < self.end {
                    let s = self.start;
                    let e = self.end;
                    self.start = self.end;
                    return Some(&self.buf[s..e]);
                }
                return None;
            }

            // No newline yet: if the current un-framed region already exceeds the
            // line-length ceiling, reject it as malformed input rather than
            // growing the buffer without bound (mirrors the TCP framer's
            // `max_frame_len` backstop). Surface the condition via `last_error`
            // so callers can tell it apart from a clean EOF.
            if self.end - self.start > self.max_line_len {
                self.last_error = Some(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "line exceeds max_line_len without a delimiter",
                ));
                self.done = true;
                return None;
            }

            // No newline yet: compact the buffer and read more.
            if self.start > 0 {
                self.buf.copy_within(self.start..self.end, 0);
                self.end -= self.start;
                self.start = 0;
            }
            if self.end == self.buf.len() {
                self.buf.resize(self.buf.len() * 2 + 1, 0);
            }
            let n = {
                let tmp = &mut self.buf[self.end..];
                match self.reader.read(tmp) {
                    Ok(0) => {
                        self.done = true;
                        0
                    }
                    Ok(k) => k,
                    Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {
                        // Transient: retry the read on the next loop iteration.
                        continue;
                    }
                    Err(e) => {
                        // A genuine I/O error: record it, stop, and let the
                        // caller distinguish it from a clean EOF via `last_error`.
                        self.last_error = Some(e);
                        self.done = true;
                        0
                    }
                }
            };
            if n == 0 {
                continue;
            }
            self.end += n;
        }
    }

    /// Returns the last I/O error encountered while reading, if any.
    ///
    /// A `None` result means the stream ended cleanly (EOF) with no error, or no
    /// read has happened yet. This lets callers tell a genuine transport error
    /// apart from end-of-input.
    pub fn last_error(&self) -> Option<&io::Error> {
        self.last_error.as_ref()
    }
}

// ---------------------------------------------------------------------------
// Allocation-tracking harness (opt-in via `alloc-counter` feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-counter")]
mod alloc_counter {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A `GlobalAlloc` that delegates to the system allocator while counting the
    /// number of allocation/reallocation calls.
    pub struct CountingAlloc;

    static COUNT: AtomicU64 = AtomicU64::new(0);

    thread_local! {
        static TRACKING: Cell<bool> = const { Cell::new(false) };
    }

    #[global_allocator]
    static A: CountingAlloc = CountingAlloc;

    // SAFETY: we forward all operations to the system allocator unchanged.
    unsafe impl GlobalAlloc for CountingAlloc {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if TRACKING.with(|t| t.get()) {
                COUNT.fetch_add(1, Ordering::SeqCst);
            }
            System.alloc(layout)
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout)
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            if TRACKING.with(|t| t.get()) {
                COUNT.fetch_add(1, Ordering::SeqCst);
            }
            System.realloc(ptr, layout, new_size)
        }
    }

    /// Total allocation calls observed so far.
    pub fn alloc_count() -> u64 {
        COUNT.load(Ordering::SeqCst)
    }

    /// Reset the allocation counter and enable per-thread tracking.
    pub fn reset() {
        COUNT.store(0, Ordering::SeqCst);
        TRACKING.with(|t| t.set(true));
    }
}

#[cfg(feature = "alloc-counter")]
pub use alloc_counter::{alloc_count, reset as reset_alloc};

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const ASA: &str = r#"
        format CiscoASA {
          pattern: "%ASA-%{severity:int}-%{msg_id:int}: %{message:string}";
          coerce severity to enum { EMERGENCY, ALERT, CRITICAL, ERROR, WARNING, NOTICE, INFO, DEBUG };
        }
    "#;

    #[test]
    fn parser_dispatch_end_to_end() {
        let p = Parser::new(parse(ASA).unwrap()).unwrap();
        let rec = p
            .parse_line("%ASA-6-302013: Built inbound TCP connection")
            .unwrap();
        assert_eq!(rec.format, "CiscoASA");
        assert_eq!(
            rec.fields
                .iter()
                .find(|f| f.name == "severity")
                .unwrap()
                .value,
            Value::Enum(6)
        );
    }

    #[test]
    fn stream_reader_frames_lines() {
        let data = b"line one\nline two\r\nline three\n";
        let mut r = StreamReader::new(Cursor::new(&data[..]));
        let mut got = Vec::new();
        while let Some(line) = r.next_line() {
            got.push(String::from_utf8_lossy(line).into_owned());
        }
        assert_eq!(got, vec!["line one", "line two", "line three"]);
    }

    #[test]
    fn stream_reader_handles_unterminated_final_line() {
        let data = b"first\nsecond";
        let mut r = StreamReader::new(Cursor::new(&data[..]));
        assert_eq!(r.next_line().unwrap(), b"first");
        assert_eq!(r.next_line().unwrap(), b"second");
        assert!(r.next_line().is_none());
    }

    /// A reader that yields a partial line, then fails with an I/O error.
    struct FailingRead {
        given: &'static [u8],
        exhausted: bool,
    }

    impl Read for FailingRead {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if !self.exhausted {
                let n = self.given.len().min(buf.len());
                buf[..n].copy_from_slice(&self.given[..n]);
                self.exhausted = true;
                return Ok(n);
            }
            Err(io::Error::other("boom"))
        }
    }

    #[test]
    fn stream_reader_surfaces_io_error_via_last_error() {
        let mut r = StreamReader::new(FailingRead {
            given: b"partial",
            exhausted: false,
        });
        // The partial line is returned once.
        assert_eq!(r.next_line().unwrap(), b"partial");
        // Then the read error surfaces as a clean None with the error recorded.
        assert!(r.next_line().is_none());
        assert!(r.last_error().is_some());
        assert_eq!(r.last_error().unwrap().to_string(), "boom");
    }

    #[test]
    fn stream_reader_rejects_overlong_line() {
        // A line with no delimiter that exceeds the ceiling is rejected.
        let data = vec![b'a'; 1024];
        let mut r = StreamReader::new(Cursor::new(&data[..])).with_max_line_len(64);
        // The whole buffer is one undelimited line of 1024 bytes > 64.
        let got = r.next_line();
        assert!(got.is_none());
        let err = r.last_error().expect("error should be recorded");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(r.max_line_len(), 64);
    }

    #[test]
    fn stream_reader_respects_max_line_len_across_chunks() {
        // The ceiling is checked on the un-framed region regardless of how the
        // bytes arrive; a delimiter before the limit still yields the line.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"short\n");
        buf.extend_from_slice(b"loooooong-no-newline-yet");
        let mut r = StreamReader::with_capacity(Cursor::new(&buf[..]), 4).with_max_line_len(8);
        assert_eq!(r.next_line().unwrap(), b"short");
        assert!(r.next_line().is_none());
        assert_eq!(
            r.last_error().map(|e| e.kind()),
            Some(io::ErrorKind::InvalidData)
        );
    }

    #[cfg(feature = "alloc-counter")]
    #[test]
    fn zero_alloc_steady_state_match_loop() {
        let p = Parser::new(parse(ASA).unwrap()).unwrap();
        let lines = "%ASA-6-302013: Built inbound TCP connection\n\
                     %ASA-3-106001: connection denied\n\
                     %ASA-4-106023: deny tcp src inside\n";
        let mut r = StreamReader::new(Cursor::new(lines));
        let mut ctx = MatchCtx::new(8);

        // Warm up: pull one line + match so internal buffers reach capacity.
        let first = r.next_line().unwrap();
        let _ = p.matches(std::str::from_utf8(first).unwrap(), &mut ctx);
        reset_alloc();

        let mut matched = 0;
        while let Some(line) = r.next_line() {
            let s = std::str::from_utf8(line).unwrap();
            if p.matches(s, &mut ctx) {
                matched += 1;
            }
        }
        assert!(matched >= 2);
        // Steady-state framing + matching performs no new heap allocation.
        assert_eq!(alloc_count(), 0, "unexpected allocations in hot loop");
    }
}
