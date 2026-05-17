//! mzML reader.
//!
//! Strategy:
//! - Read the entire file into a byte buffer (acceptable for typical
//!   mzML sizes; can be revisited for streaming if needed).
//! - Capture the **prefix** verbatim: everything from the start of the
//!   file up to (but not including) the opening `<spectrumList>` tag.
//! - Capture the **inter** block verbatim: anything that appears
//!   between `</spectrumList>` and `<chromatogramList>` (typically
//!   empty / just whitespace, but preserved exactly).
//! - Capture the **suffix** verbatim: everything from after the last
//!   close (`</spectrumList>` or `</chromatogramList>`) through
//!   `</mzML>`. The indexedmzML index block is stripped so the writer
//!   can regenerate it.
//! - Parse each `<spectrum>` element into a [`Spectrum`] with peak
//!   arrays decoded and CV/userParams captured as a JSON blob.
//! - Parse each `<chromatogram>` element into a [`Chromatogram`].
//!
//! The verbatim parts are stored in [`Run::run_metadata`] as a JSON
//! blob with `prefix`, `inter`, and `suffix` keys so the writer can
//! reassemble a byte-identical document outside the spectrum and
//! chromatogram lists.

use std::io::Read as _;
use std::path::Path;

use base64::Engine;
use flate2::read::ZlibDecoder;
use prolance_core::{Chromatogram, Precursor, Run, Spectrum};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

use crate::error::{MsError, MsResult};

/// Container returned by [`read_mzml`].
#[derive(Debug, Default)]
pub struct MzmlData {
    pub run: Run,
    pub spectra: Vec<Spectrum>,
    pub chromatograms: Vec<Chromatogram>,
}

/// Verbatim XML blocks captured from the source mzML.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Verbatim {
    pub prefix: String,
    pub spectrum_list_open: String,
    pub inter: String,
    pub chromatogram_list_open: Option<String>,
    pub suffix: String,
    pub indexed: bool,
}

/// Parse an mzML file into a [`MzmlData`] bundle.
pub fn read_mzml<P: AsRef<Path>>(path: P) -> MsResult<MzmlData> {
    let path = path.as_ref();
    let bytes = std::fs::read(path)?;
    parse_bytes(&bytes, path.to_string_lossy().to_string())
}

/// Parse mzML bytes directly (useful for tests).
pub fn parse_bytes(bytes: &[u8], source_path: String) -> MsResult<MzmlData> {
    let mut verbatim = Verbatim {
        indexed: looks_indexed(bytes),
        ..Default::default()
    };

    let spec_list_open_start = find_tag_open(bytes, b"spectrumList")
        .ok_or_else(|| MsError::Malformed("no <spectrumList> found".into()))?;
    let spec_list_open_end = find_tag_close(bytes, spec_list_open_start)
        .ok_or_else(|| MsError::Malformed("malformed <spectrumList> open".into()))?;
    verbatim.prefix = strip_indexed_prefix(&bytes[..spec_list_open_start]);
    verbatim.spectrum_list_open =
        String::from_utf8_lossy(&bytes[spec_list_open_start..=spec_list_open_end]).into_owned();

    let spec_list_close = find_subslice(bytes, b"</spectrumList>", spec_list_open_end + 1)
        .ok_or_else(|| MsError::Malformed("no </spectrumList> found".into()))?;
    let after_spec_list = spec_list_close + b"</spectrumList>".len();

    let chrom_list_open_start = find_tag_open_after(bytes, b"chromatogramList", after_spec_list);
    let inter_end = chrom_list_open_start.unwrap_or(after_spec_list);
    verbatim.inter = String::from_utf8_lossy(&bytes[after_spec_list..inter_end]).into_owned();

    if let Some(cls) = chrom_list_open_start {
        let cls_end = find_tag_close(bytes, cls)
            .ok_or_else(|| MsError::Malformed("malformed <chromatogramList> open".into()))?;
        verbatim.chromatogram_list_open =
            Some(String::from_utf8_lossy(&bytes[cls..=cls_end]).into_owned());
        let chrom_close = find_subslice(bytes, b"</chromatogramList>", cls_end + 1)
            .ok_or_else(|| MsError::Malformed("no </chromatogramList> found".into()))?;
        let after_chrom = chrom_close + b"</chromatogramList>".len();
        verbatim.suffix = strip_indexed_suffix(&bytes[after_chrom..]);
    } else {
        verbatim.suffix = strip_indexed_suffix(&bytes[after_spec_list..]);
    }

    let run_id = derive_run_id(&source_path, bytes.len() as u64);
    let mut run = Run {
        run_id: run_id.clone(),
        source_path: Some(source_path.clone()),
        source_format: "mzml".into(),
        ingested_at: Some(chrono::Utc::now().to_rfc3339()),
        ..Default::default()
    };
    extract_run_attrs(&verbatim.prefix, &mut run);

    let spectra = parse_spectra(
        &bytes[spec_list_open_end + 1..spec_list_close],
        &run_id,
    )?;
    let chromatograms = if let Some(cls) = chrom_list_open_start {
        let cls_end = find_tag_close(bytes, cls).unwrap();
        let chrom_close = find_subslice(bytes, b"</chromatogramList>", cls_end + 1).unwrap();
        parse_chromatograms(&bytes[cls_end + 1..chrom_close], &run_id)?
    } else {
        Vec::new()
    };

    let mut ms1 = 0u32;
    let mut ms2 = 0u32;
    for s in &spectra {
        if s.ms_level == 1 {
            ms1 += 1;
        } else if s.ms_level >= 2 {
            ms2 += 1;
        }
    }
    run.spectrum_count = Some(spectra.len() as u32);
    run.ms1_count = Some(ms1);
    run.ms2_count = Some(ms2);
    run.run_metadata = Some(serde_json::to_string(&verbatim)?);

    Ok(MzmlData {
        run,
        spectra,
        chromatograms,
    })
}

