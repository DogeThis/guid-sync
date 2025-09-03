use anyhow::{Result, Context};
use regex::Regex;
use std::fs;
use std::path::Path;

pub struct MetaFile;

impl MetaFile {
    /// Extract GUID from a meta file without parsing YAML
    pub fn get_guid_from_file(path: &Path) -> Result<String> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read meta file: {}", path.display()))?;
        
        // Match guid line: can be with or without quotes
        let guid_regex = Regex::new(r#"^guid:\s*['"]?([a-f0-9]{32})['"]?\s*$"#)?;
        
        for line in content.lines() {
            if let Some(captures) = guid_regex.captures(line) {
                if let Some(guid) = captures.get(1) {
                    return Ok(guid.as_str().to_string());
                }
            }
        }
        
        anyhow::bail!("No GUID found in meta file: {}", path.display())
    }
    
    /// Update only the GUID in a meta file, preserving all formatting
    pub fn update_guid_in_file(path: &Path, new_guid: &str) -> Result<()> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read meta file: {}", path.display()))?;
        
        // Match guid line: can be with or without quotes, preserve the format
        let guid_regex = Regex::new(r#"^(guid:\s*)(['"]?)([a-f0-9]{32})(['"]?)\s*$"#)?;
        
        let mut updated = false;
        let new_content: Vec<String> = content
            .lines()
            .map(|line| {
                if let Some(captures) = guid_regex.captures(line) {
                    updated = true;
                    // Preserve the original formatting (quotes or no quotes)
                    format!("{}{}{}{}",
                        captures.get(1).map_or("", |m| m.as_str()),
                        captures.get(2).map_or("", |m| m.as_str()),
                        new_guid,
                        captures.get(4).map_or("", |m| m.as_str())
                    )
                } else {
                    line.to_string()
                }
            })
            .collect();
        
        if !updated {
            anyhow::bail!("No GUID found to update in meta file: {}", path.display())
        }
        
        // Join with newlines and add final newline if original had one
        let final_content = if content.ends_with('\n') {
            new_content.join("\n") + "\n"
        } else {
            new_content.join("\n")
        };
        
        fs::write(path, final_content)
            .with_context(|| format!("Failed to write meta file: {}", path.display()))?;
        
        Ok(())
    }
}