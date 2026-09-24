//! Turning a document into text a deployment can read.
//!
//! A PDF in a workspace used to be read as though it were a text file, and the
//! campaign of 2026-09-06 measured what that costs: the deployment received a
//! header, an object graph and an ASCII85 image stream in place of a CV, spent
//! eight actions and both its compactions on variations of that read, and never
//! learned the file was not text. The read now refuses it by signature. A
//! refusal alone leaves the work undone, though: the document is still the task.
//!
//! This is the other half. It is deliberately narrow, and its refusals are the
//! point as much as its output is: a document whose text cannot be recovered
//! honestly is reported as one, never approximated. Mojibake that reads like
//! text is worse than nothing, because nothing is visibly nothing.
//!
//! What it does: inflate a PDF's content streams, read the text-showing
//! operators, decode simple-font bytes, and collect the link targets the
//! document declares. What it does not: OCR a scan, resolve a CID font's glyph
//! ids without a `ToUnicode` map, preserve layout, or claim that reading order
//! is visual order.

use std::io::Read as _;

/// How much of a PDF is examined. A document larger than this is refused rather
/// than partly extracted: a truncated CV is a wrong CV.
const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;

/// Fraction of recovered characters that must be plausible text before the
/// result is offered as text at all.
///
/// The failure this guards against is a CID-keyed font: its bytes are glyph
/// indices into an embedded font program, and decoding them as characters
/// yields fluent-looking rubbish. A run cannot tell that from a document
/// written in a language it does not know, so the harness must.
const MIN_PLAUSIBLE_RATIO: f64 = 0.90;

/// Text recovered from a document, with what is needed to judge it.
#[derive(Debug, Clone)]
pub struct Extraction {
    pub text: String,
    /// Link targets the document declares, in the order first seen.
    pub links: Vec<String>,
    /// Pages the document says it has, when it says so plainly.
    pub pages: Option<usize>,
    /// How the text was recovered, for the artifact's own header.
    pub method: &'static str,
}

/// Why a document's text could not be recovered.
///
/// Separate from the tool error so each reason can be phrased as the
/// prerequisite it is: "no text in it" and "text this extractor cannot decode"
/// send a caller to different places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractionFailure {
    NotADocument,
    TooLarge,
    NoTextContent,
    Undecodable,
    Malformed(String),
}

impl std::fmt::Display for ExtractionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotADocument => write!(
                f,
                "not a PDF; this extractor reads PDF documents and nothing else"
            ),
            Self::TooLarge => write!(
                f,
                "larger than {} MiB, which this extractor refuses rather than partly reads",
                MAX_DOCUMENT_BYTES / (1024 * 1024)
            ),
            Self::NoTextContent => write!(
                f,
                "carries no text content streams. A scanned document is images of text, \
                 and no tool in this run performs OCR"
            ),
            Self::Undecodable => write!(
                f,
                "encodes its text as font glyph ids without a ToUnicode map, so recovering \
                 characters from it would be guesswork. What came out did not look like text \
                 and has been discarded rather than returned"
            ),
            Self::Malformed(detail) => write!(f, "could not be parsed: {detail}"),
        }
    }
}

