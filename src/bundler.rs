// src/bundler.rs
// Module for file/directory creation and output logic

use crate::parser::ParsedEntry;
use anyhow::{Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Creates directories and files from parsed bundle entries.
///
/// Assumes that bundle parsing and collision checks (if `force` is false) have already passed.
/// Parent directories are created as needed. If `force` is true, `fs::write` will
/// overwrite existing files. I/O errors are propagated.
pub fn create_files_from_bundle(
    entries: &[ParsedEntry],
    output_dir: &Path,
    _force: bool, // Indicate unused variable, logic is handled by skipping collision check
) -> Result<()> {
    for entry in entries {
        let full_target_path = output_dir.join(&entry.path);

        // If forcing, we don't care if the file exists, but we still need to ensure parent dirs are there.
        // If not forcing, collision check should have already happened.
        if let Some(parent_path) = full_target_path.parent() {
            if !parent_path.exists() {
                fs::create_dir_all(parent_path).with_context(|| {
                    format!("Failed to create parent directory: {:?}", parent_path)
                })?;
            } else if parent_path.is_file() {
                // This case should ideally be caught by check_for_collisions if not forcing.
                // If forcing, and a parent path component is a file, fs::write will fail later.
                // This is a safeguard or clarity, fs::write would fail anyway.
                return Err(anyhow::anyhow!(
                    "Cannot create file {:?}, its parent {:?} is an existing file.",
                    full_target_path,
                    parent_path
                ));
            }
        }

        fs::write(&full_target_path, &entry.content)
            .with_context(|| format!("Failed to write file: {:?}", full_target_path))?;
    }
    Ok(())
}

/// Checks for path collisions in the output directory before any files are written.
///
/// Verifies that no target path from `entries` already exists or would conflict
/// with directory creation (e.g., a file exists where a directory needs to be created).
/// Returns an error detailing all collisions if any are found.
pub fn check_for_collisions(entries: &[ParsedEntry], output_dir: &Path) -> Result<()> {
    let mut collisions = Vec::new();

    for entry in entries {
        let target_path = output_dir.join(&entry.path);
        if target_path.exists() {
            collisions.push(target_path);
        } else {
            let mut current_check_path = PathBuf::new();
            for component in entry
                .path
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .components()
            {
                current_check_path.push(component);
                let full_component_path = output_dir.join(&current_check_path);
                if full_component_path.is_file()
                    && entry
                        .path
                        .strip_prefix(&current_check_path)
                        .is_ok_and(|p| !p.as_os_str().is_empty())
                {
                    collisions.push(full_component_path);
                    break;
                }
            }
        }
    }

    if !collisions.is_empty() {
        let collision_details = collisions
            .iter()
            .map(|p| format!("  - {}", p.display()))
            .collect::<Vec<String>>()
            .join("\n");
        return Err(anyhow::anyhow!(
            "Output path collision detected. The following paths already exist or conflict with directory creation:\n{}",
            collision_details
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParsedEntry;
    use std::fs::{self, File};
    use tempfile::tempdir;

    fn create_parsed_entry(path_str: &str, content_str: &str) -> ParsedEntry {
        ParsedEntry {
            path: PathBuf::from(path_str),
            content: String::from(content_str),
        }
    }

    #[test]
    fn test_check_for_collisions_no_collision() {
        let dir = tempdir().unwrap();
        let output_dir = dir.path();
        let entries = vec![
            create_parsed_entry("file1.txt", "content1"),
            create_parsed_entry("dir1/file2.txt", "content2"),
        ];

        let result = check_for_collisions(&entries, output_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_for_collisions_single_file_collision() {
        let dir = tempdir().unwrap();
        let output_dir = dir.path();
        File::create(output_dir.join("file1.txt")).unwrap();

        let entries = vec![
            create_parsed_entry("file1.txt", "content1"),
            create_parsed_entry("file2.txt", "content2"),
        ];

        let result = check_for_collisions(&entries, output_dir);
        assert!(result.is_err());
        let error_message = result.err().unwrap().to_string();
        assert!(error_message.contains("Output path collision detected"));
        assert!(error_message.contains(&output_dir.join("file1.txt").display().to_string()));
    }

    #[test]
    fn test_check_for_collisions_multiple_file_collisions() {
        let dir = tempdir().unwrap();
        let output_dir = dir.path();
        File::create(output_dir.join("file1.txt")).unwrap();
        fs::create_dir_all(output_dir.join("dir1")).unwrap();
        File::create(output_dir.join("dir1/file2.txt")).unwrap();

        let entries = vec![
            create_parsed_entry("file1.txt", "c1"),
            create_parsed_entry("dir1/file2.txt", "c2"),
            create_parsed_entry("file3.txt", "c3"),
        ];

        let result = check_for_collisions(&entries, output_dir);
        assert!(result.is_err());
        let error_message = result.err().unwrap().to_string();
        assert!(error_message.contains(&output_dir.join("file1.txt").display().to_string()));
        assert!(error_message.contains(&output_dir.join("dir1/file2.txt").display().to_string()));
    }

    #[test]
    fn test_check_for_collisions_directory_as_file_collision() {
        let dir = tempdir().unwrap();
        let output_dir = dir.path();
        fs::create_dir_all(output_dir.join("item")).unwrap();

        let entries = vec![create_parsed_entry("item", "content")];

        let result = check_for_collisions(&entries, output_dir);
        assert!(result.is_err());
        let error_message = result.err().unwrap().to_string();
        assert!(error_message.contains(&output_dir.join("item").display().to_string()));
    }

    #[test]
    fn test_check_for_collisions_file_as_directory_collision() {
        let dir = tempdir().unwrap();
        let output_dir = dir.path();
        File::create(output_dir.join("item")).unwrap();

        let entries = vec![create_parsed_entry("item/another.txt", "content")];

        let result = check_for_collisions(&entries, output_dir);
        assert!(result.is_err());
        let error_message = result.err().unwrap().to_string();
        assert!(error_message.contains(&output_dir.join("item").display().to_string()));
        assert!(error_message.contains("conflict with directory creation"));
    }

    #[test]
    fn test_check_for_collisions_deep_file_as_directory_collision() {
        let dir = tempdir().unwrap();
        let output_dir = dir.path();
        fs::create_dir_all(output_dir.join("level1")).unwrap();
        File::create(output_dir.join("level1/item")).unwrap();

        let entries = vec![create_parsed_entry("level1/item/another.txt", "content")];

        let result = check_for_collisions(&entries, output_dir);
        assert!(result.is_err());
        let error_message = result.err().unwrap().to_string();
        // Different OS path separators might cause issues, so we compare with both forms
        let expected_path = output_dir.join("level1").join("item");
        assert!(
            error_message.contains(&expected_path.display().to_string())
                || error_message.contains(&expected_path.display().to_string().replace("\\", "/")),
            "Error message '{}' doesn't contain path '{}'",
            error_message,
            expected_path.display()
        );
    }

    #[test]
    fn test_create_single_file() -> Result<()> {
        let dir = tempdir()?;
        let output_dir = dir.path();
        let entries = vec![create_parsed_entry("file1.txt", "Hello World")];

        create_files_from_bundle(&entries, output_dir, false)?;

        let file_path = output_dir.join("file1.txt");
        assert!(file_path.exists());
        assert_eq!(fs::read_to_string(file_path)?, "Hello World");
        Ok(())
    }

    #[test]
    fn test_create_multiple_files() -> Result<()> {
        let dir = tempdir()?;
        let output_dir = dir.path();
        let entries = vec![
            create_parsed_entry("file1.txt", "Content 1"),
            create_parsed_entry("file2.txt", "Content 2"),
        ];

        create_files_from_bundle(&entries, output_dir, false)?;

        let file_path1 = output_dir.join("file1.txt");
        assert!(file_path1.exists());
        assert_eq!(fs::read_to_string(file_path1)?, "Content 1");

        let file_path2 = output_dir.join("file2.txt");
        assert!(file_path2.exists());
        assert_eq!(fs::read_to_string(file_path2)?, "Content 2");
        Ok(())
    }

    #[test]
    fn test_create_files_in_nested_directories() -> Result<()> {
        let dir = tempdir()?;
        let output_dir = dir.path();
        let entries = vec![
            create_parsed_entry("dir1/file1.txt", "Nested Content 1"),
            create_parsed_entry("dir1/dir2/file2.txt", "Deeply Nested Content 2"),
            create_parsed_entry("file3.txt", "Root Content 3"),
        ];

        create_files_from_bundle(&entries, output_dir, false)?;

        let path1 = output_dir.join("dir1/file1.txt");
        assert!(path1.exists());
        assert_eq!(fs::read_to_string(path1)?, "Nested Content 1");
        assert!(output_dir.join("dir1").is_dir());

        let path2 = output_dir.join("dir1/dir2/file2.txt");
        assert!(path2.exists());
        assert_eq!(fs::read_to_string(path2)?, "Deeply Nested Content 2");
        assert!(output_dir.join("dir1/dir2").is_dir());

        let path3 = output_dir.join("file3.txt");
        assert!(path3.exists());
        assert_eq!(fs::read_to_string(path3)?, "Root Content 3");
        Ok(())
    }

    #[test]
    fn test_create_file_with_empty_content() -> Result<()> {
        let dir = tempdir()?;
        let output_dir = dir.path();
        let entries = vec![create_parsed_entry("empty.txt", "")];

        create_files_from_bundle(&entries, output_dir, false)?;

        let file_path = output_dir.join("empty.txt");
        assert!(file_path.exists());
        assert_eq!(fs::read_to_string(file_path)?, "");
        Ok(())
    }

    #[test]
    fn test_create_files_complex_paths_and_content() -> Result<()> {
        let dir = tempdir()?;
        let output_dir = dir.path();
        let entries = vec![
            create_parsed_entry("src/main.rs", "fn main() {\n    println!(\"Hello\");\n}"),
            create_parsed_entry("docs/README.md", "# My Project\n\nThis is a test."),
            create_parsed_entry("config/settings.toml", "key = \"value\"\nnumber = 123"),
        ];

        create_files_from_bundle(&entries, output_dir, false)?;

        let path_rs = output_dir.join("src/main.rs");
        assert!(path_rs.exists());
        assert_eq!(
            fs::read_to_string(path_rs)?,
            "fn main() {\n    println!(\"Hello\");\n}"
        );
        assert!(output_dir.join("src").is_dir());

        let path_md = output_dir.join("docs/README.md");
        assert!(path_md.exists());
        assert_eq!(
            fs::read_to_string(path_md)?,
            "# My Project\n\nThis is a test."
        );
        assert!(output_dir.join("docs").is_dir());

        let path_toml = output_dir.join("config/settings.toml");
        assert!(path_toml.exists());
        assert_eq!(
            fs::read_to_string(path_toml)?,
            "key = \"value\"\nnumber = 123"
        );
        assert!(output_dir.join("config").is_dir());

        Ok(())
    }

    #[test]
    fn test_create_files_overwrite_with_force() -> Result<()> {
        let dir = tempdir()?;
        let output_dir = dir.path();
        let file_path = output_dir.join("file1.txt");

        fs::write(&file_path, "Initial Content")?;
        assert_eq!(fs::read_to_string(&file_path)?, "Initial Content");

        let entries = vec![create_parsed_entry("file1.txt", "Overwritten Content")];

        create_files_from_bundle(&entries, output_dir, true)?;

        assert!(file_path.exists());
        assert_eq!(fs::read_to_string(&file_path)?, "Overwritten Content");
        Ok(())
    }

    #[test]
    fn test_create_files_fail_on_parent_is_file_even_with_force() -> Result<()> {
        let dir = tempdir()?;
        let output_dir = dir.path();
        let file_acting_as_parent_path = output_dir.join("parent_file");

        fs::write(&file_acting_as_parent_path, "I am a file, not a directory.")?;

        let entries = vec![create_parsed_entry(
            "parent_file/child.txt",
            "This should not be written.",
        )];

        let result = create_files_from_bundle(&entries, output_dir, true);

        assert!(result.is_err());
        let error_message = result.err().unwrap().to_string();
        assert!(error_message.contains("its parent"));
        assert!(error_message.contains("is an existing file"));

        // Ensure the original "parent_file" is untouched and no "child.txt" was created
        assert_eq!(
            fs::read_to_string(&file_acting_as_parent_path)?,
            "I am a file, not a directory."
        );
        assert!(!output_dir.join("parent_file/child.txt").exists());

        Ok(())
    }

    #[test]
    fn test_create_files_parent_creation_failure_due_to_file_ancestor() -> Result<()> {
        let dir = tempdir()?;
        let output_dir = dir.path();

        let ancestor_file_path = output_dir.join("ancestor_is_a_file");
        fs::write(
            &ancestor_file_path,
            "I am a file, blocking directory creation.",
        )?;

        let entries = vec![create_parsed_entry(
            "ancestor_is_a_file/new_subdir/target_file.txt",
            "This content should not be written.",
        )];

        let result = create_files_from_bundle(&entries, output_dir, false);

        assert!(
            result.is_err(),
            "Expected an error due to parent creation failure"
        );
        let error_message = result.err().unwrap().to_string();

        // Check for the specific context message from line 34
        let expected_parent_path_to_fail = output_dir.join("ancestor_is_a_file/new_subdir");
        assert!(
            error_message.contains(&format!(
                "Failed to create parent directory: {:?}",
                expected_parent_path_to_fail
            )),
            "Error message did not contain the expected context. Got: {}",
            error_message
        );

        // Ensure no part of the new path was created
        assert!(!output_dir.join("ancestor_is_a_file/new_subdir").exists());
        assert!(
            !output_dir
                .join("ancestor_is_a_file/new_subdir/target_file.txt")
                .exists()
        );

        Ok(())
    }

    #[test]
    fn test_check_for_collisions_path_parent_is_none() {
        // Use an empty path for output_dir. An empty path does not "exist" as a filesystem object,
        // so output_dir.join("") (which is also an empty path) will have .exists() == false.
        let output_dir = Path::new("");

        // Entry path "" (empty string) makes entry.path.parent() return None.
        // This is the condition to trigger the .unwrap_or_else(|| Path::new("")) on line 74.
        let entries = vec![create_parsed_entry("", "content")]; // entry.path is PathBuf::from("")

        // Walkthrough for this setup:
        // 1. entry.path = PathBuf::from("")
        // 2. output_dir = Path::new("")
        // 3. target_path = output_dir.join(&entry.path) results in Path::new("")
        //    (since Path::new("").join(Path::new("")) is Path::new(""))
        // 4. target_path.exists() (for Path::new("")) is false.
        // 5. The 'else' block (line 69 in check_for_collisions) is entered.
        // 6. entry.path.parent() (for PathBuf::from("")) is None.
        // 7. The .unwrap_or_else(|| Path::new("")) on line 74 is hit, and its closure returns Path::new("").
        // 8. Path::new("").components() yields an empty iterator.
        // 9. The loop `for component in ...components()` (line 71) does not iterate.
        // 10. No collisions are added to the `collisions` vector.
        // 11. The function should return Ok(()).
        let result = check_for_collisions(&entries, output_dir);
        assert!(
            result.is_ok(),
            "Expected Ok for empty output_dir and empty entry.path, but got Err: {:?}",
            result.err()
        );
    }
}
