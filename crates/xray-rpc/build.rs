fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    let protos = [
        "proto/common/serial/typed_message.proto",
        "proto/common/protocol/user.proto",
        "proto/app/stats/command/command.proto",
        "proto/app/proxyman/command/command.proto",
        "proto/proxy/trojan/config.proto",
        "proto/proxy/vless/account.proto",
        "proto/proxy/shadowsocks/config.proto",
        "proto/proxy/shadowsocks_2022/config.proto",
        "proto/proxy/hysteria/account/config.proto",
    ];

    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&protos, &["proto"])?;

    println!("cargo:rerun-if-changed=proto");
    Ok(())
}
