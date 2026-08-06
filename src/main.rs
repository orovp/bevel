use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

use bevel::project::Project;
use bevel::spec::{Spec, Status};
use bevel::{
    affected, board, config, context, docs, gate, inbox, index, lifecycle, lockfile, method,
    migrate, packs, paths, project, review, spec, summary, sync, templates, validate, verify,
    workspace, VERSION,
};

#[derive(Parser)]
#[command(
    name = "bevel",
    version = VERSION,
    about = "Spec-driven development harness for coding agents",
    long_about = "Turns inbox ideas into approved specs, and approved specs into code.\n\
                  Approval, validation and completion are exit codes here, not judgments."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage bevel inside this project
    #[command(subcommand)]
    Project(ProjectCmd),
    /// Capture and inspect inbox items
    #[command(subcommand)]
    Inbox(InboxCmd),
    /// Reserve an id and scaffold a spec from an inbox item
    Shape(ShapeArgs),
    /// Check specs against the deterministic rules
    Validate(IdArgs),
    /// Freeze the contract. Requires a terminal: this is the human gate
    Approve(IdArgs),
    /// Exit 0 if a spec may be implemented, 1 otherwise
    Gate(IdArgs),
    /// Claim the active slot for an approved spec
    Start(IdArgs),
    /// Finish a spec: enforces markers, verification and human judgement
    Close(IdArgs),
    /// Release the active slot without losing the approval
    Pause(IdArgs),
    /// Assemble the dossier a human approves or closes from
    Review(ReviewArgs),
    /// The whole pipeline on one page, for a human
    Board(OpenArgs),
    /// Regenerate specs/README.md
    Index(IndexArgs),
    /// Fixed-size summary, independent of spec count
    Status(StatusArgs),
    /// Enumerate specs
    List(ListArgs),
    /// Install the method into ~/.claude and this project's .claude/settings.json
    Sync(SyncArgs),
    /// Print the agent notes for this project, to apply yourself if you want them
    Notes(NotesArgs),
    /// Inspect, print or download the method tree
    #[command(subcommand)]
    Method(MethodCmd),
    /// Format a file using the pack that owns it
    Fmt(FmtArgs),
    /// Advisory: report unfinished work on the active spec. Always exits 0
    Pending,
    /// Move this project onto the running bevel version
    Migrate(MigrateArgs),
    /// Fetch documentation pinned to the version in your lockfile
    Docs(DocsArgs),
    /// Run the active packs' checks
    Verify(VerifyArgs),
    /// Inspect layers, workspace, packs and gates; refresh the package map
    Doctor(DoctorArgs),
}

