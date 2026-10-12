//! Texture records: plain JPEG/PNG blobs in several rdb types (1010004, 1010008, 1010009, ...).

use anyhow::{Context, Result};
use ao_rdb::RecordStore;
use ao_scene::{Texture, TextureKey, TextureSource};

/// Seven rdb 1010004 records (shared by 178 character materials) lost the first four PNG signature
/// bytes (`89 50 4E 47`) to a `02 00 00 00` prefix; everything after is a valid PNG.
const BROKEN_PNG_PREFIX: [u8; 8] = [2, 0, 0, 0, 0x0D, 0x0A, 0x1A, 0x0A];

pub fn decode_texture(bytes: &[u8]) -> Result<Texture> {
    let fixed;
    let bytes = if bytes.starts_with(&BROKEN_PNG_PREFIX) {
        fixed = [&[0x89, b'P', b'N', b'G'][..], &bytes[4..]].concat();
        &fixed[..]
    } else {
        bytes
    };
    let img = image::load_from_memory(bytes).context("decoding texture")?;
    let source = match (bytes.starts_with(b"\x89PNG\r\n\x1a\n"), img.color().has_alpha()) {
        (true, true) => TextureSource::PngRgba,
        (true, false) => TextureSource::PngRgb,
        (false, true) => TextureSource::Rgba,
        (false, false) => TextureSource::Rgb,
    };
    let img = img.to_rgba8();
    Ok(Texture { source, width: img.width(), height: img.height(), rgba: img.into_raw() })
}

pub fn load_texture(store: &RecordStore, key: TextureKey) -> Result<Option<Texture>> {
    store.get(key.rdb_type, key.id)?.map(|b| decode_texture(&b)).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    #[test]
    fn png_provenance_uses_source_channels_not_decoded_opacity() {
        for (colour, pixels, expected) in [
            (image::ExtendedColorType::Rgb8, vec![10, 20, 30], TextureSource::PngRgb),
            (image::ExtendedColorType::Rgba8, vec![10, 20, 30, 255], TextureSource::PngRgba),
        ] {
            let mut bytes = Vec::new();
            image::codecs::png::PngEncoder::new(&mut bytes).write_image(&pixels, 1, 1, colour).unwrap();
            let texture = decode_texture(&bytes).unwrap();
            assert_eq!(texture.source, expected);
            assert_eq!(texture.rgba, [10, 20, 30, 255]);
            bytes[..4].copy_from_slice(&[2, 0, 0, 0]);
            assert_eq!(decode_texture(&bytes).unwrap().source, expected);
        }
    }

    #[test]
    fn jpeg_remains_compressible_rgb() {
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut bytes).encode(&[10, 20, 30], 1, 1, image::ExtendedColorType::Rgb8).unwrap();
        assert_eq!(decode_texture(&bytes).unwrap().source, TextureSource::Rgb);
        assert!(decode_texture(b"invalid image").is_err());
    }
}
