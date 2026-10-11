//! CSZ1 / zlib-blocks-v1: shared Bridge encoder and host decoder HAL.
use miniz_oxide::{deflate::compress_to_vec_zlib, inflate::core::{decompress, DecompressorOxide, inflate_flags}};
pub const BLOCK: usize = 16384;
const MAX_PACKED: usize = BLOCK + 128;

fn inflate_exact(input: &[u8], out: &mut [u8]) -> bool {
    let mut decoder = DecompressorOxide::new();
    let (status, consumed, written) = decompress(&mut decoder, input, out, 0,
        inflate_flags::TINFL_FLAG_PARSE_ZLIB_HEADER | inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF);
    status == miniz_oxide::inflate::TINFLStatus::Done && consumed == input.len() && written == out.len()
}

#[no_mangle]
unsafe extern "C" fn codex_bundle_inflate(out: *mut u8, cap: usize, input: *const u8, size: usize) -> bool {
    if cap == 0 || cap > BLOCK || size == 0 || size > MAX_PACKED { return false; }
    inflate_exact(unsafe { std::slice::from_raw_parts(input, size) }, unsafe { std::slice::from_raw_parts_mut(out, cap) })
}

fn crc(bytes: &[u8]) -> u32 { crc32fast::hash(bytes) }

pub fn encode(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.is_empty() || raw.len() > u32::MAX as usize / 2 { return None; }
    let blocks = raw.len().div_ceil(BLOCK);
    let mut out = b"CSZ1".to_vec();
    for v in [raw.len() as u32, crc(raw), BLOCK as u32, blocks as u32, 0] { out.extend(v.to_le_bytes()); }
    out.resize(24 + blocks * 4, 0);
    for (i, chunk) in raw.chunks(BLOCK).enumerate() {
        let packed = compress_to_vec_zlib(chunk, 6);
        if packed.len() > MAX_PACKED { return None; }
        out[24 + i*4..28 + i*4].copy_from_slice(&(packed.len() as u32).to_le_bytes());
        out.extend(packed);
    }
    (out.len() < raw.len()).then_some(out)
}

pub fn decode(input: &[u8], limit: usize) -> Result<Vec<u8>, &'static str> {
    if !input.starts_with(b"CSZ1") {
        return if input.len() <= limit { Ok(input.to_vec()) } else { Err("decoded_limit") };
    }
    if input.len() < 24 { return Err("header"); }
    let field = |i: usize| u32::from_le_bytes(input[i..i+4].try_into().unwrap()) as usize;
    let (length, count) = (field(4), field(16));
    if length == 0 || length > limit || field(12) != BLOCK || count != length.div_ceil(BLOCK) || field(20) != 0 ||
        count > (input.len() - 24) / 4 { return Err("header"); }
    let mut at = 24 + count * 4;
    // Check every length before allocating decoded storage.
    for i in 0..count {
        let n = field(24 + i*4);
        if n == 0 || n > MAX_PACKED || n > input.len().saturating_sub(at) { return Err("length"); }
        at += n;
    }
    if at != input.len() { return Err("trailing"); }
    let mut out = vec![0; length]; at = 24 + count * 4;
    for (i, chunk) in out.chunks_mut(BLOCK).enumerate() {
        let n = field(24 + i*4);
        if !inflate_exact(&input[at..at+n], chunk) { return Err("zlib"); }
        at += n;
    }
    if crc(&out) != field(8) as u32 { return Err("crc"); }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_blocks_and_corruption() {
        for n in [1, BLOCK-1, BLOCK, BLOCK+1, BLOCK*8+17] {
            let raw = vec![b'x'; n];
            let packed = encode(&raw).unwrap_or(raw.clone());
            assert_eq!(decode(&packed, n).unwrap(), raw);
            assert!(decode(&packed, n-1).is_err());
            if packed.starts_with(b"CSZ1") {
                for i in [4, 8, 12, 16, 20, 24, packed.len()-1] {
                    let mut bad = packed.clone(); bad[i] ^= 0x80; assert!(decode(&bad, n).is_err());
                }
                let mut bad = packed.clone(); bad.push(0); assert!(decode(&bad, n).is_err());
                assert!(decode(&packed[..packed.len()-1], n).is_err());
            }
        }
    }
}