// ── verbatim helpers ─────────────────────────────────────────────────────────

fn looks_indexed(bytes: &[u8]) -> bool {
    memchr::memmem::find(&bytes[..bytes.len().min(4096)], b"<indexedmzML").is_some()
}

fn strip_indexed_prefix(prefix: &[u8]) -> String {
    let s = String::from_utf8_lossy(prefix).into_owned();
    if let Some(idx) = s.find("<indexedmzML") {
        if let Some(end) = s[idx..].find('>') {
            let mut out = String::with_capacity(s.len());
            out.push_str(&s[..idx]);
            out.push_str(&s[idx + end + 1..]);
            return trim_leading_ws_keeping_xml_decl(&out);
        }
    }
    s
}

fn trim_leading_ws_keeping_xml_decl(s: &str) -> String {
    // Drop a single leading newline if present (from the indexedmzML strip).
    let trimmed = s.trim_start_matches(|c: char| c == '\n' || c == '\r');
    trimmed.to_string()
}

fn strip_indexed_suffix(suffix: &[u8]) -> String {
    let s = String::from_utf8_lossy(suffix).into_owned();
    if let Some(idx) = s.find("<indexList") {
        let before = &s[..idx];
        return before.trim_end().to_string();
    }
    if let Some(close) = s.find("</mzML>") {
        return s[..close + "</mzML>".len()].to_string();
    }
    s
}

fn find_subslice(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    memchr::memmem::find(&haystack[from..], needle).map(|i| from + i)
}

fn find_tag_open(bytes: &[u8], name: &[u8]) -> Option<usize> {
    find_tag_open_after(bytes, name, 0)
}

fn find_tag_open_after(bytes: &[u8], name: &[u8], from: usize) -> Option<usize> {
    let mut pos = from;
    while pos < bytes.len() {
        let i = memchr::memmem::find(&bytes[pos..], b"<")?;
        let abs = pos + i;
        let after = &bytes[abs + 1..];
        if after.starts_with(name) {
            let next = after.get(name.len()).copied();
            if matches!(
                next,
                Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') | Some(b'>') | Some(b'/')
            ) {
                return Some(abs);
            }
        }
        pos = abs + 1;
    }
    None
}

fn find_tag_close(bytes: &[u8], open: usize) -> Option<usize> {
    memchr::memchr(b'>', &bytes[open..]).map(|i| open + i)
}

