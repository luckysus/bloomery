use super::ParseError;
use quick_xml::events::{BytesStart, BytesText};
use quick_xml::Reader;

pub(super) fn attribute(
    reader: &Reader<&[u8]>,
    element: &BytesStart<'_>,
    name: &[u8],
) -> Result<Option<String>, ParseError> {
    for attribute in element.attributes() {
        let attribute =
            attribute.map_err(|error| ParseError::new("invalid_xml", error.to_string()))?;
        if attribute.key.local_name().as_ref() == name {
            return attribute
                .decode_and_unescape_value(reader.decoder())
                .map(|value| Some(value.into_owned()))
                .map_err(|error| ParseError::new("invalid_xml", error.to_string()));
        }
    }
    Ok(None)
}

pub(super) fn text(event: &BytesText<'_>) -> Result<String, ParseError> {
    let decoded = event
        .decode()
        .map_err(|error| ParseError::new("invalid_xml", error.to_string()))?;
    quick_xml::escape::unescape(&decoded)
        .map(|value| value.into_owned())
        .map_err(|error| ParseError::new("invalid_xml", error.to_string()))
}
