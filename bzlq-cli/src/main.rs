use anyhow::Result;
use bzlq::{
	// format 1 by 1
	find_bazel_workspace_path,
	get_query_bin_file_path,
	get_target_details,
	get_workspace_name,
	protos::bzlq::TargetDetails,
	update_external,
	update_query,
	update_target_details,
	UpdateQueryOptions,
};
use clap::{Parser, Subcommand};
use std::io::Write;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	/// Update query data
	Update {},

	/// List bazel targets
	Targets {
		#[arg()]
		search: Option<String>,

		/// Only show executable targets
		#[arg(short, long)]
		run_only: bool,

		/// Only show test targets
		#[arg(short, long)]
		test_only: bool,
	},

	/// Start build event service
	Bes {},
}

fn query_bin_exists(workspace_name: &str, filename: &str) -> bool {
	get_query_bin_file_path(workspace_name, filename).exists()
}

fn main() -> Result<()> {
	let cli = Cli::parse();

	let workspace = find_bazel_workspace_path(std::env::current_dir()?)?;
	let workspace_name = get_workspace_name(workspace)?;

	match &cli.command {
		Commands::Update {} => {
			eprintln!(
				"Updating {}",
				get_query_bin_file_path(&workspace_name, "query")
					.to_string_lossy()
			);
			update_query(UpdateQueryOptions {
				workspace_name: workspace_name.to_string(),
				filename: "query".to_string(),
				query: "//...".to_string(),
			})?;

			eprintln!(
				"Updating {}",
				get_query_bin_file_path(&workspace_name, "external")
					.to_string_lossy()
			);
			update_external(&workspace_name)?;

			eprintln!(
				"Updating {}",
				get_query_bin_file_path(&workspace_name, "targets")
					.to_string_lossy()
			);
			update_target_details(&workspace_name)?;
			eprintln!("Done!");
		}
		Commands::Targets {
			run_only,
			test_only,
			search,
		} => {
			let mut stdout = std::io::stdout();
			let target_details: TargetDetails;
			if !query_bin_exists(&workspace_name, "targets") {
				target_details = update_target_details(&workspace_name)?;
			} else {
				target_details = get_target_details(&workspace_name)?;
			}

			for target in target_details.target_detail {
				if *run_only && !target.is_executable {
					continue;
				}

				if *test_only && !target.is_test {
					continue;
				}

				if let Some(search) = search {
					if !search.is_empty() && !target.label.starts_with(search) {
						continue;
					}
				}

				stdout.write_all("{\"label\":\"".as_bytes())?;
				stdout.write_all(target.label.as_bytes())?;
				stdout.write_all("\",\"description\":\"".as_bytes())?;
				stdout.write_all(target.description.as_bytes())?;
				stdout.write_all("\"}\n".as_bytes())?;
			}
		}
		Commands::Bes {} => {}
	}

	return Ok(());
}