/// Recovers a document's text, or says why it cannot be recovered.
pub fn extract(bytes: &[u8]) -> Result<Extraction, ExtractionFailure> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_DOCUMENT_BYTES {
        return Err(ExtractionFailure::TooLarge);
    }
    if !bytes.starts_with(b"%PDF-") {
        return Err(ExtractionFailure::NotADocument);
    }

    // Every stream, inflated where it is inflatable. The page tree is not
    // walked: resolving it needs the cross-reference table, an incrementally
    // updated file has several, and a document that has been edited has objects
    // the last table does not name. Reading order comes out the same for the
    // documents this handles, and a stream that turns out not to be content is
    // discarded by the test below rather than by having been found the hard way.
    let mut segments = Vec::new();
    let mut links = Vec::new();
    // Counted in the file as written, once: an object that also appears inside
    // an object stream would otherwise be counted twice, which is how a
    // two-page CV first reported four.
    let pages = count_pages(bytes);
    collect_uris(bytes, &mut links);

    for (dictionary, stream) in streams(bytes) {
        let Some(data) = decode_stream(&dictionary, &stream) else {
            continue;
        };
        collect_uris(&data, &mut links);
        if !carries_text_operators(&data) {
            continue;
        }
        segments.push(show_text(&data));
    }

    if segments.is_empty() {
        return Err(ExtractionFailure::NoTextContent);
    }
    let text = segments.join("\n").trim().to_string();
    if text.is_empty() {
        return Err(ExtractionFailure::NoTextContent);
    }
    if plausible_ratio(&text) < MIN_PLAUSIBLE_RATIO {
        return Err(ExtractionFailure::Undecodable);
    }
    Ok(Extraction {
        text,
        links,
        pages,
        method: "PDF content streams, Flate-decoded, simple-font encodings",
    })
}

/// Every `stream ... endstream` in the file, with the dictionary that declares
/// what it holds.
///
/// The dictionary is not optional after all. Reading the campaign's own CV
/// without it produced 40 KiB of ASCII85 on stdout: the image stream is
/// printable ASCII, so "believe whatever decodes" believed it, and the
/// plausibility gate below could not tell printable from meaningful. That is
/// the same mistake the read tool made, one layer down.
fn streams(bytes: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut found = Vec::new();
    let mut at = 0usize;
    while let Some(start) = find(bytes, b"stream", at) {
        // `endstream` also contains `stream`; skipping past it here would find
        // the tail of the keyword that closes the stream we just read.
        let is_keyword = start
            .checked_sub(3)
            .is_none_or(|before| &bytes[before..start] != b"end");
        let after_keyword = start + b"stream".len();
        if !is_keyword {
            at = after_keyword;
            continue;
        }
        // The data begins after the end-of-line that follows the keyword, and
        // a lone carriage return is not one of the two forms the format allows.
        let data_start = match bytes.get(after_keyword) {
            Some(b'\r') if bytes.get(after_keyword + 1) == Some(&b'\n') => after_keyword + 2,
            Some(b'\n') => after_keyword + 1,
            _ => after_keyword,
        };
        let Some(end) = find(bytes, b"endstream", data_start) else {
            break;
        };
        let mut data_end = end;
        while data_end > data_start && matches!(bytes[data_end - 1], b'\n' | b'\r') {
            data_end -= 1;
        }
        let dictionary_start = start.saturating_sub(2048);
        let dictionary = bytes[dictionary_start..start]
            .rsplit(|byte| *byte == b'<')
            .next()
            .map(<[u8]>::to_vec)
            .unwrap_or_default();
        found.push((dictionary, bytes[data_start..data_end].to_vec()));
        at = end + b"endstream".len();
    }
    found
}

/// Decodes a stream according to the filters its dictionary declares.
///
/// `None` for anything this extractor does not decode, and for anything it can
/// see is not text: an image's bytes are not the document's words, and the one
/// in the campaign's CV is ASCII85 -- printable, and therefore indistinguishable
/// from text by any test that does not read the declaration.
fn decode_stream(dictionary: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() {
        return None;
    }
    let declared = String::from_utf8_lossy(dictionary);
    if declared.contains("/Subtype") && declared.contains("/Image") {
        return None;
    }
    let mut decoded = data.to_vec();
    let mut applied = 0usize;
    for filter in declared.split('/').skip(1) {
        let name: String = filter
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric())
            .collect();
        decoded = match name.as_str() {
            "FlateDecode" => inflate(&decoded)?,
            "ASCII85Decode" => ascii85(&decoded)?,
            "ASCIIHexDecode" => ascii_hex(&decoded)?,
            // An image codec, an unsupported compression, or a key that is not
            // a filter at all. The first two mean this stream is not text; the
            // third leaves `decoded` alone.
            "DCTDecode" | "JPXDecode" | "CCITTFaxDecode" | "JBIG2Decode" | "LZWDecode"
            | "RunLengthDecode" | "Crypt" => return None,
            _ => continue,
        };
        applied += 1;
    }
    // An undeclared stream is usable only if it is text to begin with.
    if applied == 0 && std::str::from_utf8(&decoded).is_err() {
        return None;
    }
    Some(decoded)
}

