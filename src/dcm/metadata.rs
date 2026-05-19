//! Flatten a DICOM file's tags into rows the metadata panel can display.

use anyhow::{Context, Result};
use dicom::core::dictionary::{DataDictionary, DataDictionaryEntryRef};
use dicom::core::header::Header;
use dicom::core::{Tag, VR};
use dicom::object::mem::InMemElement;
use dicom::object::OpenFileOptions;
use dicom_dictionary_std::{tags, StandardDataDictionary};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct TagRow {
    pub tag: Tag,
    pub vr: String,
    pub name: String,
    pub value: String,
}

impl TagRow {
    pub fn tag_label(&self) -> String {
        format!("({:04X},{:04X})", self.tag.0, self.tag.1)
    }
    pub fn group_label(&self) -> String {
        match self.tag.0 {
            0x0002 => "0002 — File Meta",
            0x0008 => "0008 — Identifying",
            0x0010 => "0010 — Patient",
            0x0018 => "0018 — Acquisition",
            0x0020 => "0020 — Image",
            0x0028 => "0028 — Image Pixel",
            0x0032 => "0032 — Study",
            0x0038 => "0038 — Visit",
            0x0040 => "0040 — Procedure",
            0x0054 => "0054 — Nuclear Medicine",
            0x0070 => "0070 — Presentation",
            0x0072 => "0072 — Hanging Protocol",
            0x0088 => "0088 — Storage",
            0x0400 => "0400 — Digital Signature",
            0x2000 => "2000 — Film",
            0x3006 => "3006 — RT Structure",
            0x300A => "300A — RT Plan",
            0x300C => "300C — RT Relationship",
            0x300E => "300E — RT Approval",
            0x4000 => "4000 — Text",
            0x7FE0 => "7FE0 — Pixel Data",
            g if g & 1 == 1 => "Private group",
            _ => "Other",
        }
        .to_string()
    }
}

pub fn collect_tags(path: &Path) -> Result<Vec<TagRow>> {
    let obj = OpenFileOptions::new()
        .read_until(tags::PIXEL_DATA)
        .open_file(path)
        .with_context(|| format!("open {}", path.display()))?;

    let mut rows: Vec<TagRow> = Vec::new();
    for elem in obj.iter() {
        let tag: Tag = Header::tag(elem);
        let vr_label = format!("{:?}", elem.vr());
        let name = lookup_name(tag);
        let value = format_value(elem);
        rows.push(TagRow {
            tag,
            vr: vr_label,
            name,
            value,
        });
    }
    rows.sort_by_key(|r| (r.tag.0, r.tag.1));
    Ok(rows)
}

fn lookup_name(tag: Tag) -> String {
    let dict = StandardDataDictionary;
    let entry: Option<&DataDictionaryEntryRef<'static>> = dict.by_tag(tag);
    match entry {
        Some(e) => e.alias.to_string(),
        None => "(unknown)".to_string(),
    }
}

fn format_value(elem: &InMemElement) -> String {
    const MAX_LEN: usize = 240;

    if elem.vr() == VR::SQ {
        let count = elem.items().map(<[_]>::len).unwrap_or(0);
        return format!("<sequence, {count} item(s)>");
    }

    let raw = match elem.value().to_str() {
        Ok(s) => s.to_string(),
        Err(_) => {
            return "<binary value>".to_string();
        }
    };

    let cleaned: String = raw
        .chars()
        .map(|c: char| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect();
    let trimmed = cleaned.trim_end_matches('\0').trim().to_string();
    if trimmed.chars().count() > MAX_LEN {
        let cut: String = trimmed.chars().take(MAX_LEN).collect();
        format!("{cut}…")
    } else {
        trimmed
    }
}
