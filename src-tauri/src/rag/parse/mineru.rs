use super::archive::OfficeArchive;
use super::{markdown, DocumentBlock, ParseError, ParseLimits, ParsedAsset, ParsedDocument};

pub fn parse(bytes: &[u8], limits: ParseLimits) -> Result<ParsedDocument, ParseError> {
    let mut archive = OfficeArchive::open(bytes, limits)?;
    let main = select_main(&mut archive)?;
    let markdown_bytes = archive.read_required(&main)?;
    let markdown_source = std::str::from_utf8(&markdown_bytes).map_err(|error| {
        ParseError::new(
            "invalid_utf8",
            format!("MinerU Markdown is not UTF-8: {error}"),
        )
    })?;
    let mut document = markdown::parse(markdown_source)?;
    embed_images(&mut archive, &main, markdown_source, &mut document)?;
    Ok(document)
}

fn select_main(archive: &mut OfficeArchive<'_>) -> Result<String, ParseError> {
    let candidates = archive
        .names()
        .into_iter()
        .filter(|name| {
            name.rsplit('/')
                .next()
                .is_some_and(|file| file.eq_ignore_ascii_case("full.md"))
        })
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [main] => Ok(main.clone()),
        [] => Err(ParseError::new(
            "mineru_main_missing",
            "MinerU archive does not contain full.md",
        )),
        _ => Err(ParseError::new(
            "mineru_main_ambiguous",
            "MinerU archive contains more than one full.md",
        )),
    }
}

fn embed_images(
    archive: &mut OfficeArchive<'_>,
    main: &str,
    markdown_source: &str,
    document: &mut ParsedDocument,
) -> Result<(), ParseError> {
    let targets = markdown::image_targets(markdown_source);
    let image_blocks = document
        .blocks
        .iter_mut()
        .filter_map(|block| match block {
            DocumentBlock::Image { asset_index, .. } => Some(asset_index),
            _ => None,
        })
        .collect::<Vec<_>>();
    if image_blocks.len() != targets.len() {
        return Err(ParseError::new(
            "invalid_mineru_markdown",
            "MinerU Markdown image references could not be reconciled",
        ));
    }

    let mut resolved = 0usize;
    for (asset_index, target) in image_blocks.into_iter().zip(targets) {
        if is_remote(&target) {
            continue;
        }
        let entry_name = resolve_entry(main, &target)?;
        let Some(bytes) = archive.read_optional(&entry_name)? else {
            continue;
        };
        *asset_index = Some(document.assets.len());
        document.assets.push(ParsedAsset {
            kind: "image".to_string(),
            media_type: media_type(&entry_name).to_string(),
            original_name: entry_name.rsplit('/').next().map(str::to_string),
            bytes,
            location: None,
        });
        resolved += 1;
    }
    for warning in &mut document.warnings {
        if resolved == 0 {
            break;
        }
        if warning.code == "external_asset_not_embedded" {
            warning.code = "mineru_asset_embedded".to_string();
            resolved -= 1;
        }
    }
    document
        .warnings
        .retain(|warning| warning.code != "mineru_asset_embedded");
    Ok(())
}

fn resolve_entry(main: &str, target: &str) -> Result<String, ParseError> {
    if target.is_empty() || target.starts_with('/') || target.contains('\\') {
        return Err(unsafe_asset());
    }
    let mut parts = main
        .rsplit_once('/')
        .map(|(directory, _)| directory.split('/').collect::<Vec<_>>())
        .unwrap_or_default();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.pop().is_none() => return Err(unsafe_asset()),
            ".." => {}
            value => parts.push(value),
        }
    }
    if parts.is_empty() {
        Err(unsafe_asset())
    } else {
        Ok(parts.join("/"))
    }
}

fn is_remote(target: &str) -> bool {
    let target = target.to_ascii_lowercase();
    target.starts_with("http://") || target.starts_with("https://") || target.starts_with("data:")
}

fn unsafe_asset() -> ParseError {
    ParseError::new(
        "mineru_asset_path_unsafe",
        "MinerU Markdown contains an unsafe image path",
    )
}

fn media_type(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}