/// Zlib, then raw deflate. Streams are written both ways in the wild.
fn inflate(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    if flate2::read::ZlibDecoder::new(data)
        .read_to_end(&mut out)
        .is_ok()
        && !out.is_empty()
    {
        return Some(out);
    }
    out.clear();
    if flate2::read::DeflateDecoder::new(data)
        .read_to_end(&mut out)
        .is_ok()
        && !out.is_empty()
    {
        return Some(out);
    }
    None
}

/// ASCII85, in the form the format uses: `z` for four zero bytes, `~>` to end.
fn ascii85(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut group = [0u8; 5];
    let mut filled = 0usize;
    for byte in data {
        match byte {
            b'~' => break,
            b'z' if filled == 0 => out.extend_from_slice(&[0, 0, 0, 0]),
            b'!'..=b'u' => {
                group[filled] = byte - b'!';
                filled += 1;
                if filled == 5 {
                    out.extend_from_slice(&group_value(&group).to_be_bytes());
                    filled = 0;
                }
            }
            byte if byte.is_ascii_whitespace() => {}
            // Anything else means this is not ASCII85, whatever the dictionary
            // said, and half a decode is worse than none.
            _ => return None,
        }
    }
    if filled > 0 {
        for slot in group.iter_mut().skip(filled) {
            *slot = 84;
        }
        let bytes = group_value(&group).to_be_bytes();
        out.extend_from_slice(&bytes[..filled - 1]);
    }
    (!out.is_empty()).then_some(out)
}

fn group_value(group: &[u8; 5]) -> u32 {
    group.iter().fold(0u32, |value, digit| {
        value.wrapping_mul(85).wrapping_add(u32::from(*digit))
    })
}

/// Hex digits, two per byte, `>` to end.
fn ascii_hex(data: &[u8]) -> Option<Vec<u8>> {
    let mut digits = Vec::new();
    for byte in data {
        match byte {
            b'>' => break,
            byte if byte.is_ascii_hexdigit() => digits.push(*byte),
            byte if byte.is_ascii_whitespace() => {}
            _ => return None,
        }
    }
    if digits.len() % 2 == 1 {
        digits.push(b'0');
    }
    let out: Vec<u8> = digits
        .chunks(2)
        .filter_map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect();
    (!out.is_empty()).then_some(out)
}

/// Whether a decoded stream is page content rather than a font, an image or an
/// object stream.
///
/// Operators are matched as whole tokens. `BT` and `Tj` as bare substrings
/// occur by chance in any large blob, and matching them that way is how an
/// image stream first got mistaken for a page.
fn carries_text_operators(data: &[u8]) -> bool {
    token(data, b"BT") && token(data, b"ET") && (token(data, b"Tj") || token(data, b"TJ"))
}

/// Whether `needle` appears in `data` delimited the way an operator is.
fn token(data: &[u8], needle: &[u8]) -> bool {
    let delimits = |byte: Option<&u8>| match byte {
        None => true,
        Some(byte) => {
            byte.is_ascii_whitespace()
                || matches!(byte, b'[' | b']' | b'(' | b')' | b'<' | b'>' | b'/')
        }
    };
    let mut at = 0usize;
    while let Some(found) = find(data, needle, at) {
        let before = found.checked_sub(1).map(|index| &data[index]);
        let after = data.get(found + needle.len());
        if delimits(before) && delimits(after) {
            return true;
        }
        at = found + 1;
    }
    false
}

