use anyhow::Result;

pub fn handle_request(req: lsp_server::Message::Request) {
	#[rustfmt_skip]
	match req.method.as_str() {
		Formatting::METHOD => handle_formatting(serde_json::from_value(req.params)),
		_ => {}
	}
}

fn handle_formatting(
	params: lsp_types::DocumentFormattingParams,
) -> Result<Option<Vec<lsp_types::TextEdit>>> {
	let doc_path = params.text_document.uri.to_file_path()?;
	let doc_file = doc_path.file_name().unwrap().to_str()?;

	let type_arg = if doc_file == "BUILD" || doc_file == "BUILD.bazel" {
		"build"
	} else if doc_file == "WORKSPACE" || doc_file == "WORKSPACE.bazel" {
		"workspace"
	} else if doc_file.ends_with(".bzl") {
		"bzl"
	} else if doc_file == "MODULE.bazel" {
		"module"
	} else {
		"default"
	};
}
