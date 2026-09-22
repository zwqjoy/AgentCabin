//! Golden Fixtures for Work Business Acceptance Suite.
//!
//! Provides minimal valid Office packages (docx, pptx, xlsx) that pass
//! `ArtifactValidator`, as well as business scenario input fixtures.

use std::fs;
use std::io::Write;
use std::path::Path;

/// Generate minimal valid PPTX ZIP package that passes `ArtifactValidator`.
pub fn minimal_pptx() -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types></Types>")
            .unwrap();
        zip.start_file("ppt/presentation.xml", options).unwrap();
        zip.write_all(
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:presentation></p:presentation>",
        )
        .unwrap();
        zip.finish().unwrap();
    }
    buffer
}

/// Generate minimal valid DOCX ZIP package that passes `ArtifactValidator`.
pub fn minimal_docx() -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types></Types>")
            .unwrap();
        zip.start_file("word/document.xml", options).unwrap();
        zip.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document></w:document>")
            .unwrap();
        zip.finish().unwrap();
    }
    buffer
}

/// Generate minimal valid XLSX ZIP package that passes `ArtifactValidator`.
pub fn minimal_xlsx() -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types></Types>")
            .unwrap();
        zip.start_file("xl/workbook.xml", options).unwrap();
        zip.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><workbook></workbook>")
            .unwrap();
        zip.finish().unwrap();
    }
    buffer
}

/// Sets up 12 contract files across 3 clients (A, B, C) in the external contracts directory.
pub fn setup_contract_fixtures(external_dir: &Path) -> Result<Vec<String>, String> {
    let contracts_dir = external_dir.join("contracts");
    fs::create_dir_all(&contracts_dir).map_err(|e| format!("cannot create contracts dir: {e}"))?;
    let mut contract_names = Vec::new();

    let clients = ["A", "B", "C"];
    for client in clients {
        for idx in 1..=4 {
            let name = format!("{}-{:03}.txt", client, idx);
            let file_path = contracts_dir.join(&name);
            let content = format!(
                "CONTRACT AGREEMENT\nClient: {}\nContract ID: {}\nStatus: Signed\nEffective Date: 2026-01-01\nTerms: Confidential and Binding.\n",
                client, name
            );
            fs::write(&file_path, content)
                .map_err(|e| format!("cannot write contract {}: {e}", file_path.display()))?;
            contract_names.push(name);
        }
    }
    Ok(contract_names)
}

/// Sets up monthly CSV input data in `input/` of the workspace.
pub fn setup_sales_input_fixtures(workspace_dir: &Path) -> Result<(), String> {
    let input_dir = workspace_dir.join("input");
    fs::create_dir_all(&input_dir).map_err(|e| format!("cannot create input dir: {e}"))?;

    fs::write(
        input_dir.join("january.csv"),
        "product,region,revenue,cost\nAlpha,North,50000,20000\nBeta,South,30000,15000\n",
    )
    .map_err(|e| format!("cannot write january.csv: {e}"))?;

    fs::write(
        input_dir.join("february.csv"),
        "product,region,revenue,cost\nAlpha,North,55000,21000\nBeta,South,32000,16000\n",
    )
    .map_err(|e| format!("cannot write february.csv: {e}"))?;

    fs::write(
        input_dir.join("march.csv"),
        "product,region,revenue,cost\nAlpha,North,60000,22000\nBeta,South,35000,17000\n",
    )
    .map_err(|e| format!("cannot write march.csv: {e}"))?;

    Ok(())
}
