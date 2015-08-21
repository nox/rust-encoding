// This is a part of rust-encoding.
// Copyright (c) 2013-2015, Kang Seonghoon.
// See README.md and LICENSE.txt for details.

//! 7-bit ASCII encoding.

use std::convert::Into;
use types::*;

#[cfg(all(feature = "enable-simd",
          target_feature = "sse2",
          not(all(target_os = "macos", debug_assertions))))]
use simd::u8x16;
#[cfg(all(feature = "enable-simd",
          target_feature = "sse2",
          not(all(target_os = "macos", debug_assertions))))]
use simd::x86::sse2::Movemask;
#[cfg(all(feature = "enable-simd",
          target_feature = "sse2",
          not(all(target_os = "macos", debug_assertions))))]
const CHUNK_SIZE: usize = 16;

/**
 * ASCII, also known as ISO/IEC 646:US.
 *
 * It is both a basis and a lowest common denominator of many other encodings
 * including UTF-8, which Rust internally assumes.
 */
#[derive(Clone, Copy)]
pub struct ASCIIEncoding;

impl Encoding for ASCIIEncoding {
    fn name(&self) -> &'static str { "ascii" }
    fn raw_encoder(&self) -> Box<RawEncoder> { ASCIIEncoder::new() }
    fn raw_decoder(&self) -> Box<RawDecoder> { ASCIIDecoder::new() }
}

/// An encoder for ASCII.
#[derive(Clone, Copy)]
pub struct ASCIIEncoder;

impl ASCIIEncoder {
    pub fn new() -> Box<RawEncoder> { Box::new(ASCIIEncoder) }
}

impl RawEncoder for ASCIIEncoder {
    fn from_self(&self) -> Box<RawEncoder> { ASCIIEncoder::new() }
    fn is_ascii_compatible(&self) -> bool { true }

    fn raw_feed(&mut self, input: &str, output: &mut ByteWriter) -> (usize, Option<CodecError>) {
        output.writer_hint(input.len());
        match raw_feed(input.as_bytes(), output) {
            first_error if first_error < input.len() => {
                let len = input[first_error..].chars().next().unwrap().len_utf8();
                (first_error, Some(CodecError {
                    upto: (first_error + len) as isize,
                    cause: "unrepresentable character".into()
                }))
            },
            len => {
                (len, None)
            }
        }
    }

    fn raw_finish(&mut self, _output: &mut ByteWriter) -> Option<CodecError> {
        None
    }
}

/// A decoder for ASCII.
#[derive(Clone, Copy)]
pub struct ASCIIDecoder;

impl ASCIIDecoder {
    pub fn new() -> Box<RawDecoder> { Box::new(ASCIIDecoder) }
}

impl RawDecoder for ASCIIDecoder {
    fn from_self(&self) -> Box<RawDecoder> { ASCIIDecoder::new() }
    fn is_ascii_compatible(&self) -> bool { true }

    fn raw_feed(&mut self, input: &[u8], output: &mut StringWriter) -> (usize, Option<CodecError>) {
        output.writer_hint(input.len());
        match raw_feed(input, unsafe { output.as_byte_writer() }) {
            first_error if first_error < input.len() => {
                (first_error, Some(CodecError {
                    upto: first_error as isize + 1,
                    cause: "invalid sequence".into()
                }))
            },
            len => {
                (len, None)
            }
        }
    }

    fn raw_finish(&mut self, _output: &mut StringWriter) -> Option<CodecError> {
        None
    }
}

#[cfg(any(not(feature = "enable-simd"),
          not(target_feature = "sse2"),
          all(target_os = "macos", debug_assertions)))]
#[inline]
fn raw_feed(input: &[u8], output: &mut ByteWriter) -> usize {
    slow_raw_feed(input, output, 0)
}

#[cfg(all(feature = "enable-simd",
          target_feature = "sse2",
          not(all(target_os = "macos", debug_assertions))))]
#[inline]
fn raw_feed(input: &[u8], output: &mut ByteWriter) -> usize {
    let total = input.len() / CHUNK_SIZE * CHUNK_SIZE;
    let mut sofar = 0;
    while sofar != total {
        let v = u8x16::load(input, sofar);
        let mask = v.movemask();
        if mask == 0 {
            sofar += CHUNK_SIZE;
        } else {
            sofar += (mask as u16).trailing_zeros() as usize;
            output.write_bytes(&input[..sofar]);
            return sofar;
        }
    }
    slow_raw_feed(input, output, sofar)
}

#[inline]
fn slow_raw_feed(input: &[u8], output: &mut ByteWriter, sofar: usize)
               -> usize {
    let sofar = match input[sofar..].iter().position(|&ch| ch >= 0x80) {
        Some(first_error) => sofar + first_error,
        None => input.len(),
    };
    output.write_bytes(&input[..sofar]);
    sofar
}

#[cfg(test)]
mod tests {
    extern crate test;
    use super::ASCIIEncoding;
    use testutils;
    use types::*;

    #[test]
    fn test_encoder() {
        let mut e = ASCIIEncoding.raw_encoder();
        assert_feed_ok!(e, "A", "", [0x41]);
        assert_feed_ok!(e, "BC", "", [0x42, 0x43]);
        assert_feed_ok!(e, "", "", []);
        assert_feed_err!(e, "", "\u{a0}", "", []);
        assert_feed_err!(e, "X", "\u{a0}", "Z", [0x58]);
        assert_finish_ok!(e, []);
    }

    #[test]
    fn test_decoder() {
        let mut d = ASCIIEncoding.raw_decoder();
        assert_feed_ok!(d, [0x41], [], "A");
        assert_feed_ok!(d, [0x42, 0x43], [], "BC");
        assert_feed_ok!(d, [], [], "");
        assert_feed_err!(d, [], [0xa0], [], "");
        assert_feed_err!(d, [0x58], [0xa0], [0x5a], "X");
        assert_finish_ok!(d, "");
    }

    #[bench]
    fn bench_encode(bencher: &mut test::Bencher) {
        let s = testutils::ASCII_TEXT;
        bencher.bytes = s.len() as u64;
        bencher.iter(|| test::black_box({
            ASCIIEncoding.encode(s, EncoderTrap::Strict)
        }))
    }

    #[bench]
    fn bench_decode(bencher: &mut test::Bencher) {
        let s = testutils::ASCII_TEXT.as_bytes();
        bencher.bytes = s.len() as u64;
        bencher.iter(|| test::black_box({
            ASCIIEncoding.decode(s, DecoderTrap::Strict)
        }))
    }

    #[bench]
    fn bench_encode_replace(bencher: &mut test::Bencher) {
        let s = testutils::KOREAN_TEXT;
        bencher.bytes = s.len() as u64;
        bencher.iter(|| test::black_box({
            ASCIIEncoding.encode(s, EncoderTrap::Replace)
        }))
    }

    #[bench]
    fn bench_decode_replace(bencher: &mut test::Bencher) {
        let s = testutils::INVALID_UTF8_TEXT;
        bencher.bytes = s.len() as u64;
        bencher.iter(|| test::black_box({
            ASCIIEncoding.decode(s, DecoderTrap::Replace)
        }))
    }
}
