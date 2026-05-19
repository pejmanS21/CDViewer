//! Folder scanning and DICOM metadata extraction.
//!
//! Reads each file with `OpenFileOptions::read_until(PIXEL_DATA)` so we
//! never pay the cost of decoding pixels just to populate the study tree.

use anyhow::{Context, Result};
use dicom::object::{OpenFileOptions, ReadError};
use dicom_dictionary_std::tags;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use super::study::{Instance, Series, Study};

/// Scan a folder recursively for DICOM files and group them by study/series.
pub fn load_folder(root: &Path) -> Result<Vec<Study>> {
    let started = std::time::Instant::now();
    let paths = collect_candidate_files(root)?;
    info!(count = paths.len(), root = %root.display(), "scanning folder");

    let parsed: Vec<Instance> = paths
        .par_iter()
        .filter_map(|p| match parse_instance(p) {
            Ok(inst) => Some(inst),
            Err(e) => {
                debug!(path = %p.display(), error = %e, "skip file");
                None
            }
        })
        .collect();

    let studies = group_into_studies(parsed);
    info!(
        studies = studies.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "folder scan complete"
    );
    Ok(studies)
}

fn collect_candidate_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk(root, &mut out)?;
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let md = std::fs::metadata(dir).with_context(|| format!("stat {}", dir.display()))?;
    if md.is_file() {
        if looks_like_dicom(dir) {
            out.push(dir.to_path_buf());
        }
        return Ok(());
    }
    // nosemgrep: path-traversal — `dir` is a folder the user explicitly
    // selected via File ▸ Open Folder or supplied on the CLI. Symlinks can't
    // be used to escape the tree because `DirEntry::file_type()` reports
    // symlinks as neither `is_file` nor `is_dir`, so the branches below
    // skip them before any file is opened.
    for entry in std::fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            walk(&path, out)?;
        } else if ft.is_file() && looks_like_dicom(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn looks_like_dicom(p: &Path) -> bool {
    // Accept .dcm explicitly; otherwise accept files with no extension
    // (common on CDs) and let the parser reject non-DICOM content.
    match p.extension().and_then(|e| e.to_str()) {
        Some(ext) => ext.eq_ignore_ascii_case("dcm") || ext.eq_ignore_ascii_case("dicom"),
        None => {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            !name.starts_with('.') && name != "DICOMDIR" && {
                // Heuristic: probe the DICOM "DICM" magic at byte 128.
                file_has_dicm_magic(p).unwrap_or(false)
            }
        }
    }
}

fn file_has_dicm_magic(p: &Path) -> std::io::Result<bool> {
    use std::io::{Read, Seek, SeekFrom};
    // nosemgrep: path-traversal — `p` is a regular file already vetted by
    // `walk` (symlinks pre-filtered, see comment there). The probe reads
    // exactly 4 bytes at offset 128 and returns a bool; no path-controlled
    // content is reflected back or written anywhere.
    let mut f = std::fs::File::open(p)?;
    if f.metadata()?.len() < 132 {
        return Ok(false);
    }
    f.seek(SeekFrom::Start(128))?;
    let mut buf = [0u8; 4];
    f.read_exact(&mut buf)?;
    Ok(&buf == b"DICM")
}

fn parse_instance(path: &Path) -> Result<Instance, ReadError> {
    let obj = OpenFileOptions::new()
        .read_until(tags::PIXEL_DATA)
        .open_file(path)?;

    let take_str = |tag| {
        obj.element(tag)
            .ok()
            .and_then(|e| e.to_str().ok())
            .map(|s| s.trim_end_matches('\0').trim().to_string())
            .unwrap_or_default()
    };
    let take_opt_str = |tag| {
        obj.element(tag)
            .ok()
            .and_then(|e| e.to_str().ok())
            .map(|s| s.trim_end_matches('\0').trim().to_string())
    };
    let take_f64 = |tag| obj.element(tag).ok().and_then(|e| e.to_float64().ok());
    let take_int = |tag, default: i32| {
        obj.element(tag)
            .ok()
            .and_then(|e| e.to_int::<i32>().ok())
            .unwrap_or(default)
    };
    let take_u16 = |tag, default: u16| {
        obj.element(tag)
            .ok()
            .and_then(|e| e.to_int::<u16>().ok())
            .unwrap_or(default)
    };

    let pixel_spacing = obj
        .element(tags::PIXEL_SPACING)
        .ok()
        .and_then(|e| e.to_multi_float64().ok())
        .and_then(|v| {
            if v.len() >= 2 {
                Some((v[0], v[1]))
            } else {
                None
            }
        });

    let image_position_z = obj
        .element(tags::IMAGE_POSITION_PATIENT)
        .ok()
        .and_then(|e| e.to_multi_float64().ok())
        .and_then(|v| v.get(2).copied());

    Ok(Instance {
        path: path.to_path_buf(),
        sop_instance_uid: take_str(tags::SOP_INSTANCE_UID),
        instance_number: take_int(tags::INSTANCE_NUMBER, 0),
        rows: take_u16(tags::ROWS, 0),
        cols: take_u16(tags::COLUMNS, 0),
        modality: take_str(tags::MODALITY),
        photometric: take_str(tags::PHOTOMETRIC_INTERPRETATION),
        window_center: take_f64(tags::WINDOW_CENTER),
        window_width: take_f64(tags::WINDOW_WIDTH),
        rescale_slope: take_f64(tags::RESCALE_SLOPE).unwrap_or(1.0),
        rescale_intercept: take_f64(tags::RESCALE_INTERCEPT).unwrap_or(0.0),
        pixel_spacing,
        view_position: take_opt_str(tags::VIEW_POSITION),
        image_laterality: take_opt_str(tags::IMAGE_LATERALITY),
        image_position_z,
    })
}

fn group_into_studies(instances: Vec<Instance>) -> Vec<Study> {
    // study UID -> series UID -> Vec<Instance>
    let mut by_study: BTreeMap<String, BTreeMap<String, Vec<Instance>>> = BTreeMap::new();
    let mut study_meta: BTreeMap<String, StudyMeta> = BTreeMap::new();
    let mut series_meta: BTreeMap<(String, String), SeriesMeta> = BTreeMap::new();

    for inst in instances {
        let study_uid = read_meta_tag(&inst.path, tags::STUDY_INSTANCE_UID)
            .unwrap_or_else(|| "(unknown study)".into());
        let series_uid = read_meta_tag(&inst.path, tags::SERIES_INSTANCE_UID)
            .unwrap_or_else(|| "(unknown series)".into());

        study_meta
            .entry(study_uid.clone())
            .or_insert_with(|| StudyMeta::read(&inst.path));
        series_meta
            .entry((study_uid.clone(), series_uid.clone()))
            .or_insert_with(|| SeriesMeta::read(&inst.path));

        by_study
            .entry(study_uid)
            .or_default()
            .entry(series_uid)
            .or_default()
            .push(inst);
    }

    let mut studies: Vec<Study> = Vec::with_capacity(by_study.len());
    for (study_uid, series_map) in by_study {
        let meta = study_meta.remove(&study_uid).unwrap_or_default();
        let mut series_vec: Vec<Series> = Vec::with_capacity(series_map.len());
        let mut modalities = Vec::<String>::new();
        for (series_uid, mut insts) in series_map {
            sort_slices(&mut insts);
            let smeta = series_meta
                .remove(&(study_uid.clone(), series_uid.clone()))
                .unwrap_or_default();
            if !smeta.modality.is_empty() && !modalities.contains(&smeta.modality) {
                modalities.push(smeta.modality.clone());
            }
            series_vec.push(Series {
                series_instance_uid: series_uid,
                series_number: smeta.series_number,
                modality: smeta.modality,
                description: smeta.description,
                instances: insts,
            });
        }
        series_vec.sort_by_key(|s| s.series_number);

        studies.push(Study {
            study_instance_uid: study_uid,
            patient_name: meta.patient_name,
            patient_id: meta.patient_id,
            study_date: meta.study_date,
            study_description: meta.study_description,
            modalities,
            series: series_vec,
        });
    }

    // Newest studies first — radiologists scan top-down.
    studies.sort_by(|a, b| b.study_date.cmp(&a.study_date));

    if studies.is_empty() {
        warn!("no DICOM files parsed");
    }
    studies
}

/// Order slices the way a radiologist expects to scroll them. When every
/// instance carries ImagePositionPatient, sort by z — anatomically correct.
/// Otherwise fall back to InstanceNumber. Stable ties are broken by
/// InstanceNumber so duplicate-z slices stay deterministic.
fn sort_slices(insts: &mut [Instance]) {
    let all_have_z = !insts.is_empty() && insts.iter().all(|i| i.image_position_z.is_some());
    if all_have_z {
        insts.sort_by(|a, b| {
            let az = a.image_position_z.unwrap_or(0.0);
            let bz = b.image_position_z.unwrap_or(0.0);
            az.partial_cmp(&bz)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.instance_number.cmp(&b.instance_number))
        });
    } else {
        insts.sort_by_key(|i| i.instance_number);
    }
}

