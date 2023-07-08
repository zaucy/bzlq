mod bazel;
mod protos;

use std::{io::Write, path::PathBuf};

use anyhow::{anyhow, Result};
use bazel::build::QueryResult;
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
		#[arg()]
		search: String,

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

fn query_bin_path(
	workspace_name: &str,
	dirs: &directories::ProjectDirs,
	filename: &str,
) -> PathBuf {
	let cache_dir = dirs.cache_dir().join(workspace_name);
	let query_bin_path = cache_dir.join(format!("{}.bin", filename));
	return query_bin_path;
}

fn query_bin_exists(
	workspace_name: &str,
	dirs: &directories::ProjectDirs,
	filename: &str,
) -> bool {
	return query_bin_path(workspace_name, dirs, filename).exists();
}

fn update_query(
	workspace_name: &str,
	dirs: &directories::ProjectDirs,
	filename: &str,
	query: &str,
) -> Result<Vec<u8>> {
	let path = query_bin_path(workspace_name, dirs, filename);
	let query_bin = std::process::Command::new("bazel")
		.arg("query")
		.arg(query)
		.arg("--output=proto")
		.output()?
		.stdout;
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent)?;
	}
	std::fs::write(&path, &query_bin)?;
	eprintln!("updated {}", &path.to_string_lossy());
	return Ok(query_bin);
}

fn update_external(
	workspace_name: &str,
	dirs: &directories::ProjectDirs,
) -> Result<Vec<u8>> {
	return update_query(workspace_name, dirs, "external", "//external:*");
}

fn create_target_details(
	rule: &bazel::build::Rule,
) -> protos::bzlq::TargetDetail {
	let mut target_details = protos::bzlq::TargetDetail::new();
	target_details.label = rule.name().to_string();
	target_details.description = rule.rule_class().to_string();
	target_details.is_executable = is_executable_rule(&rule);
	target_details.is_test = is_test_rule(&rule);
	return target_details;
}

fn list_targets(
	workspace_name: &str,
	dirs: &directories::ProjectDirs,
	query: &str,
) -> Result<Vec<protos::bzlq::TargetDetail>> {
	let cache_dir = dirs.cache_dir().join(workspace_name);
	let query_bin_path = cache_dir.join("query.bin");
	let query_bin: Vec<u8>;

	if !query_bin_path.exists() {
		query_bin = update_query(workspace_name, dirs, "query", query)?;
	} else {
		query_bin = std::fs::read(query_bin_path)?;
	}

	let query_result = QueryResult::parse_from_bytes(&query_bin)?;

	let mut target_details: Vec<protos::bzlq::TargetDetail> =
		Vec::with_capacity(query_result.target.len());

	for target in query_result.target {
		target_details.push(match target.type_() {
			bazel::build::target::Discriminator::RULE => {
				let rule = target.rule.unwrap();
				create_target_details(&rule)
			}
			bazel::build::target::Discriminator::SOURCE_FILE => todo!(),
			bazel::build::target::Discriminator::GENERATED_FILE => todo!(),
			bazel::build::target::Discriminator::PACKAGE_GROUP => todo!(),
			bazel::build::target::Discriminator::ENVIRONMENT_GROUP => todo!(),
		});
	}

	return Ok(target_details);
}

fn create_target_details_message(
	workspace_name: &str,
	dirs: &directories::ProjectDirs,
) -> Result<protos::bzlq::TargetDetails> {
	let mut all_targets = list_targets(workspace_name, dirs, "//...")?;
	all_targets.append(&mut list_external_targets(workspace_name, dirs)?);
	let all_targets = all_targets;

	let mut target_details = protos::bzlq::TargetDetails::new();
	target_details.target_detail.reserve(all_targets.len());

	for target in all_targets {
		target_details.target_detail.push(target);
	}

	return Ok(target_details);
}

fn update_target_details(
	workspace_name: &str,
	dirs: &directories::ProjectDirs,
) -> Result<protos::bzlq::TargetDetails> {
	let msg = create_target_details_message(workspace_name, dirs)?;
	let path = query_bin_path(workspace_name, dirs, "targets");
	let mut file = std::fs::File::create(&path)?;

	msg.write_to_writer(&mut file)?;
	eprintln!("updated {}", path.to_string_lossy());

	return Ok(msg);
}

fn get_target_details(
	workspace_name: &str,
	dirs: &directories::ProjectDirs,
) -> Result<protos::bzlq::TargetDetails> {
	let path = query_bin_path(workspace_name, dirs, "targets");
	let mut file = std::fs::File::open(path)?;
	let msg = protos::bzlq::TargetDetails::parse_from_reader(&mut file)?;
	return Ok(msg);
}

fn list_external_targets(
	workspace_name: &str,
	dirs: &directories::ProjectDirs,
) -> Result<Vec<protos::bzlq::TargetDetail>> {
	let cache_dir = dirs.cache_dir().join(workspace_name);

	let mut target_details: Vec<protos::bzlq::TargetDetail> = Vec::new();

	let external_bin_path = cache_dir.join("external.bin");
	let external_bin: Vec<u8>;
	if !external_bin_path.exists() {
		external_bin = update_external(workspace_name, dirs)?;
	} else {
		external_bin = std::fs::read(external_bin_path)?;
	}

	let external_query_result = QueryResult::parse_from_bytes(&external_bin)?;

	for target in external_query_result.target {
		match target.type_() {
			bazel::build::target::Discriminator::RULE => {
				let rule = target.rule.unwrap();
				let target_name = rule.name();
				let external_prefix = "//external:";
				if !target_name.starts_with(&external_prefix) {
					continue;
				}
				if target_name.ends_with("WORKSPACE.bazel") {
					continue;
				}

				let dep_name = &target_name[external_prefix.len()..];

				if dep_name.contains("/") {
					continue;
				}

				let dep_name = &target_name[external_prefix.len()..];
				if let Ok(ref mut ext_targets) =
					list_targets(dep_name, dirs, &format!("@{}//...", dep_name))
				{
					target_details.append(ext_targets);
				}
			}
			bazel::build::target::Discriminator::SOURCE_FILE => {}
			bazel::build::target::Discriminator::GENERATED_FILE => {}
			bazel::build::target::Discriminator::PACKAGE_GROUP => {}
			bazel::build::target::Discriminator::ENVIRONMENT_GROUP => {}
		}
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
			update_query(&workspace_name, &proj_dirs, "query", "//...")?;
			update_external(&workspace_name, &proj_dirs)?;
			update_target_details(&workspace_name, &proj_dirs)?;
		}
		Commands::Targets {
			run_only,
			test_only,
			search,
		} => {
			let mut stdout = std::io::stdout();
			let target_details: protos::bzlq::TargetDetails;
			if !query_bin_exists(&workspace_name, &proj_dirs, "targets") {
				target_details =
					update_target_details(&workspace_name, &proj_dirs)?;
			} else {
				target_details =
					get_target_details(&workspace_name, &proj_dirs)?;
			}

			for target in target_details.target_detail {
				if *run_only && !target.is_executable {
					continue;
				}

				if *test_only && !target.is_test {
					continue;
				}

				if !search.is_empty() && !target.label.starts_with(search) {
					continue;
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
