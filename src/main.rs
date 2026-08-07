use bucketlang::ast::BucketKind;
use bucketlang::canonical::canonical_repr;
use bucketlang::compile::{compile_file, compile_with_base, BuildProfile, CompileOptions};
use bucketlang::eval::eval_bucket;
use bucketlang::graph::{build_graph, to_dot};
use bucketlang::harness::{build_context, run_subject_tests, splice_bucket_body, EditResult};
use bucketlang::lint::unused_warnings;
use bucketlang::render::{render_labelled, render_raw};
use bucketlang::value::{parse_arg, Value};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(name = "bkt", version, about = "Bucketlang compiler / interpreter")]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Parse, resolve, contracts, complexity
    Check {
        file: PathBuf,
        #[arg(long)]
        no_warn: bool,
        /// Include @test shadow buckets (default)
        #[arg(long, group = "profile")]
        dev: bool,
        /// Strip @test from the compiled program
        #[arg(long, group = "profile")]
        release: bool,
        #[arg(long = "non-strict", alias = "anon")]
        non_strict: bool,
    },
    /// Run (dev: run tests then entry; release: entry only)
    Run {
        file: PathBuf,
        #[arg(long, action = clap::ArgAction::Append)]
        arg: Vec<String>,
        #[arg(long)]
        stdin_arg: bool,
        #[arg(long)]
        entry: Option<String>,
        /// Print entry return value as JSON (host interop)
        #[arg(long)]
        json: bool,
        /// Print entry return value as `=> N`
        #[arg(long)]
        show_result: bool,
        /// Print test pass summary on stderr
        #[arg(long)]
        show_tests: bool,
        /// Show tests summary + => result (same as --show-tests --show-result)
        #[arg(long, short = 'v')]
        verbose: bool,
        /// Suppress unused param/local warnings
        #[arg(long)]
        no_warn: bool,
        /// Include @test shadow buckets (default)
        #[arg(long, group = "run_profile")]
        dev: bool,
        /// Strip @test; run entry only
        #[arg(long, group = "run_profile")]
        release: bool,
        #[arg(long = "non-strict", alias = "anon")]
        non_strict: bool,
    },
    /// Dump compiler layers (alias: dump)
    #[command(visible_alias = "dump")]
    Inspect {
        file: PathBuf,
        #[arg(long)]
        tokens: bool,
        #[arg(long)]
        parse_ast: bool,
        #[arg(long)]
        ast: bool,
        #[arg(long)]
        labelled: bool,
        #[arg(long)]
        manifest: bool,
        #[arg(long)]
        labels: bool,
        #[arg(long)]
        cores: bool,
        #[arg(long)]
        graph: bool,
        #[arg(long)]
        graph_dot: bool,
        #[arg(long)]
        entry: bool,
        #[arg(long)]
        tests: bool,
        #[arg(long)]
        complexity: bool,
        #[arg(long)]
        canonical: bool,
        #[arg(long)]
        pipeline: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        bucket: Option<String>,
        #[arg(long)]
        no_cores: bool,
        #[arg(long)]
        no_tests: bool,
        /// Include @test in dumps (default)
        #[arg(long, group = "inspect_profile")]
        dev: bool,
        /// Strip @test from compiled dump
        #[arg(long, group = "inspect_profile")]
        release: bool,
        #[arg(long = "non-strict", alias = "anon")]
        non_strict: bool,
    },
    /// JSON context pack for one bucket (LLM iterate)
    Context {
        file: PathBuf,
        #[arg(long)]
        bucket: String,
        #[arg(long, default_value_t = 1)]
        depth: usize,
        #[arg(long = "non-strict", alias = "anon")]
        non_strict: bool,
    },
    /// Replace one bucket body, recompile, run related @tests
    Edit {
        file: PathBuf,
        #[arg(long)]
        bucket: String,
        /// New body (expression / statements inside the braces)
        #[arg(long)]
        body: String,
        /// Write changes back to the file (default: dry-run, print new source)
        #[arg(long)]
        write: bool,
        #[arg(long = "non-strict", alias = "anon")]
        non_strict: bool,
    },
}

fn read_source(path: &PathBuf) -> Result<String, String> {
    if path.as_os_str() == "-" {
        let mut s = String::new();
        io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| e.to_string())?;
        Ok(s)
    } else {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "bkt" && ext != "bucket" && path.as_os_str() != "-" {
            eprintln!("warning: expected .bkt or .bucket extension");
        }
        std::fs::read_to_string(path).map_err(|e| e.to_string())
    }
}

