//! mzML writer.
//!
//! Replays the verbatim prefix / inter / suffix blocks captured by the
//! reader and rebuilds the spectrum and chromatogram lists from the
//! structured records, restoring CV/userParams from the JSON blob
//! captured at parse time.
//!
//! If the source document was wrapped in `<indexedmzML>`, the writer
//! regenerates the wrapper, byte-offset index, and SHA-1 fileChecksum.

use std::io::{Cursor, Write};

use base64::Engine;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use prolance_core::{Chromatogram, Run, Spectrum};
use serde::Deserialize;
use sha1::{Digest, Sha1};

use crate::error::{MsError, MsResult};
use crate::mzml::reader::Verbatim;

#[derive(Debug, Deserialize, Clone, Default)]
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

/// Serialize a run + its spectra + chromatograms to mzML.
pub fn write_mzml<W: Write>(
    out: &mut W,
    run: &Run,
    spectra: &[Spectrum],
    chromatograms: &[Chromatogram],
) -> MsResult<()> {
    let verbatim: Verbatim = match run.run_metadata.as_deref() {
        Some(s) => serde_json::from_str(s)?,
        None => Verbatim::default(),
    };

    // Build the body (everything inside indexedmzML, if applicable) in
    // memory so we can compute the index offsets and the SHA-1 checksum.
    let mut body: Vec<u8> = Vec::new();
    let prefix = if verbatim.prefix.is_empty() {
        default_prefix(run, spectra)
    } else {
        verbatim.prefix.clone()
    };
    body.extend_from_slice(prefix.as_bytes());
    ensure_trailing_newline(&mut body);

    let spec_list_open = if verbatim.spectrum_list_open.is_empty() {
        format!(
            "    <spectrumList count=\"{}\" defaultDataProcessingRef=\"prolance\">",
            spectra.len()
        )
    } else {
        update_count_attr(&verbatim.spectrum_list_open, spectra.len())
    };
    body.extend_from_slice(spec_list_open.as_bytes());
    body.push(b'\n');

    let mut spectrum_offsets: Vec<(String, usize)> = Vec::with_capacity(spectra.len());
    for (i, s) in spectra.iter().enumerate() {
        let offset = body.len();
        let id = s
            .native_id
            .clone()
            .unwrap_or_else(|| format!("scan={}", i + 1));
        spectrum_offsets.push((id, offset));
        write_spectrum(&mut body, s, i)?;
    }
    body.extend_from_slice(b"    </spectrumList>\n");

    body.extend_from_slice(verbatim.inter.as_bytes());

    let mut chromatogram_offsets: Vec<(String, usize)> = Vec::with_capacity(chromatograms.len());
    if !chromatograms.is_empty() {
        let cl_open = verbatim
            .chromatogram_list_open
            .as_deref()
            .map(|s| update_count_attr(s, chromatograms.len()))
            .unwrap_or_else(|| {
                format!(
                    "    <chromatogramList count=\"{}\" defaultDataProcessingRef=\"prolance\">",
                    chromatograms.len()
                )
            });
        body.extend_from_slice(cl_open.as_bytes());
        body.push(b'\n');
        for (i, c) in chromatograms.iter().enumerate() {
            let offset = body.len();
            chromatogram_offsets.push((c.chrom_id.clone(), offset));
            write_chromatogram(&mut body, c, i)?;
        }
        body.extend_from_slice(b"    </chromatogramList>\n");
    }

    let suffix = if verbatim.suffix.is_empty() {
        "  </run>\n</mzML>".to_string()
    } else {
        verbatim.suffix.clone()
    };
    body.extend_from_slice(suffix.as_bytes());
    ensure_trailing_newline(&mut body);

    if verbatim.indexed {
        write_indexed(out, &body, &spectrum_offsets, &chromatogram_offsets)?;
    } else {
        out.write_all(&body)?;
    }
    Ok(())
}

fn ensure_trailing_newline(buf: &mut Vec<u8>) {
    if buf.last().copied() != Some(b'\n') {
        buf.push(b'\n');
    }
}

