mod bazel;
pub mod protos;

use anyhow::anyhow;
use anyhow::Result;
use bazel::build::QueryResult;
use protobuf::Message;
use std::path::PathBuf;

pub fn list_targets(
	workspace_name: &str,
	query: &str,
) -> Result<Vec<protos::bzlq::TargetDetail>> {
	let query_bin_path = get_query_bin_file_path(workspace_name, "query");
	let query_bin: Vec<u8>;

	if !query_bin_path.exists() {
		query_bin = update_query(UpdateQueryOptions {
			workspace_name: workspace_name.to_string(),
			filename: "query".to_string(),
			query: query.to_string(),
		})?;
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

pub fn create_target_details_message(
	workspace_name: &str,
) -> Result<protos::bzlq::TargetDetails> {
	let mut all_targets = list_targets(workspace_name, "//...")?;
	all_targets.append(&mut list_external_targets(workspace_name)?);
	let all_targets = all_targets;

	let mut target_details = protos::bzlq::TargetDetails::new();
	target_details.target_detail.reserve(all_targets.len());

	for target in all_targets {
		target_details.target_detail.push(target);
	}

	return Ok(target_details);
}

pub fn create_target_details(
	rule: &bazel::build::Rule,
) -> protos::bzlq::TargetDetail {
	let mut target_details = protos::bzlq::TargetDetail::new();
	target_details.label = rule.name().to_string();
	target_details.description = rule.rule_class().to_string();
	target_details.is_executable = is_executable_rule(&rule);
	target_details.is_test = is_test_rule(&rule);
	return target_details;
}

pub fn get_target_details(
	workspace_name: &str,
) -> Result<protos::bzlq::TargetDetails> {
	let path = get_query_bin_file_path(workspace_name, "targets");
	let mut file = std::fs::File::open(path)?;
	let msg = protos::bzlq::TargetDetails::parse_from_reader(&mut file)?;
	return Ok(msg);
}

pub fn list_external_targets(
	workspace_name: &str,
) -> Result<Vec<protos::bzlq::TargetDetail>> {
	let mut target_details: Vec<protos::bzlq::TargetDetail> = Vec::new();
	let external_bin_path = get_query_bin_file_path(workspace_name, "external");
	let external_bin: Vec<u8>;
	if !external_bin_path.exists() {
		external_bin = update_external(workspace_name)?;
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
					list_targets(dep_name, &format!("@{}//...", dep_name))
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

pub fn update_target_details(
	workspace_name: &str,
) -> Result<protos::bzlq::TargetDetails> {
	let msg = create_target_details_message(workspace_name)?;
	let path = get_query_bin_file_path(workspace_name, "targets");
	let mut file = std::fs::File::create(&path)?;

	msg.write_to_writer(&mut file)?;

	return Ok(msg);
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

pub fn get_root_cache_dir() -> PathBuf {
	dirs::cache_dir()
		.expect("Failed get cache dir")
		.join("bzlq")
}

pub fn get_cache_dir(workspace_name: &str) -> PathBuf {
	get_root_cache_dir().join(workspace_name)
}

pub fn get_query_bin_file_path(
	workspace_name: &str,
	filename: &str,
) -> PathBuf {
	get_cache_dir(workspace_name).join(format!("{}.bin", filename))
}

pub struct UpdateQueryOptions {
	pub workspace_name: String,
	pub filename: String,
	pub query: String,
}

pub fn update_query(options: UpdateQueryOptions) -> Result<Vec<u8>> {
	let query_bin_file_path =
		get_query_bin_file_path(&options.workspace_name, &options.filename);

	if let Some(parent) = query_bin_file_path.parent() {
		std::fs::create_dir_all(parent)?;
	}

	let query_bin = std::process::Command::new("bazel")
		.arg("query")
		.arg(options.query)
		.arg("--output=proto")
		.output()?
		.stdout;

	std::fs::write(&query_bin_file_path, &query_bin)?;

	return Ok(query_bin);
}

pub fn update_external(workspace_name: &str) -> Result<Vec<u8>> {
	update_query(UpdateQueryOptions {
		workspace_name: workspace_name.to_string(),
		filename: "external".to_string(),
		query: "//external:*".to_string(),
	})
}

pub fn find_bazel_workspace_path(base_dir: PathBuf) -> Result<PathBuf> {
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

pub fn get_workspace_name(path: PathBuf) -> Result<String> {
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
