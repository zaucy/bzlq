let googleapis_ref = "ef2e2ea532248d6dc40a56bc6c95cea858ba31b6";

let bazel_protos = [
	"src/main/protobuf/build.proto",
	"src/main/protobuf/analysis_v2.proto",
];

[
	"google/devtools/build/v1/publish_build_event.proto",
	"google/api/annotations.proto",
	"google/api/client.proto",
	"google/api/field_behavior.proto",
	"google/devtools/build/v1/build_events.proto",
	"google/api/http.proto",
	"google/api/launch_stage.proto",
	"google/devtools/build/v1/build_status.proto",
] | each {|f|
	let url = $"https://github.com/googleapis/googleapis/raw/($googleapis_ref)/($f)";
	let local_path = $"googleapis/($f)";

	mkdir ($local_path | path parse | get parent);
	http get $url | save $local_path -f;
};