fn update_count_attr(tag: &str, n: usize) -> String {
    if let Some(start) = tag.find("count=\"") {
        let after = start + "count=\"".len();
        if let Some(end_rel) = tag[after..].find('"') {
            let mut s = String::with_capacity(tag.len() + 4);
            s.push_str(&tag[..after]);
            s.push_str(&n.to_string());
            s.push_str(&tag[after + end_rel..]);
            return s;
        }
    }
    tag.to_string()
}

fn default_prefix(run: &Run, spectra: &[Spectrum]) -> String {
    let id = run.run_id.clone();
    let start = run.start_time.as_deref().unwrap_or("");
    let _ = spectra;
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <mzML xmlns=\"http://psi.hupo.org/ms/mzml\" version=\"1.1.0\" id=\"{id}\">\n  \
           <cvList count=\"2\">\n    \
             <cv id=\"MS\" fullName=\"Mass spectrometry ontology\" version=\"4.1\" URI=\"https://raw.githubusercontent.com/HUPO-PSI/psi-ms-CV/master/psi-ms.obo\"/>\n    \
             <cv id=\"UO\" fullName=\"Unit Ontology\" version=\"09:04:2014\" URI=\"https://raw.githubusercontent.com/bio-ontology-research-group/unit-ontology/master/unit.obo\"/>\n  \
           </cvList>\n  \
           <fileDescription><fileContent/></fileDescription>\n  \
           <softwareList count=\"1\"><software id=\"prolance\" version=\"0.1.0\"><cvParam cvRef=\"MS\" accession=\"MS:1000799\" name=\"custom unreleased software tool\" value=\"prolance\"/></software></softwareList>\n  \
           <instrumentConfigurationList count=\"1\"><instrumentConfiguration id=\"IC1\"><componentList count=\"3\"><source order=\"1\"/><analyzer order=\"2\"/><detector order=\"3\"/></componentList></instrumentConfiguration></instrumentConfigurationList>\n  \
           <dataProcessingList count=\"1\"><dataProcessing id=\"prolance\"><processingMethod order=\"0\" softwareRef=\"prolance\"><cvParam cvRef=\"MS\" accession=\"MS:1000544\" name=\"Conversion to mzML\"/></processingMethod></dataProcessing></dataProcessingList>\n  \
           <run id=\"{id}\" defaultInstrumentConfigurationRef=\"IC1\" startTimeStamp=\"{start}\">\n"
    )
}

// ── spectrum / chromatogram emission ────────────────────────────────────────