#[derive(Subcommand)]
enum ProjectCmd {
    /// Scaffold .bevel/, specs/ and INBOX.md
    Init {
        /// Also create apps/ and crates/
        #[arg(long)]
        monorepo: bool,
        /// Defaults to the current directory
        #[arg(long)]
        path: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum InboxCmd {
    /// Append an idea. Capture is meant to be cheap; precision comes later
    Add {
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Show inbox items with their indices
    List,
}

#[derive(Args)]
struct ShapeArgs {
    /// An inbox item number, or free text to shape directly
    #[arg(required = true, trailing_var_arg = true)]
    target: Vec<String>,
    /// Title for the spec; defaults to the item text
    #[arg(long)]
    title: Option<String>,
}

#[derive(Args)]
struct IdArgs {
    /// Spec id: 7, 0007 or 0007-slug. Validate accepts none, meaning all
    id: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ReviewArgs {
    /// Spec id: 7, 0007 or 0007-slug
    id: String,
    /// Also open it in the default browser
    #[arg(long)]
    open: bool,
}

#[derive(Args)]
struct IndexArgs {
    /// Also write the decision log and supersession graph, for a human
    #[arg(long)]
    html: bool,
    /// With --html, also open it in the default browser
    #[arg(long)]
    open: bool,
}

#[derive(Args)]
struct OpenArgs {
    /// Also open it in the default browser
    #[arg(long)]
    open: bool,
}

#[derive(Args)]
struct StatusArgs {
    #[arg(long)]
    json: bool,
    /// A few lines only, for injection at session start
    #[arg(long)]
    brief: bool,
}

/// `bevel notes [FILE]`, printing markdown and nothing else.
///
/// Stdout stays pure so `bevel notes > AGENTS.md` is the whole workflow. A
/// "wrote it for you" line here would be the same overreach the command exists
/// to undo, one stream over.
#[derive(Args)]
struct NotesArgs {
    /// Which file the markdown is for
    #[arg(value_enum, default_value_t = NotesFile::Agents)]
    file: NotesFile,
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum NotesFile {
    /// The body a project says about itself
    Agents,
    /// The two-line pointer at AGENTS.md
    Claude,
}

#[derive(Subcommand)]
enum MethodCmd {
    /// List the skills and subagents available
    List,
    /// Print one body, for agents without slash commands
    Show { name: String },
    /// Download the method tree from GitHub into the cache
    Fetch {
        /// Branch, tag or commit SHA; defaults to the configured ref
        #[arg(long, value_name = "REF")]
        r#ref: Option<String>,
    },
    /// Show where the method resolves from and what it contains
    Where,
}

#[derive(Args)]
struct SyncArgs {
    /// Which agents to render for, comma-separated. Defaults to whichever are
    /// detected in your home directory
    #[arg(long, value_name = "LIST", value_delimiter = ',')]
    agent: Vec<String>,
    /// Install the format-on-write, session-start and stop hooks
    #[arg(long)]
    hooks: bool,
}

#[derive(Args)]
struct FmtArgs {
    #[arg(long, value_name = "PATH")]
    file: Option<PathBuf>,
    /// Read the file path from a hook payload on stdin
    #[arg(long)]
    hook: bool,
}

#[derive(Args)]
struct MigrateArgs {
    /// Report what would change without touching anything
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args)]
struct VerifyArgs {
    /// Scope to the packages touched by your changes, plus their dependents
    #[arg(long)]
    affected: bool,
    /// Also count everything committed since this ref
    #[arg(long, value_name = "REF")]
    since: Option<String>,
    /// Run only this pack
    #[arg(long, value_name = "ID")]
    pack: Option<String>,
}

#[derive(Args)]
struct DocsArgs {
    /// A pack short name (tokio, angular) or a Context7 library id
    library: String,
    /// Narrow to one subject, e.g. "graceful shutdown"
    #[arg(long)]
    topic: Option<String>,
    /// Serve only from cache; never touch the network
    #[arg(long)]
    offline: bool,
    /// Write into .bevel/cache/context-pack-<id>.md for this spec
    #[arg(long, value_name = "ID")]
    spec: Option<String>,
}

#[derive(Args)]
struct DoctorArgs {
    /// Detect the workspace and write the package map into project.toml
    #[arg(long)]
    write: bool,
    /// Audit what the harness injects against its token budget
    #[arg(long)]
    context: bool,
    /// Render the audit as a page, with the trend a table cannot show
    #[arg(long)]
    html: bool,
    /// With --html, also open it in the default browser
    #[arg(long)]
    open: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ListArgs {
    /// draft | review | approved | implementing | done | superseded
    #[arg(long)]
    status: Option<String>,
    #[arg(long)]
    json: bool,
}

/// Rust ignores SIGPIPE, which makes `println!` panic once the reader closes the
/// pipe — so `bevel list | head` or quitting a pager mid-output dies with a
/// backtrace. Restoring the OS default makes the process exit quietly, which is
/// what every other CLI does and matters doubly here because editor hooks invoke
/// this binary and would surface the panic as a tool failure.
#[cfg(unix)]
fn restore_sigpipe() {
    // SAFETY: called once at startup, before any thread or handler exists.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_sigpipe() {}

fn main() -> ExitCode {
    restore_sigpipe();
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Project(ProjectCmd::Init { monorepo, path }) => {
            let root = match path {
                Some(p) => p,
                None => std::env::current_dir()?,
            };
            let cfg = project::init(&root, monorepo)?;
            println!("initialised {}", cfg.display());
            println!("  next: bevel inbox add \"your first idea\"");
            Ok(ExitCode::SUCCESS)
        }

        Command::Inbox(InboxCmd::Add { text }) => {
            let p = Project::discover()?;
            let joined = text.join(" ");
            inbox::add(&p.inbox_path(), &joined)?;
            let n = inbox::parse(&p.inbox_path())?.len();
            println!("added item {n}: {joined}");
            Ok(ExitCode::SUCCESS)
        }

        Command::Inbox(InboxCmd::List) => {
            let p = Project::discover()?;
            let items = inbox::parse(&p.inbox_path())?;
            if items.is_empty() {
                println!("inbox is empty");
            }
            for i in items {
                match i.linked {
                    Some(link) => println!("  {:>3}  {}  -> {link}", i.index, i.text),
                    None => println!("  {:>3}  {}", i.index, i.text),
                }
            }
            Ok(ExitCode::SUCCESS)
        }

        Command::Shape(args) => cmd_shape(args),
        Command::Validate(args) => cmd_validate(args),
        Command::Approve(args) => cmd_approve(args),
        Command::Gate(args) => cmd_gate(args),
        Command::Start(args) => cmd_start(args),
        Command::Close(args) => cmd_close(args),
        Command::Pause(args) => cmd_pause(args),
        Command::Review(args) => cmd_review(args),
        Command::Board(args) => {
            let p = Project::discover()?;
            let path = board::write(&p, args.open)?;
            println!("wrote {}", p.display_path(&path));
            println!("  file://{}", path.display());
            Ok(ExitCode::SUCCESS)
        }
        Command::Index(args) => {
            let p = Project::discover()?;
            let path = index::write(&p)?;
            println!("wrote {}", p.display_path(&path));
            if args.html {
                let html = index::write_html(&p, args.open)?;
                println!("wrote {}", p.display_path(&html));
                println!("  file://{}", html.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Status(args) => cmd_status(args),
        Command::List(args) => cmd_list(args),
        Command::Verify(args) => cmd_verify(args),
        Command::Doctor(args) => cmd_doctor(args),
        Command::Sync(args) => cmd_sync(args),
        Command::Notes(args) => cmd_notes(args),
        Command::Docs(args) => cmd_docs(args),
        Command::Method(cmd) => cmd_method(cmd),
        Command::Fmt(args) => cmd_fmt(args),
        Command::Pending => cmd_pending(),
        Command::Migrate(args) => cmd_migrate(args),
    }
}

fn cmd_docs(args: DocsArgs) -> Result<ExitCode> {
    let p = Project::discover()?;
    let layers = paths::Layers::resolve()?;
    let cfg = config::Config::load(&layers)?;
    let deps = lockfile::scan(&p.root);
    let m = method::resolve(Some(&p), &layers, &cfg.method);

    // Accept a pack short name, a pack id, or a raw Context7 library id.
    let all_packs = packs::load_all(&p, &layers, &m)?;
    let pack = all_packs
        .iter()
        .find(|pk| pk.short_id() == args.library || pk.id == args.library);

    let (library, version) = match pack {
        Some(pk) => {
            let lib = pk
                .library()
                .with_context(|| format!("pack `{}` declares no Context7 library", pk.id))?;
            let dep = pk.detect_dependency.as_deref().unwrap_or(pk.short_id());
            (
                lib.to_string(),
                deps.version(pk.ecosystem, dep).map(str::to_string),
            )
        }
        None if args.library.starts_with('/') => (
            args.library.clone(),
            deps.any_version(&args.library).map(|(_, v)| v.to_string()),
        ),
        None => bail!(
            "no pack named `{}` and it is not a Context7 library id (those start with `/`)\n  \
             known packs: {}",
            args.library,
            all_packs
                .iter()
                .filter(|p| p.library().is_some())
                .map(|p| p.short_id())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };

    let req = docs::Request {
        library,
        version: version.clone(),
        topic: args.topic.clone(),
    };
    let outcome = docs::fetch(&cfg.context7, &layers.context7_cache(), &req, args.offline)?;

    let (doc, note) = match &outcome {
        docs::Outcome::Fetched(d) => (d, "fetched".to_string()),
        docs::Outcome::Cached(d) => (d, "from cache".to_string()),
        docs::Outcome::Stale(d, why) => (d, format!("stale cache ({why})")),
        docs::Outcome::Unavailable { marker, reason } => {
            // Not an error: nothing here is on the critical path, and the
            // marker is the deliverable when the network is closed.
            eprintln!("unavailable: {reason}");
            println!("{marker}");
            if let Some(id) = &args.spec {
                let spec = spec::find(&p.specs_dir(), id)?;
                append_note(&spec.dir.join("notes.md"), marker)?;
                eprintln!("recorded in {}", p.display_path(&spec.dir.join("notes.md")));
            }
            return Ok(ExitCode::SUCCESS);
        }
    };

    let pinned = if doc.version_pinned {
        format!(
            "version-pinned to {}",
            doc.version.as_deref().unwrap_or("?")
        )
    } else if doc.version.is_some() {
        format!(
            "NOT version-pinned (lockfile says {}, no versioned library exists)",
            doc.version.as_deref().unwrap_or("?")
        )
    } else {
        "no version known from any lockfile".to_string()
    };
    eprintln!("{} — {note}, {pinned}", doc.library_used);

    match &args.spec {
        Some(id) => {
            let spec = spec::find(&p.specs_dir(), id)?;
            let path = p
                .cache_dir()
                .join(format!("context-pack-{}.md", spec.front.id));
            std::fs::create_dir_all(p.cache_dir())?;
            let header = format!(
                "# Context pack for {}\n\n\
                 Library: `{}`\nVersion: {}\nTopic: {}\n\n---\n\n",
                spec.front.id,
                doc.library_used,
                doc.version.as_deref().unwrap_or("unknown"),
                doc.topic.as_deref().unwrap_or("(all)")
            );
            std::fs::write(&path, format!("{header}{}", doc.text))?;
            println!("{}", p.display_path(&path));
            if !doc.version_pinned && doc.version.is_some() {
                append_note(
                    &spec.dir.join("notes.md"),
                    &docs::marker(
                        &req.library,
                        doc.version.as_deref(),
                        "no versioned library available",
                    ),
                )?;
            }
        }
        None => print!("{}", doc.text),
    }
    Ok(ExitCode::SUCCESS)
}

fn append_note(path: &std::path::Path, line: &str) -> Result<()> {
    let mut text = std::fs::read_to_string(path).unwrap_or_default();
    if text.contains(line) {
        return Ok(());
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(line);
    text.push('\n');
    std::fs::write(path, text)?;
    Ok(())
}

fn cmd_sync(args: SyncArgs) -> Result<ExitCode> {
    // Optional on purpose. The method installs into `$HOME` and belongs to the
    // machine, not to any project, so `sync` has to work from anywhere — which
    // includes a fresh machine that has no projects yet.
    let p = Project::discover().ok();
    let layers = paths::Layers::resolve()?;
    let cfg = config::Config::load(&layers)?;
    let mut source = method::resolve(p.as_ref(), &layers, &cfg.method);

    // First run on a new machine: fetching here rather than erroring is the
    // difference between one command and two, and `sync` is a setup step where
    // a network call is expected.
    if !source.is_usable() && !source.is_local() {
        println!("method not cached; fetching {}", source.origin);
        let report = method::fetch(
            &layers,
            &cfg.method,
            None,
            cfg.context7.timeout_secs.max(20),
        )?;
        println!("  {} {}", report.git_ref, report.content_hash);
        source = method::resolve(p.as_ref(), &layers, &cfg.method);
    }

    // An explicit list beats detection, and an unknown name in it stops the
    // run rather than rendering nothing for it and reporting success.
    let agents = if args.agent.is_empty() {
        sync::detect(&layers)
    } else {
        args.agent
            .iter()
            .map(|n| sync::Agent::parse(n))
            .collect::<Result<Vec<_>>>()?
    };

    println!("method: {}", source.origin);
    println!(
        "agents: {}",
        agents
            .iter()
            .map(|a| a.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    for action in sync::sync(p.as_ref(), &layers, &source, &agents, args.hooks)? {
        println!("{action}");
    }
    // Say which half ran. Silence here reads as "everything is installed", and
    // the settings file really is missing.
    if p.is_none() {
        println!(
            "\nno project here, so only the machine-wide method was installed.\n  \
             run `bevel sync` inside a repository for its .claude/settings.json"
        );
    }
    // Sync used to write AGENTS.md and CLAUDE.md, so this is where someone
    // finds out that it does not any more — and what to run instead. Named on
    // every run rather than only when the file is missing: bevel deliberately
    // does not look, and guessing from an absence it never checks would be a
    // worse habit than one extra line.
    println!("\nproject notes are yours to write. `bevel notes` prints a starting point:\n  bevel notes > AGENTS.md");
    Ok(ExitCode::SUCCESS)
}

/// Print the markdown, and nothing else at all.
///
/// No project lookup, no method tree, no layers: the text is a constant in the
/// binary, so this works in an empty directory and on a machine where nothing
/// has been installed yet. That is deliberate — it is the command someone runs
/// *before* the repository looks like anything.
fn cmd_notes(args: NotesArgs) -> Result<ExitCode> {
    print!(
        "{}",
        sync::notes(match args.file {
            NotesFile::Agents => sync::Notes::Agents,
            NotesFile::Claude => sync::Notes::Claude,
        })
    );
    Ok(ExitCode::SUCCESS)
}

fn cmd_method(cmd: MethodCmd) -> Result<ExitCode> {
    let layers = paths::Layers::resolve()?;
    let cfg = config::Config::load(&layers)?;
    let project = Project::discover().ok();
    let source = method::resolve(project.as_ref(), &layers, &cfg.method);

    match cmd {
        MethodCmd::Fetch { r#ref } => {
            let report = method::fetch(
                &layers,
                &cfg.method,
                r#ref.as_deref(),
                cfg.context7.timeout_secs.max(20),
            )?;
            println!("{}@{}", cfg.method.repo, report.git_ref);
            println!("  {}", report.root.display());
            println!("  {}", report.content_hash);
            println!(
                "  {}",
                if report.changed {
                    "the method changed"
                } else {
                    "unchanged since the last fetch"
                }
            );
            if source.is_local() {
                // Otherwise the fetch looks like it did nothing.
                println!(
                    "note: a local override is active ({}), so this download is not what will be used",
                    source.origin
                );
            }
            Ok(ExitCode::SUCCESS)
        }

        MethodCmd::Where => {
            println!("source   {}", source.origin);
            println!("root     {}", source.root.display());
            println!("usable   {}", if source.is_usable() { "yes" } else { "NO" });
            if let Some(m) = method::meta(&source.root) {
                println!("ref      {}@{}", m.repo, m.git_ref);
                println!("hash     {}", m.content_hash);
                println!("fetched  {}", m.fetched_at);
            } else if source.is_local() {
                println!("hash     {}", method::hash_tree(&source.root));
            }
            println!();
            for (label, path, which) in sync::method_sources(&layers, &source) {
                println!("  {label:<27} {which:<8} {}", path.display());
            }
            if !source.is_usable() {
                eprintln!("\n{}", method::missing_help(&source, &layers));
                return Ok(ExitCode::FAILURE);
            }
            Ok(ExitCode::SUCCESS)
        }

        MethodCmd::List => {
            for n in sync::method_names() {
                println!("{n}");
            }
            Ok(ExitCode::SUCCESS)
        }

        MethodCmd::Show { name } => match sync::method_body(&layers, &source, &name) {
            Some(body) => {
                print!("{body}");
                Ok(ExitCode::SUCCESS)
            }
            None if !source.is_usable() => {
                bail!("{}", method::missing_help(&source, &layers))
            }
            None => bail!(
                "no method named `{name}`\n  available: {}",
                sync::method_names().join(", ")
            ),
        },
    }
}

fn cmd_fmt(args: FmtArgs) -> Result<ExitCode> {
    let file = match (&args.file, args.hook) {
        (Some(f), _) => f.clone(),
        (None, true) => match hook_file_path()? {
            Some(f) => f,
            // Nothing to format is the common case for most tool calls.
            None => return Ok(ExitCode::SUCCESS),
        },
        (None, false) => bail!("fmt needs --file <path> or --hook"),
    };

    let Some(ext) = file.extension().and_then(|e| e.to_str()) else {
        return Ok(ExitCode::SUCCESS);
    };

    let p = Project::discover()?;
    let layers = paths::Layers::resolve()?;
    let cfg = config::Config::load(&layers)?;
    let m = method::resolve(Some(&p), &layers, &cfg.method);
    // A missing method must not fail a file write.
    if !m.is_usable() {
        return Ok(ExitCode::SUCCESS);
    }
    let ws = workspace::detect(&p.root)?;
    let deps = lockfile::scan(&p.root);
    let package_paths: Vec<String> = ws.packages.iter().map(|p| p.path.clone()).collect();
    let active = packs::active(
        &packs::load_all(&p, &layers, &m)?,
        &p.root,
        &package_paths,
        &deps,
    );

    for pack in active.iter().filter(|pk| pk.owns_extension(ext)) {
        let Some(step) = pack.fix_step() else {
            continue;
        };
        let Some(fix) = &step.fix else { continue };
        let status = bevel::shell::command(fix).current_dir(&p.root).status();
        // A formatter that is not installed must not fail a write.
        if let Ok(s) = status {
            if !s.success() {
                eprintln!("{}/{} exited non-zero", pack.id, step.name);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Claude Code passes hook input as JSON on stdin.
fn hook_file_path() -> Result<Option<PathBuf>> {
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).ok();
    if buf.trim().is_empty() {
        return Ok(None);
    }
    let v: serde_json::Value = match serde_json::from_str(&buf) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    Ok(v.get("tool_input")
        .and_then(|t| t.get("file_path"))
        .and_then(|f| f.as_str())
        .map(PathBuf::from))
}

/// Advisory only. A Stop hook that blocks can trap an agent in a loop, so this
/// reports and always succeeds.
fn cmd_pending() -> Result<ExitCode> {
    let p = Project::discover()?;
    let Some(id) = gate::active_spec(&p)? else {
        return Ok(ExitCode::SUCCESS);
    };
    let s = spec::find(&p.specs_dir(), &id)?;
    let total = s.tier_a_tests().len();
    let remaining = validate::pending_markers(&p.root, &id);

    if remaining > 0 {
        println!(
            "spec {id} is still open: {}/{total} criteria live, {remaining} pending. \
             Run `bevel verify --affected`, then reconcile notes.md.",
            total.saturating_sub(remaining)
        );
    } else {
        println!("spec {id} has all {total} criteria live. Reconcile notes.md and close it.");
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_migrate(args: MigrateArgs) -> Result<ExitCode> {
    let mut p = Project::discover()?;
    let plan = migrate::plan(&p)?;

    for orphan in migrate::orphan_dirs(&p.specs_dir()) {
        println!("orphan  specs/{orphan} has no spec.md");
    }

    if args.dry_run {
        if plan.is_noop() && !plan.blocked() {
            println!("nothing to migrate");
        } else {
            if plan.pin_changes {
                println!("would set pin {} -> {}", plan.current_pin, plan.new_pin);
            }
            for (id, v) in &plan.upgradable {
                println!("would upgrade spec {id} from schema {v}");
            }
            for (id, v) in &plan.ahead {
                println!("BLOCKED spec {id} is at schema {v}, newer than this binary");
            }
        }
        return Ok(if plan.blocked() {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        });
    }

    let done = migrate::apply(&mut p)?;
    if done.is_empty() {
        println!("nothing to migrate");
    }
    for line in done {
        println!("{line}");
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_verify(args: VerifyArgs) -> Result<ExitCode> {
    let p = Project::discover()?;
    let layers = paths::Layers::resolve()?;
    let cfg = config::Config::load(&layers)?;
    let m = method::resolve(Some(&p), &layers, &cfg.method);
    if !m.is_usable() {
        bail!("{}", method::missing_help(&m, &layers));
    }
    let ws = workspace::detect(&p.root)?;
    let all_packs = packs::load_all(&p, &layers, &m)?;
    let deps = lockfile::scan(&p.root);
    let package_paths: Vec<String> = ws.packages.iter().map(|p| p.path.clone()).collect();
    let active = packs::active(&all_packs, &p.root, &package_paths, &deps);

    if active.is_empty() {
        println!("no active packs — nothing to verify");
        return Ok(ExitCode::SUCCESS);
    }

    let scope = if args.affected || args.since.is_some() {
        affected::compute(&p.root, &ws, args.since.as_deref())?
    } else {
        affected::Scope::Full("--affected not requested".into())
    };

    match &scope {
        affected::Scope::Nothing => {
            println!("no changes since the base — nothing to verify");
            return Ok(ExitCode::SUCCESS);
        }
        affected::Scope::Full(reason) => println!("scope: whole workspace ({reason})"),
        affected::Scope::Packages(names) => println!("scope: {}", names.join(", ")),
    }

    let results = verify::run(&p.root, &active, &ws.packages, &scope, args.pack.as_deref())?;
    let ok = results.iter().all(|r| r.ok());
    println!("{}", verify::summarise(&results));
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn cmd_doctor(args: DoctorArgs) -> Result<ExitCode> {
    let mut p = Project::discover()?;
    let layers = paths::Layers::resolve()?;
    let mut healthy = true;

    // A separate report rather than another section: this one is about the
    // harness itself, not about the project.
    if args.context {
        let cfg = config::Config::load(&layers)?;
        let m = method::resolve(Some(&p), &layers, &cfg.method);
        let audit = context::audit(&p, &layers, &m)?;
        if args.json {
            println!("{}", serde_json::to_string_pretty(&audit)?);
        } else if args.html {
            let page = context::render_html(&audit, &context::history(&p, &audit));
            let path = bevel::html::write(&p, "context-budget.html", &page, args.open)?;
            println!("wrote {}", p.display_path(&path));
            println!("  file://{}", path.display());
        } else {
            print!("{}", context::render(&audit));
        }
        return Ok(if audit.over_budget().is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }

    println!("bevel      {VERSION}");
    println!("config     {}", layers.config.display());
    println!("cache      {}", layers.cache.display());
    println!("project    {}", p.root.display());

    // Version drift between machines must fail loudly, not behave subtly
    // differently on the other laptop.
    match project::classify_pin(&p.config.bevel, VERSION) {
        project::Pin::Satisfied => println!("pin        {} satisfied", p.config.bevel),
        project::Pin::BinaryTooOld => {
            healthy = false;
            println!(
                "pin        MISMATCH: project wants {}, this is {VERSION}\n           \
                 This binary is behind. Upgrade it:\n           \
                 npm i -g @orovp/bevel   (or: cargo install bevel)",
                p.config.bevel
            );
        }
        project::Pin::ProjectTooOld => {
            healthy = false;
            println!(
                "pin        MISMATCH: project pinned to {}, this is {VERSION}\n           \
                 The project is behind. Move it forward:\n           \
                 bevel migrate",
                p.config.bevel
            );
        }
        project::Pin::Unrecognised => {
            println!("pin        {} (unrecognised, not checked)", p.config.bevel)
        }
    }

    let ws = workspace::detect(&p.root)?;
    println!("\nworkspace  {} packages", ws.packages.len());
    for pkg in &ws.packages {
        let deps = if pkg.depends_on.is_empty() {
            String::new()
        } else {
            format!("  -> {}", pkg.depends_on.join(", "))
        };
        println!("  {:<10} {:<24}{deps}", pkg.name, pkg.path);
    }
    if !ws.graph_complete {
        healthy = false;
        println!("  graph incomplete — verify --affected will widen to a full run");
    }
    for note in &ws.notes {
        println!("  note: {note}");
    }

    let dcfg = config::Config::load(&layers)?;
    let m = method::resolve(Some(&p), &layers, &dcfg.method);
    println!("\nmethod     {}", m.origin);
    println!("  root     {}", m.root.display());
    if !m.is_usable() {
        healthy = false;
        println!("  NOT INSTALLED — run `bevel method fetch`");
    } else if let Some(meta) = method::meta(&m.root) {
        println!("  {} fetched {}", meta.content_hash, meta.fetched_at);
    } else if m.is_local() {
        println!("  {} (live local tree)", method::hash_tree(&m.root));
    }

    let all_packs = if m.is_usable() {
        packs::load_all(&p, &layers, &m)?
    } else {
        Vec::new()
    };
    let deps = lockfile::scan(&p.root);
    let package_paths: Vec<String> = ws.packages.iter().map(|p| p.path.clone()).collect();
    let active = packs::active(&all_packs, &p.root, &package_paths, &deps);
    println!(
        "\npacks      {} active of {}",
        active.len(),
        all_packs.len()
    );
    for pack in &active {
        let trigger = pack
            .trigger
            .as_ref()
            .map(|t| t.to_string())
            .unwrap_or_default();
        let gotchas = match packs::gotchas(pack, &p, &layers) {
            Some(_) => "gotchas",
            // Absent by design in built-ins: a shipped pack cannot know your
            // conventions, and inventing them would be worse than a gap.
            None => "no gotchas",
        };
        println!(
            "  {:<14} {:<8} {:<22} {:>2} checks  {}",
            pack.id,
            pack.source.as_str(),
            trigger,
            pack.verify.len(),
            gotchas
        );
    }
    if let Some(pack) = active
        .iter()
        .find(|pk| packs::gotchas(pk, &p, &layers).is_none())
    {
        let where_to = packs::gotchas_candidates(pack, &p, &layers);
        println!(
            "  to add conventions for {}: {}",
            pack.id,
            where_to[1].display()
        );
    }

    // "Why is my skill edit not taking effect?" is an afternoon without this.
    println!("\nskills");
    for (name, path, which) in sync::method_sources(&layers, &m) {
        println!("  {name:<27} {which:<8} {}", path.display());
    }

    // Where each kind lands per agent. opencode reads Claude Code's skills
    // directory, and if that ever stops being true nothing breaks loudly — the
    // skills simply stop loading. This is what makes that visible.
    let agents = sync::detect(&layers);
    println!(
        "\nagents     {} detected",
        agents
            .iter()
            .map(|a| a.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    for (agent, kind, dest) in sync::destinations(&layers, &agents) {
        println!("  {agent:<10} {kind:<13} {}", dest.display());
    }
    if agents.contains(&sync::Agent::Opencode) {
        println!("  opencode subagents require opencode v2 or newer");
    }

    // A gate that silently stopped matching is worse than one that never existed.
    let mut broken = Vec::new();
    for s in spec::all(&p.specs_dir())? {
        if matches!(s.front.status, Status::Approved | Status::Implementing)
            && gate::check(&p, &s)? == gate::Verdict::HashMismatch
        {
            broken.push(s.front.id);
        }
    }
    println!("\ngates      {} reopened by edits", broken.len());
    for id in &broken {
        healthy = false;
        println!("  {id} was approved, then changed — re-approve before implementing");
    }

    if args.write {
        p.config.packages = ws.packages.clone();
        let path = p.state_dir().join(project::CONFIG_FILE);
        std::fs::write(&path, toml::to_string_pretty(&p.config)?)?;
        println!("\nwrote the package map to {}", p.display_path(&path));
    }

    Ok(if healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Bookkeeping only: reserve the id, create the directory, link the inbox item.
/// Every part of this is the kind of task a model gets wrong occasionally and
/// `read_dir` never does.
fn cmd_shape(args: ShapeArgs) -> Result<ExitCode> {
    let p = Project::discover()?;
    let joined = args.target.join(" ");

    let (source_text, item_index) = match joined.trim().parse::<usize>() {
        Ok(n) => {
            let items = inbox::parse(&p.inbox_path())?;
            let item = items
                .iter()
                .find(|i| i.index == n)
                .with_context(|| format!("no inbox item {n} (there are {})", items.len()))?;
            if let Some(link) = &item.linked {
                bail!("inbox item {n} is already shaped: {link}");
            }
            (item.text.clone(), Some(n))
        }
        Err(_) => (joined.clone(), None),
    };

    let title = args.title.unwrap_or_else(|| source_text.clone());
    let id = spec::next_id(&p.specs_dir())?;
    let slug = spec::slugify(&title);
    let dir = p.specs_dir().join(format!("{id}-{slug}"));
    if dir.exists() {
        bail!("{} already exists", dir.display());
    }
    std::fs::create_dir_all(&dir)?;

    let created = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let layers = paths::Layers::resolve()?;
    let cfg = config::Config::load(&layers)?;
    let m = method::resolve(Some(&p), &layers, &cfg.method);
    if !m.is_usable() {
        bail!("{}", method::missing_help(&m, &layers));
    }
    templates::write_all(&m, &dir, &id, &title, &created, &source_text)?;

    if let Some(n) = item_index {
        let rel = format!("{}/{id}-{slug}/spec.md", p.config.specs_dir);
        inbox::link(&p.inbox_path(), n, &id, &rel)?;
    }

    index::write(&p)?;
    println!("created {}", p.display_path(&dir));
    println!("  next: shape it with /shape, then bevel validate {id}");
    Ok(ExitCode::SUCCESS)
}

fn cmd_validate(args: IdArgs) -> Result<ExitCode> {
    let p = Project::discover()?;
    let targets: Vec<Spec> = match &args.id {
        Some(id) => vec![spec::find(&p.specs_dir(), id)?],
        None => spec::all(&p.specs_dir())?,
    };
    if targets.is_empty() {
        println!("no specs yet");
        return Ok(ExitCode::SUCCESS);
    }

    let mut all_findings = Vec::new();
    let mut clean = true;
    for mut s in targets {
        let findings = validate::validate(&p, &s)?;
        if findings.is_empty() {
            let promoted = validate::promote_if_clean(&mut s, &findings)?;
            if !args.json {
                let note = if promoted { "  -> review" } else { "" };
                println!("ok    {} {}{note}", s.front.id, s.front.title);
            }
        } else {
            clean = false;
            if !args.json {
                println!("FAIL  {} {}", s.front.id, s.front.title);
                for f in &findings {
                    println!("        {f}");
                }
            }
            all_findings.push((s.front.id.clone(), findings));
        }
    }

    if args.json {
        let payload: Vec<_> = all_findings
            .iter()
            .map(|(id, fs)| {
                serde_json::json!({
                    "id": id,
                    "findings": fs.iter().map(|f| serde_json::json!({
                        "rule": f.rule, "message": f.message
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
    }

    Ok(if clean {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn cmd_approve(args: IdArgs) -> Result<ExitCode> {
    let p = Project::discover()?;
    let id = args.id.context("approve needs a spec id")?;
    let mut s = spec::find(&p.specs_dir(), &id)?;

    let findings = validate::validate(&p, &s)?;
    if !findings.is_empty() {
        eprintln!("spec {} does not validate:", s.front.id);
        for f in &findings {
            eprintln!("  {f}");
        }
        return Ok(ExitCode::FAILURE);
    }

    gate::approve(&p, &mut s, false)?;
    index::write(&p)?;
    println!("approved {} {}", s.front.id, s.front.title);
    println!("  the hash is frozen: editing the spec reopens the gate");
    Ok(ExitCode::SUCCESS)
}

fn cmd_gate(args: IdArgs) -> Result<ExitCode> {
    let p = Project::discover()?;
    let id = args.id.context("gate needs a spec id")?;
    let s = spec::find(&p.specs_dir(), &id)?;
    let verdict = gate::check(&p, &s)?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "id": s.front.id,
                "open": verdict.is_open(),
                "verdict": format!("{verdict:?}"),
                "message": verdict.explain(&s.front.id),
            }))?
        );
    } else if verdict.is_open() {
        println!("{}", verdict.explain(&s.front.id));
    } else {
        eprintln!("{}", verdict.explain(&s.front.id));
    }

    Ok(if verdict.is_open() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn cmd_start(args: IdArgs) -> Result<ExitCode> {
    let p = Project::discover()?;
    let id = args.id.context("start needs a spec id")?;
    let mut s = spec::find(&p.specs_dir(), &id)?;
    lifecycle::start(&p, &mut s)?;
    index::write(&p)?;
    let total = s.tier_a_tests().len();
    println!("implementing {} {}", s.front.id, s.front.title);
    println!("  {total} tier A criteria to make live");
    Ok(ExitCode::SUCCESS)
}

/// The counterpart to `approve`: the point where "am I done?" stops being an
/// opinion. Verification runs here rather than being taken on trust.
fn cmd_close(args: IdArgs) -> Result<ExitCode> {
    let p = Project::discover()?;
    let id = args.id.context("close needs a spec id")?;
    let mut s = spec::find(&p.specs_dir(), &id)?;

    let verify_ok = match run_verification(&p) {
        Ok(ok) => ok,
        Err(e) => {
            eprintln!("could not verify: {e:#}");
            false
        }
    };

    let human = std::io::IsTerminal::is_terminal(&std::io::stdin());
    let blockers = lifecycle::blockers(&p, &s, verify_ok, human)?;
    if !blockers.is_empty() {
        eprintln!("cannot close {}:", s.front.id);
        for b in &blockers {
            eprintln!("  {}", b.explain(&s.front.id));
        }
        // Tier C in particular is unjudgeable from a terminal — it points into
        // the mockup, and the report is where the two sit side by side.
        eprintln!(
            "\nthe criteria and their evidence: bevel review {}",
            s.front.id
        );
        return Ok(ExitCode::FAILURE);
    }

    let commit = lifecycle::finish(&p, &mut s)?;
    index::write(&p)?;
    println!("closed {} {}", s.front.id, s.front.title);
    match commit {
        Some(sha) => println!("  recorded at {}", &sha[..sha.len().min(12)]),
        None => println!("  no commit recorded (not a git repository)"),
    }
    println!("  unresolved deviations in notes.md belong in the inbox");
    Ok(ExitCode::SUCCESS)
}

/// Shared by `close`, so closing proves the same thing `verify --affected` does.
fn run_verification(p: &Project) -> Result<bool> {
    let layers = paths::Layers::resolve()?;
    let cfg = config::Config::load(&layers)?;
    let m = method::resolve(Some(p), &layers, &cfg.method);
    if !m.is_usable() {
        bail!("{}", method::missing_help(&m, &layers));
    }
    let ws = workspace::detect(&p.root)?;
    let deps = lockfile::scan(&p.root);
    let package_paths: Vec<String> = ws.packages.iter().map(|x| x.path.clone()).collect();
    let active = packs::active(
        &packs::load_all(p, &layers, &m)?,
        &p.root,
        &package_paths,
        &deps,
    );
    if active.is_empty() {
        return Ok(true);
    }
    let scope = affected::compute(&p.root, &ws, None)?;
    let scope = match scope {
        affected::Scope::Nothing => affected::Scope::Full("closing a spec".into()),
        other => other,
    };
    let results = verify::run(&p.root, &active, &ws.packages, &scope, None)?;
    println!("{}", verify::summarise(&results));
    Ok(results.iter().all(|r| r.ok()))
}

/// The human channel. Everything an agent needs is behind `--json` on the other
/// commands; this one exists so a person can hold five files in their head at
/// once, and it deliberately cannot act on what it shows.
fn cmd_review(args: ReviewArgs) -> Result<ExitCode> {
    let p = Project::discover()?;
    let s = spec::find(&p.specs_dir(), &args.id)?;
    let path = review::write(&p, &s, args.open)?;
    println!("wrote {}", p.display_path(&path));
    println!("  file://{}", path.display());
    Ok(ExitCode::SUCCESS)
}

fn cmd_pause(args: IdArgs) -> Result<ExitCode> {
    let p = Project::discover()?;
    let id = args.id.context("pause needs a spec id")?;
    let mut s = spec::find(&p.specs_dir(), &id)?;
    gate::pause(&mut s)?;
    index::write(&p)?;
    let pending = validate::pending_markers(&p.root, &s.front.id);
    let total = s.tier_a_tests().len();
    println!("paused {} — approval intact, resume any time", s.front.id);
    println!(
        "  progress: {}/{total} criteria live",
        total.saturating_sub(pending)
    );
    Ok(ExitCode::SUCCESS)
}

fn cmd_status(args: StatusArgs) -> Result<ExitCode> {
    let p = Project::discover()?;
    let s = summary::build(&p)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&s)?);
    } else if args.brief {
        print!("{}", s.render_brief());
    } else {
        print!("{}", s.render());
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_list(args: ListArgs) -> Result<ExitCode> {
    let p = Project::discover()?;
    let filter = match &args.status {
        Some(s) => Some(Status::parse(s).with_context(|| format!("unknown status `{s}`"))?),
        None => None,
    };
    let specs: Vec<Spec> = spec::all(&p.specs_dir())?
        .into_iter()
        .filter(|s| filter.is_none() || filter == Some(s.front.status))
        .collect();

    if args.json {
        let payload: Vec<_> = specs
            .iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.front.id,
                    "title": s.front.title,
                    "status": s.front.status.as_str(),
                    "dir": p.display_path(&s.dir),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        if specs.is_empty() {
            println!("no matching specs");
        }
        for s in &specs {
            println!(
                "  {}  {:<14} {}",
                s.front.id,
                s.front.status.as_str(),
                s.front.title
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}
