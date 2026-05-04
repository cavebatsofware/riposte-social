/*  This file is part of riposte-social
 *  Copyright (C) 2026 Grant DeFayette
 *
 *  riposte-social is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation, version 3 of the License (GPL-3.0-only).
 *
 *  riposte-social is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with riposte-social.  If not, see <https://www.gnu.org/licenses/gpl-3.0.html>.
 */
use crate::errors::AppError;
use std::io::{Cursor, Read, Write};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

/// Process a DOCX template by replacing  placeholder with actual access code
/// DOCX is a ZIP file containing XML files. We need to:
/// 1. Unzip the DOCX
/// 2. Find and replace  in word/_rels/document.xml.rels
/// 3. Re-zip everything back into a DOCX
pub fn process_docx_template(docx_bytes: &[u8], access_code: &str) -> Result<Vec<u8>, AppError> {
    // Open the DOCX as a ZIP archive
    let cursor = Cursor::new(docx_bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|e| {
        AppError::InternalError(format!("Failed to open DOCX as ZIP archive: {}", e))
    })?;

    // Create a new ZIP archive for the output
    let output_cursor = Cursor::new(Vec::new());
    let mut zip_writer = ZipWriter::new(output_cursor);

    // Iterate through all files in the ZIP
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| {
            AppError::InternalError(format!("Failed to read file from DOCX archive: {}", e))
        })?;

        let file_name = file.name().to_string();
        let is_target_file = file_name == "word/_rels/document.xml.rels";

        // Read file contents
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).map_err(|e| {
            AppError::InternalError(format!(
                "Failed to read file '{}' from DOCX: {}",
                file_name, e
            ))
        })?;

        // If this is the relationships file, perform substitution
        let processed_contents = if is_target_file {
            let xml_string = String::from_utf8(contents.clone()).map_err(|e| {
                AppError::InternalError(format!(
                    "Invalid UTF-8 in DOCX file '{}': {}",
                    file_name, e
                ))
            })?;

            let processed_xml = xml_string.replace("", access_code);
            processed_xml.into_bytes()
        } else {
            contents
        };

        // Write the file to the new archive
        let options = SimpleFileOptions::default()
            .compression_method(file.compression())
            .unix_permissions(file.unix_mode().unwrap_or(0o644));

        zip_writer
            .start_file(file_name.clone(), options)
            .map_err(|e| {
                AppError::InternalError(format!(
                    "Failed to start file '{}' in output DOCX: {}",
                    file_name, e
                ))
            })?;

        zip_writer.write_all(&processed_contents).map_err(|e| {
            AppError::InternalError(format!(
                "Failed to write file '{}' to output DOCX: {}",
                file_name, e
            ))
        })?;
    }

    // Finalize the ZIP archive
    let output_cursor = zip_writer
        .finish()
        .map_err(|e| AppError::InternalError(format!("Failed to finalize output DOCX: {}", e)))?;

    Ok(output_cursor.into_inner())
}