fn write_spectrum<W: Write>(out: &mut W, s: &Spectrum, idx: usize) -> MsResult<()> {
    let id = s
        .native_id
        .clone()
        .unwrap_or_else(|| format!("scan={}", idx + 1));
    let n = s.mz.len().max(s.intensity.len());
    write!(
        out,
        "      <spectrum index=\"{}\" id=\"{}\" defaultArrayLength=\"{}\">\n",
        idx,
        xml_escape(&id),
        n
    )?;

    let extra: Vec<CvParam> = match s.cv_params.as_deref() {
        Some(j) => serde_json::from_str(j).unwrap_or_default(),
        None => Vec::new(),
    };

    // spectrum-level cv params
    write_spectrum_top_cv(out, s)?;
    write_extras(out, &extra, "spectrum")?;

    // scanList
    write!(out, "        <scanList count=\"1\">\n")?;
    write_cv(out, "MS:1000795", "no combination", None, None)?;
    write!(out, "          <scan>\n")?;
    if let Some(rt) = s.rt {
        write!(
            out,
            "            <cvParam cvRef=\"MS\" accession=\"MS:1000016\" name=\"scan start time\" value=\"{}\" unitCvRef=\"UO\" unitAccession=\"UO:0000010\" unitName=\"second\"/>\n",
            rt
        )?;
    }
    if let Some(im) = s.inv_mobility {
        write!(
            out,
            "            <cvParam cvRef=\"MS\" accession=\"MS:1002476\" name=\"ion mobility drift time\" value=\"{}\"/>\n",
            im
        )?;
    }
    write_extras(out, &extra, "scan")?;
    if s.scan_window_lower.is_some() || s.scan_window_upper.is_some() {
        write!(out, "            <scanWindowList count=\"1\"><scanWindow>\n")?;
        if let Some(v) = s.scan_window_lower {
            write!(
                out,
                "              <cvParam cvRef=\"MS\" accession=\"MS:1000501\" name=\"scan window lower limit\" value=\"{}\" unitCvRef=\"MS\" unitAccession=\"MS:1000040\" unitName=\"m/z\"/>\n",
                v
            )?;
        }
        if let Some(v) = s.scan_window_upper {
            write!(
                out,
                "              <cvParam cvRef=\"MS\" accession=\"MS:1000500\" name=\"scan window upper limit\" value=\"{}\" unitCvRef=\"MS\" unitAccession=\"MS:1000040\" unitName=\"m/z\"/>\n",
                v
            )?;
        }
        write_extras(out, &extra, "scanWindow")?;
        write!(out, "            </scanWindow></scanWindowList>\n")?;
    }
    write!(out, "          </scan>\n")?;
    write!(out, "        </scanList>\n")?;

    if let Some(p) = &s.precursor {
        write!(out, "        <precursorList count=\"1\">\n")?;
        write!(out, "          <precursor>\n")?;
        write!(out, "            <isolationWindow>\n")?;
        if let Some(v) = p.isolation_window_target {
            write_cv(
                out,
                "MS:1000827",
                "isolation window target m/z",
                Some(&v.to_string()),
                Some(("MS:1000040", "m/z", "MS")),
            )?;
        }
        if let Some(v) = p.isolation_window_lower {
            write_cv(
                out,
                "MS:1000828",
                "isolation window lower offset",
                Some(&v.to_string()),
                Some(("MS:1000040", "m/z", "MS")),
            )?;
        }
        if let Some(v) = p.isolation_window_upper {
            write_cv(
                out,
                "MS:1000829",
                "isolation window upper offset",
                Some(&v.to_string()),
                Some(("MS:1000040", "m/z", "MS")),
            )?;
        }
        write_extras(out, &extra, "isolationWindow")?;
        write!(out, "            </isolationWindow>\n")?;
        write!(out, "            <selectedIonList count=\"1\"><selectedIon>\n")?;
        if let Some(v) = p.mz {
            write_cv(
                out,
                "MS:1000744",
                "selected ion m/z",
                Some(&v.to_string()),
                Some(("MS:1000040", "m/z", "MS")),
            )?;
        }
        if let Some(v) = p.charge {
            write_cv(out, "MS:1000041", "charge state", Some(&v.to_string()), None)?;
        }
        if let Some(v) = p.intensity {
            write_cv(out, "MS:1000042", "peak intensity", Some(&v.to_string()), None)?;
        }
        write_extras(out, &extra, "selectedIon")?;
        write!(out, "            </selectedIon></selectedIonList>\n")?;
        write!(out, "            <activation>\n")?;
        if let Some(a) = &s.activation {
            let (acc, name) = activation_terms(a);
            write_cv(out, acc, name, None, None)?;
        }
        if let Some(ce) = s.collision_energy {
            write_cv(
                out,
                "MS:1000045",
                "collision energy",
                Some(&ce.to_string()),
                Some(("UO:0000266", "electronvolt", "UO")),
            )?;
        }
        write_extras(out, &extra, "activation")?;
        write!(out, "            </activation>\n")?;
        write!(out, "          </precursor>\n")?;
        write!(out, "        </precursorList>\n")?;
    }

    // binary data arrays
    let mz_prec = s.mz_precision.unwrap_or(64);
    let int_prec = s.intensity_precision.unwrap_or(32);
    write!(out, "        <binaryDataArrayList count=\"2\">\n")?;
    write_binary_array_f64(out, &s.mz, mz_prec, "MS:1000514", "m/z array", true)?;
    write_binary_array_f32(
        out,
        &s.intensity,
        int_prec,
        "MS:1000515",
        "intensity array",
        true,
    )?;
    write!(out, "        </binaryDataArrayList>\n")?;
    write!(out, "      </spectrum>\n")?;
    Ok(())
}

