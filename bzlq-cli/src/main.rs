mod handler;

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
use lsp_server::Connection;
use lsp_types::{
	request::{Formatting, Request},
	ClientCapabilities, DocumentFormattingParams, InitializeParams,
	ServerCapabilities,
};
use std::{io::Write, path::Path};

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

	LanguageServer {
		#[arg()]
		stdio: bool,
	},
}

fn query_bin_exists(workspace_name: &str, filename: &str) -> bool {
	get_query_bin_file_path(workspace_name, filename).exists()
}

fn run_buildifier(
	params: lsp_types::DocumentFormattingParams,
) -> Result<Option<Vec<lsp_types::TextEdit>>> {
}

fn language_server() -> Result<()> {
	let (connection, io_threads) = Connection::stdio();

	let (id, params) = connection.initialize_start()?;

	let init_params: InitializeParams = serde_json::from_value(params).unwrap();
	let client_capabilities: ClientCapabilities = init_params.capabilities;
	let server_capabilities = ServerCapabilities::default();

	let initialize_data = serde_json::json!({
		"capabilities": server_capabilities,
		"serverInfo": {
			"name": env!("CARGO_PKG_NAME"),
			"version": env!("CARGO_PKG_VERSION"),
		}
	});

	connection.initialize_finish(id, initialize_data)?;

	for msg in &connection.receiver {
		match msg {
			lsp_server::Message::Request(req) => {
				if connection.handle_shutdown(&req)? {
					return Ok(());
				}

				let res = lsp_server::Response {
					id,
					result: None,
					error: None,
				};
				let res: lsp_server::Response =
					match handler::request::handle_request(req) {
						Ok(result) => {}
						_ => (),
					};
			}
			lsp_server::Message::Response(_) => todo!(),
			lsp_server::Message::Notification(_) => todo!(),
		}
	}

	Ok(())
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
		Commands::LanguageServer { stdio } => {
			language_server()?;
		}
	}

	return Ok(());
}
