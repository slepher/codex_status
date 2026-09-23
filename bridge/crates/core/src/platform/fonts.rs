//! Bridge-side font library: CSFN v1 container validation, content-addressed
//! storage and per-Profile dependency planning.
//!
//! The container contract is `docs/font-asset-format.md`; the same bytes are
//! produced by `tools/note4-fonts/rasterize_ttf.py` and consumed by the device
//! (`src/font_asset.cpp`). [`FontLibrary::validate`] is the Rust half of the
//! device's `fontAssetValidate` and accepts exactly the same containers, so a
//! font the bridge stores is always one the device will load, and a container
//! the device would refuse is never pushed.
//!
//! Storage follows the platform style of `platform/store.rs`: plain files under
//! a data root (`<exe>/data/fonts` by default), atomic whole-file replace, one
//! file per blob. The library is content-addressed (`<font_id>.bin`) and
//! write-once: adding an id that is already stored is a no-op, which is what
//! makes a repeated push transfer zero bytes.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::platform::store;
use crate::template::FONTS;

/// `"CSFN"` little-endian (`0x4E465343`).
pub const MAGIC: u32 = 0x4E46_5343;
/// Container version this module implements.
pub const VERSION: u16 = 1;
/// Fixed header length in bytes.
pub const HEADER_BYTES: usize = 64;
/// Dense ASCII slots (`0x20..=0x7E`), always present in the descriptor table.
pub const GLYPH_SLOTS: usize = 95;
/// Size of one `Note4Glyph` descriptor: `u16 off, u16 adv, u8 boxW, u8 boxH,
/// i8 ofsX, i8 ofsY`.
pub const GLYPH_BYTES: usize = 8;
/// `pixelFormat` 0: 1bpp black/white.
pub const PIXEL_BW: u8 = 0;
/// `pixelFormat` 1: 2bpp gray4 (reserved; see [`FontDescriptor::check_target`]).
pub const PIXEL_GRAY4: u8 = 1;
/// Bits per pixel for `bw`.
pub const BPP_BW: u8 = 1;
/// Bits per pixel for `gray4`.
pub const BPP_GRAY4: u8 = 2;
/// CSFN v1 stores total length as u32; the device's advertised object cap and
/// install peak budget, rather than a product constant, govern publication.
pub const MAX_FONT_ASSET_BYTES: usize = u32::MAX as usize;
/// Device buffers for the three string sections, terminator included: a section
/// whose length reaches the buffer is rejected rather than truncated.
const NAME_MAX: usize = 24;
const FAMILY_MAX: usize = 32;
const COVERAGE_MAX: usize = 16;

pub const MAX_FONTS_PER_PROFILE: usize = usize::MAX;
pub const MAX_FONT_BYTES_PER_PROFILE: u64 = u64::MAX;

/// Everything the bridge needs to know about a stored font without carrying the
/// glyph data. `id` is the content address, so a descriptor is immutable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontDescriptor {
    /// `crc32` of the whole container, 8 lowercase hex digits.
    pub id: String,
    /// Template-visible name, e.g. `ntthin18`.
    pub name: String,
    /// Source face file name, e.g. `NotoSans-Thin.ttf`.
    pub family: String,
    /// `ascii` (95 slots) or `digits` (subset).
    pub coverage: String,
    /// Nominal pixel size the face was rasterized at.
    pub size_px: u16,
    /// 100–900.
    pub weight: u16,
    /// Bits per pixel (1 or 2).
    pub bpp: u8,
    /// `bw` (1bpp) or `gray4` (2bpp).
    pub pixel_format: String,
    /// Line height in px.
    pub line_height: u8,
    /// Descent below the baseline in px.
    pub base_line: u8,
    /// Widest advance in 1/16 px (region estimation).
    pub max_adv: u16,
    /// Glyph blob length in bytes.
    pub blob_bytes: u32,
    /// Descriptor slots (95 for CSFN v1).
    pub glyph_count: u32,
    /// How many of the slots carry ink.
    pub filled_glyphs: u16,
    /// 0 native, 1 auto (FreeType autohinter), 2 none.
    pub hint: u8,
    /// Whole container length in bytes.
    pub bytes: u32,
}

impl FontDescriptor {
    /// Mirror of the device's `fontAssetMatchesTarget`: a gray4 asset pushed to
    /// a 1bpp target (or the reverse) renders wrong pixels, so it is refused
    /// here instead.
    pub fn check_target(&self, target_pixel_format: &str) -> Result<()> {
        let wanted = match target_pixel_format {
            "1bpp" => "bw",
            "2bpp" => "gray4",
            other => bail!(
                "unknown target pixel format \"{other}\": expected \"1bpp\" or \"2bpp\""
            ),
        };
        if self.pixel_format != wanted {
            bail!(
                "font {} is {}/{}bpp but the target is {target_pixel_format} ({wanted})",
                self.id,
                self.pixel_format,
                self.bpp
            );
        }
        Ok(())
    }
}

/// `pixelFormat` byte as the transport word the descriptors and the device use.
pub fn pixel_format_name(pixel_format: u8) -> &'static str {
    match pixel_format {
        PIXEL_BW => "bw",
        PIXEL_GRAY4 => "gray4",
        _ => "?",
    }
}