/// Reads the text-showing operators of one content stream.
///
/// A tokeniser rather than a parser: the operand stack is not modelled, because
/// the only operands that matter here are the strings themselves and the
/// positioning operators that separate one line from the next.
fn show_text(data: &[u8]) -> String {
    let mut out = String::new();
    let mut line = String::new();
    let mut index = 0usize;
    let mut pending_break = false;

    let flush = |line: &mut String, out: &mut String| {
        let trimmed = line.trim_end();
        if !trimmed.is_empty() {
            out.push_str(trimmed);
            out.push('\n');
        }
        line.clear();
    };

    while index < data.len() {
        match data[index] {
            b'(' => {
                let (text, next) = literal_string(data, index);
                if pending_break {
                    flush(&mut line, &mut out);
                    pending_break = false;
                }
                line.push_str(&text);
                index = next;
            }
            b'<' if data.get(index + 1) != Some(&b'<') => {
                let (text, next) = hex_string(data, index);
                if pending_break {
                    flush(&mut line, &mut out);
                    pending_break = false;
                }
                line.push_str(&text);
                index = next;
            }
            // A kerning adjustment inside a TJ array, large enough to be a gap
            // rather than letter spacing. Without this, "Full Stack" arrives as
            // "FullStack" wherever the writer set the space by position.
            b'-' if in_array(data, index) => {
                let (value, next) = number(data, index);
                if value <= -180.0 && !line.ends_with(' ') && !line.is_empty() {
                    line.push(' ');
                }
                index = next;
            }
            b'T' => {
                // Line positioning: what follows belongs on a new line.
                if let Some(b'd' | b'D' | b'*' | b'm') = data.get(index + 1) {
                    pending_break = true;
                }
                index += 2;
            }
            b'\'' | b'"' => {
                pending_break = true;
                index += 1;
            }
            b'E' if data[index..].starts_with(b"ET") => {
                flush(&mut line, &mut out);
                pending_break = false;
                index += 2;
            }
            _ => index += 1,
        }
    }
    flush(&mut line, &mut out);
    out
}

/// Whether the byte at `index` sits inside a `[ ... ]` array, which is what
/// distinguishes a kerning number from a coordinate.
fn in_array(data: &[u8], index: usize) -> bool {
    let window = index.saturating_sub(512);
    let mut depth = 0i32;
    for byte in &data[window..index] {
        match byte {
            b'[' => depth += 1,
            b']' => depth -= 1,
            _ => {}
        }
    }
    depth > 0
}

/// A PDF literal string, with its escapes resolved and its parens balanced.
fn literal_string(data: &[u8], start: usize) -> (String, usize) {
    let mut bytes = Vec::new();
    let mut depth = 1i32;
    let mut index = start + 1;
    while index < data.len() {
        match data[index] {
            b'\\' => {
                let Some(escaped) = data.get(index + 1) else {
                    break;
                };
                match escaped {
                    b'n' => bytes.push(b'\n'),
                    b'r' => bytes.push(b'\r'),
                    b't' => bytes.push(b'\t'),
                    b'b' => bytes.push(8),
                    b'f' => bytes.push(12),
                    b'\n' => {}
                    b'0'..=b'7' => {
                        let mut value = 0u16;
                        let mut digits = 0;
                        while digits < 3
                            && let Some(digit @ b'0'..=b'7') = data.get(index + 1 + digits)
                        {
                            value = value * 8 + u16::from(digit - b'0');
                            digits += 1;
                        }
                        bytes.push(value as u8);
                        index += digits + 1;
                        continue;
                    }
                    other => bytes.push(*other),
                }
                index += 2;
            }
            b'(' => {
                depth += 1;
                bytes.push(b'(');
                index += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    index += 1;
                    break;
                }
                bytes.push(b')');
                index += 1;
            }
            byte => {
                bytes.push(byte);
                index += 1;
            }
        }
    }
    (decode_simple_font(&bytes), index)
}

