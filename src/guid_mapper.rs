use anyhow::{Result, Context};
use colored::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::meta_parser::MetaFile;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SyncReport {
    meta_files_changed: usize,
    files_with_references: HashSet<PathBuf>,
    total_references_replaced: usize,
    guid_reference_counts: HashMap<String, usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncOperation {
    pub old_guid: String,
    pub new_guid: String,
    pub asset_path: PathBuf,
    pub asset_name: String,
    pub meta_file_update: MetaFileUpdate,
    pub reference_updates: Vec<ReferenceUpdate>,
    pub total_references: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetaFileUpdate {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceUpdate {
    pub file_path: PathBuf,
    pub file_type: String,
    pub reference_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncOperationsReport {
    pub summary: SyncSummary,
    pub operations: Vec<SyncOperation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncSummary {
    pub total_guid_differences: usize,
    pub total_meta_files_to_update: usize,
    pub total_files_with_references: usize,
    pub total_reference_updates: usize,
}

impl SyncReport {
    fn new() -> Self {
        Self::default()
    }

    pub fn export_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn print(&self) {
        println!("\n{}", "═══════════════════════════════════════".bright_white());
        println!("{}", "       DRY RUN REPORT SUMMARY".bright_white().bold());
        println!("{}", "═══════════════════════════════════════".bright_white());
        
        println!("\n{}", "📁 Meta Files to Update:".bright_cyan().bold());
        println!("   {} meta files will have their GUIDs changed", self.meta_files_changed);
        
        println!("\n{}", "🔗 Reference Updates:".bright_cyan().bold());
        println!("   {} total GUID references will be updated", self.total_references_replaced);
        println!("   {} files contain references that need updating", self.files_with_references.len());
        
        if !self.guid_reference_counts.is_empty() {
            println!("\n{}", "📊 Top Referenced GUIDs:".bright_cyan().bold());
            let mut counts: Vec<_> = self.guid_reference_counts.iter().collect();
            counts.sort_by(|a, b| b.1.cmp(a.1));
            for (guid, count) in counts.iter().take(10) {
                println!("   {} - {} references", guid.bright_yellow(), count);
            }
        }
        
        println!("\n{}", "═══════════════════════════════════════".bright_white());
        println!("{}", "To apply these changes, run without --dry-run flag".bright_green());
        println!("{}", "WARNING: This will modify files in the subordinate project!".bright_red().bold());
        println!("{}", "═══════════════════════════════════════".bright_white());
    }
}

pub struct GuidSyncer {
    main_project: PathBuf,
    subordinate_project: PathBuf,
    guid_mappings: HashMap<PathBuf, (String, String)>, // relative_path -> (main_guid, sub_guid)
}

impl GuidSyncer {
    pub fn new(main_project: PathBuf, subordinate_project: PathBuf) -> Self {
        Self {
            main_project,
            subordinate_project,
            guid_mappings: HashMap::new(),
        }
    }
    
    pub fn get_difference_count(&self) -> usize {
        self.guid_mappings.len()
    }

    pub fn scan_projects(&mut self) -> Result<()> {
        println!("{}", "Scanning projects for GUID mappings...".bright_blue());
        
        let main_metas = self.scan_meta_files(&self.main_project)?;
        let sub_metas = self.scan_meta_files(&self.subordinate_project)?;

        for (rel_path, main_guid) in &main_metas {
            if let Some(sub_guid) = sub_metas.get(rel_path) {
                if main_guid != sub_guid {
                    println!(
                        "{}: {} -> {}",
                        format!("GUID difference found for {}", rel_path.display()).yellow(),
                        sub_guid.red(),
                        main_guid.green()
                    );
                    self.guid_mappings.insert(
                        rel_path.clone(),
                        (main_guid.clone(), sub_guid.clone()),
                    );
                }
            }
        }

        println!(
            "{}",
            format!("Found {} GUID differences", self.guid_mappings.len()).bright_yellow()
        );
        Ok(())
    }

    fn scan_meta_files(&self, project_path: &Path) -> Result<HashMap<PathBuf, String>> {
        let mut mappings = HashMap::new();

        for entry in WalkDir::new(project_path)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("meta") {
                // Skip Library folder to avoid UTF-8 BOM issues
                if path.components().any(|c| c.as_os_str() == "Library") {
                    continue;
                }
                
                match MetaFile::get_guid_from_file(path) {
                    Ok(guid) => {
                        let relative_path = path
                            .strip_prefix(project_path)?
                            .to_path_buf();
                        mappings.insert(relative_path, guid);
                    }
                    Err(e) => {
                        // Log error but continue scanning
                        eprintln!("Warning: Could not read {}: {}", path.display(), e);
                    }
                }
            }
        }

        Ok(mappings)
    }

    pub fn sync_guids(&self, dry_run: bool, verbose: bool) -> Result<SyncReport> {
        if self.guid_mappings.is_empty() {
            println!("{}", "No GUID differences to resolve!".green());
            return Ok(SyncReport::new());
        }

        if verbose {
            println!(
                "{}",
                format!(
                    "Syncing GUIDs in subordinate project ({})...",
                    if dry_run { "DRY RUN" } else { "LIVE" }
                )
                .bright_blue()
            );
        }

        let mut report = SyncReport::new();

        // Update meta files
        for (rel_path, (main_guid, _sub_guid)) in &self.guid_mappings {
            let meta_path = self.subordinate_project.join(rel_path);
            self.update_meta_file(&meta_path, main_guid, dry_run, verbose)?;
            report.meta_files_changed += 1;
        }

        // Update references in all Unity files
        self.update_guid_references_with_report(dry_run, verbose, &mut report)?;

        if dry_run {
            report.print();
        }

        println!("{}", "GUID sync completed!".bright_green());
        Ok(report)
    }

    fn update_meta_file(&self, path: &Path, new_guid: &str, dry_run: bool, verbose: bool) -> Result<()> {
        if dry_run && verbose {
            println!("  {} {}", "[DRY RUN]".cyan(), path.display());
            return Ok(());
        }

        if !dry_run {
            MetaFile::update_guid_in_file(path, new_guid)
                .with_context(|| format!("Failed to update meta file: {}", path.display()))?;
            if verbose {
                println!("  {} {}", "Updated".green(), path.display());
            }
        }
        Ok(())
    }

    fn update_guid_references_with_report(&self, dry_run: bool, verbose: bool, report: &mut SyncReport) -> Result<()> {
        if verbose {
            println!("{}", "Updating GUID references in Unity files...".bright_blue());
        }

        let guid_regex = Regex::new(r"guid:\s*([a-f0-9]{32})")?;
        let file_id_regex = Regex::new(r"\{fileID:\s*\d+,\s*guid:\s*([a-f0-9]{32}),\s*type:\s*\d+\}")?;

        for entry in WalkDir::new(&self.subordinate_project)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            
            // Skip meta files and non-files
            if !path.is_file() || path.extension() == Some(std::ffi::OsStr::new("meta")) {
                continue;
            }
            
            // Check if file is likely a Unity YAML file by checking first line
            if let Ok(file) = std::fs::File::open(path) {
                let reader = BufReader::new(file);
                if let Some(Ok(first_line)) = reader.lines().next() {
                    // Unity YAML files typically start with %YAML
                    if first_line.starts_with("%YAML") || first_line.starts_with("---") {
                        self.update_file_guids_with_report(path, &guid_regex, &file_id_regex, dry_run, verbose, report)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn update_file_guids_with_report(
        &self,
        path: &Path,
        guid_regex: &Regex,
        file_id_regex: &Regex,
        dry_run: bool,
        verbose: bool,
        report: &mut SyncReport,
    ) -> Result<()> {
        // Try to read file as UTF-8, skip if it fails
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("Warning: Could not read {} as UTF-8: {}", path.display(), e);
                return Ok(());
            }
        };
        let mut modified = false;
        let mut new_content = content.clone();
        let mut file_ref_count = 0;

        // Build reverse mapping: sub_guid -> main_guid
        let guid_map: HashMap<&str, &str> = self
            .guid_mappings
            .values()
            .map(|(main, sub)| (sub.as_str(), main.as_str()))
            .collect();

        // Replace in guid: patterns
        for cap in guid_regex.captures_iter(&content) {
            if let Some(old_guid) = cap.get(1) {
                if let Some(new_guid) = guid_map.get(old_guid.as_str()) {
                    let old_match = cap.get(0).unwrap().as_str();
                    let new_match = format!("guid: {}", new_guid);
                    new_content = new_content.replace(old_match, &new_match);
                    modified = true;
                    file_ref_count += 1;
                    *report.guid_reference_counts.entry(old_guid.as_str().to_string()).or_insert(0) += 1;
                }
            }
        }

        // Replace in {fileID: ..., guid: ..., type: ...} patterns
        for cap in file_id_regex.captures_iter(&content) {
            if let Some(old_guid) = cap.get(1) {
                if let Some(new_guid) = guid_map.get(old_guid.as_str()) {
                    let old_match = cap.get(0).unwrap().as_str();
                    let new_match = old_match.replace(old_guid.as_str(), new_guid);
                    new_content = new_content.replace(old_match, &new_match);
                    modified = true;
                    file_ref_count += 1;
                    *report.guid_reference_counts.entry(old_guid.as_str().to_string()).or_insert(0) += 1;
                }
            }
        }

        if modified {
            report.files_with_references.insert(path.to_path_buf());
            report.total_references_replaced += file_ref_count;
            
            if dry_run && verbose {
                println!("  {} {} ({} references)", "[DRY RUN]".cyan(), path.display(), file_ref_count);
            } else if !dry_run {
                fs::write(path, new_content)?;
                if verbose {
                    println!("  {} {} ({} references)", "Updated references in".green(), path.display(), file_ref_count);
                }
            }
        }

        Ok(())
    }

    pub fn generate_sync_operations_report(&self) -> Result<SyncOperationsReport> {
        println!("{}", "Generating detailed sync operations report...".bright_blue());
        
        let mut operations = Vec::new();
        
        // First pass: scan all files for references
        let mut guid_references: HashMap<String, Vec<ReferenceUpdate>> = HashMap::new();
        
        for entry in WalkDir::new(&self.subordinate_project)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let ext = path.extension().and_then(|s| s.to_str());
            
            if matches!(ext, Some("unity") | Some("prefab") | Some("asset") | Some("mat") | Some("controller")) {
                // Try to read file as UTF-8, skip if it fails
                let content = match fs::read_to_string(path) {
                    Ok(content) => content,
                    Err(e) => {
                        eprintln!("Warning: Could not read {} for report: {}", path.display(), e);
                        continue;
                    }
                };
                let guid_regex = Regex::new(r"guid:\s*([a-f0-9]{32})")?;
                let file_id_regex = Regex::new(r"\{fileID:\s*\d+,\s*guid:\s*([a-f0-9]{32}),\s*type:\s*\d+\}")?;
                
                let mut file_guid_counts: HashMap<String, usize> = HashMap::new();
                
                // Count guid: patterns
                for cap in guid_regex.captures_iter(&content) {
                    if let Some(guid) = cap.get(1) {
                        *file_guid_counts.entry(guid.as_str().to_string()).or_insert(0) += 1;
                    }
                }
                
                // Count {fileID: ..., guid: ..., type: ...} patterns
                for cap in file_id_regex.captures_iter(&content) {
                    if let Some(guid) = cap.get(1) {
                        *file_guid_counts.entry(guid.as_str().to_string()).or_insert(0) += 1;
                    }
                }
                
                // Add to reference tracking
                for (guid, count) in file_guid_counts {
                    // Check if this GUID is one we're replacing
                    if self.guid_mappings.values().any(|(_, sub)| sub == &guid) {
                        let file_type = ext.unwrap_or("unknown").to_string();
                        let relative_path = path.strip_prefix(&self.subordinate_project)
                            .unwrap_or(path)
                            .to_path_buf();
                        
                        guid_references.entry(guid).or_insert_with(Vec::new).push(
                            ReferenceUpdate {
                                file_path: relative_path,
                                file_type,
                                reference_count: count,
                            }
                        );
                    }
                }
            }
        }
        
        // Second pass: create sync operations
        for (rel_path, (main_guid, sub_guid)) in &self.guid_mappings {
            let asset_name = rel_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
                .replace(".meta", "");
            
            let references = guid_references.get(sub_guid).cloned().unwrap_or_default();
            let total_refs: usize = references.iter().map(|r| r.reference_count).sum();
            
            operations.push(SyncOperation {
                old_guid: sub_guid.clone(),
                new_guid: main_guid.clone(),
                asset_path: rel_path.clone(),
                asset_name,
                meta_file_update: MetaFileUpdate {
                    path: rel_path.clone(),
                },
                reference_updates: references,
                total_references: total_refs,
            });
        }
        
        // Sort operations by number of references (most referenced first)
        operations.sort_by(|a, b| b.total_references.cmp(&a.total_references));
        
        let total_files_with_refs: HashSet<PathBuf> = operations
            .iter()
            .flat_map(|op| op.reference_updates.iter().map(|r| r.file_path.clone()))
            .collect();
        
        let total_reference_updates: usize = operations
            .iter()
            .map(|op| op.total_references)
            .sum();
        
        let report = SyncOperationsReport {
            summary: SyncSummary {
                total_guid_differences: operations.len(),
                total_meta_files_to_update: operations.len(),
                total_files_with_references: total_files_with_refs.len(),
                total_reference_updates,
            },
            operations,
        };
        
        Ok(report)
    }

    pub fn print_summary(&self) {
        if self.guid_mappings.is_empty() {
            return;
        }

        println!("\n{}", "GUID Mapping Summary:".bright_white().underline());
        for (path, (main_guid, sub_guid)) in &self.guid_mappings {
            println!("  {}", path.display().to_string().bright_cyan());
            println!("    {} {}", "Main:".green(), main_guid);
            println!("    {} {}", "Sub: ".red(), sub_guid);
        }
    }
}