fn main() {
    prost_build::compile_protos(&["src/gossipsub/rpc.proto"], &["src/gossipsub"]).unwrap();
    prost_build::compile_protos(&["src/floodsub/rpc.proto"], &["src/floodsub"]).unwrap();
}