/// A PDF hex string. Two digits per byte, an odd trailing digit padded with
/// zero, as the format specifies.
fn hex_string(data: &[u8], start: usize) -> (String, usize) {
    let mut digits = Vec::new();
    let mut index = start + 1;
    while index < data.len() && data[index] != b'>' {
        if data[index].is_ascii_hexdigit() {
            digits.push(data[index]);
        }
        index += 1;
    }
    if digits.len() % 2 == 1 {
        digits.push(b'0');
    }
    let bytes: Vec<u8> = digits
        .chunks(2)
        .filter_map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect();
    (decode_simple_font(&bytes), index + 1)
}

/// A number starting at `start`, and the index just past it.
fn number(data: &[u8], start: usize) -> (f64, usize) {
    let mut index = start;
    if data.get(index) == Some(&b'-') {
        index += 1;
    }
    while matches!(data.get(index), Some(b'0'..=b'9' | b'.')) {
        index += 1;
    }
    let value = std::str::from_utf8(&data[start..index])
        .ok()
        .and_then(|text| text.parse::<f64>().ok())
        .unwrap_or(0.0);
    (value, index)
}

/// Bytes of a simple font's string, as characters.
///
/// WinAnsiEncoding for the range where it differs from Latin-1, Latin-1
/// elsewhere. This is the assumption the whole extractor rests on, and the
/// plausibility gate above is what catches the documents it is wrong about.
fn decode_simple_font(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| match byte {
            0x80 => '\u{20ac}',
            0x82 => '\u{201a}',
            0x83 => '\u{0192}',
            0x84 => '\u{201e}',
            0x85 => '\u{2026}',
            0x86 => '\u{2020}',
            0x87 => '\u{2021}',
            0x88 => '\u{02c6}',
            0x89 => '\u{2030}',
            0x8a => '\u{0160}',
            0x8b => '\u{2039}',
            0x8c => '\u{0152}',
            0x8e => '\u{017d}',
            0x91 => '\u{2018}',
            0x92 => '\u{2019}',
            0x93 => '\u{201c}',
            0x94 => '\u{201d}',
            0x95 => '\u{2022}',
            0x96 => '\u{2013}',
            0x97 => '\u{2014}',
            0x98 => '\u{02dc}',
            0x99 => '\u{2122}',
            0x9a => '\u{0161}',
            0x9b => '\u{203a}',
            0x9c => '\u{0153}',
            0x9e => '\u{017e}',
            0x9f => '\u{0178}',
            other => char::from(*other),
        })
        .collect()
}

/// The fraction of characters that look like text rather than like glyph ids.
fn plausible_ratio(text: &str) -> f64 {
    let mut plausible = 0usize;
    let mut total = 0usize;
    for character in text.chars() {
        total += 1;
        if character.is_alphanumeric()
            || character.is_whitespace()
            || character.is_ascii_punctuation()
            || matches!(
                character,
                '\u{2013}'..='\u{201d}' | '\u{2022}' | '\u{20ac}' | '\u{00a0}'
            )
        {
            plausible += 1;
        }
    }
    if total == 0 {
        return 0.0;
    }
    plausible as f64 / total as f64
}

/// Link targets the document declares, in the order first seen.
fn collect_uris(bytes: &[u8], into: &mut Vec<String>) {
    let mut at = 0usize;
    while let Some(found) = find(bytes, b"/URI", at) {
        at = found + 4;
        let Some(open) = bytes[at..]
            .iter()
            .position(|byte| *byte == b'(')
            .filter(|offset| *offset < 8)
        else {
            continue;
        };
        let (uri, next) = literal_string(bytes, at + open);
        let uri = uri.trim().to_string();
        if !uri.is_empty() && !into.contains(&uri) {
            into.push(uri);
        }
        at = next;
    }
}