fn write_spectrum_top_cv<W: Write>(out: &mut W, s: &Spectrum) -> MsResult<()> {
    if s.ms_level == 1 {
        write_cv(out, "MS:1000579", "MS1 spectrum", None, None)?;
    } else if s.ms_level >= 2 {
        write_cv(out, "MS:1000580", "MSn spectrum", None, None)?;
    }
    write_cv(out, "MS:1000511", "ms level", Some(&s.ms_level.to_string()), None)?;
    match s.polarity {
        Some(1) => write_cv(out, "MS:1000130", "positive scan", None, None)?,
        Some(-1) => write_cv(out, "MS:1000129", "negative scan", None, None)?,
        _ => {}
    }
    match s.centroided {
        Some(true) => write_cv(out, "MS:1000127", "centroid spectrum", None, None)?,
        Some(false) => write_cv(out, "MS:1000128", "profile spectrum", None, None)?,
        _ => {}
    }
    if let Some(v) = s.tic {
        write_cv(out, "MS:1000285", "total ion current", Some(&v.to_string()), None)?;
    }
    if let Some(v) = s.base_peak_mz {
        write_cv(
            out,
            "MS:1000504",
            "base peak m/z",
            Some(&v.to_string()),
            Some(("MS:1000040", "m/z", "MS")),
        )?;
    }
    if let Some(v) = s.base_peak_intensity {
        write_cv(
            out,
            "MS:1000505",
            "base peak intensity",
            Some(&v.to_string()),
            Some(("MS:1000131", "number of detector counts", "MS")),
        )?;
    }
    Ok(())
}

fn write_chromatogram<W: Write>(out: &mut W, c: &Chromatogram, idx: usize) -> MsResult<()> {
    let n = c.time.len().max(c.intensity.len());
    write!(
        out,
        "      <chromatogram index=\"{}\" id=\"{}\" defaultArrayLength=\"{}\">\n",
        idx,
        xml_escape(&c.chrom_id),
        n
    )?;
    match c.chrom_type.as_deref() {
        Some("TIC") => write_cv(out, "MS:1000235", "total ion current chromatogram", None, None)?,
        Some("SIC") => write_cv(out, "MS:1000627", "selected ion current chromatogram", None, None)?,
        Some("BPC") => write_cv(out, "MS:1000628", "basepeak chromatogram", None, None)?,
        Some("SRM") => write_cv(out, "MS:1001473", "selected reaction monitoring chromatogram", None, None)?,
        _ => {}
    }
    let extras: Vec<CvParam> = match c.cv_params.as_deref() {
        Some(j) => serde_json::from_str(j).unwrap_or_default(),
        None => Vec::new(),
    };
    write_extras(out, &extras, "chromatogram")?;

    write!(out, "        <binaryDataArrayList count=\"2\">\n")?;
    write_binary_array_f32(
        out,
        &c.time,
        32,
        "MS:1000595",
        "time array",
        true,
    )?;
    write_binary_array_f32(
        out,
        &c.intensity,
        32,
        "MS:1000515",
        "intensity array",
        true,
    )?;
    write!(out, "        </binaryDataArrayList>\n")?;
    write!(out, "      </chromatogram>\n")?;
    Ok(())
}

fn write_cv<W: Write>(
    out: &mut W,
    acc: &str,
    name: &str,
    value: Option<&str>,
    unit: Option<(&str, &str, &str)>,
) -> MsResult<()> {
    write!(out, "        <cvParam cvRef=\"MS\" accession=\"{}\" name=\"{}\"", acc, xml_escape(name))?;
    if let Some(v) = value {
        write!(out, " value=\"{}\"", xml_escape(v))?;
    }
    if let Some((ua, un, ucv)) = unit {
        write!(out, " unitCvRef=\"{}\" unitAccession=\"{}\" unitName=\"{}\"", ucv, ua, xml_escape(un))?;
    }
    write!(out, "/>\n")?;
    Ok(())
}

