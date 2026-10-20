//! Page and font aware extraction. Unsupported text is refused, not guessed.
use super::*;
use lopdf::{Document, Encoding, LoadOptions, Object, content::Content};

fn failure(error: lopdf::Error) -> ExtractionFailure {
    match error {
        lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded { .. }) => {
            ExtractionFailure::ExpansionTooLarge
        }
        other => ExtractionFailure::Malformed(other.to_string()),
    }
}

pub(super) fn extract(bytes: &[u8]) -> Result<Extraction, ExtractionFailure> {
    // Preflight the existing global expansion budget before object-stream load.
    let mut budget = DecodeBudget::default();
    for (dictionary, stream) in streams(bytes) {
        let _ = decode_stream(&dictionary, &stream, &mut budget);
        if budget.exceeded {
            return Err(ExtractionFailure::ExpansionTooLarge);
        }
    }
    let doc = Document::load_mem_with_options(
        bytes,
        LoadOptions {
            max_decompressed_size: Some(MAX_STREAM_BYTES),
            ..Default::default()
        },
    )
    .map_err(failure)?;
    if doc.objects.len() > 100_000 {
        return Err(ExtractionFailure::TooLarge);
    }
    // Check real parsed streams too: indirect lengths and nested filters cannot
    // evade the byte-scanning preflight's accounting.
    let mut decoded = 0usize;
    for object in doc.objects.values() {
        if let Object::Stream(stream) = object {
            // Images are not text. Leave their codecs alone; no OCR is claimed.
            if stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image") {
                continue;
            }
            let content = if stream.dict.has(b"Filter") {
                stream
                    .decompressed_content_with_limit(MAX_STREAM_BYTES)
                    .map_err(failure)?
            } else {
                stream.content.clone()
            };
            if content.len() > MAX_STREAM_BYTES {
                return Err(ExtractionFailure::ExpansionTooLarge);
            }
            decoded = decoded
                .checked_add(content.len())
                .ok_or(ExtractionFailure::ExpansionTooLarge)?;
            if decoded > MAX_DECODED_DOCUMENT_BYTES {
                return Err(ExtractionFailure::ExpansionTooLarge);
            }
        }
    }
    let pages = doc.get_pages();
    if pages.len() > 10_000 {
        return Err(ExtractionFailure::TooLarge);
    }
    let mut text = String::new();
    let mut mapped_codes = 0usize;
    for page_id in pages.values() {
        let fonts = doc.get_page_fonts(*page_id).map_err(failure)?;
        let mut encodings = std::collections::BTreeMap::new();
        for (name, font) in fonts {
            if let Ok(map) = font.get_deref(b"ToUnicode", &doc) {
                let stream = map.as_stream().map_err(failure)?;
                let data = if stream.dict.has(b"Filter") {
                    stream
                        .decompressed_content_with_limit(MAX_STREAM_BYTES)
                        .map_err(failure)?
                } else {
                    stream.content.clone()
                };
                // The library constructs a reverse CMap too. Bound expanded
                // code counts, not just compressed/decoded bytes, before parsing.
                mapped_codes += validate_cmap(&data)?;
                if mapped_codes > 1_000_000 {
                    return Err(ExtractionFailure::ExpansionTooLarge);
                }
            }
            let encoding = font
                .get_font_encoding_with_limit(&doc, MAX_STREAM_BYTES)
                .map_err(failure)?;
            // lopdf intentionally falls back to StandardEncoding after a broken
            // CID CMap. That leniency is inappropriate for evidence sent to an agent.
            if (font.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Type0")
                || font.has(b"ToUnicode"))
                && !matches!(encoding, Encoding::UnicodeMapEncoding(_))
            {
                return Err(ExtractionFailure::Undecodable);
            }
            encodings.insert(name, encoding);
        }
        let content = doc
            .get_page_content_with_limit(*page_id, MAX_STREAM_BYTES)
            .map_err(failure)?;
        let content = Content::decode(&content).map_err(failure)?;
        let mut selected = None;
        for operation in content.operations {
            match operation.operator.as_str() {
                "Tf" => {
                    selected = operation
                        .operands
                        .first()
                        .and_then(|o| o.as_name().ok())
                        .and_then(|name| encodings.get(name));
                }
                "Tj" | "TJ" | "'" | "\"" => {
                    if matches!(operation.operator.as_str(), "'" | "\"") {
                        line_break(&mut text);
                    }
                    let encoding = selected.ok_or(ExtractionFailure::Undecodable)?;
                    for operand in &operation.operands {
                        append(operand, encoding, &mut text)?;
                    }
                }
                "Td" | "TD" => {
                    let y = operation
                        .operands
                        .get(1)
                        .and_then(|o| o.as_float().ok())
                        .unwrap_or(0.0);
                    if y.abs() > 0.01 {
                        line_break(&mut text);
                    }
                }
                "Tm" | "T*" | "ET" => line_break(&mut text),
                // Form XObjects may contain more text. Refuse instead of silently
                // returning a partial document until recursive extraction is supported.
                "Do" => {
                    let name = operation.operands.first().and_then(|o| o.as_name().ok());
                    let (resources, inherited) =
                        doc.get_page_resources(*page_id).map_err(failure)?;
                    for resources in resources.into_iter().chain(
                        inherited
                            .iter()
                            .filter_map(|id| doc.get_dictionary(*id).ok()),
                    ) {
                        if let (Some(name), Ok(xobjects)) = (
                            name,
                            resources
                                .get(b"XObject")
                                .and_then(|o| doc.dereference(o).map(|(_, o)| o))
                                .and_then(Object::as_dict),
                        ) && let Ok(Object::Stream(stream)) = xobjects
                            .get(name)
                            .and_then(|o| doc.dereference(o).map(|(_, o)| o))
                            && stream.dict.get(b"Subtype").and_then(Object::as_name).ok()
                                == Some(b"Form")
                        {
                            return Err(ExtractionFailure::Undecodable);
                        }
                    }
                }
                _ => {}
            }
            if text.len() > MAX_DECODED_DOCUMENT_BYTES {
                return Err(ExtractionFailure::ExpansionTooLarge);
            }
        }
        line_break(&mut text);
    }
    let text = text.trim().to_owned();
    if text.is_empty() {
        return Err(ExtractionFailure::NoTextContent);
    }
    if text.contains('\u{fffd}') || plausible_ratio(&text) < MIN_PLAUSIBLE_RATIO {
        return Err(ExtractionFailure::Undecodable);
    }
    let mut links = Vec::new();
    for object in doc.objects.values() {
        if let Ok(dict) = object.as_dict()
            && let Ok(uri) = dict.get(b"URI")
        {
            let uri = lopdf::decode_text_string(uri).map_err(failure)?;
            if !links.contains(&uri) {
                links.push(uri);
            }
        }
    }
    Ok(Extraction {
        text,
        links,
        pages: Some(pages.len()),
        method: "PDF live pages and font resources, bounded ToUnicode decoding (lopdf)",
    })
}
fn line_break(text: &mut String) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
}
fn append(
    object: &Object,
    encoding: &Encoding<'_>,
    text: &mut String,
) -> Result<(), ExtractionFailure> {
    match object {
        Object::String(bytes, _) => decode_text(bytes, encoding, text)?,
        Object::Array(values) => {
            for value in values {
                append(value, encoding, text)?;
            }
        }
        Object::Integer(value) if *value <= -180 => {
            if !text.ends_with([' ', '\n']) {
                text.push(' ');
            }
        }
        Object::Real(value) if *value <= -180.0 && !text.ends_with([' ', '\n']) => {
            text.push(' ');
        }
        _ => {}
    }
    if text.len() > MAX_DECODED_DOCUMENT_BYTES {
        return Err(ExtractionFailure::ExpansionTooLarge);
    }
    Ok(())
}

