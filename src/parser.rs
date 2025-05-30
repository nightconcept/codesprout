// src/parser.rs
// Module for parsing the bundle file

use anyhow::{Context, Result, anyhow};
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

const FILE_HEADER_SEPARATOR: &str = "================================================";
const FILE_PATH_PREFIX: &str = "File: ";

/// Represents a single parsed file entry from the bundle.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ParsedEntry {
    pub path: PathBuf,
    pub content: String,
}

/// Specific errors that can occur during bundle parsing and validation.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum BundleValidationError {
    ContentBeforeFirstHeader {
        line_number: usize,
        content_excerpt: String,
    },
    MalformedHeaderMissingFilePrefix {
        line_number: usize,
        header_line: String,
    },
    MalformedHeaderMissingSeparatorAfterPath {
        line_number: usize,
        path_line: String,
    },
    MalformedHeaderSeparatorWithExtraContent {
        line_number: usize,
        separator_line_content: String, // The actual content of the separator line
    },
    MalformedHeaderPathLineInterruptedBySeparator {
        line_number: usize,
        path_line: String,
    },
    MalformedHeaderPathLineMissingNewline {
        line_number: usize,
        path_line: String,
    },
    EmptyPath {
        line_number: usize,
    },
    AbsolutePathNotAllowed {
        line_number: usize,
        path: String,
    },
    DuplicatePath {
        line_number: usize,
        path: String,
    },
    PrematureEOFBeforePathLine {
        line_number: usize,
    },
    PrematureEOFBeforeContentSeparator {
        line_number: usize,
        path: String,
    },
    PrematureEOFBeforeContentSeparatorNewline {
        line_number: usize,
        path: String,
    },
    UnexpectedContentAfterLastEntry {
        line_number: usize,
        content_excerpt: String,
    },
}

impl fmt::Display for BundleValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BundleValidationError::ContentBeforeFirstHeader {
                line_number,
                content_excerpt,
            } => write!(
                f,
                "L{}: Content found before the first file header. Starts with: \"{}\"",
                line_number, content_excerpt
            ),
            BundleValidationError::MalformedHeaderMissingFilePrefix {
                line_number,
                header_line,
            } => write!(
                f,
                "L{}: Malformed file header. Expected '{}' after separator line, found: \"{}\"",
                line_number, FILE_PATH_PREFIX, header_line
            ),
            BundleValidationError::MalformedHeaderSeparatorWithExtraContent {
                line_number,
                separator_line_content,
            } => write!(
                f,
                "L{}: Malformed file header. Separator line has unexpected content after '{}': \"{}\"",
                line_number,
                FILE_HEADER_SEPARATOR,
                separator_line_content
                    .trim_start_matches(FILE_HEADER_SEPARATOR)
                    .trim()
            ),
            BundleValidationError::MalformedHeaderMissingSeparatorAfterPath {
                line_number,
                path_line,
            } => write!(
                f,
                "L{}: Malformed file header. Expected separator line after path line, found: \"{}\"",
                line_number, path_line
            ),
            BundleValidationError::MalformedHeaderPathLineInterruptedBySeparator {
                line_number,
                path_line,
            } => write!(
                f,
                "L{}: Malformed file header. File path line is interrupted by a separator: \"{}\"",
                line_number, path_line
            ),
            BundleValidationError::MalformedHeaderPathLineMissingNewline {
                line_number,
                path_line,
            } => write!(
                f,
                "L{}: Malformed file header. File path line does not end with a newline: \"{}\"",
                line_number, path_line
            ),
            BundleValidationError::EmptyPath { line_number } => {
                write!(f, "L{}: File path is empty.", line_number)
            }
            BundleValidationError::AbsolutePathNotAllowed { line_number, path } => write!(
                f,
                "L{}: Absolute path not allowed: \"{}\"",
                line_number, path
            ),
            BundleValidationError::DuplicatePath { line_number, path } => {
                write!(f, "L{}: Duplicate path found: \"{}\"", line_number, path)
            }
            BundleValidationError::PrematureEOFBeforePathLine { line_number } => write!(
                f,
                "L{}: Premature EOF. Expected 'File: <path>' line after separator.",
                line_number
            ),
            BundleValidationError::PrematureEOFBeforeContentSeparator { line_number, path } => {
                write!(
                    f,
                    "L{}: Premature EOF for file \"{}\". Expected second separator line after path.",
                    line_number, path
                )
            }
            BundleValidationError::PrematureEOFBeforeContentSeparatorNewline {
                line_number,
                path,
            } => write!(
                f,
                "L{}: Premature EOF for file \"{}\". Expected newline after content separator.",
                line_number, path
            ),
            BundleValidationError::UnexpectedContentAfterLastEntry {
                line_number,
                content_excerpt,
            } => write!(
                f,
                "L{}: Unexpected content found after the last valid file entry. Starts with: \"{}\"",
                line_number, content_excerpt
            ),
        }
    }
}

