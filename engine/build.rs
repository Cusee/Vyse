// build.rs — runs before the Rust compiler on every `cargo build`.
//
// Compiles proto/vyse.proto into Rust source code using tonic-build.
// The generated file lands in OUT_DIR (typically target/debug/build/...)
// and is included into src/proto/mod.rs via `include_proto!("vyse.v1")`.
//
// If the .proto file has not changed since the last build, tonic-build
// is a no-op — there is no performance penalty for having this file.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        // Emit the file descriptor set so runtime reflection works.
        // Useful for tools like grpcurl during development.
        .file_descriptor_set_path(
            std::path::PathBuf::from(std::env::var("OUT_DIR")?)
                .join("vyse_descriptor.bin"),
        )
        // Derive serde Serialize/Deserialize on all generated types so they
        // can be serialised to JSON for the admin REST API and audit logs
        // without a manual conversion step.
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        // Skip the serde derive on maps (prost generates BTreeMap which
        // serde handles natively; the attribute would cause a duplicate).
        .compile(
            &["../proto/vyse.proto"],
            // Include paths — the first argument to protoc's -I flag.
            &["../proto"],
        )?;

    // Tell Cargo to re-run this build script if the proto definition changes.
    println!("cargo:rerun-if-changed=../proto/vyse.proto");
    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}