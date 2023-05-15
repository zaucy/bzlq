fn main() {
	protobuf_codegen::Codegen::new()
		.protoc()
		.protoc_path(&protoc_bin_vendored::protoc_bin_path().unwrap())
		.includes(&["bazel"])
		.input("bazel/src/main/protobuf/build.proto")
		.input("bazel/src/main/protobuf/analysis_v2.proto")
		.out_dir("src/bazel")
		.run_from_script();
}