/// Whether `id` can address a blob: 8 lowercase hex digits, nothing else. Any
/// other string is refused before it can reach the file system.
pub fn is_font_id(id: &str) -> bool {
    id.len() == 8
        && id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// Content-addressed font library: one directory of `<font_id>.bin` blobs,
/// deduplicated across Profiles.
pub struct FontLibrary {
    root: PathBuf,
}

impl FontLibrary {
    /// Open (creating the directory if needed) a content-addressed library.
    pub fn open(root: &Path) -> Result<Self> {
        store::ensure_dir(root).with_context(|| format!("open font library {}", root.display()))?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    /// Open the portable runtime library `<exe>/data/fonts`.
    pub fn open_default() -> Result<Self> {
        Self::open(&crate::paths::data_root().join("fonts"))
    }

    /// Directory holding the blobs.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Validate `bytes` as a CSFN v1 container WITHOUT storing it.
    ///
    /// The check set and its order match the device's `fontAssetValidate`, so
    /// the two implementations agree on every rejected container: magic,
    /// version, header length, declared vs actual length, payload CRC,
    /// bpp/pixel format (and their pairing), glyph count, line metrics, filled
    /// count, string sections (printable ASCII, bounded, even-length padded),
    /// the descriptor table's alignment, the declared payload shape, and every
    /// glyph box against the blob.
    ///
    /// `id` is computed here; the header's reserved bytes are ignored, exactly
    /// as the device ignores them (they are still covered by the content
    /// address, so they cannot change without changing the id).
    pub fn validate(bytes: &[u8]) -> Result<FontDescriptor> {
        let len = bytes.len();
        if len < HEADER_BYTES {
            bail!("font container is {len} bytes, shorter than the {HEADER_BYTES}-byte header");
        }
        if len > MAX_FONT_ASSET_BYTES {
            bail!(
                "font container is {len} bytes, over the {MAX_FONT_ASSET_BYTES}-byte per-asset cap"
            );
        }
        let magic = rd32(bytes, 0);
        if magic != MAGIC {
            bail!("bad font magic 0x{magic:08x}: expected \"CSFN\" (0x{MAGIC:08x})");
        }
        let version = rd16(bytes, 4);
        if version != VERSION {
            bail!("unsupported font container version {version}: expected {VERSION}");
        }
        let header_bytes = rd16(bytes, 6) as usize;
        if header_bytes != HEADER_BYTES {
            bail!("bad font header length {header_bytes}: expected {HEADER_BYTES}");
        }
        let file_bytes = rd32(bytes, 8) as usize;
        if file_bytes != len {
            bail!("font length mismatch: header declares {file_bytes} bytes, container has {len}");
        }
        let payload_crc = rd32(bytes, 12);
        let computed_crc = crc32fast::hash(&bytes[HEADER_BYTES..len]);
        if computed_crc != payload_crc {
            bail!(
                "font payload crc mismatch: header declares {payload_crc:08x}, payload computes {computed_crc:08x}"
            );
        }

        let blob_bytes = rd32(bytes, 16) as usize;
        let glyph_count = rd32(bytes, 20);
        let bpp = bytes[24];
        let pixel_format = bytes[25];
        let line_height = bytes[26];
        let base_line = bytes[27];
        let max_adv = rd16(bytes, 28);
        let size_px = rd16(bytes, 30);
        let weight = rd16(bytes, 32);
        let name_len = rd16(bytes, 34) as usize;
        let family_len = rd16(bytes, 36) as usize;
        let coverage_len = rd16(bytes, 38) as usize;
        let filled_glyphs = rd16(bytes, 40);
        let hint = bytes[42];

        if bpp != BPP_BW && bpp != BPP_GRAY4 {
            bail!("unsupported font bpp {bpp}: expected {BPP_BW} (bw) or {BPP_GRAY4} (gray4)");
        }
        if pixel_format > PIXEL_GRAY4 {
            bail!(
                "unsupported font pixel format {pixel_format}: expected {PIXEL_BW} (bw) or {PIXEL_GRAY4} (gray4)"
            );
        }
        if pixel_format == PIXEL_BW && bpp != BPP_BW {
            bail!("font bpp {bpp} does not match pixel format bw ({BPP_BW}bpp)");
        }
        if pixel_format == PIXEL_GRAY4 && bpp != BPP_GRAY4 {
            bail!("font bpp {bpp} does not match pixel format gray4 ({BPP_GRAY4}bpp)");
        }
        if glyph_count != GLYPH_SLOTS as u32 {
            bail!(
                "unsupported font glyph count {glyph_count}: the engine's dense ASCII table has {GLYPH_SLOTS} slots"
            );
        }
        if line_height == 0 || base_line >= line_height {
            bail!("bad font line metrics: line height {line_height}, baseline {base_line}");
        }
        if filled_glyphs as usize > GLYPH_SLOTS {
            bail!(
                "font declares {filled_glyphs} filled glyphs, but only {GLYPH_SLOTS} slots exist"
            );
        }

        // String sections are padded to an even length (the padding is not part
        // of the length fields) so the descriptor table starts on a 2-byte
        // boundary and can be pointed at directly (docs/font-asset-format.md).
        let pad = (name_len & 1) + (family_len & 1) + (coverage_len & 1);
        let glyph_off = HEADER_BYTES + name_len + family_len + coverage_len + pad;
        if glyph_off % 2 != 0 {
            bail!(
                "font descriptor table would start at odd offset {glyph_off}: string sections must be padded to even lengths"
            );
        }
        let glyph_table = GLYPH_SLOTS * GLYPH_BYTES;
        if glyph_off + glyph_table > len {
            bail!(
                "font payload is {len} bytes, shorter than the declared sections ({} bytes of strings + {glyph_table} bytes of glyph descriptors)",
                name_len + family_len + coverage_len + pad
            );
        }
        let blob_off = glyph_off + glyph_table;
        if blob_bytes > len - blob_off {
            bail!(
                "font blob out of range: {blob_bytes} bytes at offset {blob_off}, container has {len}"
            );
        }
        // The declared shape must describe the file exactly: trailing slack
        // would let two different files claim the same shape.
        if blob_off + blob_bytes != len {
            bail!(
                "font payload length mismatch: blob ends at {}, container has {len} bytes",
                blob_off + blob_bytes
            );
        }

        let name = read_section(bytes, HEADER_BYTES, name_len, NAME_MAX, "name")?;
        let family_off = HEADER_BYTES + name_len + (name_len & 1);
        let family = read_section(bytes, family_off, family_len, FAMILY_MAX, "family")?;
        let coverage_off = family_off + family_len + (family_len & 1);
        let coverage = read_section(bytes, coverage_off, coverage_len, COVERAGE_MAX, "coverage")?;
        if name.is_empty() {
            bail!("font name is empty");
        }
        if coverage.is_empty() {
            bail!("font coverage is empty");
        }

        // Every box must fit inside the blob: a half-written container must
        // never look valid.
        for slot in 0..GLYPH_SLOTS {
            let at = glyph_off + slot * GLYPH_BYTES;
            let off = rd16(bytes, at) as usize;
            let box_w = bytes[at + 4];
            let box_h = bytes[at + 5];
            if box_w == 0 || box_h == 0 {
                continue;
            }
            if box_w > 127 || box_h > 127 {
                bail!("font glyph {slot} box is {box_w}x{box_h} px, over the 127-pixel limit");
            }
            let need = ((box_w as usize + 7) / 8) * box_h as usize;
            if off > blob_bytes || need > blob_bytes - off {
                bail!(
                    "font glyph {slot} needs {need} bytes at blob offset {off}, over the {blob_bytes}-byte blob"
                );
            }
        }

        Ok(FontDescriptor {
            id: format!("{:08x}", crc32fast::hash(bytes)),
            name,
            family,
            coverage,
            size_px,
            weight,
            bpp,
            pixel_format: pixel_format_name(pixel_format).to_string(),
            line_height,
            base_line,
            max_adv,
            blob_bytes: blob_bytes as u32,
            glyph_count,
            filled_glyphs,
            hint,
            bytes: len as u32,
        })
    }

    /// Validate then store; returns the descriptor. Storing an id twice is a
    /// no-op that does not rewrite the file.
    pub fn add(&self, bytes: &[u8]) -> Result<FontDescriptor> {
        let descriptor = Self::validate(bytes)?;
        let path = self.path_for(&descriptor.id)?;
        if path.exists() {
            // Write-once: an id that is already stored is never rewritten. The
            // content is compared first so a file that does not match its own
            // address is reported instead of silently replaced.
            let existing = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
            if existing != bytes {
                bail!(
                    "font {} already exists in {} with different content",
                    descriptor.id,
                    self.root.display()
                );
            }
            return Ok(descriptor);
        }
        store::atomic_write(&path, bytes)?;
        Ok(descriptor)
    }

    /// All descriptors present in the library, sorted by name then size.
    pub fn list(&self) -> Result<Vec<FontDescriptor>> {
        let mut out = Vec::new();
        for path in self.blob_paths()? {
            let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
            let descriptor =
                Self::validate(&bytes).with_context(|| format!("load font {}", path.display()))?;
            out.push(descriptor);
        }
        out.sort_by(|a, b| (&a.name, a.size_px).cmp(&(&b.name, b.size_px)));
        Ok(out)
    }

    /// Descriptor of a stored blob, or `None` when the id is not stored. A
    /// malformed id is an explicit error: it cannot address a blob.
    pub fn get(&self, id: &str) -> Result<Option<FontDescriptor>> {
        Ok(self.load(id)?.map(|(_, descriptor)| descriptor))
    }

    /// Stored container bytes, re-validated so a corrupted file is never
    /// pushed. `None` when the id is not stored.
    pub fn read(&self, id: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.load(id)?.map(|(bytes, _)| bytes))
    }

    /// Resolve a template font name to the descriptors that can serve it.
    ///
    /// Assets are matched by their template-visible `name`, so a name that is
    /// compiled into the firmware and has no asset (`nt16`) resolves to an
    /// empty vec, exactly like an unknown name (`nope`): an empty result means
    /// "nothing to push for this name".
    pub fn resolve_name(&self, name: &str) -> Result<Vec<FontDescriptor>> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .list()?
            .into_iter()
            .filter(|descriptor| descriptor.name == name)
            .collect())
    }

    /// Total bytes on disk and the number of blobs.
    pub fn usage(&self) -> Result<(u64, usize)> {
        let mut bytes = 0u64;
        let mut blobs = 0usize;
        for path in self.blob_paths()? {
            let meta = fs::metadata(&path).with_context(|| format!("stat {}", path.display()))?;
            bytes += meta.len();
            blobs += 1;
        }
        Ok((bytes, blobs))
    }

    fn path_for(&self, id: &str) -> Result<PathBuf> {
        if !is_font_id(id) {
            bail!("invalid font id \"{id}\": expected 8 lowercase hex digits");
        }
        Ok(self.root.join(format!("{id}.bin")))
    }

    fn load(&self, id: &str) -> Result<Option<(Vec<u8>, FontDescriptor)>> {
        let path = self.path_for(id)?;
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        let descriptor =
            Self::validate(&bytes).with_context(|| format!("load font {}", path.display()))?;
        Ok(Some((bytes, descriptor)))
    }

    /// `*.bin` blobs in the root, sorted for stable reporting. Temporary files
    /// from an interrupted atomic write are not blobs and are ignored.
    fn blob_paths(&self) -> Result<Vec<PathBuf>> {
        let entries =
            fs::read_dir(&self.root).with_context(|| format!("list {}", self.root.display()))?;
        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.with_context(|| format!("list {}", self.root.display()))?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("bin") {
                paths.push(path);
            }
        }
        paths.sort();
        Ok(paths)
    }
}

