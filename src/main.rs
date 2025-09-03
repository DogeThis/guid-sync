mod guid_mapper;
mod meta_parser;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::*;
use std::fs;
use std::path::PathBuf;

use guid_mapper::GuidSyncer;

#[derive(Parser)]
#[command(name = "guid-sync")]
#[command(about = "Unity GUID synchronization tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan projects and show GUID differences
    Scan {
        /// Path to the main Unity project (GUIDs from this project will be preserved)
        #[arg(short, long)]
        main: PathBuf,
        
        /// Path to the subordinate Unity project (GUIDs will be updated to match main)
        #[arg(short, long)]
        subordinate: PathBuf,
    },
    
    /// Generate detailed sync operations report
    Report {
        /// Path to the main Unity project (GUIDs from this project will be preserved)
        #[arg(short, long)]
        main: PathBuf,
        
        /// Path to the subordinate Unity project (GUIDs will be updated to match main)
        #[arg(short, long)]
        subordinate: PathBuf,
        
        /// Output file for the report (JSON format)
        #[arg(short, long)]
        output: PathBuf,
    },
    
    /// Synchronize GUIDs from main project to subordinate project
    Sync {
        /// Path to the main Unity project (GUIDs from this project will be preserved)
        #[arg(short, long)]
        main: PathBuf,
        
        /// Path to the subordinate Unity project (GUIDs will be updated to match main)
        #[arg(short, long)]
        subordinate: PathBuf,
        
        /// Perform a dry run without making changes
        #[arg(short, long)]
        dry_run: bool,
        
        /// Verbose output - show all file updates
        #[arg(short, long)]
        verbose: bool,
        
        /// Export detailed report to a JSON file
        #[arg(short = 'r', long)]
        report: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Scan { main, subordinate } => {
            validate_paths(&main, &subordinate)?;
            scan_projects(main, subordinate)?;
        }
        Commands::Report { main, subordinate, output } => {
            validate_paths(&main, &subordinate)?;
            generate_operations_report(main, subordinate, output)?;
        }
        Commands::Sync { main, subordinate, dry_run, verbose, report } => {
            validate_paths(&main, &subordinate)?;
            sync_projects(main, subordinate, dry_run, verbose, report)?;
        }
    }
    
    Ok(())
}

fn validate_paths(main: &PathBuf, subordinate: &PathBuf) -> Result<()> {
    if !main.exists() {
        anyhow::bail!("Main project path does not exist: {}", main.display());
    }
    if !subordinate.exists() {
        anyhow::bail!("Subordinate project path does not exist: {}", subordinate.display());
    }
    
    // Check for Assets folder
    let main_assets = main.join("Assets");
    let sub_assets = subordinate.join("Assets");
    
    if !main_assets.exists() && !main.ends_with("Assets") {
        anyhow::bail!("Main project does not contain an Assets folder");
    }
    if !sub_assets.exists() && !subordinate.ends_with("Assets") {
        anyhow::bail!("Subordinate project does not contain an Assets folder");
    }
    
    Ok(())
}

fn generate_operations_report(main: PathBuf, subordinate: PathBuf, output: PathBuf) -> Result<()> {
    println!("{}", "Unity GUID Sync Operations Reporter".bright_white().bold());
    println!("{}", "====================================".bright_white());
    println!("Main project: {}", main.display().to_string().green());
    println!("Subordinate project: {}", subordinate.display().to_string().yellow());
    println!("Output report: {}", output.display().to_string().bright_cyan());
    println!();
    
    // Adjust paths to Assets folder if needed
    let main_path = if main.ends_with("Assets") {
        main
    } else {
        main.join("Assets")
    };
    
    let sub_path = if subordinate.ends_with("Assets") {
        subordinate
    } else {
        subordinate.join("Assets")
    };
    
    let mut syncer = GuidSyncer::new(main_path, sub_path);
    syncer.scan_projects()?;
    
    let report = syncer.generate_sync_operations_report()?;
    
    // Save report to file
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&output, json)?;
    
    // Print summary
    println!("\n{}", "Report Summary:".bright_white().bold());
    println!("  Total GUID to change: {}", report.summary.total_guid_differences);
    println!("  Meta files to update: {}", report.summary.total_meta_files_to_update);
    println!("  Files with references: {}", report.summary.total_files_with_references);
    println!("  Total reference updates: {}", report.summary.total_reference_updates);
    
    for (i, op) in report.operations.iter().take(10).enumerate() {
        println!("  {}. {} ({} references)", 
            i + 1,
            op.asset_name.bright_yellow(),
            op.total_references
        );
        println!("     {} -> {}", 
            op.old_guid[..8].to_string().red(),
            op.new_guid[..8].to_string().green()
        );
    }
    
    println!("\n{}", format!("Full report saved to: {}", output.display()).bright_green());
    
    Ok(())
}

fn scan_projects(main: PathBuf, subordinate: PathBuf) -> Result<()> {
    println!("{}", "Unity GUID Scanner".bright_white().bold());
    println!("{}", "===================".bright_white());
    println!("Main project: {}", main.display().to_string().green());
    println!("Subordinate project: {}", subordinate.display().to_string().yellow());
    println!();
    
    // Adjust paths to Assets folder if needed
    let main_path = if main.ends_with("Assets") {
        main
    } else {
        main.join("Assets")
    };
    
    let sub_path = if subordinate.ends_with("Assets") {
        subordinate
    } else {
        subordinate.join("Assets")
    };
    
    let mut syncer = GuidSyncer::new(main_path, sub_path);
    syncer.scan_projects()?;
    syncer.print_summary();
    
    Ok(())
}

fn sync_projects(main: PathBuf, subordinate: PathBuf, dry_run: bool, verbose: bool, report_path: Option<PathBuf>) -> Result<()> {
    println!("{}", "Unity GUID Synchronizer".bright_white().bold());
    println!("{}", "========================".bright_white());
    println!("Main project: {}", main.display().to_string().green());
    println!("Subordinate project: {}", subordinate.display().to_string().yellow());
    if dry_run {
        println!("{}", "Mode: DRY RUN (no changes will be made)".bright_cyan());
    } else {
        println!("{}", "Mode: LIVE (files will be modified)".bright_red().bold());
    }
    if verbose {
        println!("{}", "Verbose: ON".bright_magenta());
    }
    println!();
    
    // Adjust paths to Assets folder if needed
    let main_path = if main.ends_with("Assets") {
        main
    } else {
        main.join("Assets")
    };
    
    let sub_path = if subordinate.ends_with("Assets") {
        subordinate
    } else {
        subordinate.join("Assets")
    };
    
    let mut syncer = GuidSyncer::new(main_path, sub_path);
    syncer.scan_projects()?;
    
    if verbose {
        syncer.print_summary();
    } else {
        // Just show count for non-verbose
        println!("Found {} GUID differences to resolve", syncer.get_difference_count());
    }
    
    if !dry_run && syncer.get_difference_count() > 0 {
        println!();
        println!("{}", "WARNING: This will modify files in the subordinate project!".bright_red().bold());
        println!("Press Enter to continue or Ctrl+C to cancel...");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
    }
    
    let sync_report = syncer.sync_guids(dry_run, verbose)?;
    
    if let Some(report_path) = report_path {
        sync_report.export_to_file(&report_path)?;
        println!("\n{}", format!("Report exported to: {}", report_path.display()).bright_cyan());
    }
    
    Ok(())
}