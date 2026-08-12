use std::io::{Cursor, Write};

use base64::{engine::general_purpose::STANDARD, Engine};
use meeting_intelligence_core::{Artifact, ArtifactEncoding, ArtifactKind};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use crate::{
    assignee_label, due_label, evidence, format_timestamp, ArtifactExporter, ExportBundle,
    ExportError,
};

pub const DOCX_MEDIA_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

/// Writes a real WordprocessingML package rather than renaming an HTML file.
///
/// The archive is built with a fixed timestamp and fixed entry order so two
/// exports of the same meeting are byte-identical.
#[derive(Clone, Copy, Debug, Default)]
pub struct DocxExporter;

impl ArtifactExporter for DocxExporter {
    fn export(&self, bundle: &ExportBundle) -> Result<Artifact, ExportError> {
        let package = build_package(bundle)?;

        Ok(Artifact {
            id: format!("artifact-{}-docx", bundle.meeting.id),
            meeting_id: bundle.meeting.id.clone(),
            kind: ArtifactKind::Docx,
            media_type: DOCX_MEDIA_TYPE.to_owned(),
            content: STANDARD.encode(package),
            encoding: ArtifactEncoding::Base64,
            created_at: None,
        })
    }
}

fn build_package(bundle: &ExportBundle) -> Result<Vec<u8>, ExportError> {
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buffer);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            // A fixed timestamp keeps the archive reproducible.
            .last_modified_time(
                zip::DateTime::from_date_and_time(2026, 1, 1, 0, 0, 0)
                    .map_err(|_| ExportError::Docx)?,
            );

        for (name, contents) in [
            ("[Content_Types].xml", CONTENT_TYPES.to_owned()),
            ("_rels/.rels", ROOT_RELATIONSHIPS.to_owned()),
            (
                "word/_rels/document.xml.rels",
                DOCUMENT_RELATIONSHIPS.to_owned(),
            ),
            ("word/styles.xml", STYLES.to_owned()),
            ("word/document.xml", document_xml(bundle)),
        ] {
            zip.start_file(name, options)
                .map_err(|_| ExportError::Docx)?;
            zip.write_all(contents.as_bytes())
                .map_err(|_| ExportError::Docx)?;
        }
        zip.finish().map_err(|_| ExportError::Docx)?;
    }
    Ok(buffer.into_inner())
}

fn document_xml(bundle: &ExportBundle) -> String {
    let mut body = String::new();

    body.push_str(&heading(&bundle.meeting.title, 1));
    if let Some(started_at) = &bundle.meeting.started_at {
        body.push_str(&paragraph(&format!("Date: {started_at}"), "Meta"));
    }
    if let Some(duration_ms) = bundle.meeting.duration_ms {
        body.push_str(&paragraph(
            &format!("Duration: {}", format_timestamp(duration_ms)),
            "Meta",
        ));
    }
    if !bundle.meeting.participants.is_empty() {
        body.push_str(&paragraph(
            &format!("Participants: {}", bundle.meeting.participants.join(", ")),
            "Meta",
        ));
    }
    if let Some(note) = bundle.speaker_scope_note() {
        body.push_str(&paragraph(note, "Meta"));
    }

    body.push_str(&heading("Summary", 2));
    body.push_str(&paragraph(&bundle.analysis.summary, "Body"));

    body.push_str(&heading("Decisions", 2));
    if bundle.analysis.decisions.is_empty() {
        body.push_str(&paragraph("None", "Body"));
    } else {
        for decision in &bundle.analysis.decisions {
            body.push_str(&paragraph(
                &format!(
                    "{} (evidence: {})",
                    decision.statement,
                    evidence(&decision.evidence_segment_ids)
                ),
                "Body",
            ));
        }
    }

    body.push_str(&heading("Action items", 2));
    if bundle.analysis.action_items.is_empty() {
        body.push_str(&paragraph("None", "Body"));
    } else {
        for item in &bundle.analysis.action_items {
            body.push_str(&paragraph(
                &format!(
                    "{} — {} (due: {}) (evidence: {})",
                    assignee_label(item.assignee_profile_id.as_deref()),
                    item.description,
                    due_label(item.due_at.as_deref()),
                    evidence(&item.evidence_segment_ids)
                ),
                "Body",
            ));
        }
    }

    body.push_str(&heading("Open questions", 2));
    if bundle.analysis.open_questions.is_empty() {
        body.push_str(&paragraph("None", "Body"));
    } else {
        for question in &bundle.analysis.open_questions {
            body.push_str(&paragraph(
                &format!(
                    "{} (evidence: {})",
                    question.question,
                    evidence(&question.evidence_segment_ids)
                ),
                "Body",
            ));
        }
    }

    body.push_str(&heading("Transcript", 2));
    for segment in &bundle.transcript.segments {
        body.push_str(&paragraph(
            &format!(
                "[{}] {}–{} {}: {}",
                segment.id,
                format_timestamp(segment.start_ms),
                format_timestamp(segment.end_ms),
                bundle.speaker_label(&segment.speaker_id),
                segment.text
            ),
            "Body",
        ));
    }

    format!(
        "{XML_DECLARATION}\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/>\
         <w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\"/>\
         </w:sectPr></w:body></w:document>"
    )
}

fn heading(text: &str, level: u8) -> String {
    paragraph(text, &format!("Heading{level}"))
}

fn paragraph(text: &str, style: &str) -> String {
    format!(
        "<w:p><w:pPr><w:pStyle w:val=\"{}\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        escape_xml(style),
        escape_xml(text)
    )
}

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            // Control characters other than tab/newline are not legal in XML.
            control if control.is_control() && control != '\t' && control != '\n' => {}
            other => escaped.push(other),
        }
    }
    escaped
}

