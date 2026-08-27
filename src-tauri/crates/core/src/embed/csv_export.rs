//! SPHIN-21: export generated metadata to CSV for manual review, or as a
//! bulk-upload template for a stock site, ahead of automated upload (SFTP
//! automation is SPHIN-6).

use crate::metadata::GeneratedMetadata;

/// One row to export: an asset's file name plus its generated metadata.
pub struct ExportRow<'a> {
    pub file_name: &'a str,
    pub metadata: &'a GeneratedMetadata,
}

/// Render `rows` as CSV text: `Filename,Title,Description,Keywords`, with
/// keywords semicolon-joined (the convention most stock-site bulk-upload
/// templates use for a single delimited field) and RFC 4180 quoting for
/// fields containing a comma, quote, or newline.
pub fn export_csv(rows: &[ExportRow]) -> String {
    let mut out = String::from("Filename,Title,Description,Keywords\n");
    for row in rows {
        let keywords = row.metadata.keywords.join("; ");
        out.push_str(&csv_field(row.file_name));
        out.push(',');
        out.push_str(&csv_field(&row.metadata.title));
        out.push(',');
        out.push_str(&csv_field(&row.metadata.description));
        out.push(',');
        out.push_str(&csv_field(&keywords));
        out.push('\n');
    }
    out
}

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(title: &str, description: &str, keywords: &[&str]) -> GeneratedMetadata {
        GeneratedMetadata {
            title: title.to_string(),
            description: description.to_string(),
            keywords: keywords.iter().map(|s| s.to_string()).collect(),
            profile: "Shutterstock".to_string(),
            meets_minimum_keywords: true,
        }
    }

    #[test]
    fn empty_rows_produce_header_only() {
        assert_eq!(export_csv(&[]), "Filename,Title,Description,Keywords\n");
    }

    #[test]
    fn a_plain_row_is_rendered_unquoted() {
        let m = meta("A dog", "A dog runs.", &["dog", "park"]);
        let rows = [ExportRow { file_name: "dog.jpg", metadata: &m }];
        assert_eq!(
            export_csv(&rows),
            "Filename,Title,Description,Keywords\ndog.jpg,A dog,A dog runs.,dog; park\n"
        );
    }

    #[test]
    fn fields_with_commas_or_quotes_are_rfc4180_quoted() {
        let m = meta("Dogs, cats", "He said \"hi\".", &["a"]);
        let rows = [ExportRow { file_name: "x.jpg", metadata: &m }];
        let csv = export_csv(&rows);
        assert!(csv.contains("\"Dogs, cats\""));
        assert!(csv.contains("\"He said \"\"hi\"\".\""));
    }

    #[test]
    fn multiple_rows_are_each_on_their_own_line() {
        let m1 = meta("First", "d1", &["a"]);
        let m2 = meta("Second", "d2", &["b"]);
        let rows = [
            ExportRow { file_name: "1.jpg", metadata: &m1 },
            ExportRow { file_name: "2.jpg", metadata: &m2 },
        ];
        let csv = export_csv(&rows);
        assert_eq!(csv.lines().count(), 3); // header + 2 rows
    }
}