/// Container for multiple validation errors.
#[derive(Debug)]
pub struct BundleParseError {
    pub errors: Vec<BundleValidationError>,
}

impl fmt::Display for BundleParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Bundle parsing failed with {} error(s):",
            self.errors.len()
        )?;
        for error in &self.errors {
            writeln!(f, "- {}", error)?;
        }
        Ok(())
    }
}

impl std::error::Error for BundleParseError {}

/// Parses and validates a bundle file, extracting file entries.
///
/// Returns a list of `ParsedEntry` or a `BundleParseError` if validation fails,
/// collecting all format errors encountered.
pub fn parse_bundle(bundle_path: &Path) -> Result<Vec<ParsedEntry>> {
    let bundle_content = fs::read_to_string(bundle_path)
        .with_context(|| format!("Failed to read bundle file: {:?}", bundle_path))?;

    if bundle_content.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    let mut validation_errors = Vec::new();
    let mut paths_seen = HashSet::new();

    let lines: Vec<&str> = bundle_content.lines().collect();

    let mut first_header_line_idx: Option<usize> = None;
    for (idx, line_content) in lines.iter().enumerate() {
        if line_content.trim_start().starts_with(FILE_HEADER_SEPARATOR)
            && idx + 1 < lines.len()
            && lines[idx + 1].trim_start().starts_with(FILE_PATH_PREFIX)
        {
            first_header_line_idx = Some(idx);
            break;
        }
    }

    let mut start_processing_from_line_idx = 0;
    let mut skipped_pre_header_line_numbers: Vec<usize> = Vec::new();

    if let Some(fh_idx) = first_header_line_idx {
        for (line_idx, line_content) in lines.iter().enumerate().take(fh_idx) {
            if !line_content.trim().is_empty() {
                skipped_pre_header_line_numbers.push(line_idx + 1);
            }
        }

        if !skipped_pre_header_line_numbers.is_empty() {
            let min_line = *skipped_pre_header_line_numbers.iter().min().unwrap();
            let max_line = *skipped_pre_header_line_numbers.iter().max().unwrap();
            if min_line == max_line {
                eprintln!(
                    "Warning: Line {} excluded due to content before the first file header.",
                    min_line
                );
            } else {
                eprintln!(
                    "Warning: Lines {}-{} excluded due to content before the first file header.",
                    min_line, max_line
                );
            }
        }
        start_processing_from_line_idx = fh_idx;
    } else if !bundle_content.trim().is_empty() {
        let first_actual_content_line_str = lines
            .iter()
            .find(|line| !line.trim().is_empty())
            .map_or("", |line| line.trim());

        validation_errors.push(BundleValidationError::ContentBeforeFirstHeader {
            line_number: 1,
            content_excerpt: first_actual_content_line_str.chars().take(50).collect(),
        });
    }

    let mut current_bundle_offset = if start_processing_from_line_idx < lines.len() {
        // Calculate the byte offset of the first line to be processed (the first header line).
        // `lines[start_processing_from_line_idx]` is the string slice from `bundle_content`
        // representing this line. Its `.as_ptr()` gives a raw pointer into `bundle_content`.
        // Subtracting `bundle_content.as_ptr()` gives the byte offset.
        lines[start_processing_from_line_idx].as_ptr() as usize - bundle_content.as_ptr() as usize
    } else {
        // If start_processing_from_line_idx is out of bounds (e.g., empty file, or all lines
        // are before the first valid header, or no valid header at all),
        // effectively means no processable content starts at/after this index.
        // Set offset to bundle_content.len() so the main parsing loop condition
        // `current_bundle_offset < bundle_content.len()` will be false.
        bundle_content.len()
    };

    while current_bundle_offset < bundle_content.len() {
        let remaining_content = &bundle_content[current_bundle_offset..];
        let search_start_line = bundle_content[..current_bundle_offset].lines().count();

        match remaining_content.find(FILE_HEADER_SEPARATOR) {
            Some(header_relative_start) => {
                let header_absolute_start = current_bundle_offset + header_relative_start;
                let header_line_number =
                    bundle_content[..header_absolute_start].lines().count() + 1;

                let skipped_content = &bundle_content[current_bundle_offset..header_absolute_start];
                if !skipped_content.trim().is_empty() {
                    validation_errors.push(
                        BundleValidationError::UnexpectedContentAfterLastEntry {
                            line_number: search_start_line,
                            content_excerpt: skipped_content
                                .trim()
                                .lines()
                                .next()
                                .unwrap_or("")
                                .chars()
                                .take(50)
                                .collect(),
                        },
                    );
                }

                let current_separator_line_num = header_line_number;
                let first_separator_line_content = bundle_content[header_absolute_start..]
                    .lines()
                    .next()
                    .unwrap_or("");

                let trailing_content_on_first_sep_line =
                    first_separator_line_content.trim_start_matches(FILE_HEADER_SEPARATOR);

                if !trailing_content_on_first_sep_line.trim().is_empty() {
                    validation_errors.push(
                        BundleValidationError::MalformedHeaderSeparatorWithExtraContent {
                            line_number: current_separator_line_num,
                            separator_line_content: first_separator_line_content.to_string(),
                        },
                    );
                    // Attempt to find the next valid header to continue parsing if possible,
                    // or stop if this error is considered fatal for the entry.
                    // For now, let's assume we try to advance past this malformed line.
                    current_bundle_offset =
                        header_absolute_start + first_separator_line_content.len();
                    if current_bundle_offset < bundle_content.len()
                        && bundle_content.as_bytes()[current_bundle_offset] == b'\n'
                    {
                        current_bundle_offset += 1; // also skip the newline
                    }
                    continue;
                }
                // If we are here, the first separator line is valid (or only has trailing whitespace)

                let mut next_char_offset_after_sep_text =
                    header_absolute_start + first_separator_line_content.len();

                // Advance past potential \r
                if next_char_offset_after_sep_text < bundle_content.len()
                    && bundle_content.as_bytes()[next_char_offset_after_sep_text] == b'\r'
                {
                    next_char_offset_after_sep_text += 1;
                }

                // Now check for \n
                if next_char_offset_after_sep_text >= bundle_content.len()
                    || bundle_content.as_bytes()[next_char_offset_after_sep_text] != b'\n'
                {
                    validation_errors.push(BundleValidationError::PrematureEOFBeforePathLine {
                        line_number: current_separator_line_num,
                    });
                    current_bundle_offset = bundle_content.len();
                    continue;
                }

                let path_line_num = current_separator_line_num + 1;
                let path_line_start = next_char_offset_after_sep_text + 1; // Start of the actual next line
                if path_line_start >= bundle_content.len() {
                    validation_errors.push(BundleValidationError::PrematureEOFBeforePathLine {
                        line_number: path_line_num,
                    });
                    current_bundle_offset = bundle_content.len();
                    continue;
                }
                if !bundle_content[path_line_start..].starts_with(FILE_PATH_PREFIX) {
                    validation_errors.push(
                        BundleValidationError::MalformedHeaderMissingFilePrefix {
                            line_number: path_line_num,
                            header_line: bundle_content[path_line_start..]
                                .lines()
                                .next()
                                .unwrap_or("")
                                .to_string(),
                        },
                    );
                    current_bundle_offset = bundle_content.len();
                    continue;
                }

                let path_actual_start = path_line_start + FILE_PATH_PREFIX.len();
                let path_line_terminator_search_slice = &bundle_content[path_actual_start..];
                let newline_pos_in_slice = path_line_terminator_search_slice.find('\n');

                let path_str_end_offset = match newline_pos_in_slice {
                    Some(nl_idx) => {
                        if path_line_terminator_search_slice[..nl_idx]
                            .contains(FILE_HEADER_SEPARATOR)
                        {
                            validation_errors.push(BundleValidationError::MalformedHeaderPathLineInterruptedBySeparator {
                                line_number: path_line_num,
                                path_line: bundle_content[path_actual_start .. path_actual_start + nl_idx].trim_end().to_string(),
                            });
                            current_bundle_offset = bundle_content.len();
                            continue;
                        }
                        path_actual_start + nl_idx
                    }
                    None => {
                        validation_errors.push(
                            BundleValidationError::MalformedHeaderPathLineMissingNewline {
                                line_number: path_line_num,
                                path_line: path_line_terminator_search_slice
                                    .lines()
                                    .next()
                                    .unwrap_or("")
                                    .trim_end()
                                    .to_string(),
                            },
                        );
                        current_bundle_offset = bundle_content.len();
                        continue;
                    }
                };

                let file_path_str = bundle_content[path_actual_start..path_str_end_offset].trim();
                if file_path_str.is_empty() {
                    validation_errors.push(BundleValidationError::EmptyPath {
                        line_number: path_line_num,
                    });
                }

                let path = PathBuf::from(file_path_str);
                // This variable will track if the current entry is valid for actual use,
                // considering emptiness, path type, and duplication.
                let mut is_valid_for_adding_to_entries = !file_path_str.is_empty();

                if !file_path_str.is_empty() {
                    let first_component = path.components().next();
                    let is_problematic_path_type = path.is_absolute()
                        || matches!(
                            first_component,
                            Some(std::path::Component::RootDir)
                                | Some(std::path::Component::Prefix(_))
                        );

                    if is_problematic_path_type {
                        validation_errors.push(BundleValidationError::AbsolutePathNotAllowed {
                            line_number: path_line_num,
                            path: file_path_str.to_string(),
                        });
                        is_valid_for_adding_to_entries = false;
                    }

                    // For duplicate check: only consider if not already invalidated by path type.
                    // `paths_seen` should only store valid, relative paths.
                    if is_valid_for_adding_to_entries && !paths_seen.insert(path.clone()) {
                        validation_errors.push(BundleValidationError::DuplicatePath {
                            line_number: path_line_num,
                            path: file_path_str.to_string(),
                        });
                        is_valid_for_adding_to_entries = false; // Mark as invalid if duplicate
                    }
                }
                // If file_path_str was empty, is_valid_for_adding_to_entries is already false,
                // and an EmptyPath error was added earlier.

                let second_sep_line_num = path_line_num + 1;

                let second_sep_start = path_str_end_offset + 1;
                if second_sep_start >= bundle_content.len() {
                    validation_errors.push(
                        BundleValidationError::PrematureEOFBeforeContentSeparator {
                            line_number: second_sep_line_num,
                            path: file_path_str.to_string(),
                        },
                    );
                    current_bundle_offset = bundle_content.len();
                    continue;
                }
                if !bundle_content[second_sep_start..].starts_with(FILE_HEADER_SEPARATOR) {
                    validation_errors.push(
                        BundleValidationError::MalformedHeaderMissingSeparatorAfterPath {
                            line_number: second_sep_line_num,
                            path_line: file_path_str.to_string(),
                        },
                    );
                    current_bundle_offset = bundle_content.len();
                    continue;
                }

                // Validate the second separator line
                let second_separator_line_content = bundle_content[second_sep_start..]
                    .lines()
                    .next()
                    .unwrap_or("");

                let trailing_content_on_second_sep_line =
                    second_separator_line_content.trim_start_matches(FILE_HEADER_SEPARATOR);

                if !trailing_content_on_second_sep_line.trim().is_empty() {
                    validation_errors.push(
                        BundleValidationError::MalformedHeaderSeparatorWithExtraContent {
                            line_number: second_sep_line_num,
                            separator_line_content: second_separator_line_content.to_string(),
                        },
                    );
                    current_bundle_offset = second_sep_start + second_separator_line_content.len();
                    if current_bundle_offset < bundle_content.len()
                        && bundle_content.as_bytes()[current_bundle_offset] == b'\n'
                    {
                        current_bundle_offset += 1;
                    }
                    continue;
                }
                // If we are here, the second separator line is valid (or only has trailing whitespace)

                let mut next_char_offset_after_second_sep_text =
                    second_sep_start + second_separator_line_content.len();

                // Advance past potential \r
                if next_char_offset_after_second_sep_text < bundle_content.len()
                    && bundle_content.as_bytes()[next_char_offset_after_second_sep_text] == b'\r'
                {
                    next_char_offset_after_second_sep_text += 1;
                }

                // Now check for \n
                if next_char_offset_after_second_sep_text >= bundle_content.len()
                    || bundle_content.as_bytes()[next_char_offset_after_second_sep_text] != b'\n'
                {
                    validation_errors.push(
                        BundleValidationError::PrematureEOFBeforeContentSeparatorNewline {
                            line_number: second_sep_line_num,
                            path: file_path_str.to_string(),
                        },
                    );
                    current_bundle_offset = bundle_content.len();
                    continue;
                }

                let content_actual_start = next_char_offset_after_second_sep_text + 1; // Start of the actual content line

                let next_entry_header_search_start = content_actual_start;
                let content_end_offset = bundle_content[next_entry_header_search_start..]
                    .find(FILE_HEADER_SEPARATOR)
                    .map(|pos| next_entry_header_search_start + pos)
                    .unwrap_or_else(|| bundle_content.len());

                let content = bundle_content[content_actual_start..content_end_offset].to_string();

                if is_valid_for_adding_to_entries {
                    entries.push(ParsedEntry { path, content });
                }

                current_bundle_offset = content_end_offset;
            }
            None => {
                let final_remaining_content = &bundle_content[current_bundle_offset..];
                if !final_remaining_content.trim().is_empty() && !entries.is_empty() {
                    validation_errors.push(
                        BundleValidationError::UnexpectedContentAfterLastEntry {
                            line_number: bundle_content[..current_bundle_offset].lines().count()
                                + 1,
                            content_excerpt: final_remaining_content
                                .trim()
                                .lines()
                                .next()
                                .unwrap_or("")
                                .chars()
                                .take(50)
                                .collect(),
                        },
                    );
                }
                current_bundle_offset = bundle_content.len();
            }
        }
    }

    if !validation_errors.is_empty() {
        return Err(anyhow!(BundleParseError {
            errors: validation_errors
        }));
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_bundle_file(content: &str) -> NamedTempFile {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        write!(temp_file, "{}", content).expect("Failed to write to temp file");
        temp_file
    }

    fn assert_specific_error(
        result: &Result<Vec<ParsedEntry>, anyhow::Error>,
        expected_error: BundleValidationError,
    ) {
        match result {
            Err(err) => {
                if let Some(bundle_parse_error) = err.downcast_ref::<BundleParseError>() {
                    assert!(
                        bundle_parse_error.errors.contains(&expected_error),
                        "Expected error {:?} not found in {:?}",
                        expected_error,
                        bundle_parse_error.errors
                    );
                } else {
                    panic!("Error is not a BundleParseError: {:?}", err);
                }
            }
            Ok(_) => panic!("Expected error, but got Ok"),
        }
    }

    #[test]
    fn test_parse_empty_bundle_file() {
        let temp_file = create_temp_bundle_file("");
        let entries = parse_bundle(temp_file.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_bundle_file_with_only_whitespace() {
        let temp_file = create_temp_bundle_file("   \n\t  \n");
        let entries = parse_bundle(temp_file.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_error_content_before_first_header() {
        let bundle_content = format!(
            "Some introductory text.\n\
            {}\n\
            {}path/to/file1.txt\n\
            {}\n\
            Content of file1.",
            FILE_HEADER_SEPARATOR, FILE_PATH_PREFIX, FILE_HEADER_SEPARATOR
        );
        let temp_file = create_temp_bundle_file(&bundle_content);
        let result = parse_bundle(temp_file.path());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let entries = result.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, PathBuf::from("path/to/file1.txt"));
        assert_eq!(entries[0].content, "Content of file1.");
    }

    #[test]
    fn test_error_content_before_first_header_no_valid_header_at_all() {
        let temp_file =
            create_temp_bundle_file("This is just some text, no valid file entries at all.");
        let result = parse_bundle(temp_file.path());
        assert_specific_error(
            &result,
            BundleValidationError::ContentBeforeFirstHeader {
                line_number: 1,
                content_excerpt: "This is just some text, no valid file entries at a".to_string(),
            },
        );
    }

    #[test]
    fn test_parse_single_entry() {
        let bundle_content = format!(
            "{}\n\
            {}file.txt\n\
            {}\n\
            Hello, world!",
            FILE_HEADER_SEPARATOR, FILE_PATH_PREFIX, FILE_HEADER_SEPARATOR
        );
        let temp_file = create_temp_bundle_file(&bundle_content);
        let entries = parse_bundle(temp_file.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, PathBuf::from("file.txt"));
        assert_eq!(entries[0].content, "Hello, world!");
    }

    #[test]
    fn test_parse_multiple_entries() {
        let bundle_content = format!(
            "{}\n\
            {}file1.txt\n\
            {}\n\
            Content of file1.\n\
            {}\n\
            {}path/to/file2.rs\n\
            {}\n\
            // Rust code\nfn main() {{}}\n\
            {}\n\
            {}another.md\n\
            {}\n\
            ## Markdown Content",
            FILE_HEADER_SEPARATOR,
            FILE_PATH_PREFIX,
            FILE_HEADER_SEPARATOR,
            FILE_HEADER_SEPARATOR,
            FILE_PATH_PREFIX,
            FILE_HEADER_SEPARATOR,
            FILE_HEADER_SEPARATOR,
            FILE_PATH_PREFIX,
            FILE_HEADER_SEPARATOR
        );
        let temp_file = create_temp_bundle_file(&bundle_content);
        let entries = parse_bundle(temp_file.path()).unwrap();
        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].path, PathBuf::from("file1.txt"));
        assert_eq!(entries[0].content, "Content of file1.\n");

        assert_eq!(entries[1].path, PathBuf::from("path/to/file2.rs"));
        assert_eq!(entries[1].content, "// Rust code\nfn main() {}\n");

        assert_eq!(entries[2].path, PathBuf::from("another.md"));
        assert_eq!(entries[2].content, "## Markdown Content");
    }

    #[test]
    fn test_parse_entry_with_empty_content() {
        let bundle_content = format!(
            "{}\n\
            {}empty_file.txt\n\
            {}\n",
            FILE_HEADER_SEPARATOR, FILE_PATH_PREFIX, FILE_HEADER_SEPARATOR
        );
        let temp_file = create_temp_bundle_file(&bundle_content);
        let entries = parse_bundle(temp_file.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, PathBuf::from("empty_file.txt"));
        assert_eq!(entries[0].content, "");
    }

    #[test]
    fn test_error_malformed_header_missing_file_prefix() {
        let bundle_content = format!(
            "{}\n\
            Not File: path/to/file.txt\n\
            {}\n\
            Content",
            FILE_HEADER_SEPARATOR, FILE_HEADER_SEPARATOR
        );
        let temp_file = create_temp_bundle_file(&bundle_content);
        let result = parse_bundle(temp_file.path());
        assert_specific_error(
            &result,
            BundleValidationError::MalformedHeaderMissingFilePrefix {
                line_number: 2,
                header_line: "Not File: path/to/file.txt".to_string(),
            },
        );
    }

    #[test]
    fn test_error_malformed_header_missing_separator_after_path() {
        let bundle_content = format!(
            "{}\n\
            {}path/to/file.txt\n\
            Content without second separator",
            FILE_HEADER_SEPARATOR, FILE_PATH_PREFIX
        );
        let temp_file = create_temp_bundle_file(&bundle_content);
        let result = parse_bundle(temp_file.path());
        assert_specific_error(
            &result,
            BundleValidationError::MalformedHeaderMissingSeparatorAfterPath {
                line_number: 3,
                path_line: "path/to/file.txt".to_string(),
            },
        );
    }

    #[test]
    fn test_error_path_line_interrupted_by_separator() {
        let bundle_content = format!(
            "{}\n\
            {}path/to{}file.txt\n\
            {}\n\
            Content",
            FILE_HEADER_SEPARATOR, FILE_PATH_PREFIX, FILE_HEADER_SEPARATOR, FILE_HEADER_SEPARATOR
        );
        let temp_file = create_temp_bundle_file(&bundle_content);
        let result = parse_bundle(temp_file.path());
        assert_specific_error(
            &result,
            BundleValidationError::MalformedHeaderPathLineInterruptedBySeparator {
                line_number: 2,
                path_line: format!("path/to{}file.txt", FILE_HEADER_SEPARATOR),
            },
        );
    }

    #[test]
    fn test_error_path_line_missing_newline() {
        let bundle_content = format!(
            "{}\n\
            {}path/to/file.txt",
            FILE_HEADER_SEPARATOR, FILE_PATH_PREFIX
        );
        let temp_file = create_temp_bundle_file(&bundle_content);
        let result = parse_bundle(temp_file.path());
        assert_specific_error(
            &result,
            BundleValidationError::MalformedHeaderPathLineMissingNewline {
                line_number: 2,
                path_line: "path/to/file.txt".to_string(),
            },
        );
    }

    #[test]
    fn test_error_empty_path() {
        let bundle_content = format!(
            "{}\n\
            {}\n\
            {}\n\
            Content",
            FILE_HEADER_SEPARATOR, FILE_PATH_PREFIX, FILE_HEADER_SEPARATOR
        );
        let temp_file = create_temp_bundle_file(&bundle_content);
        let result = parse_bundle(temp_file.path());
        assert_specific_error(&result, BundleValidationError::EmptyPath { line_number: 2 });
    }

    #[test]
    fn test_error_absolute_path() {
        let absolute_path_str = "/an/absolute/path.txt";
        let bundle_content = format!(
            "{}\n\
            {}{}\n\
            {}\n\
            Content",
            FILE_HEADER_SEPARATOR, FILE_PATH_PREFIX, absolute_path_str, FILE_HEADER_SEPARATOR
        );
        let temp_file = create_temp_bundle_file(&bundle_content);
        let result = parse_bundle(temp_file.path());
        assert_specific_error(
            &result,
            BundleValidationError::AbsolutePathNotAllowed {
                line_number: 2,
                path: absolute_path_str.to_string(),
            },
        );
    }

    #[test]
    fn test_error_duplicate_path() {
        let bundle_content = format!(
            "{}\n\
            {}file.txt\n\
            {}\n\
            Content1\n\
            {}\n\
            {}file.txt\n\
            {}\n\
            Content2",
            FILE_HEADER_SEPARATOR,
            FILE_PATH_PREFIX,
            FILE_HEADER_SEPARATOR,
            FILE_HEADER_SEPARATOR,
            FILE_PATH_PREFIX,
            FILE_HEADER_SEPARATOR
        );
        let temp_file = create_temp_bundle_file(&bundle_content);
        let result = parse_bundle(temp_file.path());
        assert_specific_error(
            &result,
            BundleValidationError::DuplicatePath {
                line_number: 6,
                path: "file.txt".to_string(),
            },
        );
    }

    #[test]
    fn test_error_premature_eof_after_first_separator() {
        let bundle_content = FILE_HEADER_SEPARATOR;
        let temp_file = create_temp_bundle_file(bundle_content);
        let result = parse_bundle(temp_file.path());
        assert_specific_error(
            &result,
            BundleValidationError::PrematureEOFBeforePathLine { line_number: 1 },
        );
    }

    #[test]
    fn test_error_premature_eof_after_file_prefix() {
        let bundle_content = format!("{}\n{}", FILE_HEADER_SEPARATOR, FILE_PATH_PREFIX);
        let temp_file = create_temp_bundle_file(&bundle_content);
        let result = parse_bundle(temp_file.path());
        assert_specific_error(
            &result,
            BundleValidationError::MalformedHeaderPathLineMissingNewline {
                line_number: 2,
                path_line: "".to_string(),
            },
        );
    }

    #[test]
    fn test_error_unexpected_content_after_last_entry() {
        let bundle_content = format!(
            "{}\n\
            {}file.txt\n\
            {}\n\
            Content\n\
            Some trailing garbage text.",
            FILE_HEADER_SEPARATOR, FILE_PATH_PREFIX, FILE_HEADER_SEPARATOR
        );
        let temp_file = create_temp_bundle_file(&bundle_content);
        let result = parse_bundle(temp_file.path());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let entries = result.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, PathBuf::from("file.txt"));
        assert_eq!(entries[0].content, "Content\nSome trailing garbage text.");
    }

    #[test]
    fn test_multiple_errors_reported() {
        let bundle_content = format!(
            "Leading garbage.\n\
            {}\n\
            {}/abs/path.txt\n\
            {}\n\
            Content1\n\
            {}\n\
            {}\n\
            {}\n\
            Content2\n\
            Trailing garbage.",
            FILE_HEADER_SEPARATOR,
            FILE_PATH_PREFIX,
            FILE_HEADER_SEPARATOR,
            FILE_HEADER_SEPARATOR,
            FILE_PATH_PREFIX,
            FILE_HEADER_SEPARATOR
        );
        let temp_file = create_temp_bundle_file(&bundle_content);
        let result = parse_bundle(temp_file.path());

        assert!(result.is_err());
        if let Err(err) = result {
            if let Some(bundle_parse_error) = err.downcast_ref::<BundleParseError>() {
                assert_eq!(
                    bundle_parse_error.errors.len(),
                    2,
                    "Expected 2 errors, got {}. Errors: {:?}",
                    bundle_parse_error.errors.len(),
                    bundle_parse_error.errors
                );

                assert!(
                    !bundle_parse_error.errors.contains(
                        &BundleValidationError::ContentBeforeFirstHeader {
                            line_number: 1,
                            content_excerpt: "Leading garbage.".to_string()
                        }
                    ),
                    "ContentBeforeFirstHeader should now be a warning, not an error."
                );

                assert!(bundle_parse_error.errors.contains(
                    &BundleValidationError::AbsolutePathNotAllowed {
                        line_number: 3,
                        path: "/abs/path.txt".to_string()
                    }
                ));
                assert!(
                    bundle_parse_error
                        .errors
                        .contains(&BundleValidationError::EmptyPath { line_number: 7 })
                );
            } else {
                panic!("Error is not a BundleParseError: {:?}", err);
            }
        } else {
            panic!("Expected an error, but got Ok. Result: {:?}", result);
        }
    }

    #[test]
    fn test_error_premature_eof_after_path_line() {
        let content = format!(
            "{}\n{}incomplete_path.txt\n{}", // No newline after last separator
            FILE_HEADER_SEPARATOR, FILE_PATH_PREFIX, FILE_HEADER_SEPARATOR
        );
        let temp_file = create_temp_bundle_file(&content);
        let result = parse_bundle(temp_file.path());

        assert_specific_error(
            &result,
            BundleValidationError::PrematureEOFBeforeContentSeparatorNewline {
                line_number: 3, // The line of the second separator
                path: "incomplete_path.txt".to_string(),
            },
        );
    }

    #[test]
    fn test_error_premature_eof_before_content_separator_after_valid_path() {
        let content = format!(
            "{}\n{}a/valid/path.txt", // EOF right after the path line
            FILE_HEADER_SEPARATOR, FILE_PATH_PREFIX
        );
        let temp_file = create_temp_bundle_file(&content);
        let result = parse_bundle(temp_file.path());

        assert_specific_error(
            &result,
            BundleValidationError::MalformedHeaderPathLineMissingNewline {
                line_number: 2, // The line of the path
                path_line: "a/valid/path.txt".to_string(),
            },
        );
    }

    #[test]
    fn test_display_malformed_header_path_line_missing_newline() {
        let error = BundleValidationError::MalformedHeaderPathLineMissingNewline {
            line_number: 50,
            path_line: "path/to/file.txt".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "L50: Malformed file header. File path line does not end with a newline: \"path/to/file.txt\""
        );
    }

    #[test]
    fn test_display_empty_path() {
        let error = BundleValidationError::EmptyPath { line_number: 70 };
        assert_eq!(error.to_string(), "L70: File path is empty.");
    }

    #[test]
    fn test_display_absolute_path() {
        let error = BundleValidationError::AbsolutePathNotAllowed {
            line_number: 80,
            path: "/abs/path/to/file.txt".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "L80: Absolute path not allowed: \"/abs/path/to/file.txt\""
        );
    }

    #[test]
    fn test_display_duplicate_path() {
        let error = BundleValidationError::DuplicatePath {
            line_number: 90,
            path: "duplicate/path.txt".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "L90: Duplicate path found: \"duplicate/path.txt\""
        );
    }

    #[test]
    fn test_display_premature_eof_before_path_line() {
        let error = BundleValidationError::PrematureEOFBeforePathLine { line_number: 100 };
        assert_eq!(
            error.to_string(),
            "L100: Premature EOF. Expected 'File: <path>' line after separator."
        );
    }

    #[test]
    fn test_display_premature_eof_before_content_separator() {
        let error = BundleValidationError::PrematureEOFBeforeContentSeparator {
            line_number: 110,
            path: "file/path.txt".to_string(),
        };
        let error_str = format!("{}", error);
        // Expected based on Display impl: "L110: Premature EOF for file \"file/path.txt\". Expected second separator line after path."
        assert!(
            error_str.contains("L110"),
            "Error string should contain formatted line number with L prefix"
        );
        assert!(
            error_str.contains("Premature EOF"),
            "Error string should contain 'Premature EOF'"
        );
        assert!(
            error_str.contains("file \"file/path.txt\""),
            "Error string should contain formatted path as file identifier"
        );
        assert!(
            error_str.contains("Expected second separator line after path."),
            "Error string should contain the specific cause"
        );
    }

    #[test]
    fn test_display_premature_eof_before_content_separator_newline() {
        let error = BundleValidationError::PrematureEOFBeforeContentSeparatorNewline {
            line_number: 120,
            path: "another/file.txt".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "L120: Premature EOF for file \"another/file.txt\". Expected newline after content separator."
        );
    }

    #[test]
    fn test_display_unexpected_content_after_last_entry() {
        let error = BundleValidationError::UnexpectedContentAfterLastEntry {
            line_number: 15,
            content_excerpt: "extra stuff".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "L15: Unexpected content found after the last valid file entry. Starts with: \"extra stuff\""
        );
    }

    #[test]
    fn test_display_malformed_header_missing_file_prefix() {
        let error = BundleValidationError::MalformedHeaderMissingFilePrefix {
            line_number: 2,
            header_line: "Not File:".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "L2: Malformed file header. Expected 'File: ' after separator line, found: \"Not File:\""
        );
    }

    #[test]
    fn test_display_malformed_header_missing_separator_after_path() {
        let error = BundleValidationError::MalformedHeaderMissingSeparatorAfterPath {
            line_number: 3,
            path_line: "File: path/to/file.txt".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "L3: Malformed file header. Expected separator line after path line, found: \"File: path/to/file.txt\""
        );
    }

    #[test]
    fn test_display_malformed_header_path_line_interrupted_by_separator() {
        let error = BundleValidationError::MalformedHeaderPathLineInterruptedBySeparator {
            line_number: 4,
            path_line: "File: path/================================================/file.txt"
                .to_string(),
        };
        assert_eq!(
            error.to_string(),
            "L4: Malformed file header. File path line is interrupted by a separator: \"File: path/================================================/file.txt\""
        );
    }
}