#[derive(Default)]
struct StudyMeta {
    patient_name: String,
    patient_id: String,
    study_date: String,
    study_description: String,
}

impl StudyMeta {
    fn read(path: &Path) -> Self {
        let obj = match OpenFileOptions::new()
            .read_until(tags::PIXEL_DATA)
            .open_file(path)
        {
            Ok(o) => o,
            Err(_) => return Self::default(),
        };
        let s = |tag| {
            obj.element(tag)
                .ok()
                .and_then(|e| e.to_str().ok())
                .map(|s| s.trim_end_matches('\0').trim().to_string())
                .unwrap_or_default()
        };
        Self {
            patient_name: s(tags::PATIENT_NAME),
            patient_id: s(tags::PATIENT_ID),
            study_date: s(tags::STUDY_DATE),
            study_description: s(tags::STUDY_DESCRIPTION),
        }
    }
}

#[derive(Default)]
struct SeriesMeta {
    modality: String,
    description: String,
    series_number: i32,
}

impl SeriesMeta {
    fn read(path: &Path) -> Self {
        let obj = match OpenFileOptions::new()
            .read_until(tags::PIXEL_DATA)
            .open_file(path)
        {
            Ok(o) => o,
            Err(_) => return Self::default(),
        };
        Self {
            modality: obj
                .element(tags::MODALITY)
                .ok()
                .and_then(|e| e.to_str().ok())
                .map(|s| s.trim_end_matches('\0').trim().to_string())
                .unwrap_or_default(),
            description: obj
                .element(tags::SERIES_DESCRIPTION)
                .ok()
                .and_then(|e| e.to_str().ok())
                .map(|s| s.trim_end_matches('\0').trim().to_string())
                .unwrap_or_default(),
            series_number: obj
                .element(tags::SERIES_NUMBER)
                .ok()
                .and_then(|e| e.to_int::<i32>().ok())
                .unwrap_or(0),
        }
    }
}

fn read_meta_tag(path: &Path, tag: dicom::core::Tag) -> Option<String> {
    let obj = OpenFileOptions::new()
        .read_until(tags::PIXEL_DATA)
        .open_file(path)
        .ok()?;
    obj.element(tag)
        .ok()
        .and_then(|e| e.to_str().ok())
        .map(|s| s.trim_end_matches('\0').trim().to_string())
}
