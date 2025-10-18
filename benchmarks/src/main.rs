//! Syscall analysis tool for arsync
//!
//! Parses strace output and generates markdown reports analyzing:
//! - io_uring usage and batching efficiency
//! - Metadata operations (statx, openat, etc.)
//! - Security posture (FD-based vs path-based operations)
//! - Per-file and per-directory syscall breakdowns

use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "syscall-analyzer")]
#[command(about = "Analyze strace output for arsync performance and security")]
struct Args {
    /// Path to strace raw output file
    #[arg(long)]
    trace_raw: PathBuf,

    /// Path to strace summary output file  
    #[arg(long)]
    trace_summary: PathBuf,

    /// Output markdown report path
    #[arg(long)]
    output: PathBuf,

    /// Number of test files
    #[arg(long)]
    num_files: usize,

    /// File size in MB
    #[arg(long)]
    file_size_mb: usize,

    /// Binary path being analyzed
    #[arg(long)]
    binary: String,

    /// Source test directory
    #[arg(long)]
    test_dir_src: PathBuf,

    /// Destination test directory
    #[arg(long)]
    test_dir_dst: PathBuf,
}

#[derive(Debug, Default)]
struct SyscallStats {
    io_uring_setup: usize,
    io_uring_enter: usize,
    io_uring_batch_sizes: Vec<usize>,
    statx_total: usize,
    statx_path_based: usize,
    statx_fd_based: usize,
    openat_total: usize,
    openat_path_based: usize,
    fallocate: usize,
    fchmod: usize,
    fchown: usize,
    utimensat_total: usize,
    utimensat_fd_based: usize,
    utimensat_path_based: usize,
    per_file: HashMap<String, FileStats>,
    dir_stats: DirectoryStats,
}

#[derive(Debug, Default)]
struct FileStats {
    statx: usize,
    openat: usize,
    mentions: usize,
}

#[derive(Debug, Default)]
struct DirectoryStats {
    src_statx: usize,
    src_openat: usize,
    getdents: usize,
    dst_fchmod: usize,
    dst_fchown: usize,
}

#[derive(Debug)]
enum ExitCode {
    Success = 0,
    Warning = 1,
    Failure = 2,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Parse strace output
    let stats = parse_strace_output(&args)?;

    // Generate markdown report
    let report = generate_markdown_report(&args, &stats)?;

    // Write report
    fs::write(&args.output, report)
        .with_context(|| format!("Failed to write report to {:?}", args.output))?;

    println!("✅ Report generated: {:?}", args.output);

    // Determine exit code based on analysis
    let exit_code = determine_exit_code(&stats, args.num_files);

    std::process::exit(exit_code as i32);
}

