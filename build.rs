// Compiles the one proto the CLI owns locally: `quantiz/adaptive.proto`,
// the cloud `AdaptiveService` memory surface the CLI dials over gRPC-Web
// (see `src/memory_client.rs`). Core no longer ships this proto to library
// consumers after the purify cut (commit 2cc29b3), so the CLI generates its
// own prost message types. Mirrors core/build.rs's `compile_proto` helper.
//
// The generated file lands at `src/proto/quantiz.adaptive.rs` and is included
// by `src/proto/mod.rs`.
fn compile_proto(
    proto_dir: &std::path::Path,
    out_dir: &str,
    proto_file: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let proto_path = proto_dir.join(proto_file);
    if !proto_path.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(out_dir)?;
    let proto_dir_buf = proto_dir.to_path_buf();
    tonic_prost_build::configure()
        .build_server(false)
        .build_client(false)
        .out_dir(out_dir)
        .compile_protos(&[&proto_path], &[&proto_dir_buf])?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = std::path::Path::new("../foundry/proto");
    compile_proto(proto_dir, "src/proto", "quantiz/adaptive.proto")?;
    Ok(())
}