fn write_extras<W: Write>(out: &mut W, extras: &[CvParam], section: &str) -> MsResult<()> {
    for cv in extras.iter().filter(|c| c.section == section) {
        let tag = if cv.kind == "user" { "userParam" } else { "cvParam" };
        write!(out, "        <{}", tag)?;
        if let Some(v) = &cv.cv_ref {
            write!(out, " cvRef=\"{}\"", xml_escape(v))?;
        } else if cv.kind == "cv" {
            write!(out, " cvRef=\"MS\"")?;
        }
        if let Some(v) = &cv.accession {
            write!(out, " accession=\"{}\"", xml_escape(v))?;
        }
        if let Some(v) = &cv.name {
            write!(out, " name=\"{}\"", xml_escape(v))?;
        }
        if let Some(v) = &cv.value {
            write!(out, " value=\"{}\"", xml_escape(v))?;
        }
        if let Some(v) = &cv.unit_cv_ref {
            write!(out, " unitCvRef=\"{}\"", xml_escape(v))?;
        }
        if let Some(v) = &cv.unit_accession {
            write!(out, " unitAccession=\"{}\"", xml_escape(v))?;
        }
        if let Some(v) = &cv.unit_name {
            write!(out, " unitName=\"{}\"", xml_escape(v))?;
        }
        write!(out, "/>\n")?;
    }
    Ok(())
}

