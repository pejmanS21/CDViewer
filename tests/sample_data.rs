//! Smoke tests against bundled sample data.
//!
//! Skipped automatically when `sample-data/` is absent (so CI without
//! fixtures still passes).

use std::path::PathBuf;

fn sample_root(sub: &str) -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("sample-data")
        .join(sub);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

#[test]
fn ct_folder_loads_as_single_series() {
    let Some(root) = sample_root("ct-data") else {
        eprintln!("skipping: sample-data/ct-data not present");
        return;
    };
    let studies = dicom_viewer::dcm::loader::load_folder(&root).expect("load ct");
    assert!(!studies.is_empty(), "expected at least one study");
    let total_instances: usize = studies
        .iter()
        .flat_map(|s| s.series.iter())
        .map(|s| s.instances.len())
        .sum();
    assert!(
        total_instances >= 20,
        "expected many CT slices, got {total_instances}"
    );

    let has_ct = studies
        .iter()
        .flat_map(|s| s.series.iter())
        .any(|s| s.modality.eq_ignore_ascii_case("CT"));
    assert!(has_ct, "expected at least one CT series");
}

#[test]
fn ct_first_slice_decodes() {
    let Some(root) = sample_root("ct-data") else {
        eprintln!("skipping");
        return;
    };
    let studies = dicom_viewer::dcm::loader::load_folder(&root).expect("load");
    let inst = &studies[0].series[0].instances[0];
    let frame =
        dicom_viewer::dcm::decode_to_rgba(&inst.path, dicom_viewer::dcm::WindowSetting::Auto)
            .expect("decode CT");
    assert_eq!(frame.rgba.len() as u32, frame.width * frame.height * 4);
}

#[test]
fn mg_first_image_decodes() {
    let Some(root) = sample_root("mg-data") else {
        eprintln!("skipping");
        return;
    };
    let studies = dicom_viewer::dcm::loader::load_folder(&root).expect("load");
    let inst = &studies[0]
        .series
        .iter()
        .find(|s| s.is_mammography())
        .expect("mg series")
        .instances[0];
    let frame =
        dicom_viewer::dcm::decode_to_rgba(&inst.path, dicom_viewer::dcm::WindowSetting::Auto)
            .expect("decode MG");
    assert_eq!(frame.rgba.len() as u32, frame.width * frame.height * 4);
}

#[test]
fn rect_roi_stats_on_ct() {
    let Some(root) = sample_root("ct-data") else {
        eprintln!("skipping");
        return;
    };
    let studies = dicom_viewer::dcm::loader::load_folder(&root).expect("load");
    let inst = &studies[0].series[0].instances[0];
    let raw = dicom_viewer::dcm::load_raw(&inst.path).expect("raw");
    let mid_x = raw.width as f32 / 2.0;
    let mid_y = raw.height as f32 / 2.0;
    let stats = dicom_viewer::dcm::roi::rect_stats(
        &raw,
        [mid_x - 20.0, mid_y - 20.0],
        [mid_x + 20.0, mid_y + 20.0],
    )
    .expect("stats");
    assert!(stats.count > 0);
    assert!(stats.max >= stats.min);
    assert!(stats.std >= 0.0);
}

#[test]
fn length_label_uses_pixel_spacing() {
    let Some(root) = sample_root("ct-data") else {
        eprintln!("skipping");
        return;
    };
    let studies = dicom_viewer::dcm::loader::load_folder(&root).expect("load");
    let inst = &studies[0].series[0].instances[0];
    let raw = dicom_viewer::dcm::load_raw(&inst.path).expect("raw");
    let label = dicom_viewer::dcm::roi::length_label([0.0, 0.0], [100.0, 0.0], inst, &raw);
    if inst.pixel_spacing.is_some() {
        assert!(label.contains("mm"), "expected mm units, got {label}");
    } else {
        assert!(label.contains("px"), "expected px units, got {label}");
    }
}

#[test]
fn png_export_writes_file() {
    let Some(root) = sample_root("ct-data") else {
        eprintln!("skipping");
        return;
    };
    let studies = dicom_viewer::dcm::loader::load_folder(&root).expect("load");
    let inst = &studies[0].series[0].instances[0];
    let raw = dicom_viewer::dcm::load_raw(&inst.path).expect("raw");
    let window = dicom_viewer::dcm::auto_window(&raw);
    let tmp = std::env::temp_dir().join("dv-test-export.png");
    let _ = std::fs::remove_file(&tmp);
    dicom_viewer::dcm::export::export_current_view(
        inst,
        &raw,
        window,
        false,
        &[],
        false,
        &tmp,
        "CT",
    )
    .expect("export");
    let meta = std::fs::metadata(&tmp).expect("png written");
    assert!(meta.len() > 100, "png too small: {} bytes", meta.len());
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn anonymize_blanks_patient_name() {
    use dicom::core::header::Header;
    use dicom::object::open_file;
    use dicom_dictionary_std::tags;

    let Some(root) = sample_root("ct-data") else {
        eprintln!("skipping");
        return;
    };
    let studies = dicom_viewer::dcm::loader::load_folder(&root).expect("load");
    let inst = &studies[0].series[0].instances[0];
    let tmp = std::env::temp_dir().join("dv-test-anon.dcm");
    let _ = std::fs::remove_file(&tmp);
    dicom_viewer::dcm::export::anonymize_file(&inst.path, &tmp).expect("anon");
    let obj = open_file(&tmp).expect("read back");
    let name = obj
        .element(tags::PATIENT_NAME)
        .expect("pn")
        .to_str()
        .expect("str")
        .into_owned();
    assert!(name.contains("Anon") || name.is_empty(), "got '{name}'");
    let _ = std::fs::remove_file(&tmp);
    let _ = Header::tag(obj.element(tags::PATIENT_NAME).unwrap());
}

#[test]
fn raw_load_for_each_sample() {
    for (name, sub) in [("ct", "ct-data"), ("mg", "mg-data")] {
        let Some(root) = sample_root(sub) else {
            eprintln!("skipping {name}");
            continue;
        };
        let studies = dicom_viewer::dcm::loader::load_folder(&root).expect("load");
        for study in &studies {
            for series in &study.series {
                for inst in &series.instances {
                    let raw = dicom_viewer::dcm::load_raw(&inst.path)
                        .unwrap_or_else(|e| panic!("{name}/{}: {e:#}", inst.path.display()));
                    assert_eq!(
                        raw.values.len(),
                        (raw.width as usize) * (raw.height as usize),
                        "{}",
                        inst.path.display()
                    );
                    assert!(raw.width > 0 && raw.height > 0);
                }
            }
        }
    }
}

#[test]
fn mg_folder_has_four_views() {
    let Some(root) = sample_root("mg-data") else {
        eprintln!("skipping: sample-data/mg-data not present");
        return;
    };
    let studies = dicom_viewer::dcm::loader::load_folder(&root).expect("load mg");
    let mg_series: Vec<_> = studies
        .iter()
        .flat_map(|s| s.series.iter())
        .filter(|s| s.is_mammography())
        .collect();
    assert!(!mg_series.is_empty(), "expected an MG series");
    let total: usize = mg_series.iter().map(|s| s.instances.len()).sum();
    assert_eq!(total, 4, "expected 4 MG views, got {total}");
}
