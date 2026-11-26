use std::io::Cursor;

pub fn validate_bytes(bytes: &[u8], mime: &str, max_alloc: u64) -> Result<(), ()> {
    let invalid = || ();
    if mime.starts_with("image/") {
        let format = match mime {
            "image/png" => image::ImageFormat::Png,
            "image/jpeg" => image::ImageFormat::Jpeg,
            "image/webp" => image::ImageFormat::WebP,
            _ => return Err(invalid()),
        };
        if image::guess_format(bytes).map_err(|_| invalid())? != format {
            return Err(invalid());
        }
        let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(16_384);
        limits.max_image_height = Some(16_384);
        limits.max_alloc = Some(max_alloc);
        reader.limits(limits);
        reader.decode().map_err(|_| invalid())?;
    } else if mime == "application/pdf" {
        if !bytes.starts_with(b"%PDF-")
            || !bytes[bytes.len().saturating_sub(1024)..]
                .windows(5)
                .any(|w| w == b"%%EOF")
        {
            return Err(invalid());
        }
    } else if mime.starts_with("text/") {
        if std::str::from_utf8(bytes).is_err() || bytes.contains(&0) {
            return Err(invalid());
        }
    } else {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid())?;
        if zip.len() > 10_000 {
            return Err(invalid());
        }
        let required = match mime {
            "application/zip" => None,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                Some("word/document.xml")
            }
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => {
                Some("xl/workbook.xml")
            }
            "application/vnd.openxmlformats-officedocument.presentationml.presentation" => {
                Some("ppt/presentation.xml")
            }
            _ => return Err(invalid()),
        };
        if let Some(name) = required {
            zip.by_name("[Content_Types].xml").map_err(|_| invalid())?;
            zip.by_name(name).map_err(|_| invalid())?;
        }
    }
    Ok(())
}