fn emit_warnings(reg: &bucketlang::registry::Registry, no_warn: bool) {
    if no_warn {
        return;
    }
    for w in unused_warnings(reg) {
        eprintln!("warning: {}", w.message);
    }
}

fn opts(non_strict: bool, release: bool) -> CompileOptions {
    CompileOptions {
        strict: !non_strict,
        profile: if release {
            BuildProfile::Release
        } else {
            BuildProfile::Dev
        },
    }
}

fn compile_input(file: &PathBuf, o: CompileOptions) -> Result<bucketlang::CompileResult, String> {
    if file.as_os_str() == "-" {
        let src = read_source(file)?;
        bucketlang::compile::compile(&src, o).map_err(|e| e.to_string())
    } else {
        compile_file(file, o).map_err(|e| e.to_string())
    }
}

fn main() -> ExitCode {
    install_broken_pipe_hook();
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `println!` panics on BrokenPipe (e.g. `bkt … | head` or `| dot` when dot exits).
/// Treat that as a quiet successful exit instead of a scary backtrace.
fn install_broken_pipe_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info.to_string();
        if msg.contains("Broken pipe") || msg.contains("broken pipe") {
            return;
        }
        default(info);
    }));
}

fn real_main() -> Result<(), String> {
    let cli = Cli::parse();
    match cli.cmd {
        Commands::Check {
            file,
            no_warn,
            dev: _,
            release,
            non_strict,
        } => {
            let compiled = compile_input(&file, opts(non_strict, release))?;
            if compiled.registry.entry.is_none() {
                return Err("missing @entry (required for check)".into());
            }
            emit_warnings(&compiled.registry, no_warn);
            let profile = if release { "release" } else { "dev" };
            println!(
                "ok ({profile}): {} user buckets, {} tests, entry={}",
                compiled
                    .registry
                    .buckets
                    .values()
                    .filter(|b| b.kind == BucketKind::User)
                    .count(),
                compiled.registry.test_ids.len(),
                compiled.registry.entry.as_deref().unwrap()
            );
            Ok(())
        }
        Commands::Run {
            file,
            arg,
            stdin_arg,
            entry,
            json,
            show_result,
            show_tests,
            verbose,
            no_warn,
            dev: _,
            release,
            non_strict,
        } => {
            let compiled = compile_input(&file, opts(non_strict, release))?;
            let reg = &compiled.registry;
            emit_warnings(reg, no_warn);
            let show_tests = show_tests || verbose;
            let show_result = show_result || verbose;

            // Dev: run shadow tests (stdout sink). Release: test_ids is empty.
            let mut test_sink = io::sink();
            for tid in &reg.test_ids {
                let tb = reg.get(tid).unwrap();
                let result = eval_bucket(reg, tid, &[], &mut test_sink);
                if tb.expect_error {
                    if result.is_ok() {
                        return Err(format!(
                            "test {tid} failed: expected error but call succeeded"
                        ));
                    }
                } else {
                    result.map_err(|e| format!("test {tid} failed: {e}"))?;
                }
            }
            if show_tests {
                if release {
                    eprintln!("tests: skipped (release)");
                } else {
                    eprintln!("tests: {} passed", reg.test_ids.len());
                }
            }

            let entry_id = if let Some(e) = &entry {
                reg.resolve_target(e)
                    .ok_or_else(|| format!("unknown --entry {e}"))?
            } else {
                reg.entry
                    .clone()
                    .ok_or_else(|| "missing @entry".to_string())?
            };

            let entry_bucket = reg.get(&entry_id).unwrap();
            let args: Vec<Value> = if stdin_arg {
                let mut s = String::new();
                io::stdin()
                    .read_to_string(&mut s)
                    .map_err(|e| e.to_string())?;
                let parts: Vec<&str> = s.split_whitespace().collect();
                if parts.len() != entry_bucket.contract.params.len() {
                    return Err(format!(
                        "entry {} expects {} arg(s), got {}",
                        entry_id,
                        entry_bucket.contract.params.len(),
                        parts.len()
                    ));
                }
                parts
                    .iter()
                    .zip(entry_bucket.contract.params.iter())
                    .map(|(raw, p)| parse_arg(raw, Some(&p.ty)))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                if arg.len() != entry_bucket.contract.params.len() {
                    return Err(format!(
                        "entry {} expects {} arg(s), got {}",
                        entry_id,
                        entry_bucket.contract.params.len(),
                        arg.len()
                    ));
                }
                arg.iter()
                    .zip(entry_bucket.contract.params.iter())
                    .map(|(raw, p)| parse_arg(raw, Some(&p.ty)))
                    .collect::<Result<Vec<_>, _>>()?
            };

            let mut sink = io::stdout();
            let result = eval_bucket(reg, &entry_id, &args, &mut sink).map_err(|e| e.to_string())?;
            if json {
                println!("{}", result.to_json_string());
            } else if show_result {
                println!("=> {}", result.display());
            }
            Ok(())
        }
        Commands::Inspect {
            file,
            tokens,
            parse_ast,
            ast,
            labelled,
            manifest,
            labels,
            cores,
            graph,
            graph_dot,
            entry,
            tests,
            complexity,
            canonical,
            pipeline,
            json,
            bucket,
            no_cores,
            no_tests,
            dev: _,
            release,
            non_strict,
        } => {
            let compiled = compile_input(&file, opts(non_strict, release))?;
            let reg = &compiled.registry;
            let g = build_graph(reg);

            let any = tokens
                || parse_ast
                || ast
                || labelled
                || manifest
                || labels
                || cores
                || graph
                || graph_dot
                || entry
                || tests
                || complexity
                || canonical
                || pipeline;

            let show_all = !any;

            if json {
                let dump = JsonDump {
                    entry: reg.entry.clone(),
                    labels: reg.label_to_id.clone(),
                    tests: reg.test_ids.clone(),
                    manifest: reg
                        .buckets
                        .values()
                        .filter(|b| {
                            if no_cores && b.kind == BucketKind::Core {
                                return false;
                            }
                            if no_tests && b.kind == BucketKind::Test {
                                return false;
                            }
                            if let Some(filter) = &bucket {
                                return b.address == *filter
                                    || b.label.as_deref() == Some(filter.as_str());
                            }
                            true
                        })
                        .cloned()
                        .collect(),
                    graph: g,
                };
                println!("{}", serde_json::to_string_pretty(&dump).unwrap());
                return Ok(());
            }

            if show_all || pipeline {
                println!("== pipeline ==");
                println!("lex -> parse -> allocate/resolve -> contracts/complexity -> graph");
                println!();
            }
            if show_all || tokens {
                println!("== tokens ==");
                for t in &compiled.tokens {
                    if t.kind == bucketlang::lexer::TokenKind::Eof {
                        continue;
                    }
                    println!(
                        "  {:>4}:{:<3} {:?}\t{}",
                        t.line, t.col, t.kind, t.text
                    );
                }
                println!();
            }
            if show_all || parse_ast {
                println!("== parse-ast (pre-resolve) ==");
                println!("{}", serde_json::to_string_pretty(&compiled.raw_buckets).unwrap());
                println!();
            }
            if show_all || entry {
                println!("== entry ==");
                println!("  {:?}", reg.entry);
                println!();
            }
            if show_all || labels {
                println!("== labels ==");
                for (l, id) in &reg.label_to_id {
                    println!("  {l} -> {id}");
                }
                println!();
            }
            if show_all || manifest || cores || tests || complexity {
                println!("== manifest ==");
                for b in reg.buckets.values() {
                    if no_cores && b.kind == BucketKind::Core {
                        continue;
                    }
                    if no_tests && b.kind == BucketKind::Test {
                        continue;
                    }
                    if !cores && !show_all && !manifest && !tests && !complexity {
                        // unreachable combo
                    }
                    if let Some(filter) = &bucket {
                        if b.address != *filter && b.label.as_deref() != Some(filter.as_str()) {
                            continue;
                        }
                    }
                    if !show_all && cores && b.kind != BucketKind::Core {
                        continue;
                    }
                    if !show_all && tests && b.kind != BucketKind::Test && !manifest && !complexity {
                        continue;
                    }
                    let label = b.label.as_deref().unwrap_or("-");
                    let params: Vec<_> = b
                        .contract
                        .params
                        .iter()
                        .map(|p| format!("{}: {}", p.name, p.ty.name()))
                        .collect();
                    println!(
                        "  {}  label={label}  kind={:?}  ({}) -> {}  hash={}  complexity={{{}/{}/{}}}  desc={:?}",
                        b.address,
                        b.kind,
                        params.join(", "),
                        b.contract.ret.name(),
                        b.content_hash,
                        b.complexity.nodes,
                        b.complexity.depth,
                        b.complexity.calls,
                        b.desc
                    );
                }
                println!();
            }
            if show_all || ast || labelled || canonical {
                println!("== bodies ==");
                for b in reg.buckets.values() {
                    if b.kind == BucketKind::Core {
                        continue;
                    }
                    if no_tests && b.kind == BucketKind::Test {
                        continue;
                    }
                    if let Some(filter) = &bucket {
                        if b.address != *filter && b.label.as_deref() != Some(filter.as_str()) {
                            continue;
                        }
                    }
                    println!("  {}:", b.address);
                    if show_all || ast {
                        println!("    raw:      {}", render_raw(&b.body));
                    }
                    if show_all || labelled {
                        println!("    labelled: {}", render_labelled(&b.body, reg));
                    }
                    if show_all || canonical {
                        println!("    canonical: {}", canonical_repr(&b.body));
                    }
                }
                println!();
            }
            if show_all || graph {
                println!("== graph ==");
                for (id, node) in &g {
                    if no_cores && node.kind == BucketKind::Core {
                        continue;
                    }
                    if no_tests && node.kind == BucketKind::Test {
                        continue;
                    }
                    if node.kind == BucketKind::Core && node.inn.is_empty() {
                        continue;
                    }
                    let mark = if node.is_entry { " [entry]" } else { "" };
                    println!(
                        "  {id}{mark}  calls→ {:?}  called← {:?}",
                        node.out, node.inn
                    );
                }
                println!();
            }
            if graph_dot {
                print!("{}", to_dot(&g));
            }
            Ok(())
        }
        Commands::Context {
            file,
            bucket,
            depth,
            non_strict,
        } => {
            let compiled = compile_input(&file, opts(non_strict, false))?;
            let mut pack =
                build_context(&compiled.registry, &bucket, depth).map_err(|e| e.to_string())?;
            pack.file_hint = file.display().to_string();
            println!("{}", serde_json::to_string_pretty(&pack).unwrap());
            Ok(())
        }
        Commands::Edit {
            file,
            bucket,
            body,
            write,
            non_strict,
        } => {
            let src = read_source(&file)?;
            let new_src = splice_bucket_body(&src, &bucket, &body).map_err(|e| e.to_string())?;
            let compiled = match if file.as_os_str() == "-" {
                bucketlang::compile::compile(&new_src, opts(non_strict, false))
            } else {
                compile_with_base(&new_src, opts(non_strict, false), &file)
            } {
                Ok(c) => c,
                Err(e) => {
                    let result = EditResult {
                        ok: false,
                        bucket: bucket.clone(),
                        tests_run: 0,
                        tests_passed: 0,
                        error: Some(e.to_string()),
                        diff: None,
                        prints: None,
                        source: None,
                    };
                    println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    return Err(e.to_string());
                }
            };
            let mut sink = io::sink();
            match run_subject_tests(&compiled.registry, &bucket, &mut sink) {
                Ok((run, passed)) => {
                    if write {
                        if file.as_os_str() == "-" {
                            return Err("--write requires a real file path".into());
                        }
                        std::fs::write(&file, &new_src).map_err(|e| e.to_string())?;
                    }
                    let result = EditResult {
                        ok: true,
                        bucket: bucket.clone(),
                        tests_run: run,
                        tests_passed: passed,
                        error: None,
                        diff: None,
                        prints: None,
                        source: if write { None } else { Some(new_src) },
                    };
                    println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    Ok(())
                }
                Err(e) => {
                    let result = EditResult {
                        ok: false,
                        bucket: bucket.clone(),
                        tests_run: 0,
                        tests_passed: 0,
                        error: Some(e.to_string()),
                        diff: Some(e.to_string()),
                        prints: None,
                        source: None,
                    };
                    println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    Err(e.to_string())
                }
            }
        }
    }
}

#[derive(Serialize)]
struct JsonDump {
    entry: Option<String>,
    labels: std::collections::BTreeMap<String, String>,
    tests: Vec<String>,
    manifest: Vec<bucketlang::ast::Bucket>,
    graph: bucketlang::graph::Graph,
}