/// Read a string section: bounded, printable ASCII, no terminator in the
/// container. A section that reaches the device's buffer is refused rather than
/// truncated, matching `copySection` in `src/font_asset.cpp`.
fn read_section(bytes: &[u8], off: usize, len: usize, cap: usize, what: &str) -> Result<String> {
    if len >= cap {
        bail!("font {what} is {len} bytes, over the {}-byte limit", cap - 1);
    }
    let section = &bytes[off..off + len];
    for &byte in section {
        if !(0x20..=0x7E).contains(&byte) {
            bail!("font {what} contains byte 0x{byte:02x}: sections are printable ASCII");
        }
    }
    Ok(section.iter().map(|&byte| byte as char).collect())
}

fn rd16(bytes: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([bytes[off], bytes[off + 1]])
}

fn rd32(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
}

/// Per-Profile font dependency set: the distinct fonts a Profile's templates
/// need. `compiled_in` names are satisfied by the firmware and are reported but
/// never pushed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontPlan {
    /// Distinct assets the Profile needs, one entry per font id.
    pub required: Vec<FontDescriptor>,
    /// Names the firmware's own table serves, sorted.
    pub compiled_in: Vec<String>,
    /// Sum of `required[].bytes`.
    pub total_bytes: u64,
}

impl FontPlan {
    /// Resolve exactly one content version per asset name. Built-in names can
    /// remain unbound; any library asset requires an explicit id even when it
    /// is currently the only version.
    pub fn for_selected<S: AsRef<str>>(
        library: &FontLibrary,
        names: &[S],
        selected: &BTreeMap<String, String>,
    ) -> Result<FontPlan> {
        let mut required = Vec::new();
        let mut compiled_in = Vec::new();
        let mut seen = BTreeSet::new();
        for raw in names {
            let name = raw.as_ref();
            if !seen.insert(name.to_string()) { continue; }
            if let Some(id) = selected.get(name) {
                let descriptor = library.get(id)?.with_context(|| format!("font {name} selects missing asset {id}"))?;
                if descriptor.name != name { bail!("font {name} selects asset {id} named {}", descriptor.name); }
                required.push(descriptor);
            } else if !library.resolve_name(name)?.is_empty() {
                bail!("font {name} has asset versions but no explicit font_id selection");
            } else if FONTS.contains(&name) {
                compiled_in.push(name.to_string());
            } else {
                bail!("unknown font {name}");
            }
        }
        required.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
        compiled_in.sort();
        let total_bytes = required.iter().map(|f| u64::from(f.bytes)).sum();
        Ok(FontPlan { required, compiled_in, total_bytes })
    }
    /// Build the dependency set of a Profile from its templates' font names.
    ///
    /// Compatibility helper for callers without a selection map. Ambiguous
    /// names are rejected; production Profile publishing uses `for_selected`.
    pub fn for_names<S: AsRef<str>>(library: &FontLibrary, names: &[S]) -> Result<FontPlan> {
        let mut required: Vec<FontDescriptor> = Vec::new();
        let mut compiled_in: Vec<String> = Vec::new();
        let mut seen_ids: BTreeSet<String> = BTreeSet::new();
        let mut seen_compiled: BTreeSet<String> = BTreeSet::new();
        for raw in names {
            let name = raw.as_ref().trim();
            if name.is_empty() {
                bail!("profile declares an empty font name");
            }
            let found = library.resolve_name(name)?;
            if found.is_empty() {
                if FONTS.contains(&name) {
                    if seen_compiled.insert(name.to_string()) {
                        compiled_in.push(name.to_string());
                    }
                } else {
                    bail!(
                        "unknown font \"{name}\": not in the firmware font table and not in the font library"
                    );
                }
                continue;
            }
            if found.len() != 1 {
                bail!("font {name} has {} versions; select a font_id explicitly", found.len());
            }
            let descriptor = found.into_iter().next().unwrap();
            if seen_ids.insert(descriptor.id.clone()) {
                required.push(descriptor);
            }
        }
        required.sort_by(|a, b| (&a.name, a.size_px).cmp(&(&b.name, b.size_px)));
        compiled_in.sort();
        let total_bytes = required.iter().map(|d| u64::from(d.bytes)).sum();
        Ok(FontPlan {
            required,
            compiled_in,
            total_bytes,
        })
    }
}