const XML_DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

const CONTENT_TYPES: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n",
    "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
    "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
    "<Default Extension=\"xml\" ContentType=\"application/xml\"/>",
    "<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>",
    "<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>",
    "</Types>"
);

const ROOT_RELATIONSHIPS: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n",
    "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>",
    "</Relationships>"
);

const DOCUMENT_RELATIONSHIPS: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n",
    "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
    "</Relationships>"
);

const STYLES: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n",
    "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    "<w:docDefaults><w:rPrDefault><w:rPr>",
    "<w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:eastAsia=\"Microsoft JhengHei\"/>",
    "<w:sz w:val=\"22\"/></w:rPr></w:rPrDefault></w:docDefaults>",
    "<w:style w:type=\"paragraph\" w:styleId=\"Body\" w:default=\"1\">",
    "<w:name w:val=\"Body Text\"/><w:pPr><w:spacing w:after=\"120\"/></w:pPr></w:style>",
    "<w:style w:type=\"paragraph\" w:styleId=\"Meta\"><w:name w:val=\"Meta\"/>",
    "<w:pPr><w:spacing w:after=\"60\"/></w:pPr>",
    "<w:rPr><w:color w:val=\"595959\"/></w:rPr></w:style>",
    "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/>",
    "<w:pPr><w:outlineLvl w:val=\"0\"/><w:spacing w:before=\"240\" w:after=\"120\"/></w:pPr>",
    "<w:rPr><w:b/><w:sz w:val=\"36\"/></w:rPr></w:style>",
    "<w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/>",
    "<w:pPr><w:outlineLvl w:val=\"1\"/><w:spacing w:before=\"240\" w:after=\"120\"/></w:pPr>",
    "<w:rPr><w:b/><w:sz w:val=\"28\"/></w:rPr></w:style>",
    "</w:styles>"
);

#[cfg(test)]
mod tests {
    use std::io::Read;

    use meeting_intelligence_core::SpeakerScope;

    use crate::test_support::bundle;

    use super::*;

    fn package(bundle: &ExportBundle) -> Vec<u8> {
        let artifact = DocxExporter.export(bundle).expect("docx export");
        assert_eq!(artifact.encoding, ArtifactEncoding::Base64);
        STANDARD
            .decode(artifact.content)
            .expect("a Base64 artifact must decode")
    }

    fn entry(bytes: &[u8], name: &str) -> String {
        let mut archive =
            zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a valid zip package");
        let mut file = archive
            .by_name(name)
            .unwrap_or_else(|_| panic!("package must contain {name}"));
        let mut contents = String::new();
        file.read_to_string(&mut contents).expect("readable entry");
        contents
    }

    #[test]
    fn the_export_is_a_real_word_package_not_renamed_html() {
        let bytes = package(&bundle());

        assert_eq!(&bytes[0..2], b"PK", "a .docx must be a zip archive");
        let mut archive =
            zip::ZipArchive::new(Cursor::new(bytes.clone())).expect("a valid zip package");
        let names: Vec<String> = (0..archive.len())
            .map(|index| archive.by_index(index).expect("entry").name().to_owned())
            .collect();
        for required in [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/_rels/document.xml.rels",
            "word/styles.xml",
            "word/document.xml",
        ] {
            assert!(names.contains(&required.to_owned()), "missing {required}");
        }
        let document = entry(&bytes, "word/document.xml");
        assert!(document.contains("wordprocessingml/2006/main"));
        assert!(!document.contains("<html"));
    }

    #[test]
    fn the_document_contains_every_required_section() {
        let document = entry(&package(&bundle()), "word/document.xml");

        for expected in [
            "Bootstrap planning",
            "Date: 2026-08-12T09:00:00Z",
            "Duration: 00:00:12",
            "Participants: Alice, Guest",
            "Summary",
            "Decisions",
            "Action items",
            "Open questions",
            "Transcript",
            "The team agreed to keep processing offline.",
            "Use fixtures &amp; mocks only.",
            "Verify both artifacts.",
            "Which provider ships first?",
            "[seg_001] 00:00:00–00:00:04 Alice: Keep the pipeline deterministic.",
            "[seg_002] 00:00:04–00:00:08 Speaker 2: Can we export Markdown and JSON?",
        ] {
            assert!(
                document.contains(expected),
                "document is missing {expected}"
            );
        }
    }

    #[test]
    fn xml_special_characters_are_escaped() {
        let mut bundle = bundle();
        bundle.meeting.title = "Q&A <review> \"final\"".to_owned();
        let document = entry(&package(&bundle), "word/document.xml");

        assert!(document.contains("Q&amp;A &lt;review&gt; &quot;final&quot;"));
        assert!(!document.contains("<review>"));
    }

    #[test]
    fn a_manual_rename_and_scope_warning_reach_the_word_document() {
        let mut bundle = bundle();
        bundle.speakers[1].manual_display_name = Some("Mr. Wang".to_owned());
        bundle.transcript.speaker_scope = SpeakerScope::Chunk;
        let document = entry(&package(&bundle), "word/document.xml");

        assert!(document.contains("Mr. Wang: Can we export"));
        assert!(document.contains("reliable within one part only"));
    }

    #[test]
    fn the_export_is_byte_identical_across_runs() {
        let bundle = bundle();

        assert_eq!(package(&bundle), package(&bundle));
    }

    #[test]
    fn empty_analysis_sections_render_none() {
        let mut bundle = bundle();
        bundle.analysis.decisions.clear();
        bundle.analysis.action_items.clear();
        bundle.analysis.open_questions.clear();
        let document = entry(&package(&bundle), "word/document.xml");

        assert_eq!(document.matches(">None<").count(), 3);
    }
}