/// Pages the document states, taken from the page tree's own count.
///
/// Not by counting `/Type /Page` objects: the campaign's CV has four of them
/// and two pages, because a page object can be written more than once and this
/// extractor deliberately does not resolve the cross-reference table that would
/// say which copies are live. `/Count` on the page tree is the document saying
/// how many pages it has, which is a different kind of evidence from a tally.
fn count_pages(bytes: &[u8]) -> Option<usize> {
    let mut best = None;
    let mut at = 0usize;
    while let Some(found) = find(bytes, b"/Pages", at) {
        at = found + 6;
        let window_start = found.saturating_sub(256);
        let window_end = (found + 256).min(bytes.len());
        let window = String::from_utf8_lossy(&bytes[window_start..window_end]).to_string();
        let Some(count_at) = window.find("/Count") else {
            continue;
        };
        let count: String = window[count_at + 6..]
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(value) = count.parse::<usize>() {
            best = Some(best.map_or(value, |current: usize| current.max(value)));
        }
    }
    best
}

/// Index of `needle` in `haystack` at or after `from`.
fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| from + offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_is_not_a_pdf_is_said_to_be_one_thing_and_not_another() {
        assert_eq!(
            extract(b"hello").unwrap_err(),
            ExtractionFailure::NotADocument
        );
    }

    #[test]
    fn a_pdf_with_no_content_streams_names_ocr_as_the_thing_it_lacks() {
        let failure = extract(b"%PDF-1.4\n1 0 obj\n<< /Type /Page >>\nendobj\n").unwrap_err();
        assert_eq!(failure, ExtractionFailure::NoTextContent);
        assert!(failure.to_string().contains("OCR"));
    }

    #[test]
    fn text_operators_become_lines_in_reading_order() {
        let content = b"BT /F1 12 Tf 72 720 Td (Vito Santanelli) Tj 0 -14 Td \
                        [(Software) -250 (Engineer)] TJ ET";
        let mut pdf = b"%PDF-1.4\n1 0 obj\n<< /Length 99 >>\nstream\n".to_vec();
        pdf.extend_from_slice(content);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");
        let extracted = extract(&pdf).unwrap();
        assert_eq!(extracted.text, "Vito Santanelli\nSoftware Engineer");
    }

    /// The gate that keeps a CID-keyed document from arriving as fluent rubbish.
    #[test]
    fn glyph_ids_are_refused_rather_than_returned_as_text() {
        let mut content = b"BT /F1 12 Tf 72 720 Td <".to_vec();
        for code in 0u8..64 {
            content.extend_from_slice(
                format!("{:02x}", 0xe0u16.saturating_add(code.into()) as u8).as_bytes(),
            );
        }
        content.extend_from_slice(b"> Tj ET");
        let mut pdf = b"%PDF-1.4\n1 0 obj\n<< /Length 90 >>\nstream\n".to_vec();
        pdf.extend_from_slice(&content);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");
        assert_eq!(extract(&pdf).unwrap_err(), ExtractionFailure::Undecodable);
    }

    #[test]
    fn declared_links_come_back_once_each_in_the_order_seen() {
        let mut pdf = b"%PDF-1.4\n<< /A << /URI (https://github.com/VitoSanta) >> >>\n\
                        << /A << /URI (https://github.com/VitoSanta) >> >>\n\
                        << /A << /URI (https://linkedin.com/in/vito-santanelli) >> >>\n"
            .to_vec();
        pdf.extend_from_slice(b"stream\nBT (x) Tj ET\nendstream\n");
        let extracted = extract(&pdf).unwrap();
        assert_eq!(
            extracted.links,
            vec![
                "https://github.com/VitoSanta".to_string(),
                "https://linkedin.com/in/vito-santanelli".to_string()
            ]
        );
    }

    #[test]
    fn a_flate_compressed_stream_is_read_like_any_other() {
        use std::io::Write as _;
        let content = b"BT /F1 12 Tf 72 720 Td (compressed) Tj ET";
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(content).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut pdf = b"%PDF-1.4\n1 0 obj\n<< /Filter /FlateDecode >>\nstream\n".to_vec();
        pdf.extend_from_slice(&compressed);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");
        assert_eq!(extract(&pdf).unwrap().text, "compressed");
    }
}
