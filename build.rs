fn main() {
    capnpc::CompilerCommand::new()
        .file("uipd.capnp")
        .run()
        .expect("schema compiler command");
}
