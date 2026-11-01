use base64::Engine;

pub const MAX_ART_BYTES: usize = 1024 * 1024;

pub fn data_uri(bytes: &[u8]) -> Option<String> {
    let mime = image_mime(bytes)?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_images_by_magic_bytes() {
        assert!(
            data_uri(&[0x89, b'P', b'N', b'G', 13, 10, 26, 10])
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert!(
            data_uri(&[0xff, 0xd8, 0xff])
                .unwrap()
                .starts_with("data:image/jpeg;base64,")
        );
        assert!(
            data_uri(b"GIF89a")
                .unwrap()
                .starts_with("data:image/gif;base64,")
        );
        assert!(
            data_uri(b"RIFFxxxxWEBP")
                .unwrap()
                .starts_with("data:image/webp;base64,")
        );
        assert_eq!(data_uri(b"not an image"), None);
    }
}