fn validate_cmap(data: &[u8]) -> Result<usize, ExtractionFailure> {
    let operations = Content::decode(data).map_err(failure)?;
    let mut count = 0usize;
    for operation in operations.operations {
        if operation.operator == "endbfrange" {
            if operation.operands.len() % 3 != 0 {
                return Err(ExtractionFailure::Undecodable);
            }
            for triple in operation.operands.chunks(3) {
                let start = triple[0].as_str().map_err(failure)?;
                let end = triple[1].as_str().map_err(failure)?;
                if start.is_empty() || start.len() > 4 || start.len() != end.len() {
                    return Err(ExtractionFailure::Undecodable);
                }
                let code = |bytes: &[u8]| {
                    bytes
                        .iter()
                        .fold(0u64, |value, byte| value * 256 + u64::from(*byte))
                };
                let len = code(end)
                    .checked_sub(code(start))
                    .and_then(|n| n.checked_add(1))
                    .ok_or(ExtractionFailure::Undecodable)?;
                if len > 65536 {
                    return Err(ExtractionFailure::ExpansionTooLarge);
                }
                count += len as usize;
                match &triple[2] {
                    Object::String(value, _) => {
                        validate_destination(value)?;
                        let last =
                            u16::from_be_bytes([value[value.len() - 2], value[value.len() - 1]]);
                        if u64::from(last) + len - 1 > u64::from(u16::MAX) {
                            return Err(ExtractionFailure::Undecodable);
                        }
                    }
                    Object::Array(values) if values.len() == len as usize => {
                        for value in values {
                            validate_destination(value.as_str().map_err(failure)?)?;
                        }
                    }
                    _ => return Err(ExtractionFailure::Undecodable),
                }
            }
        } else if operation.operator == "endbfchar" {
            if operation.operands.len() % 2 != 0 {
                return Err(ExtractionFailure::Undecodable);
            }
            for pair in operation.operands.chunks(2) {
                let source = pair[0].as_str().map_err(failure)?;
                if source.is_empty() || source.len() > 4 {
                    return Err(ExtractionFailure::Undecodable);
                }
                validate_destination(pair[1].as_str().map_err(failure)?)?;
                count += 1;
            }
        }
        if count > 65536 {
            return Err(ExtractionFailure::ExpansionTooLarge);
        }
    }
    if count == 0 {
        return Err(ExtractionFailure::Undecodable);
    }
    Ok(count)
}
fn validate_destination(bytes: &[u8]) -> Result<(), ExtractionFailure> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(2) || bytes.len() > 128 {
        return Err(ExtractionFailure::Undecodable);
    }
    let units = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_be_bytes([b[0], b[1]]));
    if char::decode_utf16(units).any(|c| c.is_err()) {
        return Err(ExtractionFailure::Undecodable);
    }
    Ok(())
}