fn derive_run_id(source_path: &str, size: u64) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(source_path.as_bytes());
    h.update(size.to_le_bytes());
    let d = h.finalize();
    let mut out = String::with_capacity(16);
    for b in &d[..8] {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

fn extract_run_attrs(prefix: &str, run: &mut Run) {
    if let Some(idx) = prefix.find("<run") {
        let chunk = &prefix[idx..];
        if let Some(end) = chunk.find('>') {
            let attrs = &chunk[..end];
            if let Some(v) = attr_value(attrs, "startTimeStamp") {
                run.start_time = Some(v);
            }
        }
    }
    if let Some(v) = attr_value_search(prefix, "instrument model") {
        run.instrument = Some(v);
    }
}

fn attr_value(haystack: &str, name: &str) -> Option<String> {
    let needle = format!("{}=\"", name);
    let i = haystack.find(&needle)?;
    let start = i + needle.len();
    let end = haystack[start..].find('"')?;
    Some(haystack[start..start + end].to_string())
}

fn attr_value_search(haystack: &str, name_attr: &str) -> Option<String> {
    let probe = format!("name=\"{}\"", name_attr);
    let i = haystack.find(&probe)?;
    let region_start = i.saturating_sub(200);
    let region_end = (i + probe.len() + 200).min(haystack.len());
    let region = &haystack[region_start..region_end];
    attr_value(region, "value")
}

// ── structured parsing ──────────────────────────────────────────────────────

fn parse_spectra(body: &[u8], run_id: &str) -> MsResult<Vec<Spectrum>> {
    let mut out = Vec::new();
    let body_str = std::str::from_utf8(body).map_err(|e| MsError::Malformed(e.to_string()))?;
    let mut pos = 0;
    while let Some(rel) = body_str[pos..].find("<spectrum ") {
        let start = pos + rel;
        let close_rel = body_str[start..]
            .find("</spectrum>")
            .ok_or_else(|| MsError::Malformed("no </spectrum>".into()))?;
        let elem_end = start + close_rel + "</spectrum>".len();
        out.push(parse_one_spectrum(&body_str[start..elem_end], run_id)?);
        pos = elem_end;
    }
    Ok(out)
}

fn parse_one_spectrum(xml: &str, run_id: &str) -> MsResult<Spectrum> {
    let mut spec = Spectrum {
        run_id: run_id.to_string(),
        ..Default::default()
    };
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut current_binary: Option<BinaryArrayState> = None;
    let mut precursor = Precursor::default();
    let mut have_precursor = false;
    let mut extra_cv: Vec<CvParam> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) => {
                let name = bytes_to_string(e.name().as_ref());
                if stack.is_empty() && name == "spectrum" {
                    parse_spectrum_attrs(&e, &mut spec)?;
                }
                if name == "binaryDataArray" {
                    current_binary = Some(BinaryArrayState::default());
                }
                stack.push(name);
            }
            Event::End(e) => {
                let name = bytes_to_string(e.name().as_ref());
                if name == "binaryDataArray" {
                    if let Some(state) = current_binary.take() {
                        state.apply_spectrum(&mut spec)?;
                    }
                }
                stack.pop();
            }
            Event::Empty(e) => {
                let name = bytes_to_string(e.name().as_ref());
                if name == "cvParam" || name == "userParam" {
                    let cv = parse_cv(&e, &name)?;
                    apply_cv_to_spectrum(
                        &cv,
                        &stack,
                        &mut spec,
                        &mut precursor,
                        &mut have_precursor,
                        current_binary.as_mut(),
                        &mut extra_cv,
                    );
                }
            }
            Event::Text(t) => {
                if stack.last().map(|s| s.as_str()) == Some("binary") {
                    if let Some(state) = current_binary.as_mut() {
                        let txt = t.unescape().unwrap_or_default().to_string();
                        state.base64.push_str(txt.trim());
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    if have_precursor {
        spec.precursor = Some(precursor);
    }
    if !extra_cv.is_empty() {
        spec.cv_params = Some(serde_json::to_string(&extra_cv)?);
    }
    Ok(spec)
}

fn parse_spectrum_attrs(e: &BytesStart, spec: &mut Spectrum) -> MsResult<()> {
    for attr in e.attributes() {
        let attr = attr?;
        let key = bytes_to_string(attr.key.as_ref());
        let val = attr.unescape_value()?.to_string();
        match key.as_str() {
            "id" => spec.native_id = Some(val),
            "index" => {
                spec.scan_num = val.parse::<u32>().unwrap_or(0).saturating_add(1);
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct CvParam {
    section: String,
    kind: String,
    accession: Option<String>,
    name: Option<String>,
    value: Option<String>,
    unit_accession: Option<String>,
    unit_name: Option<String>,
    unit_cv_ref: Option<String>,
    cv_ref: Option<String>,
}

impl CvParam {
    fn with_section(mut self, section: &str) -> Self {
        self.section = section.to_string();
        self
    }
}

fn parse_cv(e: &BytesStart, kind: &str) -> MsResult<CvParam> {
    let mut cv = CvParam {
        kind: if kind == "cvParam" { "cv".into() } else { "user".into() },
        ..Default::default()
    };
    for attr in e.attributes() {
        let attr = attr?;
        let key = bytes_to_string(attr.key.as_ref());
        let val = attr.unescape_value()?.to_string();
        match key.as_str() {
            "accession" => cv.accession = Some(val),
            "name" => cv.name = Some(val),
            "value" => cv.value = Some(val),
            "unitAccession" => cv.unit_accession = Some(val),
            "unitName" => cv.unit_name = Some(val),
            "unitCvRef" => cv.unit_cv_ref = Some(val),
            "cvRef" => cv.cv_ref = Some(val),
            _ => {}
        }
    }
    Ok(cv)
}

fn apply_cv_to_spectrum(
    cv: &CvParam,
    stack: &[String],
    spec: &mut Spectrum,
    precursor: &mut Precursor,
    have_precursor: &mut bool,
    binary: Option<&mut BinaryArrayState>,
    extra: &mut Vec<CvParam>,
) {
    let acc = cv.accession.as_deref().unwrap_or("");
    let val = cv.value.as_deref();
    let context = stack.last().map(|s| s.as_str()).unwrap_or("");

    if let Some(b) = binary {
        if context == "binaryDataArray" {
            match acc {
                "MS:1000514" => b.kind = BinaryKind::Mz,
                "MS:1000515" => b.kind = BinaryKind::Intensity,
                "MS:1000523" => b.precision = 64,
                "MS:1000521" => b.precision = 32,
                "MS:1000574" => b.zlib = true,
                "MS:1002312" | "MS:1002313" | "MS:1002314" | "MS:1002746" | "MS:1002747"
                | "MS:1002748" => {
                    b.unsupported = Some(acc.to_string());
                }
                _ => {}
            }
            return;
        }
    }

    if context == "spectrum" {
        match acc {
            "MS:1000511" => {
                if let Some(v) = val.and_then(|s| s.parse::<u8>().ok()) {
                    spec.ms_level = v;
                }
                return;
            }
            "MS:1000285" => {
                spec.tic = val.and_then(|s| s.parse().ok());
                return;
            }
            "MS:1000504" => {
                spec.base_peak_mz = val.and_then(|s| s.parse().ok());
                return;
            }
            "MS:1000505" => {
                spec.base_peak_intensity = val.and_then(|s| s.parse().ok());
                return;
            }
            "MS:1000130" => {
                spec.polarity = Some(1);
                return;
            }
            "MS:1000129" => {
                spec.polarity = Some(-1);
                return;
            }
            "MS:1000127" => {
                spec.centroided = Some(true);
                return;
            }
            "MS:1000128" => {
                spec.centroided = Some(false);
                return;
            }
            _ => {}
        }
    }

    if context == "scan" && acc == "MS:1000016" {
        if let Some(v) = val.and_then(|s| s.parse::<f64>().ok()) {
            let unit = cv.unit_accession.as_deref().unwrap_or("");
            spec.rt = Some(if unit == "UO:0000031" || unit == "MS:1000038" {
                v * 60.0
            } else {
                v
            });
        }
        return;
    }
    if context == "scan" && acc == "MS:1002476" {
        spec.inv_mobility = val.and_then(|s| s.parse().ok());
        return;
    }

    if context == "scanWindow" {
        match acc {
            "MS:1000501" => spec.scan_window_lower = val.and_then(|s| s.parse().ok()),
            "MS:1000500" => spec.scan_window_upper = val.and_then(|s| s.parse().ok()),
            _ => extra.push(cv.clone().with_section(context)),
        }
        return;
    }

    if context == "isolationWindow" {
        *have_precursor = true;
        match acc {
            "MS:1000827" => precursor.isolation_window_target = val.and_then(|s| s.parse().ok()),
            "MS:1000828" => precursor.isolation_window_lower = val.and_then(|s| s.parse().ok()),
            "MS:1000829" => precursor.isolation_window_upper = val.and_then(|s| s.parse().ok()),
            _ => extra.push(cv.clone().with_section(context)),
        }
        return;
    }

    if context == "selectedIon" {
        *have_precursor = true;
        match acc {
            "MS:1000744" => precursor.mz = val.and_then(|s| s.parse().ok()),
            "MS:1000041" => precursor.charge = val.and_then(|s| s.parse().ok()),
            "MS:1000042" => precursor.intensity = val.and_then(|s| s.parse().ok()),
            _ => extra.push(cv.clone().with_section(context)),
        }
        return;
    }

    if context == "activation" {
        *have_precursor = true;
        match acc {
            "MS:1000133" => spec.activation = Some("CID".into()),
            "MS:1000422" => spec.activation = Some("HCD".into()),
            "MS:1000598" => spec.activation = Some("ETD".into()),
            "MS:1000599" => spec.activation = Some("PQD".into()),
            "MS:1000435" => spec.activation = Some("MPD".into()),
            "MS:1000045" => {
                spec.collision_energy = val.and_then(|s| s.parse().ok());
            }
            _ => extra.push(cv.clone().with_section(context)),
        }
        return;
    }

    extra.push(cv.clone().with_section(context));
}

#[derive(Debug, Default)]
struct BinaryArrayState {
    kind: BinaryKind,
    precision: u8,
    zlib: bool,
    base64: String,
    unsupported: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum BinaryKind {
    #[default]
    Other,
    Mz,
    Intensity,
}

impl BinaryArrayState {
    fn apply_spectrum(self, spec: &mut Spectrum) -> MsResult<()> {
        if let Some(u) = &self.unsupported {
            return Err(MsError::Unsupported(format!(
                "compression codec {} not yet supported",
                u
            )));
        }
        if matches!(self.kind, BinaryKind::Other) {
            return Ok(());
        }
        let raw = base64::engine::general_purpose::STANDARD.decode(self.base64.trim().as_bytes())?;
        let bytes = if self.zlib {
            let mut d = ZlibDecoder::new(&raw[..]);
            let mut out = Vec::with_capacity(raw.len() * 4);
            d.read_to_end(&mut out)?;
            out
        } else {
            raw
        };
        let precision = if self.precision == 0 { 64 } else { self.precision };
        match self.kind {
            BinaryKind::Mz => {
                spec.mz = decode_floats_f64(&bytes, precision)?;
                spec.mz_precision = Some(precision);
            }
            BinaryKind::Intensity => {
                spec.intensity = decode_floats_f32(&bytes, precision)?;
                spec.intensity_precision = Some(precision);
            }
            BinaryKind::Other => {}
        }
        Ok(())
    }

    fn apply_chromatogram(self, chrom: &mut Chromatogram) -> MsResult<()> {
        if self.unsupported.is_some() {
            return Err(MsError::Unsupported("compression not supported".into()));
        }
        let raw = base64::engine::general_purpose::STANDARD.decode(self.base64.trim().as_bytes())?;
        let bytes = if self.zlib {
            let mut d = ZlibDecoder::new(&raw[..]);
            let mut out = Vec::with_capacity(raw.len() * 4);
            d.read_to_end(&mut out)?;
            out
        } else {
            raw
        };
        let precision = if self.precision == 0 { 64 } else { self.precision };
        match self.kind {
            BinaryKind::Mz => chrom.time = decode_floats_f32(&bytes, precision)?,
            BinaryKind::Intensity => chrom.intensity = decode_floats_f32(&bytes, precision)?,
            BinaryKind::Other => {}
        }
        Ok(())
    }
}

fn decode_floats_f64(bytes: &[u8], precision: u8) -> MsResult<Vec<f64>> {
    match precision {
        64 => {
            if bytes.len() % 8 != 0 {
                return Err(MsError::Malformed("f64 array length not multiple of 8".into()));
            }
            Ok(bytes
                .chunks_exact(8)
                .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
                .collect())
        }
        32 => {
            if bytes.len() % 4 != 0 {
                return Err(MsError::Malformed("f32 array length not multiple of 4".into()));
            }
            Ok(bytes
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes(c.try_into().unwrap()) as f64)
                .collect())
        }
        _ => Err(MsError::Malformed(format!("unknown precision {}", precision))),
    }
}

fn decode_floats_f32(bytes: &[u8], precision: u8) -> MsResult<Vec<f32>> {
    match precision {
        64 => {
            if bytes.len() % 8 != 0 {
                return Err(MsError::Malformed("f64 array length not multiple of 8".into()));
            }
            Ok(bytes
                .chunks_exact(8)
                .map(|c| f64::from_le_bytes(c.try_into().unwrap()) as f32)
                .collect())
        }
        32 => {
            if bytes.len() % 4 != 0 {
                return Err(MsError::Malformed("f32 array length not multiple of 4".into()));
            }
            Ok(bytes
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                .collect())
        }
        _ => Err(MsError::Malformed(format!("unknown precision {}", precision))),
    }
}

fn bytes_to_string(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

// ── chromatograms ────────────────────────────────────────────────────────────

fn parse_chromatograms(body: &[u8], run_id: &str) -> MsResult<Vec<Chromatogram>> {
    let mut out = Vec::new();
    let s = std::str::from_utf8(body).map_err(|e| MsError::Malformed(e.to_string()))?;
    let mut pos = 0;
    while let Some(rel) = s[pos..].find("<chromatogram ") {
        let start = pos + rel;
        let close_rel = s[start..]
            .find("</chromatogram>")
            .ok_or_else(|| MsError::Malformed("no </chromatogram>".into()))?;
        let elem_end = start + close_rel + "</chromatogram>".len();
        out.push(parse_one_chromatogram(&s[start..elem_end], run_id)?);
        pos = elem_end;
    }
    Ok(out)
}

fn parse_one_chromatogram(xml: &str, run_id: &str) -> MsResult<Chromatogram> {
    let mut chrom = Chromatogram {
        run_id: run_id.to_string(),
        ..Default::default()
    };
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut current_binary: Option<BinaryArrayState> = None;
    let mut stack: Vec<String> = Vec::new();
    let mut extra_cv: Vec<CvParam> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) => {
                let name = bytes_to_string(e.name().as_ref());
                if name == "chromatogram" {
                    for attr in e.attributes() {
                        let attr = attr?;
                        if attr.key.as_ref() == b"id" {
                            chrom.chrom_id = attr.unescape_value()?.to_string();
                        }
                    }
                } else if name == "binaryDataArray" {
                    current_binary = Some(BinaryArrayState::default());
                }
                stack.push(name);
            }
            Event::End(e) => {
                let name = bytes_to_string(e.name().as_ref());
                if name == "binaryDataArray" {
                    if let Some(state) = current_binary.take() {
                        state.apply_chromatogram(&mut chrom)?;
                    }
                }
                stack.pop();
            }
            Event::Empty(e) => {
                let name = bytes_to_string(e.name().as_ref());
                if name == "cvParam" || name == "userParam" {
                    let cv = parse_cv(&e, &name)?;
                    let context = stack.last().map(|s| s.as_str()).unwrap_or("");
                    let acc = cv.accession.as_deref().unwrap_or("");
                    let recognised_type = match acc {
                        "MS:1000235" => {
                            chrom.chrom_type = Some("TIC".into());
                            true
                        }
                        "MS:1000627" => {
                            chrom.chrom_type = Some("SIC".into());
                            true
                        }
                        "MS:1000628" => {
                            chrom.chrom_type = Some("BPC".into());
                            true
                        }
                        "MS:1001473" => {
                            chrom.chrom_type = Some("SRM".into());
                            true
                        }
                        _ => false,
                    };
                    if let Some(b) = current_binary.as_mut() {
                        if context == "binaryDataArray" {
                            match acc {
                                "MS:1000595" => b.kind = BinaryKind::Mz, // time
                                "MS:1000515" => b.kind = BinaryKind::Intensity,
                                "MS:1000523" => b.precision = 64,
                                "MS:1000521" => b.precision = 32,
                                "MS:1000574" => b.zlib = true,
                                _ => extra_cv.push(cv.clone().with_section(context)),
                            }
                            continue;
                        }
                    }
                    if !recognised_type {
                        extra_cv.push(cv.with_section(context));
                    }
                }
            }
            Event::Text(t) => {
                if stack.last().map(|s| s.as_str()) == Some("binary") {
                    if let Some(b) = current_binary.as_mut() {
                        let txt = t.unescape().unwrap_or_default().to_string();
                        b.base64.push_str(txt.trim());
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    if !extra_cv.is_empty() {
        chrom.cv_params = Some(serde_json::to_string(&extra_cv)?);
    }
    Ok(chrom)
}