fn activation_terms(a: &str) -> (&'static str, &'static str) {
    match a {
        "CID" => ("MS:1000133", "collision-induced dissociation"),
        "HCD" => ("MS:1000422", "beam-type collision-induced dissociation"),
        "ETD" => ("MS:1000598", "electron transfer dissociation"),
        "PQD" => ("MS:1000599", "pulsed q dissociation"),
        "MPD" => ("MS:1000435", "photodissociation"),
        _ => ("MS:1000044", "dissociation method"),
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

// ── binary data encoding ────────────────────────────────────────────────────

fn encode_zlib(data: &[u8]) -> MsResult<Vec<u8>> {
    let mut enc = ZlibEncoder::new(Vec::with_capacity(data.len()), Compression::default());
    enc.write_all(data)?;
    Ok(enc.finish()?)
}

fn write_binary_array_f64<W: Write>(
    out: &mut W,
    data: &[f64],
    precision: u8,
    accession: &str,
    name: &str,
    zlib: bool,
) -> MsResult<()> {
    let mut buf: Vec<u8> = Vec::with_capacity(data.len() * 8);
    match precision {
        64 => {
            for v in data {
                buf.extend_from_slice(&v.to_le_bytes());
            }
        }
        32 => {
            for v in data {
                buf.extend_from_slice(&(*v as f32).to_le_bytes());
            }
        }
        _ => return Err(MsError::Malformed(format!("bad precision {}", precision))),
    }
    let payload = if zlib { encode_zlib(&buf)? } else { buf };
    let encoded = base64::engine::general_purpose::STANDARD.encode(&payload);
    write!(out, "          <binaryDataArray encodedLength=\"{}\">\n", encoded.len())?;
    let prec_term = if precision == 64 {
        ("MS:1000523", "64-bit float")
    } else {
        ("MS:1000521", "32-bit float")
    };
    write_cv(out, prec_term.0, prec_term.1, None, None)?;
    if zlib {
        write_cv(out, "MS:1000574", "zlib compression", None, None)?;
    } else {
        write_cv(out, "MS:1000576", "no compression", None, None)?;
    }
    write_cv(out, accession, name, None, None)?;
    write!(out, "            <binary>{}</binary>\n", encoded)?;
    write!(out, "          </binaryDataArray>\n")?;
    Ok(())
}

fn write_binary_array_f32<W: Write>(
    out: &mut W,
    data: &[f32],
    precision: u8,
    accession: &str,
    name: &str,
    zlib: bool,
) -> MsResult<()> {
    let mut buf: Vec<u8> = Vec::with_capacity(data.len() * 4);
    match precision {
        32 => {
            for v in data {
                buf.extend_from_slice(&v.to_le_bytes());
            }
        }
        64 => {
            for v in data {
                buf.extend_from_slice(&(*v as f64).to_le_bytes());
            }
        }
        _ => return Err(MsError::Malformed(format!("bad precision {}", precision))),
    }
    let payload = if zlib { encode_zlib(&buf)? } else { buf };
    let encoded = base64::engine::general_purpose::STANDARD.encode(&payload);
    write!(out, "          <binaryDataArray encodedLength=\"{}\">\n", encoded.len())?;
    let prec_term = if precision == 64 {
        ("MS:1000523", "64-bit float")
    } else {
        ("MS:1000521", "32-bit float")
    };
    write_cv(out, prec_term.0, prec_term.1, None, None)?;
    if zlib {
        write_cv(out, "MS:1000574", "zlib compression", None, None)?;
    } else {
        write_cv(out, "MS:1000576", "no compression", None, None)?;
    }
    write_cv(out, accession, name, None, None)?;
    write!(out, "            <binary>{}</binary>\n", encoded)?;
    write!(out, "          </binaryDataArray>\n")?;
    Ok(())
}

// ── indexedmzML wrapping ────────────────────────────────────────────────────

fn write_indexed<W: Write>(
    out: &mut W,
    body: &[u8],
    spec_offsets: &[(String, usize)],
    chrom_offsets: &[(String, usize)],
) -> MsResult<()> {
    // Build a Cursor on a fresh buffer so we can compute checksum.
    let mut full: Vec<u8> = Vec::with_capacity(body.len() + 4096);

    // Emit the XML declaration + indexedmzML wrapper first.
    let xml_decl_end = body.iter().position(|&b| b == b'\n').unwrap_or(0);
    let (decl, rest) = body.split_at(xml_decl_end + 1);
    full.extend_from_slice(decl);
    full.extend_from_slice(b"<indexedmzML xmlns=\"http://psi.hupo.org/ms/mzml\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:schemaLocation=\"http://psi.hupo.org/ms/mzml http://psidev.info/files/ms/mzML/xsd/mzML1.1.2_idx.xsd\">\n");

    // Body offsets shift by the size of `decl + indexedmzML opening`.
    let shift = full.len() - decl.len();
    full.extend_from_slice(rest);
    ensure_trailing_newline_vec(&mut full);

    let index_offset = full.len();
    full.extend_from_slice(b"<indexList count=\"");
    let count = if chrom_offsets.is_empty() { 1 } else { 2 };
    full.extend_from_slice(count.to_string().as_bytes());
    full.extend_from_slice(b"\">\n");
    full.extend_from_slice(b"  <index name=\"spectrum\">\n");
    for (id, off) in spec_offsets {
        let absolute = off + decl.len() + shift;
        write!(
            full,
            "    <offset idRef=\"{}\">{}</offset>\n",
            xml_escape(id),
            absolute
        )?;
    }
    full.extend_from_slice(b"  </index>\n");
    if !chrom_offsets.is_empty() {
        full.extend_from_slice(b"  <index name=\"chromatogram\">\n");
        for (id, off) in chrom_offsets {
            let absolute = off + decl.len() + shift;
            write!(
                full,
                "    <offset idRef=\"{}\">{}</offset>\n",
                xml_escape(id),
                absolute
            )?;
        }
        full.extend_from_slice(b"  </index>\n");
    }
    full.extend_from_slice(b"</indexList>\n");
    write!(full, "<indexListOffset>{}</indexListOffset>\n", index_offset)?;

    // The SHA-1 checksum is over everything up through "<fileChecksum>"
    // (inclusive of the opening tag). We compute by appending the opening
    // tag, hashing, then appending the digest + closing tag.
    full.extend_from_slice(b"<fileChecksum>");
    let mut hasher = Sha1::new();
    hasher.update(&full);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(40);
    for b in digest.iter() {
        hex.push_str(&format!("{:02x}", b));
    }
    full.extend_from_slice(hex.as_bytes());
    full.extend_from_slice(b"</fileChecksum>\n");
    full.extend_from_slice(b"</indexedmzML>\n");

    out.write_all(&full)?;
    let _ = Cursor::new(0); // silence unused import when this fn isn't used
    Ok(())
}

fn ensure_trailing_newline_vec(v: &mut Vec<u8>) {
    if v.last().copied() != Some(b'\n') {
        v.push(b'\n');
    }
}
