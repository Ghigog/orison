//! `orison`: the headless playable client (migration plan Phase 5).
//!
//! The milestone that decides whether the migration worked. Everything below
//! it — ingest, the knowledge graph, retrieval, the inference layer, the turn
//! loop — was built and measured over Phases 0-4 without anything that could
//! play a campaign end to end. This is that.
//!
//! ```text
//! orison new --title "Thornwick" --vault ./fixtures/vaults/minimal
//! orison campaigns
//! orison import thornwick ./fixtures/vaults/minimal
//! orison play thornwick --actor-model llama3.2:3b
//! ```
//!
//! Nothing here reaches an endpoint the user did not configure, and that is a
//! product pillar rather than an implementation detail: no vault content,
//! gameplay text or user data leaves the machine.

use orison_cli::{campaign, config, error, shell};

use std::collections::BTreeMap;
use std::io::{BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use orison_core::turn::{TurnConfig, TurnEngine};

use config::ModelArgs;
use error::CliError;
use shell::Shell;

#[derive(Parser)]
#[command(
    name = "orison",
    about = "Play an Orison campaign in a terminal. Local models only.",
    version
)]
struct Cli {
    /// The campaign database. One file holds every campaign.
    #[arg(long, global = true, env = "ORISON_DB")]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a campaign, optionally importing a vault at the same time.
    New {
        #[arg(long)]
        title: String,
        /// A Markdown vault to compile into it.
        #[arg(long)]
        vault: Option<PathBuf>,
        /// Folder-to-type mappings, as `folder=type`. Beats every heuristic;
        /// the real-vault run found 48% of notes untyped without them.
        #[arg(long = "folder-type", value_parser = parse_folder_type)]
        folder_types: Vec<(String, String)>,
    },
    /// List the campaigns in the database.
    Campaigns,
    /// Compile a vault into an existing campaign, replacing its graph.
    Import {
        campaign: String,
        vault: PathBuf,
        #[arg(long = "folder-type", value_parser = parse_folder_type)]
        folder_types: Vec<(String, String)>,
    },
    /// Play. Reads from stdin, so a scripted transcript can be piped in.
    Play {
        /// Which campaign. Omitted, the most recently played one.
        campaign: Option<String>,
        #[command(flatten)]
        models: ModelArgs,
    },
}

fn parse_folder_type(raw: &str) -> Result<(String, String), String> {
    raw.split_once('=')
        .map(|(folder, kind)| (folder.trim().to_string(), kind.trim().to_string()))
        .ok_or_else(|| format!("expected folder=type, got {raw:?}"))
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), CliError> {
    let cli = Cli::parse();
    let db = cli.db.unwrap_or_else(campaign::default_database);
    let store = campaign::open_store(&db)?;
    let mut out = std::io::stdout();

    match cli.command {
        Command::New {
            title,
            vault,
            folder_types,
        } => {
            let created = campaign::create(
                &store,
                &title,
                &shell::timestamp(),
                vault.as_deref(),
                &folder_types.into_iter().collect::<BTreeMap<_, _>>(),
            )?;
            writeln!(
                out,
                "Created {:?} as {}.",
                created.campaign.title, created.campaign.id
            )?;
            if let Some(report) = created.import {
                print_import(&mut out, &report)?;
            }
            writeln!(out, "Play it: orison play {}", created.campaign.id)?;
        }

        Command::Campaigns => {
            let all = campaign::list(&store)?;
            if all.is_empty() {
                return Err(CliError::NoCampaigns);
            }
            for c in all {
                writeln!(
                    out,
                    "{:<24} {:<32} last played {}  ({:.0} min)",
                    c.id,
                    c.title,
                    c.last_played,
                    c.playtime_seconds / 60.0
                )?;
            }
        }

        Command::Import {
            campaign: id,
            vault,
            folder_types,
        } => {
            let report = campaign::import(
                &store,
                &id,
                &vault,
                &folder_types.into_iter().collect::<BTreeMap<_, _>>(),
            )?;
            print_import(&mut out, &report)?;
        }

        Command::Play {
            campaign: id,
            models,
        } => {
            let id = match id {
                Some(id) => id,
                None => campaign::list(&store)?
                    .into_iter()
                    .max_by(|a, b| a.last_played.cmp(&b.last_played))
                    .map(|c| c.id)
                    .ok_or(CliError::NoCampaigns)?,
            };
            let session = campaign::open_session(&store, &id)?;
            if campaign::is_empty(&session) {
                writeln!(
                    out,
                    "{id} has no vault compiled into it yet. Run: orison import {id} <vault>"
                )?;
                return Ok(());
            }

            if models.tokenizer_is_approximate() {
                writeln!(
                    out,
                    "No --tokenizer given: token counts are word counts, so the prompt budget \
                     is approximate. Point --tokenizer at the model's tokenizer.json for real \
                     ones (§2.5)."
                )?;
            }

            let backends = models.connect().await?;
            let config = TurnConfig {
                profile: backends.profile,
                ..TurnConfig::default()
            };
            let engine = TurnEngine::new(session, backends.actor, backends.director, config);

            let mut shell = Shell::new(Arc::clone(&engine), std::io::stdout());
            shell.greet(&backends.summary)?;
            shell.prompt();
            let played = shell.run(BufReader::new(std::io::stdin().lock())).await?;
            engine.shutdown();
            let n = played.len();
            println!("\n{n} turn{} played.", if n == 1 { "" } else { "s" });
        }
    }
    Ok(())
}

fn print_import(
    out: &mut impl Write,
    report: &orison_core::ingest::IngestReport,
) -> Result<(), CliError> {
    writeln!(
        out,
        "Imported {} notes: {} sections mapped, {} overflowed, {} chunks, {} untyped notes.",
        report.notes_seen,
        report.sections_mapped,
        report.sections_overflowed,
        report.chunks,
        report.notes_without_a_type
    )?;
    // Never a count on its own: the invariant is that nothing is silently
    // discarded, so an unaccounted section is named rather than tallied.
    if !report.unaccounted_sections.is_empty() {
        writeln!(
            out,
            "  {} sections could not be accounted for. This is a bug, not a warning:",
            report.unaccounted_sections.len()
        )?;
        for section in &report.unaccounted_sections {
            writeln!(out, "    {section}")?;
        }
    }
    if !report.dangling_links.is_empty() {
        writeln!(
            out,
            "  {} links point at notes that do not exist (kept, not dropped).",
            report.dangling_links.len()
        )?;
    }
    if !report.gender_conflicts.is_empty() {
        writeln!(
            out,
            "  {} characters have frontmatter that disagrees with their prose about gender. \
             Worth a read: a wrong pronoun is rag_architecture.md Bug 3.",
            report.gender_conflicts.len()
        )?;
    }
    Ok(())
}