// Decode incrementally so one short glyph string with long mapped ligatures
// cannot allocate a huge intermediate String before the document cap is checked.
fn decode_text(
    bytes: &[u8],
    encoding: &Encoding<'_>,
    text: &mut String,
) -> Result<(), ExtractionFailure> {
    if let Encoding::UnicodeMapEncoding(map) = encoding {
        let mut code = 0u32;
        let mut width = 0u8;
        for byte in bytes {
            code = code
                .checked_mul(256)
                .and_then(|n| n.checked_add(u32::from(*byte)))
                .ok_or(ExtractionFailure::Undecodable)?;
            width += 1;
            if let Some(units) = map.get(code, width) {
                for character in char::decode_utf16(units) {
                    text.push(character.map_err(|_| ExtractionFailure::Undecodable)?);
                }
                code = 0;
                width = 0;
            } else if width == 4 {
                return Err(ExtractionFailure::Undecodable);
            }
            if text.len() > MAX_DECODED_DOCUMENT_BYTES {
                return Err(ExtractionFailure::ExpansionTooLarge);
            }
        }
        if width != 0 {
            return Err(ExtractionFailure::Undecodable);
        }
    } else {
        for byte in bytes {
            let decoded = encoding.bytes_to_string(&[*byte]).map_err(failure)?;
            if decoded.is_empty() {
                return Err(ExtractionFailure::Undecodable);
            }
            text.push_str(&decoded);
            if text.len() > MAX_DECODED_DOCUMENT_BYTES {
                return Err(ExtractionFailure::ExpansionTooLarge);
            }
        }
    }
    Ok(())
}
