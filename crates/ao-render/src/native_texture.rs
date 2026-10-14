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

/// Pre-refactor reference implementation of the box-filter mip chain + encode, kept only in tests to
/// prove [`build_texture_mips`] produces byte-identical output after the ownership/parallelism rework.
#[cfg(test)]
fn reference_mip_chain(policy: Policy, source: TextureSource, bc_supported: bool, rgba: &[u8], w: u32, h: u32) -> Vec<(u32, u32, Vec<u8>)> {
    let encoding = policy.encoding(source, bc_supported, w, h);
    let mips = 32 - w.max(h).leading_zeros();
    let (mut cw, mut ch, mut data) = (w, h, rgba.to_vec());
    let mut encoded = Vec::new();
    let mut out = Vec::new();
    for level in 0..mips {
        let bytes = if encoding == Encoding::Rgba8 { data.clone() } else { encoding.encode(&data, cw, ch, &mut encoded); encoded.clone() };
        out.push((cw, ch, bytes));
        if level + 1 == mips { break; }
        let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut s = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (sx, sy) = ((2 * x + dx).min(cw - 1), (2 * y + dy).min(ch - 1));
                        s += data[((sy * cw + sx) * 4 + c) as usize] as u32;
                    }
                    next[((y * nw + x) * 4 + c) as usize] = ((s + 2) / 4) as u8;
                }
            }
        }
        (cw, ch, data) = (nw, nh, next);
    }
    out
}

pub struct BuiltLevel<'a> {
    pub cw: u32,
    pub ch: u32,
    pub bytes: std::borrow::Cow<'a, [u8]>,
}

pub struct BuiltTexture<'a> {
    pub w: u32,
    pub h: u32,
    pub mips: u32,
    pub encoding: Encoding,
    pub levels: Vec<BuiltLevel<'a>>,
}

/// CPU-side box-filter mip chain (sRGB-space average; fine for diffuse art) plus BC/packed-format
/// encoding, factored out of `Renderer::texture_view` as a pure function (no GPU access) so a batch
/// of textures can be built across threads; the caller still issues the actual `queue.write_texture`
/// calls serially afterward (audit item 10). Base-level bytes are borrowed from `rgba` when the
/// upload format is raw RGBA8 (no copy); every other buffer is produced once by the box filter or the
/// BC/565/4444 encoder and moved, never duplicated.
pub fn build_texture_mips<'a>(policy: Policy, source: TextureSource, bc_supported: bool, rgba: &'a [u8], w: u32, h: u32) -> BuiltTexture<'a> {
    let encoding = policy.encoding(source, bc_supported, w, h);
    let mips = 32 - w.max(h).leading_zeros();
    let (mut cw, mut ch) = (w, h);
    let mut raw: std::borrow::Cow<'a, [u8]> = std::borrow::Cow::Borrowed(rgba);
    let mut levels = Vec::with_capacity(mips as usize);
    for level in 0..mips {
        let last = level + 1 == mips;
        // Downsample before consuming `raw` into this level's output bytes below.
        let next = if last {
            None
        } else {
            let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
            let mut next = vec![0u8; (nw * nh * 4) as usize];
            for y in 0..nh {
                for x in 0..nw {
                    for c in 0..4 {
                        let mut s = 0u32;
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            let (sx, sy) = ((2 * x + dx).min(cw - 1), (2 * y + dy).min(ch - 1));
                            s += raw[((sy * cw + sx) * 4 + c) as usize] as u32;
                        }
                        next[((y * nw + x) * 4 + c) as usize] = ((s + 2) / 4) as u8;
                    }
                }
            }
            Some((nw, nh, next))
        };
        let bytes: std::borrow::Cow<'a, [u8]> = if encoding == Encoding::Rgba8 {
            raw
        } else {
            let mut encoded = Vec::new();
            encoding.encode(&raw, cw, ch, &mut encoded);
            std::borrow::Cow::Owned(encoded)
        };
        levels.push(BuiltLevel { cw, ch, bytes });
        match next {
            Some((nw, nh, n)) => { cw = nw; ch = nh; raw = std::borrow::Cow::Owned(n); }
            None => break,
        }
    }
    BuiltTexture { w, h, mips, encoding, levels }
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

    fn synth_rgba(w: u32, h: u32) -> Vec<u8> {
        (0..w * h).flat_map(|i| [(i * 37 % 251) as u8, (i * 53 % 241) as u8, (i * 97 % 239) as u8, (i * 131 % 233) as u8]).collect()
    }

    type MipLevels = Vec<(u32, u32, Vec<u8>)>;
    fn built_levels(b: &BuiltTexture) -> MipLevels {
        b.levels.iter().map(|l| (l.cw, l.ch, l.bytes.to_vec())).collect()
    }

    #[test]
    fn mip_chain_byte_identical_to_pre_refactor_reference() {
        // Square power-of-two, non-square (clamp actually matters: smaller axis hits 1px first),
        // and a BC1-compressed case all have to match the old inline per-level implementation exactly.
        let cases: &[(Policy, TextureSource, bool, u32, u32)] = &[
            (Policy { quality: 2, compressed: false }, TextureSource::Rgb, false, 16, 16),
            (Policy { quality: 2, compressed: false }, TextureSource::Rgb, false, 1, 8),
            (Policy { quality: 2, compressed: false }, TextureSource::Rgb, false, 8, 1),
            (Policy { quality: 0, compressed: false }, TextureSource::PngRgb, false, 12, 6),
            (Policy { quality: 0, compressed: false }, TextureSource::PngRgba, false, 6, 12),
            (Policy { quality: 2, compressed: true }, TextureSource::Rgb, true, 8, 8),
            (Policy { quality: 2, compressed: true }, TextureSource::Rgba, true, 16, 8),
        ];
        for &(policy, source, bc_supported, w, h) in cases {
            let rgba = synth_rgba(w, h);
            let expected = reference_mip_chain(policy, source, bc_supported, &rgba, w, h);
            let built = build_texture_mips(policy, source, bc_supported, &rgba, w, h);
            assert_eq!(built_levels(&built), expected, "mismatch for {w}x{h} source={source:?} compressed={}", policy.compressed);
        }
    }

    #[test]
    fn parallel_batch_build_matches_sequential_per_texture() {
        // Simulates Renderer::upload's thread::scope batch: build several distinct textures across
        // worker threads, then assert every result matches building that same texture alone,
        // sequentially -- i.e. no texture's bytes depend on what else ran concurrently.
        let policy = Policy { quality: 2, compressed: true };
        let sizes = [(8, 8), (16, 16), (4, 4), (8, 4), (32, 8), (4, 8)];
        let rgbas: Vec<Vec<u8>> = sizes.iter().map(|&(w, h)| synth_rgba(w, h)).collect();
        let sequential: Vec<_> = sizes.iter().zip(&rgbas).map(|(&(w, h), rgba)| {
            built_levels(&build_texture_mips(policy, TextureSource::Rgb, true, rgba, w, h))
        }).collect();

        let mut parallel: Vec<Option<MipLevels>> = (0..sizes.len()).map(|_| None).collect();
        std::thread::scope(|scope| {
            for ((&(w, h), rgba), slot) in sizes.iter().zip(&rgbas).zip(parallel.iter_mut()) {
                scope.spawn(move || {
                    *slot = Some(built_levels(&build_texture_mips(policy, TextureSource::Rgb, true, rgba, w, h)));
                });
            }
        });
        for (i, (seq, par)) in sequential.iter().zip(parallel).enumerate() {
            assert_eq!(*seq, par.unwrap(), "texture {i} diverged between sequential and parallel builds");
        }
    }
}
