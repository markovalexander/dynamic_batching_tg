use std::path::PathBuf;
use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../proto/service.proto");
    fs::create_dir("src/pb").unwrap_or(());

    let mut config = prost_build::Config::new();
    config.protoc_arg("--experimental_allow_proto3_optional");

    let descriptor_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("my_descriptor.bin");

    tonic_build::configure()
        .file_descriptor_set_path(descriptor_path)
        .build_client(false)
        .build_server(true)
        .out_dir("src/pb")
        .include_file("mod.rs")
        .compile_with_config(config, &["../proto/service.proto"], &["../proto"])
        .unwrap_or_else(|e| panic!("protobuf compilation failed: {e}"));

    Ok(())
}