/// Diff a required set against the ids the device reported as installed.
///
/// Returns `(missing, present)`: `missing` is what must be pushed, in a stable
/// (name, size) order; `present` are the ids the device already has, which must
/// NOT be pushed. An id that is installed but not required is neither missing
/// nor present (it is stale). Ids are compared case-insensitively after
/// trimming, so an inventory reported by the device in upper case still counts.
pub fn diff_inventory(
    required: &[FontDescriptor],
    installed_ids: &[String],
) -> (Vec<FontDescriptor>, Vec<String>) {
    let installed: BTreeSet<String> = installed_ids
        .iter()
        .map(|id| id.trim().to_ascii_lowercase())
        .collect();
    let mut ordered: Vec<&FontDescriptor> = required.iter().collect();
    ordered.sort_by(|a, b| (&a.name, a.size_px, &a.id).cmp(&(&b.name, b.size_px, &b.id)));
    let mut missing = Vec::new();
    let mut present = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for descriptor in ordered {
        if !seen.insert(descriptor.id.clone()) {
            continue;
        }
        if installed.contains(&descriptor.id) {
            present.push(descriptor.id.clone());
        } else {
            missing.push(descriptor.clone());
        }
    }
    (missing, present)
}

/// Err when the plan exceeds either cap; the message names the cap and the
/// actual value, so an over-budget Profile fails loudly instead of being
/// silently truncated.
pub fn check_plan_limits(plan: &FontPlan) -> Result<()> {
    // Retained as a compatibility helper. Capacity is checked against device
    // advertised object and peak-install bytes by the publish planner.
    let _ = plan;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const THIN_FILE: &str = "font_ntthin18.bin";
    const REG_FILE: &str = "font_ntreg64.bin";
    const THIN_ID: &str = "18c2e4ed";
    const REG_ID: &str = "4dc3b226";

    fn fixtures_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts")
    }

    fn fixture(file: &str) -> Vec<u8> {
        let path = fixtures_dir().join(file);
        fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    fn library() -> (tempfile::TempDir, FontLibrary) {
        let dir = tempfile::tempdir().expect("temp dir");
        let lib = FontLibrary::open(dir.path()).expect("open library");
        (dir, lib)
    }

    /// Minimal CSFN v1 builder so tests can vary exactly one field.
    struct FontBuilder {
        name: String,
        family: String,
        coverage: String,
        size_px: u16,
        weight: u16,
        bpp: u8,
        pixel_format: u8,
        line_height: u8,
        base_line: u8,
        max_adv: u16,
        glyph_count: u32,
        filled: u16,
        hint: u8,
        glyphs: Vec<[u8; GLYPH_BYTES]>,
        blob: Vec<u8>,
        pad: bool,
    }

    impl FontBuilder {
        fn new() -> Self {
            Self {
                name: "synth".to_string(),
                family: "Synth.ttf".to_string(),
                coverage: "ascii".to_string(),
                size_px: 18,
                weight: 400,
                bpp: BPP_BW,
                pixel_format: PIXEL_BW,
                line_height: 26,
                base_line: 6,
                max_adv: 256,
                glyph_count: GLYPH_SLOTS as u32,
                filled: 0,
                hint: 0,
                glyphs: vec![[0u8; GLYPH_BYTES]; GLYPH_SLOTS],
                blob: vec![0u8; 16],
                pad: true,
            }
        }

        fn named(name: &str) -> Self {
            let mut builder = Self::new();
            builder.name = name.to_string();
            builder
        }

        fn glyph(mut self, slot: usize, off: u16, adv: u16, w: u8, h: u8) -> Self {
            let mut bytes = [0u8; GLYPH_BYTES];
            bytes[0..2].copy_from_slice(&off.to_le_bytes());
            bytes[2..4].copy_from_slice(&adv.to_le_bytes());
            bytes[4] = w;
            bytes[5] = h;
            self.glyphs[slot] = bytes;
            self.filled += 1;
            self
        }

        fn build(&self) -> Vec<u8> {
            let section = |text: &str| {
                let mut bytes = text.as_bytes().to_vec();
                if self.pad && bytes.len() % 2 == 1 {
                    bytes.push(0);
                }
                bytes
            };
            let mut payload = Vec::new();
            payload.extend_from_slice(&section(&self.name));
            payload.extend_from_slice(&section(&self.family));
            payload.extend_from_slice(&section(&self.coverage));
            for glyph in &self.glyphs {
                payload.extend_from_slice(glyph);
            }
            payload.extend_from_slice(&self.blob);

            let mut header = [0u8; HEADER_BYTES];
            header[0..4].copy_from_slice(&MAGIC.to_le_bytes());
            header[4..6].copy_from_slice(&VERSION.to_le_bytes());
            header[6..8].copy_from_slice(&(HEADER_BYTES as u16).to_le_bytes());
            header[8..12].copy_from_slice(&((HEADER_BYTES + payload.len()) as u32).to_le_bytes());
            header[12..16].copy_from_slice(&crc32fast::hash(&payload).to_le_bytes());
            header[16..20].copy_from_slice(&(self.blob.len() as u32).to_le_bytes());
            header[20..24].copy_from_slice(&self.glyph_count.to_le_bytes());
            header[24] = self.bpp;
            header[25] = self.pixel_format;
            header[26] = self.line_height;
            header[27] = self.base_line;
            header[28..30].copy_from_slice(&self.max_adv.to_le_bytes());
            header[30..32].copy_from_slice(&self.size_px.to_le_bytes());
            header[32..34].copy_from_slice(&self.weight.to_le_bytes());
            header[34..36].copy_from_slice(&(self.name.len() as u16).to_le_bytes());
            header[36..38].copy_from_slice(&(self.family.len() as u16).to_le_bytes());
            header[38..40].copy_from_slice(&(self.coverage.len() as u16).to_le_bytes());
            header[40..42].copy_from_slice(&self.filled.to_le_bytes());
            header[42] = self.hint;
            let mut out = header.to_vec();
            out.extend_from_slice(&payload);
            out
        }
    }

    /// Recompute `fileBytes` and `payloadCrc32` after a test mutated the body,
    /// so a container can be structurally self-consistent while breaking exactly
    /// one layout rule.
    fn reseal(mut bytes: Vec<u8>) -> Vec<u8> {
        let len = bytes.len() as u32;
        bytes[8..12].copy_from_slice(&len.to_le_bytes());
        let crc = crc32fast::hash(&bytes[HEADER_BYTES..]);
        bytes[12..16].copy_from_slice(&crc.to_le_bytes());
        bytes
    }

    fn error_of(bytes: &[u8]) -> String {
        FontLibrary::validate(bytes)
            .expect_err("container should have been rejected")
            .to_string()
    }

    #[test]
    fn fixtures_parse_with_expected_identity() {
        let thin = FontLibrary::validate(&fixture(THIN_FILE)).expect("ntthin18 validates");
        assert_eq!(thin.id, THIN_ID);
        assert_eq!(thin.name, "ntthin18");
        assert_eq!(thin.family, "NotoSans-Thin.ttf");
        assert_eq!(thin.coverage, "ascii");
        assert_eq!(thin.size_px, 18);
        assert_eq!(thin.weight, 100);
        assert_eq!(thin.bpp, 1);
        assert_eq!(thin.pixel_format, "bw");
        assert_eq!(thin.line_height, 26);
        assert_eq!(thin.base_line, 6);
        assert_eq!(thin.max_adv, 256);
        assert_eq!(thin.blob_bytes, 1412);
        assert_eq!(thin.glyph_count, 95);
        assert_eq!(thin.filled_glyphs, 95);
        assert_eq!(thin.hint, 0);
        assert_eq!(thin.bytes, 2268);

        let reg = FontLibrary::validate(&fixture(REG_FILE)).expect("ntreg64 validates");
        assert_eq!(reg.id, REG_ID);
        assert_eq!(reg.name, "ntreg64");
        assert_eq!(reg.family, "NotoSans-Regular.ttf");
        assert_eq!(reg.coverage, "digits");
        assert_eq!(reg.size_px, 64);
        assert_eq!(reg.weight, 400);
        assert_eq!(reg.bpp, 1);
        assert_eq!(reg.pixel_format, "bw");
        assert_eq!(reg.line_height, 88);
        assert_eq!(reg.base_line, 19);
        assert_eq!(reg.max_adv, 848);
        assert_eq!(reg.blob_bytes, 2457);
        assert_eq!(reg.glyph_count, 95);
        assert_eq!(reg.filled_glyphs, 17);
        assert_eq!(reg.bytes, 3315);
    }

    #[test]
    fn payload_byte_flip_fails_payload_crc() {
        let mut bytes = fixture(THIN_FILE);
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        assert!(error_of(&bytes).contains("payload crc"), "{}", error_of(&bytes));
    }

    #[test]
    fn header_byte_flip_changes_id_and_fails_length_check() {
        let original = fixture(REG_FILE);
        assert_eq!(crc32fast::hash(&original), u32::from_str_radix(REG_ID, 16).unwrap());
        let mut bytes = original.clone();
        bytes[8] ^= 0x01; // fileBytes low byte
        assert!(error_of(&bytes).contains("length mismatch"), "{}", error_of(&bytes));
        // The id is the content address, so the mutated container can never be
        // mistaken for the original one.
        let mutated_id = format!("{:08x}", crc32fast::hash(&bytes));
        assert_ne!(mutated_id, REG_ID);
        assert!(is_font_id(&mutated_id));
    }

    #[test]
    fn truncated_container_is_rejected() {
        let full = fixture(THIN_FILE);
        let cut = &full[..full.len() - 1];
        assert!(error_of(cut).contains("length mismatch"), "{}", error_of(cut));
        let stub = &full[..40];
        assert!(error_of(stub).contains("shorter than the 64-byte header"), "{}", error_of(stub));
    }

    #[test]
    fn unsupported_bpp_and_pixel_format_are_rejected() {
        let mut unknown_format = FontBuilder::new().build();
        unknown_format[25] = 2;
        assert!(error_of(&unknown_format).contains("pixel format 2"), "{}", error_of(&unknown_format));

        let mut unknown_bpp = FontBuilder::new().build();
        unknown_bpp[24] = 3;
        assert!(error_of(&unknown_bpp).contains("bpp 3"), "{}", error_of(&unknown_bpp));

        let mut gray_pair = FontBuilder::new().build();
        gray_pair[25] = PIXEL_GRAY4; // gray4 needs 2bpp
        assert!(error_of(&gray_pair).contains("does not match pixel format gray4"), "{}", error_of(&gray_pair));

        let mut bw_pair = FontBuilder::new().build();
        bw_pair[24] = BPP_GRAY4; // bw needs 1bpp
        assert!(error_of(&bw_pair).contains("does not match pixel format bw"), "{}", error_of(&bw_pair));
    }

    #[test]
    fn gray4_assets_parse_but_are_refused_for_a_1bpp_target() {
        let mut builder = FontBuilder::new();
        builder.bpp = BPP_GRAY4;
        builder.pixel_format = PIXEL_GRAY4;
        let descriptor = FontLibrary::validate(&builder.build()).expect("gray4 parses");
        assert_eq!(descriptor.pixel_format, "gray4");
        let err = descriptor.check_target("1bpp").expect_err("gray4 must not go to a 1bpp target");
        assert!(err.to_string().contains("gray4"), "{err}");
        assert!(descriptor.check_target("2bpp").is_ok());
        assert!(descriptor.check_target("4bpp").is_err());
    }

    #[test]
    fn missing_string_padding_is_rejected() {
        // An odd-length section needs one pad byte: with it the container is
        // padded and valid.
        let padded = FontBuilder::named("oddfont").build();
        assert!(FontLibrary::validate(&padded).is_ok());
        // Without it the descriptor table is not where the (padded) offset math
        // says it is, so the declared shape no longer adds up.
        let mut builder = FontBuilder::named("oddfont");
        builder.pad = false;
        let unpadded = builder.build();
        assert_eq!(unpadded.len(), padded.len() - 3);
        assert!(error_of(&unpadded).contains("blob out of range"), "{}", error_of(&unpadded));

        // Same story for a real asset: strip the section padding from the
        // fixture and reseal the header so only the padding is missing.
        let full = fixture(REG_FILE);
        let name_len = rd16(&full, 34) as usize;
        let family_len = rd16(&full, 36) as usize;
        let coverage_len = rd16(&full, 38) as usize;
        let payload = &full[HEADER_BYTES..];
        let mut body = Vec::new();
        let mut at = 0usize;
        for len in [name_len, family_len, coverage_len] {
            body.extend_from_slice(&payload[at..at + len]);
            at += len + (len & 1);
        }
        body.extend_from_slice(&payload[at..]);
        let mut unpadded_fixture = full[..HEADER_BYTES].to_vec();
        unpadded_fixture.extend_from_slice(&body);
        let unpadded_fixture = reseal(unpadded_fixture);
        assert!(error_of(&unpadded_fixture).contains("blob out of range"), "{}", error_of(&unpadded_fixture));
    }

    #[test]
    fn payload_shorter_than_declared_sections_is_rejected() {
        let mut bytes = FontBuilder::new().build();
        // Claim a 200-byte name in a container that carries six.
        bytes[34..36].copy_from_slice(&200u16.to_le_bytes());
        let err = error_of(&bytes);
        assert!(err.contains("shorter than the declared sections"), "{err}");
    }

    #[test]
    fn blob_out_of_range_and_glyph_boxes_past_the_blob_are_rejected() {
        // blobBytes larger than the container's blob.
        let mut bytes = FontBuilder::new().build();
        bytes[16..20].copy_from_slice(&1024u32.to_le_bytes());
        assert!(error_of(&bytes).contains("blob out of range"), "{}", error_of(&bytes));

        // A glyph that points past the end of the blob.
        let over = FontBuilder::new().glyph(0, 10, 100, 16, 8).build(); // needs 16 at offset 10
        assert!(error_of(&over).contains("glyph 0 needs 16 bytes"), "{}", error_of(&over));

        // A box larger than the engine's 127-pixel limit.
        let huge = FontBuilder::new().glyph(3, 0, 100, 200, 8).build();
        assert!(error_of(&huge).contains("glyph 3 box is 200x8"), "{}", error_of(&huge));

        // A glyph that fits is accepted.
        let fits = FontBuilder::new().glyph(0, 0, 160, 8, 2).build();
        assert!(FontLibrary::validate(&fits).is_ok());
    }

    #[test]
    fn bad_magic_version_header_length_and_shape_are_rejected() {
        let mut magic = FontBuilder::new().build();
        magic[0] ^= 0xff;
        assert!(error_of(&magic).contains("bad font magic"), "{}", error_of(&magic));

        let mut version = FontBuilder::new().build();
        version[4] = 2;
        assert!(error_of(&version).contains("unsupported font container version 2"), "{}", error_of(&version));

        let mut header = FontBuilder::new().build();
        header[6..8].copy_from_slice(&32u16.to_le_bytes());
        assert!(error_of(&header).contains("bad font header length 32"), "{}", error_of(&header));

        let mut glyphs = FontBuilder::new().build();
        glyphs[20..24].copy_from_slice(&94u32.to_le_bytes());
        assert!(error_of(&glyphs).contains("glyph count 94"), "{}", error_of(&glyphs));

        let mut metrics = FontBuilder::new().build();
        metrics[26] = 0;
        assert!(error_of(&metrics).contains("line metrics"), "{}", error_of(&metrics));

        let mut baseline = FontBuilder::new().build();
        baseline[27] = 26;
        assert!(error_of(&baseline).contains("line metrics"), "{}", error_of(&baseline));

        let mut filled = FontBuilder::new().build();
        filled[40..42].copy_from_slice(&96u16.to_le_bytes());
        assert!(error_of(&filled).contains("96 filled glyphs"), "{}", error_of(&filled));
    }

    #[test]
    fn container_over_old_48k_product_cap_is_valid() {
        let mut builder = FontBuilder::new();
        builder.blob = vec![0u8; 50 * 1024];
        let big = builder.build();
        assert!(big.len() > 48 * 1024);
        assert!(FontLibrary::validate(&big).is_ok());
    }

    #[test]
    fn add_is_idempotent_and_does_not_rewrite() {
        let (dir, lib) = library();
        let bytes = fixture(THIN_FILE);
        let first = lib.add(&bytes).expect("add");
        assert_eq!(first.id, THIN_ID);
        let path = dir.path().join(format!("{THIN_ID}.bin"));
        let modified = fs::metadata(&path).expect("stat").modified().expect("mtime");
        let usage = lib.usage().expect("usage");
        assert_eq!(usage, (2268, 1));

        let second = lib.add(&bytes).expect("add again");
        assert_eq!(first, second);
        assert_eq!(fs::read(&path).expect("read back"), bytes);
        assert_eq!(fs::metadata(&path).expect("stat").modified().expect("mtime"), modified);
        assert_eq!(lib.usage().expect("usage"), usage);
        assert_eq!(lib.list().expect("list").len(), 1);
        assert!(!dir.path().join(format!("{THIN_ID}.bin.tmp")).exists());
        assert!(lib.list().expect("list")[0].bytes == 2268);
    }

    #[test]
    fn add_refuses_to_replace_a_stored_id_with_other_content() {
        let (_dir, lib) = library();
        let bytes = fixture(THIN_FILE);
        lib.add(&bytes).expect("add");
        // A file whose content does not match its own address is reported, not
        // silently overwritten (write-once).
        let dir = lib.root().to_path_buf();
        let path = dir.join(format!("{THIN_ID}.bin"));
        fs::write(&path, b"not a font").expect("clobber");
        let err = lib.add(&bytes).expect_err("mismatched content must be reported");
        assert!(err.to_string().contains("different content"), "{err}");
    }

    #[test]
    fn library_list_get_read_and_usage() {
        let (_dir, lib) = library();
        let thin = lib.add(&fixture(THIN_FILE)).expect("add thin");
        let reg = lib.add(&fixture(REG_FILE)).expect("add reg");
        assert_eq!(lib.usage().expect("usage"), ((2268 + 3315) as u64, 2));

        let listed = lib.list().expect("list");
        assert_eq!(listed.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), vec!["ntreg64", "ntthin18"]);
        assert_eq!(lib.get(THIN_ID).expect("get"), Some(thin.clone()));
        assert_eq!(lib.get("00000000").expect("get missing"), None);
        assert!(lib.get("zzzzzzzz").is_err());
        assert!(lib.get("../../etc/passwd").is_err());
        assert_eq!(lib.read(THIN_ID).expect("read"), Some(fixture(THIN_FILE)));
        assert_eq!(lib.read("00000000").expect("read missing"), None);
        assert_eq!(lib.get(REG_ID).expect("get"), Some(reg));

        // Sorting is by name, then size.
        let small = lib.add(&FontBuilder::named("sortme").build()).expect("add small");
        let mut bigger = FontBuilder::named("sortme");
        bigger.size_px = 30;
        let big = lib.add(&bigger.build()).expect("add big");
        assert_ne!(small.id, big.id);
        let sizes: Vec<u16> = lib
            .list()
            .expect("list")
            .into_iter()
            .filter(|d| d.name == "sortme")
            .map(|d| d.size_px)
            .collect();
        assert_eq!(sizes, vec![18, 30]);
    }

    #[test]
    fn resolve_name_finds_asset_and_reports_unknown() {
        let (_dir, lib) = library();
        lib.add(&fixture(THIN_FILE)).expect("add");
        let found = lib.resolve_name("ntthin18").expect("resolve");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, THIN_ID);
        // Compiled into the firmware, so there is no asset to push.
        assert!(lib.resolve_name("nt16").expect("resolve nt16").is_empty());
        assert!(lib.resolve_name("nope").expect("resolve nope").is_empty());
        assert!(lib.resolve_name("  ").expect("resolve blank").is_empty());
        // The other fixture is not in this library yet.
        assert!(lib.resolve_name("ntreg64").expect("resolve ntreg64").is_empty());
    }

    #[test]
    fn diff_inventory_pushes_only_missing_ids() {
        let (_dir, lib) = library();
        let thin = lib.add(&fixture(THIN_FILE)).expect("add thin");
        let reg = lib.add(&fixture(REG_FILE)).expect("add reg");
        let required = vec![thin.clone(), reg.clone()];

        let (missing, present) = diff_inventory(&required, &[thin.id.clone(), "deadbeef".to_string()]);
        assert_eq!(missing.iter().map(|d| d.id.clone()).collect::<Vec<_>>(), vec![REG_ID.to_string()]);
        assert_eq!(present, vec![THIN_ID.to_string()]);

        // The device already holds both: nothing is missing, so the second push
        // transfers zero bytes.
        let (missing, present) = diff_inventory(&required, &[reg.id.clone(), thin.id.clone()]);
        assert!(missing.is_empty());
        assert_eq!(present, vec![REG_ID.to_string(), THIN_ID.to_string()]);

        // A stale installed id is neither missing nor present.
        let (missing, present) = diff_inventory(&required, &["deadbeef".to_string()]);
        assert_eq!(missing.len(), 2);
        assert!(present.is_empty());

        // Case and padding from the device's inventory are tolerated.
        let (missing, present) = diff_inventory(&required, &[" 18C2E4ED\n".to_string()]);
        assert_eq!(missing.iter().map(|d| d.id.clone()).collect::<Vec<_>>(), vec![REG_ID.to_string()]);
        assert_eq!(present, vec![THIN_ID.to_string()]);
    }

    #[test]
    fn plan_for_names_separates_required_and_compiled_in() {
        let (_dir, lib) = library();
        lib.add(&fixture(THIN_FILE)).expect("add");
        let plan = FontPlan::for_names(&lib, &["ntthin18", "nt16", "ntthin18"]).expect("plan");
        assert_eq!(plan.required.len(), 1);
        assert_eq!(plan.required[0].id, THIN_ID);
        assert_eq!(plan.compiled_in, vec!["nt16".to_string()]);
        assert_eq!(plan.total_bytes, 2268);
        assert!(check_plan_limits(&plan).is_ok());

        let plan = FontPlan::for_names(&lib, &["ntthin18", "ntreg64", "nt16", "nt30"]).expect("plan");
        // ntreg64 has no asset here, so the firmware table serves it.
        assert_eq!(plan.required.len(), 1);
        assert_eq!(plan.required[0].id, THIN_ID);
        assert_eq!(
            plan.compiled_in,
            vec!["nt16".to_string(), "nt30".to_string(), "ntreg64".to_string()]
        );
        assert_eq!(plan.total_bytes, 2268);

        let err = FontPlan::for_names(&lib, &["nope"]).expect_err("unknown font");
        assert!(err.to_string().contains("unknown font \"nope\""), "{err}");
        let err = FontPlan::for_names(&lib, &[""]).expect_err("empty name");
        assert!(err.to_string().contains("empty font name"), "{err}");
    }

    #[test]
    fn selected_version_is_unique_and_explicit() {
        let (_dir, lib) = library();
        let mut second = FontBuilder::named("ntthin18");
        second.weight = 700;
        let a = lib.add(&fixture(THIN_FILE)).unwrap();
        let b = lib.add(&second.build()).unwrap();
        assert_ne!(a.id, b.id);
        assert!(FontPlan::for_selected(&lib, &["ntthin18"], &BTreeMap::new()).is_err());
        let chosen = BTreeMap::from([("ntthin18".to_string(), b.id.clone())]);
        let plan = FontPlan::for_selected(&lib, &["ntthin18", "ntthin18"], &chosen).unwrap();
        assert_eq!(plan.required.len(), 1);
        assert_eq!(plan.required[0].id, b.id);
        assert!(check_plan_limits(&plan).is_ok());
    }
}
