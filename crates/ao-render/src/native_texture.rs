//! Randy 100193e4 selects RGB565/RGB888, RGBA4444/RGBA8888, or DXT1/DXT3.
//! Modern Metal has no packed 565/4444 sampled formats: expand those exact bits
//! to RGBA8. Compressed textures remain actual BC1/BC2 GPU allocations.
use ao_scene::TextureSource;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub quality: i32,
    pub compressed: bool,
}

impl Default for Policy {
    fn default() -> Self { Self { quality: 2, compressed: false } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding { Rgba8, Rgb565, Rgba4444, Bc1, Bc2 }

impl Policy {
    pub fn encoding(self, source: TextureSource, bc_supported: bool, width: u32, height: u32) -> Encoding {
        let alpha = matches!(source, TextureSource::Rgba | TextureSource::PngRgba);
        if source == TextureSource::Generated { return Encoding::Rgba8; }
        // Native .png temporarily disables compression. DXT allocation requires
        // a full block at the base level; unsupported layouts use native RGB(A).
        if self.compressed && bc_supported && matches!(source, TextureSource::Rgb | TextureSource::Rgba) && width.is_multiple_of(4) && height.is_multiple_of(4) {
            if alpha { Encoding::Bc2 } else { Encoding::Bc1 }
        } else if alpha && self.quality == 0 { Encoding::Rgba4444 }
        else if !alpha && self.quality < 2 { Encoding::Rgb565 }
        else { Encoding::Rgba8 }
    }
}

impl Encoding {
    pub fn format(self) -> wgpu::TextureFormat {
        match self { Self::Bc1 => wgpu::TextureFormat::Bc1RgbaUnormSrgb, Self::Bc2 => wgpu::TextureFormat::Bc2RgbaUnormSrgb, _ => wgpu::TextureFormat::Rgba8UnormSrgb }
    }
    pub fn block_bytes(self) -> u32 { match self { Self::Bc1 => 8, Self::Bc2 => 16, _ => 4 } }
    pub fn block_edge(self) -> u32 { match self { Self::Bc1 | Self::Bc2 => 4, _ => 1 } }
    pub fn encode(self, rgba: &[u8], width: u32, height: u32, output: &mut Vec<u8>) {
        match self {
            Self::Bc1 | Self::Bc2 => {
                let format = if self == Self::Bc1 { texpresso::Format::Bc1 } else { texpresso::Format::Bc2 };
                output.resize(format.compressed_size(width as usize, height as usize), 0);
                format.compress(rgba, width as usize, height as usize, texpresso::Params::default(), output);
            }
            Self::Rgba8 => { output.clear(); output.extend_from_slice(rgba); }
            Self::Rgb565 | Self::Rgba4444 => {
                output.clear();
                output.reserve(rgba.len());
                for pixel in rgba.as_chunks::<4>().0 {
                    let expand = |v: u8, bits: u32| {
                        let q = v >> (8 - bits);
                        (q << (8 - bits)) | (q >> (2 * bits - 8))
                    };
                    if self == Self::Rgb565 {
                        output.extend_from_slice(&[expand(pixel[0], 5), expand(pixel[1], 6), expand(pixel[2], 5), 255]);
                    } else {
                        output.extend(pixel.iter().map(|&v| expand(v, 4)));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_format_selection_and_upload_bytes() {
        let low = Policy { quality: 0, compressed: true };
        assert_eq!(low.encoding(TextureSource::Rgb, true, 4, 4), Encoding::Bc1);
        assert_eq!(low.encoding(TextureSource::Rgba, true, 4, 4), Encoding::Bc2);
        assert_eq!(low.encoding(TextureSource::PngRgb, true, 4, 4), Encoding::Rgb565);
        assert_eq!(low.encoding(TextureSource::PngRgba, true, 4, 4), Encoding::Rgba4444);
        assert_eq!(low.encoding(TextureSource::Generated, true, 4, 4), Encoding::Rgba8);
        assert_eq!(low.encoding(TextureSource::Rgb, false, 4, 4), Encoding::Rgb565);
        let mut encoded = Vec::new();
        Encoding::Rgb565.encode(&[123, 127, 251, 17], 1, 1, &mut encoded);
        assert_eq!(encoded, [123, 125, 255, 255]);
        Encoding::Rgba4444.encode(&[123, 127, 251, 17], 1, 1, &mut encoded);
        assert_eq!(encoded, [119, 119, 255, 17]);
        let pixels: Vec<_> = (0..16).flat_map(|_| [255, 0, 0, 136]).collect();
        for format in [Encoding::Bc1, Encoding::Bc2] {
            format.encode(&pixels, 4, 4, &mut encoded);
            assert_eq!(encoded.len(), format.block_bytes() as usize);
            let decoder = if format == Encoding::Bc1 { texpresso::Format::Bc1 } else { texpresso::Format::Bc2 };
            let mut decoded = vec![0; 64];
            decoder.decompress(&encoded, 4, 4, &mut decoded);
            assert_eq!(&decoded[..3], &[255, 0, 0]);
            if format == Encoding::Bc2 { assert_eq!(decoded[3], 136); }
        }
    }
}