fn parse_strace_output(args: &Args) -> Result<SyscallStats> {
    let raw_content = fs::read_to_string(&args.trace_raw)
        .with_context(|| format!("Failed to read {:?}", args.trace_raw))?;

    let mut stats = SyscallStats::default();

    // Count io_uring operations
    stats.io_uring_setup = raw_content.matches("io_uring_setup").count();
    stats.io_uring_enter = raw_content.matches("io_uring_enter").count();

    // Parse io_uring batch sizes
    let batch_re = Regex::new(r"io_uring_enter\([0-9]+, (\d+),").unwrap();
    for cap in batch_re.captures_iter(&raw_content) {
        if let Ok(size) = cap[1].parse::<usize>() {
            stats.io_uring_batch_sizes.push(size);
        }
    }

    // Count statx operations
    stats.statx_total = raw_content.matches("statx(").count();
    stats.statx_path_based = raw_content.matches("statx(AT_FDCWD").count()
        - raw_content.matches("statx(AT_FDCWD, \"\"").count();
    stats.statx_fd_based = raw_content.matches("statx(").count() - stats.statx_path_based;

    // Count openat operations
    stats.openat_total = raw_content.matches("openat(").count();
    let openat_user_re = Regex::new(r#"openat\(AT_FDCWD, "/[^"]*""#).unwrap();
    for line in raw_content.lines() {
        if openat_user_re.is_match(line) {
            // Exclude system paths
            if !line.contains("/etc")
                && !line.contains("/lib")
                && !line.contains("/proc")
                && !line.contains("/sys")
            {
                stats.openat_path_based += 1;
            }
        }
    }

    // Count other operations
    stats.fallocate = raw_content.matches("fallocate(").count();
    stats.fchmod = raw_content.matches("fchmod").count();
    stats.fchown = raw_content.matches("fchown").count();
    stats.utimensat_total = raw_content.matches("utimensat").count();

    // Count FD-based vs path-based utimensat
    let utimens_fd_re = Regex::new(r"utimensat\(\d+, NULL").unwrap();
    stats.utimensat_fd_based = utimens_fd_re.find_iter(&raw_content).count();
    let utimens_path_re = Regex::new(r#"utimensat\(AT_FDCWD, "/"#).unwrap();
    stats.utimensat_path_based = utimens_path_re.find_iter(&raw_content).count();

    // Per-file breakdown (first 3 files)
    for i in 1..=3.min(args.num_files) {
        let filename = format!("file{}.bin", i);
        let mut file_stats = FileStats::default();

        file_stats.statx = raw_content.matches(&format!("statx.*{}", filename)).count();
        file_stats.openat = raw_content
            .matches(&format!("openat.*{}", filename))
            .count();
        file_stats.mentions = raw_content.matches(&filename).count();

        stats.per_file.insert(filename, file_stats);
    }

    // Directory stats
    let src_path = args.test_dir_src.to_string_lossy();
    stats.dir_stats.src_statx = raw_content
        .matches(&format!("statx.*\"{}\"", src_path))
        .count();
    stats.dir_stats.src_openat = raw_content
        .matches(&format!("openat.*\"{}\".*O_DIRECTORY", src_path))
        .count();
    stats.dir_stats.getdents = raw_content.matches("getdents").count();
    stats.dir_stats.dst_fchmod = stats.fchmod;
    stats.dir_stats.dst_fchown = stats.fchown;

    Ok(stats)
}

fn generate_markdown_report(args: &Args, stats: &SyscallStats) -> Result<String> {
    let mut report = String::new();

    // Header
    report.push_str(&format!(
        "# 📊 Syscall Analysis Report\n\n\
        **Date:** {}\n\
        **Test:** {} files × {}MB\n\
        **Binary:** `{}`\n\n\
        ---\n\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z"),
        args.num_files,
        args.file_size_mb,
        args.binary
    ));

    // io_uring Usage
    add_io_uring_section(&mut report, stats, args.num_files);

    // Metadata Operations
    add_metadata_section(&mut report, stats, args.num_files);

    // File Operations
    add_file_operations_section(&mut report, stats, args.num_files);

    // Metadata Preservation
    add_metadata_preservation_section(&mut report, stats, args.num_files);

    // Per-directory breakdown
    add_directory_section(&mut report, stats, &args.test_dir_src, &args.test_dir_dst);

    // Security Assessment
    add_security_section(&mut report, stats);

    // Recommendations
    add_recommendations_section(&mut report, stats, args.num_files);

    // Summary Table
    add_summary_table(&mut report, stats, args.num_files);

    // Footer
    report.push_str(
        "\n---\n\n\
        📄 **Full Traces:**\n\
        - Summary: `/tmp/syscall-analysis-summary.txt`\n\
        - Detailed: `/tmp/syscall-analysis-raw.txt`\n",
    );

    Ok(report)
}

fn add_io_uring_section(report: &mut String, stats: &SyscallStats, _num_files: usize) {
    report.push_str("## 🔄 io_uring Usage\n\n");

    report.push_str(&format!(
        "- **io_uring_setup calls:** {} (one per worker thread + main)\n\
        - **io_uring_enter calls:** {}\n\n",
        stats.io_uring_setup, stats.io_uring_enter
    ));

    if stats.io_uring_enter > 100 {
        report.push_str("✅ **PASS:** Heavy io_uring usage\n\n");
    } else {
        report.push_str(&format!(
            "❌ **FAIL:** Low io_uring usage (expected >100 for {} files)\n\n",
            _num_files
        ));
    }

    // Batching efficiency
    if !stats.io_uring_batch_sizes.is_empty() {
        let single_op = stats
            .io_uring_batch_sizes
            .iter()
            .filter(|&&x| x == 1)
            .count();
        let multi_op = stats
            .io_uring_batch_sizes
            .iter()
            .filter(|&&x| x > 1)
            .count();
        let avg: f64 = stats.io_uring_batch_sizes.iter().sum::<usize>() as f64
            / stats.io_uring_batch_sizes.len() as f64;
        let max = stats.io_uring_batch_sizes.iter().max().unwrap_or(&0);

        report.push_str("### Batching Efficiency\n\n");
        report.push_str("| Metric | Value |\n");
        report.push_str("|--------|-------|\n");
        report.push_str(&format!(
            "| Single-op submissions (batch=1) | {} |\n",
            single_op
        ));
        report.push_str(&format!(
            "| Multi-op submissions (batch≥2) | {} |\n",
            multi_op
        ));
        report.push_str(&format!("| Average batch size | {:.1} ops/submit |\n", avg));
        report.push_str(&format!("| Maximum batch size | {} ops/submit |\n\n", max));

        if avg <= 1.5 {
            report.push_str(
                "⚠️  **WARNING:** Poor batching (avg≤1.5, mostly single-op submissions)\n",
            );
            report.push_str("> Better batching could reduce syscall overhead\n\n");
        } else if avg >= 3.0 {
            report.push_str("✅ **EXCELLENT:** Good batching (avg≥3 ops/submit)\n\n");
        } else {
            report.push_str("✅ **GOOD:** Decent batching (1.5 < avg < 3 ops/submit)\n\n");
        }
    }
}

fn add_metadata_section(report: &mut String, stats: &SyscallStats, num_files: usize) {
    report.push_str("## 📋 Metadata Operations\n\n");

    let statx_per_file = stats.statx_total as f64 / num_files as f64;

    report.push_str(&format!(
        "| Metric | Count |\n\
        |--------|-------|\n\
        | Total statx calls | {} |\n\
        | Path-based (AT_FDCWD + path) | {} |\n\
        | FD-based (dirfd + filename) | {} |\n\
        | **Average per file** | **{:.1}** |\n\n",
        stats.statx_total, stats.statx_path_based, stats.statx_fd_based, statx_per_file
    ));

    let expected_max = num_files * 2;
    if stats.statx_path_based > expected_max {
        report.push_str(&format!(
            "⚠️  **WARNING:** High path-based statx count (TOCTOU-vulnerable)\n\
            - Expected: ≤{} (1-2 per file)\n\
            - Got: {} (~{:.1} per file)\n\n",
            expected_max,
            stats.statx_path_based,
            stats.statx_path_based as f64 / num_files as f64
        ));
    } else if stats.statx_path_based == 0 {
        report.push_str(
            "✅ **EXCELLENT:** No path-based statx calls (100% FD-based, TOCTOU-safe)\n\n",
        );
    } else {
        report.push_str("✅ **GOOD:** Low path-based statx count\n\n");
    }

    // Per-file breakdown
    if !stats.per_file.is_empty() {
        report.push_str("### Per-File Breakdown\n\n");
        let mut files: Vec<_> = stats.per_file.iter().collect();
        files.sort_by_key(|(name, _)| *name);

        for (filename, file_stats) in files {
            report.push_str(&format!(
                "**{}:**\n\
                - statx: {}\n\
                - openat: {}\n\
                - total mentions: {}\n\n",
                filename, file_stats.statx, file_stats.openat, file_stats.mentions
            ));
        }
    }
}

fn add_file_operations_section(report: &mut String, stats: &SyscallStats, num_files: usize) {
    report.push_str("## 📁 File Operations\n\n");

    let openat_per_file = stats.openat_path_based as f64 / num_files as f64;

    report.push_str(&format!(
        "| Metric | Count |\n\
        |--------|-------|\n\
        | Total openat calls | {} |\n\
        | User file opens (path-based) | {} |\n\
        | **Average per file** | **{:.1}** |\n\n",
        stats.openat_total, stats.openat_path_based, openat_per_file
    ));

    let expected_max_openat = num_files * 4;
    if stats.openat_path_based > expected_max_openat {
        report.push_str(&format!(
            "⚠️  **WARNING:** Excessive openat calls\n\
            - Expected: ≤{} (2-4 per file)\n\
            - Got: {}\n\n",
            expected_max_openat, stats.openat_path_based
        ));
    } else {
        report.push_str("✅ **PASS:** Reasonable openat count\n\n");
    }

    // fallocate
    report.push_str(&format!(
        "**Direct fallocate syscalls:** {}\n\n",
        stats.fallocate
    ));

    if stats.fallocate > 0 {
        report.push_str("⚠️  **WARNING:** fallocate not using io_uring\n\n");
    } else {
        report.push_str("✅ **PASS:** fallocate via io_uring (no direct syscalls)\n\n");
    }
}

fn add_metadata_preservation_section(report: &mut String, stats: &SyscallStats, num_files: usize) {
    report.push_str("## 🔒 Metadata Preservation\n\n");

    report.push_str(&format!(
        "| Operation | Count |\n\
        |-----------|-------|\n\
        | fchmod (FD-based permissions) | {} |\n\
        | fchown (FD-based ownership) | {} |\n\
        | utimensat (total) | {} |\n\
        | └─ FD-based (fd, NULL, ...) | {} |\n\
        | └─ Path-based (AT_FDCWD, path, ...) | {} |\n\n",
        stats.fchmod,
        stats.fchown,
        stats.utimensat_total,
        stats.utimensat_fd_based,
        stats.utimensat_path_based
    ));

    let fd_percentage = if stats.utimensat_total > 0 {
        (stats.utimensat_fd_based as f64 / stats.utimensat_total as f64 * 100.0) as usize
    } else {
        0
    };

    if stats.utimensat_path_based > 0 {
        report.push_str("⚠️  **WARNING:** Some path-based timestamp operations (TOCTOU risk)\n\n");
    } else if stats.utimensat_fd_based >= num_files {
        report.push_str(&format!(
            "✅ **EXCELLENT:** {}% FD-based timestamp preservation (TOCTOU-safe)\n\n",
            fd_percentage
        ));
    } else {
        report.push_str("ℹ️  **INFO:** Timestamp preservation counts lower than expected\n\n");
    }
}

fn add_directory_section(report: &mut String, stats: &SyscallStats, src: &PathBuf, dst: &PathBuf) {
    report.push_str("## 📂 Directory Operations\n\n");

    report.push_str(&format!(
        "**Source directory** (`{}`):\n\
        - statx: {}\n\
        - openat (O_DIRECTORY): {}\n\
        - getdents64 (directory reads): {}\n\n",
        src.display(),
        stats.dir_stats.src_statx,
        stats.dir_stats.src_openat,
        stats.dir_stats.getdents
    ));

    report.push_str(&format!(
        "**Destination directory** (`{}`):\n\
        - fchmod: {} (includes files)\n\
        - fchown: {} (includes files)\n\n",
        dst.display(),
        stats.dir_stats.dst_fchmod,
        stats.dir_stats.dst_fchown
    ));
}

fn add_security_section(report: &mut String, stats: &SyscallStats) {
    report.push_str("## 🔐 Security Assessment\n\n");

    let total_ops = stats.statx_total + stats.openat_total + stats.utimensat_total;
    let vulnerable_ops = stats.statx_path_based + stats.utimensat_path_based;
    let security_score = if total_ops > 0 {
        ((total_ops - vulnerable_ops) as f64 / total_ops as f64 * 100.0) as usize
    } else {
        100
    };

    report.push_str(&format!(
        "**Security Score:** {}/100 {}\n\n",
        security_score,
        match security_score {
            95..=100 => "🟢 Excellent",
            80..=94 => "🟡 Good",
            60..=79 => "🟠 Fair",
            _ => "🔴 Poor",
        }
    ));

    if stats.statx_path_based > 0 {
        report.push_str("⚠️  Path-based statx: TOCTOU risk\n");
    }
    if stats.utimensat_path_based > 0 {
        report.push_str("⚠️  Path-based utimensat: TOCTOU risk\n");
    }
    if vulnerable_ops == 0 {
        report.push_str("✅  100% FD-based operations: TOCTOU-safe\n");
    }
    report.push_str("\n");
}

fn add_recommendations_section(report: &mut String, stats: &SyscallStats, num_files: usize) {
    report.push_str("## 💡 Recommendations\n\n");

    let mut has_recommendations = false;

    if stats.statx_total > num_files * 2 {
        report.push_str(&format!(
            "- **Reduce redundant statx calls** (currently ~{:.1} per file)\n\
            - Target: 1 statx per file via `DirectoryFd::statx()`\n\n",
            stats.statx_total as f64 / num_files as f64
        ));
        has_recommendations = true;
    }

    if stats.statx_path_based > 0 || stats.openat_path_based > num_files {
        report.push_str(
            "- **Use dirfd-relative operations** instead of `AT_FDCWD` + absolute paths\n\
            - Benefits: TOCTOU-safe, potentially async via io_uring\n\n",
        );
        has_recommendations = true;
    }

    if !stats.io_uring_batch_sizes.is_empty() {
        let avg: f64 = stats.io_uring_batch_sizes.iter().sum::<usize>() as f64
            / stats.io_uring_batch_sizes.len() as f64;
        if avg < 2.0 {
            report.push_str(
                "- **Improve io_uring batching** (currently low batching efficiency)\n\
                - Target: Average batch size ≥2 ops/submit\n\n",
            );
            has_recommendations = true;
        }
    }

    if !has_recommendations {
        report.push_str("✅ No major issues detected. System is well-optimized!\n\n");
    }
}

fn add_summary_table(report: &mut String, stats: &SyscallStats, num_files: usize) {
    report.push_str("## 📊 Summary Table\n\n");
    report.push_str("| Operation | Count | Target | Status |\n");
    report.push_str("|-----------|-------|--------|--------|\n");

    report.push_str(&format!(
        "| io_uring_enter | {} | >100 | {} |\n",
        stats.io_uring_enter,
        if stats.io_uring_enter > 100 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        }
    ));

    report.push_str(&format!(
        "| statx (total) | {} | <{} | {} |\n",
        stats.statx_total,
        num_files * 2,
        if stats.statx_total < num_files * 2 {
            "✅ PASS"
        } else {
            "⚠️  WARN"
        }
    ));

    report.push_str(&format!(
        "| statx (path-based) | {} | =0 | {} |\n",
        stats.statx_path_based,
        if stats.statx_path_based == 0 {
            "✅ PASS"
        } else {
            "⚠️  WARN"
        }
    ));

    report.push_str(&format!(
        "| openat (user files) | {} | <{} | {} |\n",
        stats.openat_path_based,
        num_files * 4,
        if stats.openat_path_based < num_files * 4 {
            "✅ PASS"
        } else {
            "⚠️  WARN"
        }
    ));

    report.push_str(&format!(
        "| fallocate (direct) | {} | =0 | {} |\n",
        stats.fallocate,
        if stats.fallocate == 0 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        }
    ));

    report.push_str(&format!(
        "| utimensat (path-based) | {} | =0 | {} |\n",
        stats.utimensat_path_based,
        if stats.utimensat_path_based == 0 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        }
    ));

    report.push_str(&format!(
        "| utimensat (FD-based) | {} | ={} | {} |\n\n",
        stats.utimensat_fd_based,
        num_files,
        if stats.utimensat_fd_based == num_files {
            "✅ PASS"
        } else {
            "⚠️  WARN"
        }
    ));
}

fn determine_exit_code(stats: &SyscallStats, num_files: usize) -> ExitCode {
    // Critical failures
    if stats.io_uring_enter < 100 {
        return ExitCode::Failure;
    }
    if stats.fallocate > 0 {
        return ExitCode::Failure;
    }
    if stats.utimensat_path_based > 0 {
        return ExitCode::Failure;
    }

    // Warnings (but don't fail CI)
    if stats.statx_path_based > num_files * 2 {
        return ExitCode::Warning;
    }
    if stats.openat_path_based > num_files * 4 {
        return ExitCode::Warning;
    }

    ExitCode::Success
}
