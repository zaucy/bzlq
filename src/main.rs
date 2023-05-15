mod bazel;

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use protobuf::Message;

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

fn find_bazel_workspace_path(base_dir: PathBuf) -> Result<PathBuf> {
	for file in ["MODULE.bazel", "WORKSPACE.bazel", "WORKSPACE"] {
		let workspace_file_path = base_dir.join(file);
		if workspace_file_path.exists() {
			return Ok(workspace_file_path);
		}
	}

	if let Some(parent) = base_dir.parent() {
		return find_bazel_workspace_path(parent.to_owned());
	}

	return Err(anyhow!("bzlq must be used within a bazel workspace"));
}

fn get_workspace_name(path: PathBuf) -> Result<String> {
	let file_contents = std::fs::read_to_string(&path)?;
	if path.file_stem().unwrap() == "WORKSPACE" {
		let workspace_statement_regex =
			regex::Regex::new(r#"workspace\s*\(\s*name\s*=\s*"(.*)".*\)"#)?;

		if let Some(workspace_name) = workspace_statement_regex
			.captures(&file_contents)
			.unwrap()
			.get(1)
		{
			return Ok(workspace_name.as_str().to_owned());
		}
		return Err(anyhow!(
			"unable to extract workspace name from {}",
			path.to_string_lossy()
		));
	} else {
		let module_statement_regex =
			regex::Regex::new(r#"module\s*\(\s*name\s*=\s*"(.*)".*\)"#)?;

		if let Some(workspace_name) = module_statement_regex
			.captures(&file_contents)
			.unwrap()
			.get(1)
		{
			return Ok(workspace_name.as_str().to_owned());
		}
		return Err(anyhow!(
			"unable to extract module name from {}",
			path.to_string_lossy()
		));
	}
}

struct TargetDetails {
	label: String,
	description: String,
	is_executable: bool,
	is_test: bool,
}

fn update_cquery(
	workspace_name: &str,
	dirs: &directories::ProjectDirs,
) -> Result<Vec<u8>> {
	let cache_dir = dirs.cache_dir().join(workspace_name);
	let cquery_bin_path = cache_dir.join("cquery.bin");
	let cquery_bin = std::process::Command::new("bazel")
		.arg("cquery")
		.arg("//...")
		.arg("--output=proto")
		.output()?
		.stdout;
	if let Some(parent) = cquery_bin_path.parent() {
		std::fs::create_dir_all(parent)?;
	}
	std::fs::write(&cquery_bin_path, &cquery_bin)?;
	println!("updated {}", &cquery_bin_path.to_string_lossy());
	return Ok(cquery_bin);
}

fn update_query(
	workspace_name: &str,
	dirs: &directories::ProjectDirs,
) -> Result<Vec<u8>> {
	let cache_dir = dirs.cache_dir().join(workspace_name);
	let query_bin_path = cache_dir.join("query.bin");
	let query_bin = std::process::Command::new("bazel")
		.arg("query")
		.arg("//...")
		.arg("--output=proto")
		.output()?
		.stdout;
	if let Some(parent) = query_bin_path.parent() {
		std::fs::create_dir_all(parent)?;
	}
	std::fs::write(&query_bin_path, &query_bin)?;
	println!("updated {}", &query_bin_path.to_string_lossy());
	return Ok(query_bin);
}

fn list_targets(
	workspace_name: &str,
	dirs: &directories::ProjectDirs,
) -> Result<Vec<TargetDetails>> {
	let cache_dir = dirs.cache_dir().join(workspace_name);
	let cquery_bin_path = cache_dir.join("cquery.bin");

	let cquery_bin: Vec<u8>;

	if !cquery_bin_path.exists() {
		cquery_bin = update_cquery(workspace_name, dirs)?;
	} else {
		cquery_bin = std::fs::read(cquery_bin_path)?;
	}

	let msg = bazel::analysis_v2::CqueryResult::parse_from_bytes(&cquery_bin)?;

	let mut target_details: Vec<TargetDetails> =
		Vec::with_capacity(msg.results.len());

	for entry in msg.results.iter() {
		let target = entry.target.clone().unwrap();
		target_details.push(match target.type_() {
			bazel::build::target::Discriminator::RULE => {
				let rule = target.rule.unwrap();
				TargetDetails {
					label: rule.name().to_string(),
					description: rule.rule_class().to_string(),
					is_executable: is_executable_rule(&rule),
					is_test: is_test_rule(&rule),
				}
			}
			bazel::build::target::Discriminator::SOURCE_FILE => todo!(),
			bazel::build::target::Discriminator::GENERATED_FILE => todo!(),
			bazel::build::target::Discriminator::PACKAGE_GROUP => todo!(),
			bazel::build::target::Discriminator::ENVIRONMENT_GROUP => todo!(),
		});
	}

	return Ok(target_details);
}

fn is_executable_rule(rule: &bazel::build::Rule) -> bool {
	let mut is_executable = false;
	let rule_class = rule.rule_class();

	if rule_class.ends_with("_binary") || rule_class.ends_with("_test") {
		is_executable = true;

		if rule_class == "cc_binary" {
			for attr in &rule.attribute {
				if let Some(name) = &attr.name {
					if name == "linkshared" && attr.boolean_value() {
						is_executable = false;
					}
				}
			}
		}
	}

	return is_executable;
}

fn is_test_rule(rule: &bazel::build::Rule) -> bool {
	return rule.rule_class().ends_with("_test");
}

fn main() -> Result<()> {
	let cli = Cli::parse();

	let workspace = find_bazel_workspace_path(std::env::current_dir()?)?;
	let workspace_name = get_workspace_name(workspace)?;
	let proj_dirs = directories::ProjectDirs::from("cy.zau", "zaucy", "bzlq")
		.expect("Failed to load project directories");

	match &cli.command {
		Commands::Update {} => {
			update_cquery(&workspace_name, &proj_dirs)?;
			update_query(&workspace_name, &proj_dirs)?;
		}
		Commands::Targets {
			run_only,
			test_only,
		} => {
			for target in list_targets(&workspace_name, &proj_dirs)?.iter() {
				if *run_only && !target.is_executable {
					continue;
				}

				if *test_only && !target.is_test {
					continue;
				}

				println!(
					r#"{{"label":"{}","description":"{}"}}"#,
					target.label, target.description,
				);
			}
		}
		Commands::Bes {} => {}
	}

	return Ok(());
}
